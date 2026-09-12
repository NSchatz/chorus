// The control page, graded by RENDERING it in a real browser engine.
//
// Every assertion below reads something the engine produced: laid-out text
// through `innerText`, a box through `getBoundingClientRect`, a colour out of a
// screenshot's pixels, the accessibility tree Chromium computed, or the
// browser's own Content-Security-Policy violation reports. Nothing reads the
// served HTML, the served JS or the served CSS, and there is no assertion here
// that a text search of those files could satisfy.
//
// That is not a stylistic preference. A text grader cannot decide what a rule
// applies to, what wins the cascade, or what is SHOWN rather than merely built,
// and hardening one only closes the hole it was shown.
// S0035-holdfast-dashboard-ui is why that sentence is worth writing down.
//
// Three servers stand behind these tests. Two are real chorus-server processes,
// one with zones and one with none. The third is tools/ui/fixture-server.js,
// which proxies the real page, its stylesheet, its script and its document
// through byte for byte and answers only the state request and the event stream
// itself: the four states a correct server will never produce (a figure that
// cannot be read, a feed that dies, a feed that comes back, a state request that
// has not answered yet) are reached by doctoring the state, never by doctoring
// the page.

const { test, expect } = require("@playwright/test");
const { measureControls, tooSmall } = require("./measure");
const contrast = require("./contrast");
const a11y = require("./a11y");
const reads = require("./reads");
const engine = require("./engine");
const { proves } = require("./claims");

/// The smallest border box the target-size criterion allows, in CSS pixels.
const MINIMUM = 24;
/// The most words a label or a state word on the surface may be. "A few words"
/// is the clause; twelve is this page's reading of it, and every label it ships
/// is well inside it.
const MOST_WORDS = 12;
/// A zone name at the control catalog's maximum with no break opportunity in
/// it: sixty-four characters, no space and no hyphen. docs/control-plane.md
/// admits it and the page's own rename box produces it, so it is the widest
/// heading the 360-pixel criterion has to hold for.
const UNBROKEN_NAME = "MasterBedroomEnsuiteSpeakersAndTheHallway".padEnd(64, "x");

const BASE = process.env.CHORUS_UI_BASE;
const EMPTY = process.env.CHORUS_UI_EMPTY_BASE;
const FIXTURE = process.env.CHORUS_UI_FIXTURE;

test.beforeAll(() => {
  if (!BASE || !EMPTY || !FIXTURE) {
    throw new Error(
      "CHORUS_UI_BASE, CHORUS_UI_EMPTY_BASE and CHORUS_UI_FIXTURE name the three " +
        "servers this check renders. tools/ui-render-run.sh starts them; running " +
        "this by hand needs all three."
    );
  }
});

/// Issue one control message as A SECOND SUBSCRIBER: a separate connection to
/// the same control channel, which the page under test knows nothing about.
async function secondSubscriber(base, body) {
  const response = await fetch(`${base}/api/command`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
  });
  const text = await response.text();
  if (!response.ok) {
    throw new Error(`the second subscriber's command was refused: ${text}`);
  }
  return text;
}

/// Put the real server into the state most of these tests start from: two named
/// zones, a known volume, one muted, and a kitchen whose persisted endpoint list
/// is larger than the set attached right now. The last one is a real state and
/// not a doctored one: an endpoint that attached and then left stays in
/// `endpoints` and leaves `present`, which is exactly the gap the endpoint
/// figure has to be honest about.
async function knownState(base) {
  await secondSubscriber(base, '{"v":1,"t":"name","zone":"kitchen","name":"The Kitchen"}');
  await secondSubscriber(base, '{"v":1,"t":"name","zone":"study","name":"The Study"}');
  await secondSubscriber(base, '{"v":1,"t":"volume","zone":"kitchen","volume":0.375}');
  await secondSubscriber(base, '{"v":1,"t":"volume","zone":"study","volume":1.000}');
  await secondSubscriber(base, '{"v":1,"t":"mute","zone":"kitchen","muted":false}');
  await secondSubscriber(base, '{"v":1,"t":"mute","zone":"study","muted":true}');
  await secondSubscriber(base, '{"v":1,"t":"ungroup","zone":"kitchen"}');
  await secondSubscriber(base, '{"v":1,"t":"ungroup","zone":"study"}');
  await secondSubscriber(
    base,
    '{"v":1,"t":"attach","zone":"kitchen","endpoint":"endpoint-a"}'
  );
  await secondSubscriber(
    base,
    '{"v":1,"t":"attach","zone":"kitchen","endpoint":"endpoint-b"}'
  );
  await secondSubscriber(
    base,
    '{"v":1,"t":"attach","zone":"study","endpoint":"endpoint-c"}'
  );
  await fetch(`${base}/api/leaving`, { method: "POST", body: "endpoint-b" });
}

async function scenario(wanted) {
  const response = await fetch(`${FIXTURE}/fixture/scenario`, {
    method: "POST",
    body: JSON.stringify(wanted || {}),
  });
  if (!response.ok) {
    throw new Error(`the fixture refused a scenario: ${await response.text()}`);
  }
}

async function fixtureDo(what) {
  const response = await fetch(`${FIXTURE}/fixture/${what}`, { method: "POST" });
  if (!response.ok) {
    throw new Error(`the fixture refused ${what}: ${await response.text()}`);
  }
}

/// How many event streams the fixture is still holding open. A severed feed has
/// none; a PAUSED one still has the connection it always had, which is what
/// makes the two different states rather than two names for one.
async function streamsStillOpen() {
  const response = await fetch(`${FIXTURE}/fixture/streams`);
  return (await response.json()).open;
}

/// The two-zone state the fixture serves by default, so a test can doctor one
/// field of it and leave everything else alone.
function twoZones() {
  return {
    v: 1,
    t: "state",
    serial: 4,
    zones: [
      {
        id: "kitchen",
        name: "The Kitchen",
        group: "downstairs",
        volume: 0.375,
        muted: false,
        endpoints: ["endpoint-a", "endpoint-b"],
        present: ["endpoint-a"],
        audio: "127.0.0.1:4011",
      },
      {
        id: "study",
        name: "The Study",
        group: "study",
        volume: 0.9,
        muted: true,
        endpoints: ["endpoint-c"],
        present: ["endpoint-c"],
        audio: "127.0.0.1:4012",
      },
    ],
  };
}

// --- what the page shows ----------------------------------------------------

test("every configured zone is shown by its name, volume, mute state and endpoint figure", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText("The Kitchen");

  const zones = await reads.renderedZones(page);
  expect(
    reads.zoneMismatches(zones, {
      kitchen: {
        name: "The Kitchen",
        volume: "38%",
        mute: "Not muted",
        pressed: "false",
        endpoints: "1 of 2 endpoints attached, 1 away",
        freshness: "live",
      },
      study: {
        name: "The Study",
        volume: "100%",
        mute: "Muted",
        pressed: "true",
        endpoints: "1 of 1 endpoints attached",
        freshness: "live",
      },
    })
  ).toEqual([]);

  // The whole page reads as a page about zones, with nothing leaked into it.
  const shown = await page.locator("body").innerText();
  expect(shown).toContain("The Kitchen");
  expect(shown).toContain("The Study");
  expect(shown).not.toContain("undefined");
  expect(shown).not.toContain("[object Object]");
  await expect(page.locator("[data-serial]")).toContainText("catalog version 1");

  proves("zones-shown");
});

test("a change made by another subscriber appears without the page being reloaded", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-volume="kitchen"]')).toHaveText("38%");

  // Nothing touches the page from here on. `page.goto` is not called again and
  // no reload happens; the only thing that changes is the server's state,
  // changed by somebody else. The document is marked so that a reload would be
  // visible even though the navigation timing buffer would not show one.
  const marked = await reads.markDocument(page);

  await secondSubscriber(BASE, '{"v":1,"t":"volume","zone":"kitchen","volume":0.900}');
  await expect(page.locator('[data-volume="kitchen"]')).toHaveText("90%");

  await secondSubscriber(BASE, '{"v":1,"t":"mute","zone":"kitchen","muted":true}');
  await expect(page.locator('[data-mute-state="kitchen"]')).toHaveText("Muted");

  await secondSubscriber(BASE, '{"v":1,"t":"name","zone":"kitchen","name":"Kitchen Two"}');
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText("Kitchen Two");

  await secondSubscriber(BASE, '{"v":1,"t":"group","zone":"study","group":"downstairs"}');
  await expect(page.locator('[data-zone-meta="study"]')).toContainText("group downstairs");

  // The page never navigated: same document, same time origin, so what changed
  // was the rendered node and not the page under it.
  const survived = await reads.documentSurvived(page, marked);
  expect(survived.sameDocument, JSON.stringify(survived)).toBe(true);

  proves("live-update");
});

test("every interactive control is painted at least 24 by 24 CSS pixels", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  const controls = await measureControls(page);
  expect(
    controls.length,
    "the page rendered no interactive control at all, so this check would pass vacuously"
  ).toBeGreaterThanOrEqual(8);

  const offenders = tooSmall(controls, MINIMUM);
  expect(
    offenders,
    `controls painted smaller than ${MINIMUM} by ${MINIMUM}: ${JSON.stringify(offenders, null, 2)}`
  ).toEqual([]);

  // Every kind of control the page has is among what was measured, so a kind
  // that stopped being rendered could not quietly leave this check.
  const kinds = new Set(controls.map((c) => `${c.tag}${c.type ? ":" + c.type : ""}`));
  for (const wanted of ["button:button", "select", "input:range", "input:text", "a"]) {
    expect(
      kinds.has(wanted),
      `no ${wanted} was rendered; measured kinds were ${[...kinds].join(", ")}`
    ).toBe(true);
  }

  proves("target-size");
});

test("the page was painted by a real engine, and the pixels agree with what it computed", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  const evidence = await engine.paintEvidence(page, "[data-zone]");
  expect(
    evidence.colours,
    `the rendering has ${evidence.colours} distinct colours, which is what a page nobody drew looks like`
  ).toBeGreaterThanOrEqual(16);
  expect(
    evidence.agrees,
    `the zone card computes ${evidence.computed} and the framebuffer has ${evidence.painted}`
  ).toBe(true);

  proves("rendered-by-a-real-engine");
});

// --- accessibility ----------------------------------------------------------

for (const theme of ["light", "dark"]) {
  test(`every text run and every non-text mark clears its contrast floor in the ${theme} theme`, async ({
    page,
  }) => {
    await knownState(BASE);
    await page.emulateMedia({ colorScheme: theme });
    await page.goto(`${BASE}/`);
    await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

    // A refusal is text on this page too, and a page with no refused command on
    // it paints none: the refusal run has a zero box and drops out of what is
    // measured. So one is issued first, from the page, and is graded with
    // everything else rather than being the one run nothing ever looked at.
    await page.locator('[data-rename="kitchen"]').fill("x".repeat(100));
    await page.locator('[data-rename="kitchen"]').press("Enter");
    await expect(page.locator('[data-refusal="kitchen"]')).toHaveText("Refused: name");
    await page.evaluate(() => {
      if (document.activeElement && document.activeElement.blur) {
        document.activeElement.blur();
      }
    });

    const runs = await contrast.textContrast(page);
    expect(
      runs.length,
      "no text was measured, so this check would pass vacuously"
    ).toBeGreaterThanOrEqual(12);
    expect(
      runs.some((run) => run.what.includes("data-refusal")),
      `the refusal was not among the ${runs.length} runs measured: ${JSON.stringify(runs.map((r) => r.what))}`
    ).toBe(true);
    expect(
      contrast.tooPale(runs).map((r) => ({
        what: r.what,
        text: r.text,
        ink: r.ink,
        on: r.surface,
        ratio: Number(r.ratio.toFixed(2)),
        floor: r.floor,
      })),
      `text below its floor in the ${theme} theme`
    ).toEqual([]);

    const marks = await contrast.nonTextContrast(page);
    expect(marks.length).toBeGreaterThanOrEqual(8);
    expect(
      contrast.indistinct(marks).map((m) => ({
        label: m.label,
        kind: m.kind,
        ratio: Number(m.ratio.toFixed(2)),
        surround: m.surround,
        edge: m.edge,
        track: m.track,
        thumb: m.thumb,
      })),
      `control boundaries or state marks below 3:1 in the ${theme} theme`
    ).toEqual([]);

    // A link's mark is its underline, not a box. Measured from the pixels: the
    // longest unbroken run of the link's own colour, against the width of the
    // link.
    const links = await contrast.linkMarks(page);
    expect(links.length).toBeGreaterThanOrEqual(3);
    expect(
      contrast.unmarkedLinks(links).map((l) => ({
        label: l.label,
        ink: l.ink,
        coverage: Number(l.coverage.toFixed(2)),
      })),
      `links carrying no mark but their colour in the ${theme} theme`
    ).toEqual([]);

    // And the same three measurements against the surface a working server
    // never paints: a zone whose volume, mute and group could not be read. It
    // renders three words no other state carries and it renders two controls
    // this page DISABLES, which composite differently from the ones above. A
    // state the assertion never renders is a state nothing here ever measured a
    // colour in.
    const doctored = twoZones();
    delete doctored.zones[0].volume;
    delete doctored.zones[0].muted;
    delete doctored.zones[0].group;
    await scenario({ state: doctored });
    await page.goto(`${FIXTURE}/`);
    await expect(page.locator('[data-mute-state="kitchen"]')).toHaveText("Unavailable");
    await expect(page.locator('[data-mute="kitchen"]')).toBeDisabled();
    await expect(page.locator('[data-group-select="kitchen"]')).toBeDisabled();

    const unreadableRuns = await contrast.textContrast(page);
    expect(
      unreadableRuns.some((run) => run.text === "Unavailable"),
      `no "Unavailable" run was among the ${unreadableRuns.length} measured`
    ).toBe(true);
    expect(
      contrast.tooPale(unreadableRuns).map((r) => ({
        what: r.what,
        text: r.text,
        ink: r.ink,
        on: r.surface,
        ratio: Number(r.ratio.toFixed(2)),
        floor: r.floor,
      })),
      `text below its floor in the ${theme} theme with a zone's figures unreadable`
    ).toEqual([]);

    const unreadableMarks = await contrast.nonTextContrast(page);
    expect(unreadableMarks.length).toBeGreaterThanOrEqual(6);
    expect(
      contrast.indistinct(unreadableMarks).map((m) => ({
        label: m.label,
        kind: m.kind,
        ratio: Number(m.ratio.toFixed(2)),
        surround: m.surround,
        edge: m.edge,
      })),
      `control boundaries below 3:1 in the ${theme} theme with a zone's figures unreadable`
    ).toEqual([]);
    expect(
      contrast.unmarkedLinks(await contrast.linkMarks(page)).map((l) => l.label),
      `links carrying no mark but their colour in the ${theme} theme with a zone's figures unreadable`
    ).toEqual([]);

    proves("contrast-in-both-themes");
  });

  test(`the control with focus is painted differently and clears 3:1 in the ${theme} theme`, async ({
    page,
  }) => {
    await knownState(BASE);
    await page.emulateMedia({ colorScheme: theme });
    await page.goto(`${BASE}/`);
    await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

    const rows = await contrast.focusIndicators(page);
    expect(rows.length).toBeGreaterThanOrEqual(8);
    expect(
      contrast.unfocusable(rows).map((r) => ({
        label: r.label,
        pixelsChanged: r.pixelsChanged,
        ratio: Number(r.ratio.toFixed(2)),
        drawn: r.drawn,
        over: r.over,
      })),
      `controls whose focus indicator is suppressed or below 3:1 in the ${theme} theme`
    ).toEqual([]);

    // The same measurement on the state where two of this page's controls are
    // disabled. What is graded is the controls that CAN take focus: a disabled
    // one is not "a control [that] has keyboard focus", `focus()` on it changes
    // no pixel, and reporting that as a suppressed indicator would be a false
    // alarm standing where a real one has to be visible.
    const doctored = twoZones();
    delete doctored.zones[0].volume;
    delete doctored.zones[0].muted;
    delete doctored.zones[0].group;
    await scenario({ state: doctored });
    await page.goto(`${FIXTURE}/`);
    await expect(page.locator('[data-mute="kitchen"]')).toBeDisabled();

    const unreadableRows = await contrast.focusIndicators(page);
    expect(unreadableRows.length).toBeGreaterThanOrEqual(6);
    expect(
      unreadableRows.map((r) => r.label),
      "a control that cannot take focus was graded on its focus indicator"
    ).not.toContain("Mute The Kitchen");
    expect(
      contrast.unfocusable(unreadableRows).map((r) => ({
        label: r.label,
        pixelsChanged: r.pixelsChanged,
        ratio: Number(r.ratio.toFixed(2)),
      })),
      `controls whose focus indicator is suppressed or below 3:1 in the ${theme} theme with a zone's figures unreadable`
    ).toEqual([]);

    proves("focus-visible");
  });

  test(`a ${theme} operating-system preference renders a ${theme} page and ${theme} panels`, async ({
    page,
  }) => {
    await knownState(BASE);
    await page.emulateMedia({ colorScheme: theme });
    await page.goto(`${BASE}/`);
    await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

    // Nothing on the page was found or clicked first: the preference was set on
    // the engine and the page was loaded.
    const body = await engine.surfaceLuminance(page, "body");
    const panel = await engine.surfaceLuminance(page, "[data-zone]");
    if (theme === "light") {
      expect(body.luminance, `the page painted ${body.colour}`).toBeGreaterThan(0.6);
      expect(panel.luminance, `a panel painted ${panel.colour}`).toBeGreaterThan(0.6);
    } else {
      expect(body.luminance, `the page painted ${body.colour}`).toBeLessThan(0.1);
      expect(panel.luminance, `a panel painted ${panel.colour}`).toBeLessThan(0.1);
    }

    proves("themes-follow-preference");
  });
}

test("every control is reachable by Tab in reading order and does what a pointer does", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  const order = await reads.tabOrder(page);
  expect(order.length, "Tab reached nothing at all").toBeGreaterThanOrEqual(8);
  expect(
    await reads.unreachableByKeyboard(page, order),
    "interactive controls the keyboard never reached"
  ).toEqual([]);
  expect(
    reads.outOfReadingOrder(order),
    "Tab visited controls in an order the page does not read in"
  ).toEqual([]);

  // Operated: each effect is asserted on what the page then SHOWS, and every
  // one is driven by a key rather than a click.
  await page.locator('[data-rename="kitchen"]').focus();
  await page.keyboard.press("Control+A");
  await page.keyboard.type("Kitchen By Key");
  await page.keyboard.press("Enter");
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText("Kitchen By Key");

  await page.locator('[data-volume-slider="kitchen"]').focus();
  await page.keyboard.press("Home");
  await expect(page.locator('[data-volume="kitchen"]')).toHaveText("0%");
  await page.keyboard.press("End");
  await expect(page.locator('[data-volume="kitchen"]')).toHaveText("100%");

  await page.locator('[data-mute="kitchen"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator('[data-mute-state="kitchen"]')).toHaveText("Muted");
  await expect(page.locator('[data-mute="kitchen"]')).toHaveAttribute("aria-pressed", "true");

  await page.locator('[data-group-select="study"]').focus();
  await page.keyboard.press("ArrowUp");
  await expect(page.locator('[data-zone-meta="study"]')).toContainText("group kitchen");

  proves("keyboard-operation");
});

test("every control has an accessible name saying what it does and which zone, and mute is a state", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  const nodes = await a11y.tree(page);
  const operable = a11y.controls(nodes);
  expect(
    operable.length,
    "the accessibility tree has no operable node, so this check would pass vacuously"
  ).toBeGreaterThanOrEqual(8);
  expect(
    a11y.unnamed(nodes).map((n) => ({ role: n.role, name: n.name })),
    "controls the engine computed no accessible name for"
  ).toEqual([]);

  const zones = await reads.renderedZones(page);
  const names = operable.map((n) => n.name);
  for (const zone of zones) {
    const mine = names.filter((name) => name.includes(zone.name));
    expect(
      mine.length,
      `only ${mine.length} accessible names mention "${zone.name}": ${JSON.stringify(names)}`
    ).toBeGreaterThanOrEqual(5);
    for (const action of ["Volume", "Mute", "Name", "Group", "Ungroup"]) {
      expect(
        mine.some((name) => name.startsWith(action)),
        `no control belonging to "${zone.name}" is named for ${action}: ${JSON.stringify(mine)}`
      ).toBe(true);
    }
  }

  // Mute is a pressed state and not only a word, so a thing that is not reading
  // the page can tell which it is.
  const pressed = a11y.pressed(nodes);
  expect(pressed.length).toBe(zones.length);
  for (const zone of zones) {
    const mine = pressed.find((p) => p.name.includes(zone.name));
    expect(mine, `no pressed state for "${zone.name}"`).toBeTruthy();
    expect(String(mine.pressed)).toBe(zone.mute === "Muted" ? "true" : "false");
  }

  proves("accessible-names");
});

test("every state the page distinguishes survives a rendering with the colour taken out", async ({
  page,
}) => {
  // Forced colours: the system palette replaces every colour the page chose, so
  // anything a person could only have told apart by hue is now indistinguishable
  // and the rendered words are all that is left.
  await page.emulateMedia({ forcedColors: "active" });
  await scenario({ state: twoZones() });
  await page.goto(`${FIXTURE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  // Muted from unmuted.
  const unmuted = await reads.stateMarks(page);
  const muted = twoZones();
  muted.serial = 5;
  muted.zones[0].muted = true;
  await fetch(`${FIXTURE}/fixture/push`, { method: "POST", body: JSON.stringify(muted) });
  await expect(page.locator('[data-mute-state="kitchen"]')).toHaveText("Muted");
  const afterMute = await reads.stateMarks(page);
  expect(
    reads.colourOnly(unmuted, afterMute, ["mute:kitchen"]),
    "muted and unmuted are told apart by colour alone"
  ).toEqual([]);

  // Live from stale.
  const live = await reads.stateMarks(page);
  await fixtureDo("sever");
  await expect(page.locator('[data-freshness="kitchen"]')).toHaveText("last known");
  const stale = await reads.stateMarks(page);
  expect(
    reads.colourOnly(live, stale, [
      "connection",
      "freshness:kitchen",
      "freshness:study",
      "freshness:footer",
    ]),
    "a live figure and a stale one are told apart by colour alone"
  ).toEqual([]);
  await fixtureDo("restore");

  // Available from unavailable.
  const available = await reads.stateMarks(page);
  const doctored = twoZones();
  doctored.serial = 6;
  delete doctored.zones[1].volume;
  delete doctored.zones[1].endpoints;
  await scenario({ state: doctored });
  await page.goto(`${FIXTURE}/`);
  await expect(page.locator('[data-volume="study"]')).toHaveText("Unavailable");
  const unavailable = await reads.stateMarks(page);
  expect(
    reads.colourOnly(available, unavailable, ["volume:study", "endpoints:study"]),
    "an available figure and an unavailable one are told apart by colour alone"
  ).toEqual([]);

  proves("not-colour-alone");
});

// --- honesty about what the page does not know ------------------------------

test("the endpoint figure says which rows it counted and how many it left out", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  const zones = await reads.renderedZones(page);
  const kitchen = zones.find((z) => z.id === "kitchen");
  expect(kitchen.endpoints).toBe("1 of 2 endpoints attached, 1 away");
  expect(
    reads.statesItsSet(kitchen.endpoints),
    `the figure "${kitchen.endpoints}" does not say which rows it counted`
  ).toBe(true);
  expect(
    reads.isBareCount(kitchen.endpoints),
    `the figure "${kitchen.endpoints}" is a bare count a reader would take for the whole set`
  ).toBe(false);
  expect(kitchen.endpoints).toContain("1 away");

  proves("aggregate-states-its-set");
});

test("a figure that cannot be read renders as unavailable while the rest of the view draws", async ({
  page,
}) => {
  const doctored = twoZones();
  delete doctored.zones[1].volume;
  delete doctored.zones[1].endpoints;
  await scenario({ state: doctored });
  await page.goto(`${FIXTURE}/`);
  await expect(page.locator('[data-zone-name="study"]')).toBeVisible();

  const zones = await reads.renderedZones(page);
  const study = zones.find((z) => z.id === "study");
  expect(study.volume).toBe("Unavailable");
  expect(study.endpoints).toBe("Endpoints unavailable");
  expect(
    reads.dishonestFigures({
      "volume:study": study.volume,
      "endpoints:study": study.endpoints,
    }),
    "a figure the state did not carry was rendered as though it had been measured"
  ).toEqual([]);

  // A slider has to be somewhere, and anywhere it could be is a value the page
  // would be making up, so there is none.
  await expect(page.locator('[data-volume-slider="study"]')).toHaveCount(0);

  // One unreadable figure costs nothing else: every other zone and every other
  // region still drew.
  expect(
    reads.zoneMismatches(zones, {
      kitchen: {
        name: "The Kitchen",
        volume: "38%",
        endpoints: "1 of 2 endpoints attached, 1 away",
      },
      study: { name: "The Study", mute: "Muted" },
    })
  ).toEqual([]);
  const regions = await reads.regions(page);
  const painted = regions.filter((r) => r.painted).map((r) => r.region).sort();
  expect(painted).toEqual(["footer", "header", "zone-kitchen", "zone-study"]);

  // A NAME that cannot be read is the same rule applied to the heading. The
  // card falls back to the identifier, because a card has to be identifiable,
  // and says the name was not the server's rather than passing the identifier
  // off as one.
  const nameless = twoZones();
  nameless.serial = 7;
  delete nameless.zones[1].name;
  await scenario({ state: nameless });
  await page.goto(`${FIXTURE}/`);
  await expect(page.locator('[data-zone-name="study"]')).toHaveText("study");
  await expect(page.locator('[data-zone-meta="study"]')).toContainText("name unavailable");
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText("The Kitchen");
  await expect(page.locator('[data-zone-meta="kitchen"]')).not.toContainText("name unavailable");

  // And a state whose zones ALL fail to identify is not "no zones configured".
  // Telling an operator to restart the server with --zone flags, when zones did
  // arrive and could not be read, is the loud claim being the wrong one.
  const unidentifiable = twoZones();
  unidentifiable.serial = 8;
  delete unidentifiable.zones[0].id;
  delete unidentifiable.zones[1].id;
  await scenario({ state: unidentifiable });
  await page.goto(`${FIXTURE}/`);
  await expect(page.locator("[data-error]")).toBeVisible();
  expect(reads.statesShowing(await reads.viewStates(page))).toEqual(["error"]);
  await expect(page.locator("[data-serial]")).toContainText("2 zones unreadable");

  proves("unreadable-figure");
});

test("a severed feed reads as lost within ten seconds, and a restored one reads as current again", async ({
  page,
}) => {
  await scenario({ state: twoZones() });
  await page.goto(`${FIXTURE}/`);
  await expect(page.locator('[data-freshness="kitchen"]')).toHaveText("live");
  await expect(page.locator("[data-connection]")).toHaveText("Live");
  const marked = await reads.markDocument(page);

  const severedAt = Date.now();
  await fixtureDo("sever");
  await expect
    .poll(async () => (await reads.stateMarks(page))["connection"].text, {
      timeout: 10_000,
      intervals: [100, 200, 200, 500],
    })
    .toBe("Connection lost");
  const took = Date.now() - severedAt;
  expect(took, `the page took ${took}ms to admit the feed had dropped`).toBeLessThan(10_000);

  const stale = await reads.stateMarks(page);
  expect(
    reads.stillReadingAsCurrent(stale),
    "figures still reading as current after the feed was severed"
  ).toEqual([]);
  expect(Object.keys(stale).filter((id) => id.startsWith("freshness:")).length)
    .toBeGreaterThanOrEqual(3);
  // The figures are still there. A dropped feed is not a reason to blank a page;
  // it is a reason to stop claiming the figures are current.
  const zones = await reads.renderedZones(page);
  expect(zones.find((z) => z.id === "kitchen").volume).toBe("38%");

  await fixtureDo("restore");
  await expect(page.locator("[data-connection]")).toHaveText("Live", { timeout: 15_000 });
  await expect(page.locator('[data-freshness="kitchen"]')).toHaveText("live");
  await expect(page.locator('[data-freshness="footer"]')).toHaveText("live");

  // And it came back without the page being reloaded.
  const survived = await reads.documentSurvived(page, marked);
  expect(survived.sameDocument, JSON.stringify(survived)).toBe(true);

  proves("stale-not-current");
});

test("a paused feed - nothing severed, the server no longer answering - stops reading as live and comes back", async ({
  page,
}) => {
  // The third limb of clause F6, and the one an EventSource cannot see. Nothing
  // is severed here: the fixture keeps every established connection up and
  // answers nothing, which is what a stopped server looks like from a browser.
  // `readyState` stays OPEN, no error fires, and a page that watched only its
  // connection would go on calling a minute-old figure live.
  await scenario({ state: twoZones() });
  await page.goto(`${FIXTURE}/`);
  await expect(page.locator('[data-freshness="kitchen"]')).toHaveText("live");
  await expect(page.locator("[data-connection]")).toHaveText("Live");
  const marked = await reads.markDocument(page);
  expect(
    await streamsStillOpen(),
    "the page never subscribed, so there would be nothing to pause"
  ).toBeGreaterThanOrEqual(1);

  const pausedAt = Date.now();
  await fixtureDo("pause");
  await expect
    .poll(async () => (await reads.stateMarks(page))["connection"].text, {
      timeout: 20_000,
      intervals: [250, 500, 500, 1000],
    })
    .toBe("Not answering");
  const took = Date.now() - pausedAt;
  expect(
    took,
    `the page took ${took}ms to admit the feed had stopped delivering`
  ).toBeLessThan(20_000);

  // It is the PAUSED state and not the severed one: the stream the page opened
  // is still established on the other end, so nothing dropped.
  expect(
    await streamsStillOpen(),
    "the stream was severed, so this graded the dropped case over again"
  ).toBeGreaterThanOrEqual(1);

  const stale = await reads.stateMarks(page);
  expect(
    reads.stillReadingAsCurrent(stale),
    "figures still reading as current while the server answered nothing"
  ).toEqual([]);
  expect(
    Object.keys(stale).filter((id) => id.startsWith("freshness:")).length
  ).toBeGreaterThanOrEqual(3);
  // The figures are still there, and still the last ones the server sent. A
  // paused feed is a reason to stop claiming they are current, not to blank
  // them.
  const zones = await reads.renderedZones(page);
  expect(zones.find((z) => z.id === "kitchen").volume).toBe("38%");

  await fixtureDo("resume");
  await expect(page.locator("[data-connection]")).toHaveText("Live", { timeout: 20_000 });
  await expect(page.locator('[data-freshness="kitchen"]')).toHaveText("live");
  await expect(page.locator('[data-freshness="footer"]')).toHaveText("live");

  // And it came back without the page being reloaded.
  const survived = await reads.documentSurvived(page, marked);
  expect(survived.sameDocument, JSON.stringify(survived)).toBe(true);

  proves("paused-feed-visible");
});

test("a refused command is shown where it was issued, and the control keeps the server's state", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText("The Kitchen");

  // A name longer than the catalog admits. Every other character is legal, so
  // the only fault is the one the server names.
  const tooLong = "x".repeat(100);
  await page.locator('[data-rename="kitchen"]').fill(tooLong);
  await page.locator('[data-rename="kitchen"]').press("Enter");

  await expect(page.locator('[data-refusal="kitchen"]')).toHaveText("Refused: name");
  const refusals = await reads.refusalsShown(page);
  expect(reads.refusalSwallowed(refusals, "kitchen").swallowed).toBe(false);

  // The control shows the state the server still holds, not the value it
  // refused, and the heading never moved.
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText("The Kitchen");
  await expect(page.locator('[data-rename="kitchen"]')).toHaveValue("The Kitchen");

  // Nothing was applied anywhere: a second subscriber reading the state sees
  // the name the server had before.
  const state = await (await fetch(`${BASE}/api/state`)).json();
  expect(state.zones.find((z) => z.id === "kitchen").name).toBe("The Kitchen");

  proves("refusal-shown");
});

// --- the three states -------------------------------------------------------

test("loading, empty and error are distinct, actionable, and never two at once", async ({
  page,
}) => {
  // Loading: the state request has not answered. Graded against a request that
  // NEVER answers - the server accepts the connection and returns nothing,
  // which is what a stopped or wedged chorus-server looks like from a browser -
  // and not against a delay chosen to land inside the observation window. The
  // limb the criterion states is unconditional, so the state it is graded
  // against is the one that breaks it.
  await scenario({ paused: true });
  await page.goto(`${FIXTURE}/`, { waitUntil: "commit" });
  await expect(page.locator("[data-loading]")).toBeVisible();
  let counts = await reads.viewStates(page);
  expect(reads.statesShowing(counts)).toEqual(["loading"]);
  expect((await page.locator("[data-loading]").innerText()).length).toBeGreaterThan(10);
  // A loading state is never left unresolved.
  await expect(page.locator("[data-loading]")).toHaveCount(0, { timeout: 15_000 });
  counts = await reads.viewStates(page);
  expect(reads.statesShowing(counts)).toEqual(["error"]);

  // And a request that is merely slow still draws its zones: the loading notice
  // resolves INTO the zone list rather than into an error, so the deadline
  // above did not buy the unconditional limb by calling every slow server a
  // stopped one.
  await scenario({ stateDelayMs: 4000, eventsMode: "silent" });
  await page.goto(`${FIXTURE}/`, { waitUntil: "commit" });
  await expect(page.locator("[data-loading]")).toBeVisible();
  await expect(page.locator("[data-loading]")).toHaveCount(0, { timeout: 15_000 });
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();
  counts = await reads.viewStates(page);
  expect(reads.statesShowing(counts)).toEqual([]);

  // Empty: a real server with no zone configured.
  await page.goto(`${EMPTY}/`);
  await expect(page.locator("[data-empty]")).toBeVisible();
  counts = await reads.viewStates(page);
  expect(reads.statesShowing(counts)).toEqual(["empty"]);
  expect(counts.bodyText).toBeGreaterThan(40);
  const empty = await page.locator("[data-empty]").innerText();
  expect(empty.toLowerCase()).toContain("no zones yet");
  // It says how to add one, in a form a person can act on.
  expect(empty).toContain("--zone");
  expect(empty).toContain("chorus-server");
  expect(empty.toLowerCase()).not.toContain("error");
  await expect(page.locator("[data-serial]")).toContainText("catalog version 1");

  // Error: the state cannot be read at all.
  await scenario({ stateStatus: 500, eventsMode: "severed" });
  await page.goto(`${FIXTURE}/`);
  await expect(page.locator("[data-error]")).toBeVisible();
  counts = await reads.viewStates(page);
  expect(reads.statesShowing(counts)).toEqual(["error"]);
  const error = await page.locator("[data-error]").innerText();
  expect(error.toLowerCase()).toContain("could not be read");
  expect(error.toLowerCase()).toContain("chorus-server");

  proves("three-states");
});

test("a state request that never answers is resolved into something a person can act on, and bounded", async ({
  page,
}) => {
  // The limb of the criterion that has no condition on it: "SHALL never leave a
  // loading state unresolved". A loading state is only ever left unresolved by
  // a request that does not come back, so both scenarios here are requests that
  // do not come back, and the page is watched with NO interaction of any kind.
  //
  // Both are states a stopped process produces. The fixture's `paused` is the
  // one its own comment calls "the state a SIGSTOPped server is in": every
  // established connection stays up and nothing is answered on any route the
  // page uses. `stateDelayMs` past the window is the same fault reached through
  // the knob the three-states assertion uses, so the two cannot both be
  // satisfied by a delay chosen to fit.
  const nonAnswers = [
    ["the server accepts the connection and answers nothing", { paused: true }],
    [
      "the state request is delayed past any window a person would wait",
      { stateDelayMs: 600_000, eventsMode: "silent" },
    ],
  ];

  for (const [what, wanted] of nonAnswers) {
    await scenario(wanted);
    const openedAt = Date.now();
    await page.goto(`${FIXTURE}/`, { waitUntil: "commit" });
    await expect(page.locator("[data-loading]")).toBeVisible();
    expect(reads.statesShowing(await reads.viewStates(page)), what).toEqual(["loading"]);

    await expect(
      page.locator("[data-loading]"),
      `${what}: the loading notice was still up`
    ).toHaveCount(0, { timeout: 20_000 });
    const took = Date.now() - openedAt;
    expect(
      took,
      `${what}: the page took ${took}ms to resolve its loading state`
    ).toBeLessThan(15_000);

    // What it resolved TO, because resolving a spinner into a blank body would
    // satisfy the words and none of the point. It is the error notice, alone,
    // and it says what to go and do.
    expect(reads.statesShowing(await reads.viewStates(page)), what).toEqual(["error"]);
    const error = await page.locator("[data-error]").innerText();
    expect(error.toLowerCase(), what).toContain("could not be read");
    expect(error.toLowerCase(), what).toContain("chorus-server");
    await expect(page.locator("[data-connection]"), what).toHaveText("Connection lost");
    expect((await reads.viewStates(page)).bodyText, what).toBeGreaterThan(40);
  }

  await scenario({});
  proves("loading-never-unresolved");
});

// --- words on the surface, paragraphs in the document ------------------------

test("labels stay to a few words, and each region links once to a document that resolves", async ({
  page,
}) => {
  await knownState(BASE);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  const runs = await reads.surfaceText(page);
  expect(runs.length).toBeGreaterThanOrEqual(12);
  expect(
    reads.tooManyWords(runs, MOST_WORDS),
    `text on the surface longer than ${MOST_WORDS} words; the paragraphs belong in the document`
  ).toEqual([]);

  const regions = await reads.regions(page);
  expect(regions.filter((r) => r.painted).length).toBeGreaterThanOrEqual(4);
  expect(
    reads.regionsWithoutOneLink(regions),
    "regions that do not link exactly once to the document that explains them"
  ).toEqual([]);

  // The empty state is a region too, and it carries the paragraphs that used to
  // be on the surface.
  await page.goto(`${EMPTY}/`);
  await expect(page.locator("[data-empty]")).toBeVisible();
  const emptyRuns = await reads.surfaceText(page);
  expect(reads.tooManyWords(emptyRuns, MOST_WORDS)).toEqual([]);
  expect(reads.regionsWithoutOneLink(await reads.regions(page))).toEqual([]);

  // Followed IN THE ENGINE: the link answers with the document and not a 404 or
  // a dead file reference.
  const href = (await reads.regions(page)).find((r) => r.region === "empty").links[0];
  const followed = await reads.follow(page, href);
  expect(
    followed.ok,
    `following ${href} answered ${followed.status} with ${followed.length} characters`
  ).toBe(true);
  const document = await page.locator("body").innerText();
  expect(document).toContain("endpoints");
  expect(document).toContain("last known");

  proves("short-labels-and-doc-link");
});

// --- the narrow viewport ----------------------------------------------------

test("at 360 by 640 the body does not scroll sideways and every control stays inside it", async ({
  page,
}) => {
  await knownState(BASE);
  await page.setViewportSize({ width: 360, height: 640 });
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  const laid = await reads.reflow(page, MINIMUM);
  expect(laid.viewport).toBe(360);
  expect(
    laid.scrollWidth,
    `the document scrolls to ${laid.scrollWidth} in a ${laid.clientWidth} viewport`
  ).toBeLessThanOrEqual(laid.clientWidth);
  expect(laid.bodyScrollWidth).toBeLessThanOrEqual(laid.bodyClientWidth);
  expect(laid.outside, "controls painted outside the viewport").toEqual([]);
  expect(
    laid.small,
    `controls under ${MINIMUM} by ${MINIMUM} at 360 pixels wide`
  ).toEqual([]);

  // The criterion is unconditional on the state, so it is graded against the
  // WIDEST state the control catalog admits and not only against a name chosen
  // to fit. docs/control-plane.md: a name is 1 to 64 characters and every
  // printable character is a name character, so a name with no space in it,
  // typed into this page's own rename box, is a state the server accepts. Sixty
  // four of them in one word is the worst case there is.
  await secondSubscriber(
    BASE,
    JSON.stringify({ v: 1, t: "name", zone: "kitchen", name: UNBROKEN_NAME })
  );
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText(UNBROKEN_NAME);
  expect(UNBROKEN_NAME.length).toBe(64);
  expect(UNBROKEN_NAME).not.toMatch(/[\s-]/);

  const widest = await reads.reflow(page, MINIMUM);
  expect(
    widest.scrollWidth,
    `a ${UNBROKEN_NAME.length}-character unbroken name scrolls the document to ` +
      `${widest.scrollWidth} in a ${widest.clientWidth} viewport`
  ).toBeLessThanOrEqual(widest.clientWidth);
  expect(widest.bodyScrollWidth).toBeLessThanOrEqual(widest.bodyClientWidth);
  expect(
    widest.outside,
    "controls painted outside the viewport by a name the catalog admits"
  ).toEqual([]);
  expect(
    widest.small,
    `controls under ${MINIMUM} by ${MINIMUM} beside an unbroken name`
  ).toEqual([]);
  // And the name is still all there: fitting it is a wrapping rule, not a
  // truncation that hides what a person typed.
  expect(await page.locator('[data-zone-name="kitchen"]').innerText()).toBe(UNBROKEN_NAME);

  // Content too wide to fit scrolls inside its own container rather than
  // dragging the body sideways. The empty state's command line is the widest
  // thing this page has.
  await page.goto(`${EMPTY}/`);
  await expect(page.locator("[data-empty]")).toBeVisible();
  const code = await reads.scrollsInsideItself(page, "code");
  expect(code.found).toBe(true);
  expect(code.wider, "nothing on the page was too wide, so this would pass vacuously").toBe(true);
  expect(code.scrolls, "the wide content does not scroll inside its own container").toBe(true);
  expect(code.pageScrollWidth).toBeLessThanOrEqual(code.pageClientWidth);

  proves("reflow-360");
});

// --- the policy -------------------------------------------------------------

test("the server sends a policy, the browser reports no violation, and everything still works", async ({
  page,
}) => {
  await knownState(BASE);
  const watch = await engine.watchPolicy(page);
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-zone-name="kitchen"]')).toBeVisible();

  // The page and every asset it loads were sent one.
  for (const asset of ["/", "/tokens.css", "/chorus.css", "/chorus.js"]) {
    const url = asset === "/" ? `${BASE}/` : `${BASE}${asset}`;
    const sent = watch.responses.find((r) => r.url === url);
    expect(sent, `nothing answered ${url}`).toBeTruthy();
    expect(sent.policy, `${url} carried no Content-Security-Policy`).toBeTruthy();
    expect(sent.policy).toContain("default-src 'none'");
  }

  // Everything still functions under it. The stylesheet applied, the script ran,
  // the state request answered and the event stream is delivering - the last of
  // which is proved by a change made by somebody else arriving.
  const panel = await engine.surfaceLuminance(page, "[data-zone]");
  expect(panel, "no zone card was painted, so the stylesheet or the script did not run").toBeTruthy();
  await expect(page.locator('[data-freshness="kitchen"]')).toHaveText("live");
  await secondSubscriber(BASE, '{"v":1,"t":"volume","zone":"kitchen","volume":0.500}');
  await expect(page.locator('[data-volume="kitchen"]')).toHaveText("50%");

  // And a command the PAGE issues, which is the other connection the policy has
  // to permit.
  await page.locator('[data-mute="kitchen"]').click();
  await expect(page.locator('[data-mute="kitchen"]')).toHaveAttribute("aria-pressed", "true");

  const violations = await watch.violations();
  expect(violations, "the browser reported policy violations").toEqual([]);
  expect(watch.consoleErrors, "the browser logged policy complaints").toEqual([]);

  proves("csp-clean");
});
