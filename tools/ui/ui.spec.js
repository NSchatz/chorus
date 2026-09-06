// AC-5, AC-6 and AC-10, graded by RENDERING the page in a real browser engine.
//
// AC-5  "WHEN the control UI is loaded in a browser THE SYSTEM SHALL show every
//        configured zone by its configured name with its current volume and
//        mute state, and SHALL show a change made by another subscriber without
//        the page being reloaded. Graded by RENDERING the page in a real
//        browser engine and reading the rendered text and the live-updated
//        node, never by reading the HTML, JS or CSS source: a second subscriber
//        issues the change and the assertion is on what the page then shows."
//
// AC-6  "WHEN the control UI is rendered THE SYSTEM SHALL paint every
//        interactive control at a border box of at least 24 by 24 CSS pixels,
//        measured from the rendered box in a real engine rather than from any
//        declared stylesheet value, so a rule that never wins the cascade
//        cannot pass this."
//
// AC-10 "WHEN no zone has been configured THE SYSTEM SHALL serve an empty zone
//        list and render a UI that says there are no zones yet and how to add
//        one, rather than an error page, a spinner that never resolves, or a
//        blank body."
//
// Every assertion below reads `innerText` or a `getBoundingClientRect()` off a
// live page. Nothing reads the served HTML, the served JS or the served CSS,
// and there is no assertion here that a text search of those files could
// satisfy. S0035-holdfast-dashboard-ui is why that sentence is worth writing
// down.

const { test, expect } = require("@playwright/test");
const { measureControls, tooSmall } = require("./measure");

/// The smallest border box AC-6 allows, in CSS pixels.
const MINIMUM = 24;

const BASE = process.env.CHORUS_UI_BASE;
const EMPTY = process.env.CHORUS_UI_EMPTY_BASE;

test.beforeAll(() => {
  if (!BASE || !EMPTY) {
    throw new Error(
      "CHORUS_UI_BASE and CHORUS_UI_EMPTY_BASE name the two servers this check " +
        "renders. tools/ui-render-run.sh starts them; running this by hand needs both."
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

test("every configured zone is shown by its name, volume and mute state", async ({
  page,
}) => {
  // Set the state from a second subscriber first, so what the page shows is
  // the server's state and not a default it could have been born with.
  await secondSubscriber(
    BASE,
    '{"v":1,"t":"name","zone":"kitchen","name":"The Kitchen"}'
  );
  await secondSubscriber(
    BASE,
    '{"v":1,"t":"volume","zone":"kitchen","volume":0.375}'
  );
  await secondSubscriber(BASE, '{"v":1,"t":"mute","zone":"study","muted":true}');

  await page.goto(`${BASE}/`);

  // Rendered text, not markup: innerText is what the engine laid out.
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText(
    "The Kitchen"
  );
  await expect(page.locator('[data-zone-name="study"]')).toHaveText("study");
  await expect(page.locator('[data-volume="kitchen"]')).toHaveText("38%");
  await expect(page.locator('[data-volume="study"]')).toHaveText("100%");
  await expect(page.locator('[data-mute="study"]')).toHaveText("Muted");
  await expect(page.locator('[data-mute="kitchen"]')).toHaveText("Not muted");

  // And the mute is shown as a state and not only as a word, so that something
  // other than a reader can tell.
  await expect(page.locator('[data-mute="study"]')).toHaveAttribute(
    "aria-pressed",
    "true"
  );

  // The whole page reads as a page about zones. This is the loosest assertion
  // here and it is deliberately about what a person sees.
  const shown = await page.locator("body").innerText();
  expect(shown).toContain("The Kitchen");
  expect(shown).toContain("study");
  expect(shown).not.toContain("undefined");
  expect(shown).not.toContain("[object Object]");
});

test("a change made by another subscriber appears without the page being reloaded", async ({
  page,
}) => {
  await secondSubscriber(
    BASE,
    '{"v":1,"t":"volume","zone":"kitchen","volume":0.250}'
  );
  await page.goto(`${BASE}/`);
  await expect(page.locator('[data-volume="kitchen"]')).toHaveText("25%");

  // Nothing touches the page from here on. `page.goto` is not called again and
  // no reload happens; the only thing that changes is the server's state,
  // changed by somebody else.
  const loadedAt = await page.evaluate(() => performance.timeOrigin);

  await secondSubscriber(
    BASE,
    '{"v":1,"t":"volume","zone":"kitchen","volume":0.900}'
  );
  await expect(page.locator('[data-volume="kitchen"]')).toHaveText("90%");

  await secondSubscriber(BASE, '{"v":1,"t":"mute","zone":"kitchen","muted":true}');
  await expect(page.locator('[data-mute="kitchen"]')).toHaveText("Muted");

  await secondSubscriber(
    BASE,
    '{"v":1,"t":"name","zone":"kitchen","name":"Kitchen Two"}'
  );
  await expect(page.locator('[data-zone-name="kitchen"]')).toHaveText(
    "Kitchen Two"
  );

  await secondSubscriber(
    BASE,
    '{"v":1,"t":"group","zone":"study","group":"downstairs"}'
  );
  await expect(page.locator('[data-zone-meta="study"]')).toContainText(
    "group downstairs"
  );

  // The page never navigated: same document, same time origin, so what changed
  // was the rendered node and not the page under it.
  expect(await page.evaluate(() => performance.timeOrigin)).toBe(loadedAt);
  expect(
    await page.evaluate(
      () => performance.getEntriesByType("navigation").length
    )
  ).toBe(1);
});

test("every interactive control is painted at least 24 by 24 CSS pixels", async ({
  page,
}) => {
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
    `controls painted smaller than ${MINIMUM} by ${MINIMUM}: ${JSON.stringify(
      offenders,
      null,
      2
    )}`
  ).toEqual([]);

  // Every kind of control the page has is among what was measured, so a kind
  // that stopped being rendered could not quietly leave this check.
  const kinds = new Set(controls.map((c) => `${c.tag}${c.type ? ":" + c.type : ""}`));
  for (const wanted of ["button:button", "select", "input:range", "input:text"]) {
    expect(
      kinds.has(wanted),
      `no ${wanted} was rendered; measured kinds were ${[...kinds].join(", ")}`
    ).toBe(true);
  }
});

test("a server with no zone renders an honest empty state", async ({ page }) => {
  await page.goto(`${EMPTY}/`);

  // Not a blank body.
  const shown = await page.locator("body").innerText();
  expect(shown.trim().length).toBeGreaterThan(40);

  // Not a spinner that never resolves: whatever the page shows while it is
  // loading is gone, and the empty state is what is there instead.
  await expect(page.locator("[data-empty]")).toBeVisible();
  await expect(page.locator("[data-loading]")).toHaveCount(0);
  await expect(page.locator("[data-error]")).toHaveCount(0);

  // It says there are no zones yet.
  const empty = await page.locator("[data-empty]").innerText();
  expect(empty.toLowerCase()).toContain("no zones yet");

  // And it says how to add one, in a form a person can act on: the flag, and a
  // whole command they could run.
  expect(empty).toContain("--zone");
  expect(empty).toContain("chorus-server");

  // The empty state is not an error page.
  expect(shown.toLowerCase()).not.toContain("error");
  expect(shown.toLowerCase()).not.toContain("failed");

  // A zone list of none is still a state message, and the page says which.
  await expect(page.locator("[data-serial]")).toContainText("catalog version 1");
});
