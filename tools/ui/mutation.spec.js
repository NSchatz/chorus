// The demonstrations: a check that cannot fail is not evidence.
//
// Each test below takes the SAME measuring code `ui.spec.js` grades the real
// page with, points it at a page that breaks the property on purpose, and
// requires it to report the break. They are committed rather than run once on a
// branch, so a later reader can re-run them instead of being told about them.
//
// Nothing here touches the served page. These render markup this file holds, in
// the same engine, through the same functions.

const { test, expect } = require("@playwright/test");
const { measureControls, tooSmall } = require("./measure");

const MINIMUM = 24;

/// Render a page out of a string, with no server involved.
async function render(page, html) {
  await page.setContent(html, { waitUntil: "load" });
}

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
});

test("a page whose script never resolves leaves the loading state visible", async ({
  page,
}) => {
  // The shape AC-10 forbids: a spinner that never resolves. The real page's
  // empty-state assertion is that `[data-loading]` is GONE, and this is that
  // assertion shown failing on a page where it never goes.
  await render(
    page,
    `<!doctype html><html><body><p data-loading>Loading</p></body></html>`
  );
  await expect(page.locator("[data-loading]")).toHaveCount(1);
  await expect(page.locator("[data-empty]")).toHaveCount(0);
});
