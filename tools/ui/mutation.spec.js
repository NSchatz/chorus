// The demonstrations: a check that cannot fail is not evidence.
//
// Each test below takes the SAME measuring code `ui.spec.js` grades the real
// page with, points it at a page that breaks exactly one property on purpose,
// and requires it to report the break. They are committed rather than run once
// on a branch, so a later reader can re-run them instead of being told about
// them, and tools/ui-render-run.sh refuses if any claim reaches the end of a run
// with no demonstration beside it.
//
// Almost nothing here touches the served page. These render markup this file
// holds, in the same engine, through the same functions. The two that cannot
// (a response with no policy, and a policy that silences the page it was sent
// with) are served by tools/ui/fixture-server.js, because what they mutate is a
// header and a header has to come off a wire.

const { test, expect } = require("@playwright/test");
const { measureControls, tooSmall } = require("./measure");
const contrast = require("./contrast");
const a11y = require("./a11y");
const reads = require("./reads");
const engine = require("./engine");
const reconcile = require("./reconcile");
const typeface = require("./typeface");
const { demonstrates } = require("./claims");

const MINIMUM = 24;
const MOST_WORDS = 12;
const FIXTURE = process.env.CHORUS_UI_FIXTURE;

/// Render a page out of a string, with no server involved.
async function render(page, html) {
  await page.setContent(html, { waitUntil: "load" });
}

/// Put the fixture back to its default scenario, so a demonstration that pauses
/// it does not leave it paused for whatever runs next.
async function resetFixture() {
  await fetch(`${FIXTURE}/fixture/scenario`, { method: "POST", body: "{}" });
}

async function fixtureDo(what) {
  const response = await fetch(`${FIXTURE}/fixture/${what}`, { method: "POST" });
  if (!response.ok) {
    throw new Error(`the fixture refused ${what}: ${await response.text()}`);
  }
}

// --- target size ------------------------------------------------------------

test("a control painted under the minimum is reported, with its size", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <button style="width:44px;height:44px">fine</button>
       <button style="width:44px;height:16px" aria-label="short one">short</button>
     </body></html>`
  );
  const controls = await measureControls(page);
  expect(controls.length).toBe(2);
  const offenders = tooSmall(controls, MINIMUM);
  expect(offenders.length).toBe(1);
  expect(offenders[0].label).toBe("short one");
  expect(offenders[0].height).toBeLessThan(MINIMUM);
  demonstrates("target-size");
});

test("a rule that never wins the cascade does not save a control", async ({
  page,
}) => {
  // This is the failure the criterion names by name. The stylesheet DECLARES
  // 44 pixels; a later, more specific rule overrides it; the element is painted
  // at 12. Any check that read the stylesheet would find the 44 and pass.
  await render(
    page,
    `<!doctype html><html><head><style>
       button { min-height: 44px; min-width: 44px; height: 44px; }
       body.compact button#one { min-height: 12px; min-width: 12px; height: 12px; width: 12px; }
     </style></head>
     <body class="compact"><button id="one" aria-label="overridden">x</button></body></html>`
  );
  const controls = await measureControls(page);
  const offenders = tooSmall(controls, MINIMUM);
  expect(
    offenders.length,
    "a rule that lost the cascade was read as though it had won"
  ).toBe(1);
  expect(offenders[0].label).toBe("overridden");
  expect(Math.round(offenders[0].height)).toBe(12);
  demonstrates("target-size");
});

test("a range input with no height set is under the minimum in this engine", async ({
  page,
}) => {
  // Why the real page sets a height on its sliders at all: a range input's
  // intrinsic box is well under 24 pixels tall until something says otherwise,
  // so this is the case the rule in chorus.css exists for, shown failing
  // without it.
  await render(
    page,
    `<!doctype html><html><body>
       <input type="range" aria-label="bare slider" min="0" max="100">
     </body></html>`
  );
  const controls = await measureControls(page);
  expect(controls.length).toBe(1);
  expect(
    tooSmall(controls, MINIMUM).length,
    `a bare range input measured ${JSON.stringify(controls[0])}`
  ).toBe(1);
  demonstrates("target-size");
});

test("a control that is not displayed is reported rather than passing as absent", async ({
  page,
}) => {
  // A zero box is not a small box, and it is not a control that is not there.
  // Reporting it is what stops "hide the offender" from being a way past this.
  await render(
    page,
    `<!doctype html><html><body>
       <button style="display:none" aria-label="hidden one">x</button>
     </body></html>`
  );
  const controls = await measureControls(page);
  expect(controls.length).toBe(1);
  expect(controls[0].display).toBe("none");
  expect(tooSmall(controls, MINIMUM).length).toBe(1);
  demonstrates("target-size");
});

// --- what the page shows ----------------------------------------------------

test("a card whose figures disagree with the state is reported field by field", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <section data-zone="kitchen">
         <h2 data-zone-name="kitchen">The Kitchen</h2>
         <span data-volume="kitchen">12%</span>
         <span data-mute-state="kitchen">Muted</span>
         <span data-endpoints="kitchen">1 of 2 endpoints attached, 1 away</span>
         <span data-freshness="kitchen">live</span>
       </section>
     </body></html>`
  );
  const zones = await reads.renderedZones(page);
  const problems = reads.zoneMismatches(zones, {
    kitchen: { volume: "38%", mute: "Not muted" },
  });
  expect(problems.length).toBe(2);
  expect(problems.map((p) => p.field).sort()).toEqual(["mute", "volume"]);
  demonstrates("zones-shown");
});

test("a page that had to be reloaded to change is reported as a different document", async ({
  page,
}) => {
  await page.goto(`${FIXTURE}/demo/no-csp`);
  const marked = await reads.markDocument(page);
  expect((await reads.documentSurvived(page, marked)).sameDocument).toBe(true);
  await page.reload();
  const survived = await reads.documentSurvived(page, marked);
  expect(
    survived.sameDocument,
    "a reload was read as though the page under it had never left"
  ).toBe(false);
  demonstrates("live-update");
});

test("a page that was never painted has one colour and no agreement", async ({
  page,
}) => {
  await render(page, `<!doctype html><html><body></body></html>`);
  const evidence = await engine.paintEvidence(page, "body");
  expect(
    evidence.colours,
    "a blank rendering was read as though something had been drawn"
  ).toBeLessThan(16);
  demonstrates("rendered-by-a-real-engine");
});

// --- contrast ---------------------------------------------------------------

test("text below its contrast floor is reported with the ratio it measured", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body style="background:#888888;margin:0">
       <p style="color:#777777;font-size:16px;padding:20px" data-pale>a paragraph nobody can read</p>
     </body></html>`
  );
  const runs = await contrast.textContrast(page);
  const pale = contrast.tooPale(runs);
  expect(pale.length, JSON.stringify(runs, null, 2)).toBeGreaterThanOrEqual(1);
  const offender = pale.find((run) => run.what.includes("data-pale"));
  expect(offender).toBeTruthy();
  expect(offender.ratio).toBeLessThan(contrast.TEXT_FLOOR);
  demonstrates("contrast-in-both-themes");
});

test("the control boundary this page shipped before is reported, at the ratio it measured", async ({
  page,
}) => {
  // Not an invented failure. These are the exact colours the control page
  // shipped with until the frontend-conventions work: a 1px border of #c9d2da
  // on a #ffffff panel, which is the known gap the spec named. The measurement
  // puts it at about 1.5:1 against a floor of 3, and the page carried it for as
  // long as nothing rendered it and looked.
  await render(
    page,
    `<!doctype html><html><body style="background:#ffffff;margin:0;padding:40px">
       <button aria-label="the old boundary"
               style="background:#ffffff;border:1px solid #c9d2da;color:#101418;
                      border-radius:8px;width:120px;height:44px;font-size:16px">Ungroup</button>
     </body></html>`
  );
  const marks = await contrast.nonTextContrast(page);
  const bad = contrast.indistinct(marks);
  expect(bad.length, JSON.stringify(marks, null, 2)).toBe(1);
  expect(bad[0].label).toBe("the old boundary");
  expect(bad[0].ratio).toBeLessThan(contrast.NON_TEXT_FLOOR);
  expect(bad[0].ratio).toBeGreaterThan(1.4);
  expect(bad[0].ratio).toBeLessThan(1.7);
  demonstrates("contrast-in-both-themes");
});

test("a slider whose thumb does not stand out from its track is reported", async ({
  page,
}) => {
  // The floor a slider has that a button does not: the mark carrying the value
  // has to be findable against the bar it slides along.
  await render(
    page,
    `<!doctype html><html><head><style>
       body { background: #ffffff; margin: 0; padding: 40px; }
       input[type=range] { -webkit-appearance: none; appearance: none;
         width: 200px; height: 44px; background: transparent; border: 0; }
       input[type=range]::-webkit-slider-runnable-track {
         height: 10px; background: #6f6f6f; }
       input[type=range]::-webkit-slider-thumb { -webkit-appearance: none;
         width: 24px; height: 24px; margin-top: -7px; border-radius: 50%;
         background: #7a7a7a; }
     </style></head><body>
       <input type="range" aria-label="pale thumb" min="0" max="100" value="50">
     </body></html>`
  );
  const marks = await contrast.nonTextContrast(page);
  const bad = contrast.indistinct(marks);
  expect(bad.length, JSON.stringify(marks, null, 2)).toBe(1);
  expect(bad[0].kind).toBe("slider");
  expect(bad[0].thumbRatio).toBeLessThan(contrast.NON_TEXT_FLOOR);
  demonstrates("contrast-in-both-themes");
});

test("a link carrying nothing but its colour is reported", async ({ page }) => {
  await render(
    page,
    `<!doctype html><html><body style="background:#ffffff;margin:0;padding:40px">
       <a href="/one" style="color:#0a5387;text-decoration:none;font-size:14px">what these figures mean</a>
     </body></html>`
  );
  const links = await contrast.linkMarks(page);
  const bare = contrast.unmarkedLinks(links);
  expect(
    bare.length,
    `a link with no underline measured ${JSON.stringify(links)}`
  ).toBe(1);
  expect(bare[0].coverage).toBeLessThan(0.5);
  demonstrates("contrast-in-both-themes");
});

test("a control whose focus indicator is suppressed is named and reported", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><head><style>
       body { background: #ffffff; margin: 0; padding: 40px; }
       button { width: 120px; height: 44px; }
       button:focus { outline: none; }
     </style></head><body>
       <button aria-label="no ring">press</button>
     </body></html>`
  );
  const rows = await contrast.focusIndicators(page);
  const bad = contrast.unfocusable(rows);
  expect(bad.length, JSON.stringify(rows, null, 2)).toBe(1);
  expect(bad[0].label).toBe("no ring");
  expect(bad[0].pixelsChanged).toBe(0);
  demonstrates("focus-visible");
});

test("a theme that ignores the operating-system preference is reported", async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: "light" });
  await render(
    page,
    `<!doctype html><html><body style="background:#101418;margin:0;height:200px"></body></html>`
  );
  const body = await engine.surfaceLuminance(page, "body");
  expect(
    body.luminance,
    `a light preference was answered with ${body.colour}`
  ).toBeLessThan(0.6);
  demonstrates("themes-follow-preference");
});

// --- the tokens, and the ratios recorded beside them -------------------------

/// A page painted from a token table of its own, so a demonstration can change
/// a token's value the way a person would and leave the annotation behind.
function tokenPage(roles, body) {
  const declarations = Object.entries(roles)
    .map(([name, value]) => `${name}: ${value};`)
    .join(" ");
  return `<!doctype html><html><head><style>
     :root { ${declarations} }
     body { background: var(--panel); margin: 0; padding: 40px; }
     p { color: var(--fg); font-size: 16px; margin: 0; }
   </style></head><body>${body}</body></html>`;
}

test("a token whose value changed and whose annotation did not is reported, with both numbers", async ({
  page,
}) => {
  // The failure the criterion names by name. The record says --fg on --panel is
  // 18.20, which it was; the token now resolves to a grey four times paler, and
  // nobody went back to the comment. The measurement is the same
  // `reconcile.reconcile` the real claim is graded with, over the same
  // `contrast.textContrast` rows.
  await render(
    page,
    tokenPage(
      { "--panel": "#ffffff", "--fg": "#767f88" },
      "<p>a paragraph whose ink was changed and whose annotation was not</p>"
    )
  );
  const ledger = [{ ink: "--fg", on: "--panel", ratio: 18.2, floor: 4.5, theme: "light" }];
  const resolved = await reconcile.resolvedTokens(page, ["--fg", "--panel"]);
  expect(resolved.get("--fg")).toBe("#767f88");
  const rows = reconcile.rowsFrom(await contrast.textContrast(page), []);
  const report = reconcile.reconcile({ ledger, resolved, rows, theme: "light" });
  const stale = report.problems.filter((problem) => problem.why === "stale-annotation");
  expect(
    stale.length,
    `a recorded ratio four times the measured one was read as current: ${JSON.stringify(report)}`
  ).toBe(1);
  expect(stale[0].message).toContain("18.20");
  expect(stale[0].message).toContain("4.07");
  demonstrates("tokens-reconciled");
});

test("a measured row painted in a colour no token declares is reported as mapping to nothing", async ({
  page,
}) => {
  // The other half of a total reconciliation, and the one that stops it being
  // satisfied by mapping nothing: a run painted in a literal, which is exactly
  // what the source check refuses and exactly what this would have to notice if
  // one ever reached the page.
  await render(
    page,
    tokenPage(
      { "--panel": "#ffffff", "--fg": "#10161c" },
      '<p style="color:#8a3200">a run painted in a colour no role declares</p>'
    )
  );
  const ledger = [{ ink: "--fg", on: "--panel", ratio: 18.2, floor: 4.5, theme: "light" }];
  const resolved = await reconcile.resolvedTokens(page, ["--fg", "--panel"]);
  const rows = reconcile.rowsFrom(await contrast.textContrast(page), []);
  const report = reconcile.reconcile({ ledger, resolved, rows, theme: "light" });
  const unmapped = report.problems.filter((problem) => problem.why === "unmapped-row");
  expect(
    unmapped.length,
    `a row painted in a colour no role declares was read as reconciled: ${JSON.stringify(report)}`
  ).toBe(1);
  expect(unmapped[0].message).toContain("#8a3200");
  expect(report.mapped).toBe(0);
  demonstrates("tokens-reconciled");
});

// --- the two faces -----------------------------------------------------------

test("a figure painted in the system UI face is reported, with the widths that show it", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><head><style>
       body { margin: 0; padding: 40px;
              font: 16px -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; }
     </style></head><body>
       <span data-volume="kitchen">38%</span>
     </body></html>`
  );
  const rows = await typeface.faces(page, "[data-volume]");
  expect(rows.length).toBe(1);
  const offenders = typeface.notFixedAdvance(rows);
  expect(
    offenders.length,
    `a figure in a proportional face was read as fixed-advance: ${JSON.stringify(rows)}`
  ).toBe(1);
  expect(offenders[0].narrowWide[0]).toBeLessThan(offenders[0].narrowWide[1]);
  demonstrates("figures-fixed-advance");
});

test("a heading painted in the figure face is reported", async ({ page }) => {
  // The rule read the other way. A page that answered "everything is
  // monospace" would satisfy the figures half and lose the sentences, and this
  // is the measurement that says so.
  await render(
    page,
    `<!doctype html><html><head><style>
       body { margin: 0; padding: 40px; font: 16px ui-monospace, monospace; }
     </style></head><body>
       <h2 data-zone-name="kitchen">The Kitchen</h2>
     </body></html>`
  );
  const rows = await typeface.faces(page, "h2");
  expect(rows.length).toBe(1);
  const offenders = typeface.notProportional(rows);
  expect(
    offenders.length,
    `a heading in a fixed-advance face was read as prose: ${JSON.stringify(rows)}`
  ).toBe(1);
  expect(offenders[0].narrowWide[0]).toBe(offenders[0].narrowWide[1]);
  demonstrates("figures-fixed-advance");
});

test("an empty notice whose command is not in the figure face is reported", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><head><style>
       body { margin: 0; padding: 40px;
              font: 16px -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; }
       code { display: block; font-family: inherit; }
     </style></head><body>
       <section data-empty>
         <h2>No zones yet</h2>
         <p>Start the server with one --zone for each room:</p>
         <code>chorus-server --control-listen 127.0.0.1:4020 --zone kitchen</code>
       </section>
     </body></html>`
  );
  const rows = await typeface.faces(page, "[data-empty] code");
  expect(rows.length).toBe(1);
  expect(
    typeface.notFixedAdvance(rows).length,
    `a command line in a proportional face was read as fixed-advance: ${JSON.stringify(rows)}`
  ).toBe(1);
  demonstrates("empty-notice-type-rule");
});

// --- the accessibility tree -------------------------------------------------

test("a control the engine computes no name for is reported", async ({ page }) => {
  await render(
    page,
    `<!doctype html><html><body>
       <button><span aria-hidden="true"></span></button>
       <button>named</button>
     </body></html>`
  );
  const nodes = await a11y.tree(page);
  const unnamed = a11y.unnamed(nodes);
  expect(
    unnamed.length,
    `the tree computed ${JSON.stringify(a11y.controls(nodes))}`
  ).toBe(1);
  expect(unnamed[0].role).toBe("button");
  demonstrates("accessible-names");
});

test("a mute shown as a word alone exposes no pressed state", async ({ page }) => {
  await render(
    page,
    `<!doctype html><html><body>
       <button aria-label="Mute The Kitchen">Muted</button>
     </body></html>`
  );
  const nodes = await a11y.tree(page);
  expect(
    a11y.pressed(nodes),
    "a button with no aria-pressed was read as though it carried a state"
  ).toEqual([]);
  demonstrates("accessible-names");
});

test("a control whose name does not say which zone it belongs to is reported", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <button aria-label="Mute">m</button>
     </body></html>`
  );
  const nodes = await a11y.tree(page);
  expect(a11y.withoutTheirZone(nodes, ["The Kitchen"]).length).toBe(1);
  demonstrates("accessible-names");
});

// --- colour is not the only thing saying it ---------------------------------

test("two states whose only difference is their colour are reported", async ({
  page,
}) => {
  await page.emulateMedia({ forcedColors: "active" });
  await render(
    page,
    `<!doctype html><html><body>
       <p data-freshness="kitchen" style="background:#00aa00">up to date</p>
     </body></html>`
  );
  const first = await reads.stateMarks(page);
  await page.evaluate(() => {
    document.querySelector("[data-freshness]").style.background = "#aa0000";
  });
  const second = await reads.stateMarks(page);
  const problems = reads.colourOnly(first, second, ["freshness:kitchen"]);
  expect(
    problems.length,
    "a state carried only by its background colour was read as distinguishable"
  ).toBe(1);
  expect(problems[0].why).toContain("up to date");
  demonstrates("not-colour-alone");
});

// --- the figures ------------------------------------------------------------

test("an aggregate with no set beside it is reported as a bare count", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <section data-zone="kitchen"><p data-endpoints="kitchen">2 endpoints playing</p></section>
     </body></html>`
  );
  const zones = await reads.renderedZones(page);
  const figure = zones[0].endpoints;
  expect(figure).toBe("2 endpoints playing");
  expect(
    reads.isBareCount(figure),
    "a bare count was read as though it said what it counted"
  ).toBe(true);
  expect(reads.statesItsSet(figure)).toBe(false);
  demonstrates("aggregate-states-its-set");
});

test("a figure the state did not carry, rendered as though it had been measured, is reported", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <span data-volume="a">0%</span>
       <span data-volume="b">NaN%</span>
       <span data-volume="c">-</span>
       <span data-volume="d">undefined</span>
       <span data-volume="e">Unavailable</span>
     </body></html>`
  );
  const marks = await reads.stateMarks(page);
  const figures = {};
  for (const [id, mark] of Object.entries(marks)) {
    if (id.startsWith("volume:")) {
      figures[id] = mark.text;
    }
  }
  const dishonest = reads.dishonestFigures(figures);
  expect(dishonest.map((d) => d.what).sort()).toEqual([
    "volume:a",
    "volume:b",
    "volume:c",
    "volume:d",
  ]);
  demonstrates("unreadable-figure");
});

test("figures that keep reading as current after the feed has dropped are reported", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <p data-connection>Connection lost</p>
       <span data-freshness="kitchen">live</span>
       <span data-freshness="footer">last known</span>
     </body></html>`
  );
  const marks = await reads.stateMarks(page);
  const frozen = reads.stillReadingAsCurrent(marks);
  expect(
    frozen.length,
    "a figure still claiming to be current was read as though it had been marked"
  ).toBe(1);
  expect(frozen[0].mark).toBe("freshness:kitchen");
  demonstrates("stale-not-current");
});

test("a page that watches only its connection cannot see a paused feed, and is reported", async ({
  page,
}) => {
  // Not an invented failure. This is the mechanism the control page decided
  // freshness with before this work - `EventSource.onerror` plus a watchdog on
  // `readyState`, and nothing else - served by the fixture at
  // /demo/stale-blind so that it can subscribe to a real event stream. The
  // server it is subscribed to is then PAUSED: nothing is severed, the
  // connection stays established, `readyState` stays OPEN, no error fires, and
  // so that mechanism goes on reporting a page whose figures have not moved in
  // twelve seconds as live. The measurement is the same
  // `reads.stillReadingAsCurrent` the paused-feed claim is graded with.
  await resetFixture();
  await page.goto(`${FIXTURE}/demo/stale-blind`);
  await expect(page.locator("[data-connection]")).toHaveText("Live");

  await fixtureDo("pause");
  await page.waitForTimeout(12_000);

  const marks = await reads.stateMarks(page);
  const frozen = reads.stillReadingAsCurrent(marks);
  expect(
    frozen.length,
    `a feed that had delivered nothing for twelve seconds was read as current: ${JSON.stringify(marks)}`
  ).toBeGreaterThanOrEqual(2);
  expect(marks["connection"].text).toBe("Live");
  expect(marks["volume:kitchen"].text).toBe("38%");

  await resetFixture();
  demonstrates("paused-feed-visible");
});

// --- the three states -------------------------------------------------------

test("a page whose script never resolves leaves the loading state visible", async ({
  page,
}) => {
  // The shape the criterion forbids: a spinner that never resolves. The real
  // page's assertion is that `[data-loading]` is GONE, and this is that
  // assertion shown failing on a page where it never goes.
  await render(page, `<!doctype html><html><body><p data-loading>Loading</p></body></html>`);
  const counts = await reads.viewStates(page);
  expect(reads.statesShowing(counts)).toEqual(["loading"]);
  await page.waitForTimeout(500);
  expect(reads.statesShowing(await reads.viewStates(page))).toEqual(["loading"]);
  demonstrates("three-states");
});

test("a first state request with no deadline on it leaves the loading notice up for good", async ({
  page,
}) => {
  // Not an invented failure. This is the bootstrap the control page shipped
  // before this work: one fetch("/api/state") with no deadline on it, with
  // everything else the page does started from that fetch's .finally(). It is
  // served by the fixture at /demo/loading-blind so that the request goes to a
  // server that can be PAUSED - accepting the connection and answering nothing,
  // which is what a stopped chorus-server looks like from a browser and what
  // the fixture's own comment calls "the state a SIGSTOPped server is in".
  //
  // The measurement is the same `reads.statesShowing` the three-states and
  // loading-never-unresolved claims are graded with, and the wait is longer
  // than the fifteen seconds the real page is held to, so a green real page and
  // a red one here are the same measurement at the same moment.
  await fetch(`${FIXTURE}/fixture/scenario`, {
    method: "POST",
    body: JSON.stringify({ paused: true }),
  });
  await page.goto(`${FIXTURE}/demo/loading-blind`, { waitUntil: "commit" });
  await expect(page.locator("[data-loading]")).toBeVisible();

  // No interaction of any kind from here on. Just time.
  await page.waitForTimeout(20_000);

  const counts = await reads.viewStates(page);
  expect(
    reads.statesShowing(counts),
    `a loading state with no deadline under it was read as resolved: ${JSON.stringify(counts)}`
  ).toEqual(["loading"]);
  expect(await page.locator("[data-connection]").innerText()).toBe("Connecting");

  await resetFixture();
  demonstrates("loading-never-unresolved");
});

test("a page showing two of the three states at once is reported showing two", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <p data-loading>Loading</p>
       <section data-empty><h2>No zones yet</h2></section>
     </body></html>`
  );
  expect(
    reads.statesShowing(await reads.viewStates(page)),
    "two states at once were read as one"
  ).toEqual(["loading", "empty"]);
  demonstrates("three-states");
});

// --- words and the document -------------------------------------------------

test("a paragraph of methodology left on the surface is reported by its word count", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <p data-long>This server has no zone configured, so there is nothing to name, group or
       turn down, and zones are configured when the server starts with one flag for each room.</p>
       <p data-short>No zones yet</p>
     </body></html>`
  );
  const runs = await reads.surfaceText(page);
  const verbose = reads.tooManyWords(runs, MOST_WORDS);
  expect(verbose.length).toBe(1);
  expect(verbose[0].what).toContain("data-long");
  expect(verbose[0].words).toBeGreaterThan(MOST_WORDS);
  demonstrates("short-labels-and-doc-link");
});

test("a region with no link, and one with two, are both reported", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <section data-region="none"><p>nothing to follow</p></section>
       <section data-region="two"><a href="/one">one</a><a href="/two">two</a></section>
       <section data-region="right"><a href="/one">one</a></section>
     </body></html>`
  );
  const bad = reads.regionsWithoutOneLink(await reads.regions(page));
  expect(bad.map((r) => r.region).sort()).toEqual(["none", "two"]);
  demonstrates("short-labels-and-doc-link");
});

test("a link to a document that is not there is reported by what came back", async ({
  page,
}) => {
  const followed = await reads.follow(page, `${FIXTURE}/docs/no-such-document.md`);
  expect(
    followed.ok,
    "a dead link was read as though it had answered with a document"
  ).toBe(false);
  expect(followed.status).toBe(404);
  demonstrates("short-labels-and-doc-link");
});

// --- the narrow viewport ----------------------------------------------------

test("content wider than the viewport that drags the body sideways is reported", async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 640 });
  await render(
    page,
    `<!doctype html><html><body style="margin:0">
       <div style="width:900px;height:100px;background:#cccccc">too wide</div>
       <button style="width:44px;height:44px;margin-left:800px">off</button>
     </body></html>`
  );
  const laid = await reads.reflow(page, MINIMUM);
  expect(
    laid.scrollWidth > laid.clientWidth,
    "a document that scrolls sideways was read as though it fitted"
  ).toBe(true);
  expect(laid.outside.length).toBeGreaterThanOrEqual(1);
  demonstrates("reflow-360");
});

test("a zone heading with no break opportunity drags the card's controls off the viewport", async ({
  page,
}) => {
  // Not an invented failure either: this is the zone card as chorus.css built
  // it before this work, with the wrapping rule left off the heading, and the
  // name is one docs/control-plane.md admits - sixty-four characters, no space
  // and no hyphen, which is what the page's own rename box produces. The
  // heading's max-content width sets the card's grid column, the column drags
  // the body sideways, and the rename box lands outside the viewport where a
  // finger cannot reach it. The measurement is the same `reads.reflow` the
  // 360-pixel claim is graded with.
  const name = "MasterBedroomEnsuiteSpeakersAndTheHallway".padEnd(64, "x");
  await page.setViewportSize({ width: 360, height: 640 });
  await render(
    page,
    `<!doctype html><html><head><style>
       * { box-sizing: border-box; }
       body { margin: 0; font: 16px/1.5 system-ui, sans-serif; }
       main { display: grid; gap: 1rem; padding: 1rem;
              grid-template-columns: repeat(auto-fill, minmax(min(320px, 100%), 1fr)); }
       .zone { display: grid; gap: 0.75rem; justify-items: start; min-width: 0;
               border: 1px solid #5f6c79; border-radius: 10px; padding: 1rem; }
       .zone h2 { margin: 0; font-size: 1.0625rem; }
       .row { display: flex; align-items: center; gap: 0.75rem; width: 100%; min-width: 0; }
       input[type="text"] { flex: 1 1 4rem; min-width: 4rem; height: 44px; font: inherit; }
     </style></head><body>
       <main><section class="zone">
         <h2>${name}</h2>
         <div class="row"><label for="n">Name</label>
           <input id="n" type="text" data-rename="kitchen" value="${name}"></div>
       </section></main>
     </body></html>`
  );
  const laid = await reads.reflow(page, MINIMUM);
  expect(
    laid.scrollWidth > laid.clientWidth,
    `a heading that cannot break was read as though it fitted: ${JSON.stringify(laid)}`
  ).toBe(true);
  expect(
    laid.outside.map((c) => c.what),
    "a control painted past the right-hand edge was read as inside the viewport"
  ).toContain("input[data-rename]");
  demonstrates("reflow-360");
});

test("wide content that does not scroll inside its own container is reported", async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 640 });
  await render(
    page,
    `<!doctype html><html><body style="margin:0">
       <code style="display:block;overflow-x:clip;white-space:pre;width:320px">${"chorus-server --control-listen 127.0.0.1:4020 --zone kitchen --zone study"}</code>
     </body></html>`
  );
  const code = await reads.scrollsInsideItself(page, "code");
  expect(code.found).toBe(true);
  expect(code.wider).toBe(true);
  expect(
    code.scrolls,
    "content that cannot be scrolled to was read as though it could be reached"
  ).toBe(false);
  demonstrates("reflow-360");
});

// --- the keyboard -----------------------------------------------------------

test("a control the keyboard cannot reach is reported", async ({ page }) => {
  await render(
    page,
    `<!doctype html><html><body>
       <button>first</button>
       <button tabindex="-1" aria-label="unreachable">second</button>
       <button>third</button>
     </body></html>`
  );
  const order = await reads.tabOrder(page);
  const missed = await reads.unreachableByKeyboard(page, order);
  expect(
    missed.length,
    `Tab reached ${JSON.stringify(order)}`
  ).toBeGreaterThanOrEqual(1);
  demonstrates("keyboard-operation");
});

test("a tab order that does not follow the reading order is reported", async ({
  page,
}) => {
  await render(
    page,
    `<!doctype html><html><body>
       <button tabindex="3">first in the page</button>
       <button tabindex="2">second in the page</button>
       <button tabindex="1">third in the page</button>
     </body></html>`
  );
  const order = await reads.tabOrder(page);
  expect(
    reads.outOfReadingOrder(order).length,
    `Tab visited ${JSON.stringify(order.map((o) => o.index))}`
  ).toBeGreaterThanOrEqual(1);
  demonstrates("keyboard-operation");
});

// --- the refusal ------------------------------------------------------------

test("a refusal that was swallowed is reported as swallowed", async ({ page }) => {
  await render(
    page,
    `<!doctype html><html><body>
       <section data-zone="kitchen"><p data-refusal="kitchen"></p></section>
     </body></html>`
  );
  const refusals = await reads.refusalsShown(page);
  const verdict = reads.refusalSwallowed(refusals, "kitchen");
  expect(
    verdict.swallowed,
    "an empty refusal region was read as though a refusal had been shown"
  ).toBe(true);
  expect(reads.refusalSwallowed(refusals, "study").swallowed).toBe(true);
  demonstrates("refusal-shown");
});

// --- the policy -------------------------------------------------------------

test("a page served with no policy at all is reported as having none", async ({
  page,
}) => {
  const watch = await engine.watchPolicy(page);
  await page.goto(`${FIXTURE}/demo/no-csp`);
  expect(
    watch.policyFor("/demo/no-csp"),
    "a response with no Content-Security-Policy was read as though it carried one"
  ).toBeFalsy();
  demonstrates("csp-clean");
});

test("a policy that silences the page it was sent with is reported as a violation", async ({
  page,
}) => {
  const watch = await engine.watchPolicy(page);
  await page.goto(`${FIXTURE}/demo/blocked-style`);
  await page.waitForTimeout(300);
  const violations = await watch.violations();
  expect(
    violations.length,
    "a policy that blocked the page's own stylesheet was read as clean"
  ).toBeGreaterThanOrEqual(1);
  expect(violations[0].blocked).toContain("demo.css");
  // A policy is present and the page is broken by it: the header alone is not
  // the claim, which is why the claim also asserts the page still works.
  expect(watch.policyFor("/demo/blocked-style")).toBeTruthy();
  demonstrates("csp-clean");
});
