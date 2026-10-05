// The two layouts and the breakpoint between them, as pure logic, and the
// element that reflects the one its viewport asks for. What a browser then
// paints at each width (one column, two columns, where the navigation is) is
// asserted in a browser, by the smoke test.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import "../src/chorus-app.js";
import { DESKTOP_MIN_EM, DESKTOP_QUERY, LAYOUTS, layoutFor, watchLayout } from "../src/layout.js";
import { getByLabel } from "./label-query.js";

// A matchMedia over a viewport width in em that the test moves.
function fakeViewport(width) {
  const lists = [];
  return {
    asked: [],
    matchMedia(query) {
      this.asked.push(query);
      const min = Number(/^\(min-width: (\d+)em\)$/.exec(query)[1]);
      const listeners = new Set();
      const list = {
        get matches() {
          return width >= min;
        },
        addEventListener: (type, listener) => type === "change" && listeners.add(listener),
        removeEventListener: (type, listener) => type === "change" && listeners.delete(listener),
        listeners,
      };
      lists.push(list);
      return list;
    },
    resize(to) {
      const before = lists.map((list) => list.matches);
      width = to;
      lists.forEach((list, at) => {
        if (list.matches !== before[at]) for (const listener of [...list.listeners]) listener({ matches: list.matches });
      });
    },
    listening: () => lists.reduce((count, list) => count + list.listeners.size, 0),
  };
}

let restore = null;

afterEach(() => {
  restore?.();
  restore = null;
  document.body.replaceChildren();
});

// The app on a page whose viewport is `viewport`.
async function mountIn(viewport) {
  const real = Object.getOwnPropertyDescriptor(globalThis, "matchMedia");
  Object.defineProperty(globalThis, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query) => viewport.matchMedia(query),
  });
  restore = () => {
    if (real) Object.defineProperty(globalThis, "matchMedia", real);
    else delete globalThis.matchMedia;
  };
  const app = document.createElement("chorus-app");
  document.body.append(app);
  await app.updateComplete;
  return app;
}

test("the breakpoint between the phone and the desktop layout is 48em, and the desktop layout starts on it", () => {
  assert.equal(DESKTOP_MIN_EM, 48);
  assert.equal(DESKTOP_QUERY, "(min-width: 48em)");
  assert.deepEqual([...LAYOUTS], ["phone", "desktop"]);
  assert.equal(layoutFor(DESKTOP_MIN_EM - 0.01), "phone");
  assert.equal(layoutFor(DESKTOP_MIN_EM), "desktop");
  assert.equal(layoutFor(20), "phone");
  assert.equal(layoutFor(90), "desktop");
});

test("watchLayout says the layout at once, again when the viewport crosses the breakpoint, and no more once stopped", () => {
  const viewport = fakeViewport(24);
  const said = [];
  const stop = watchLayout((layout) => said.push(layout), viewport);
  assert.deepEqual(viewport.asked, [DESKTOP_QUERY]);
  assert.deepEqual(said, ["phone"]);
  viewport.resize(30);
  assert.deepEqual(said, ["phone"], "a resize that crosses nothing says nothing");
  viewport.resize(48);
  viewport.resize(47.9);
  assert.deepEqual(said, ["phone", "desktop", "phone"]);
  stop();
  assert.equal(viewport.listening(), 0);
  viewport.resize(80);
  assert.deepEqual(said, ["phone", "desktop", "phone"]);
});

test("where the browser has no matchMedia the layout is the phone's one column", () => {
  const said = [];
  const stop = watchLayout((layout) => said.push(layout), {});
  assert.deepEqual(said, ["phone"]);
  stop();
});

test("the app reflects the layout of its viewport and follows it across the breakpoint", async () => {
  const viewport = fakeViewport(24);
  const app = await mountIn(viewport);
  assert.equal(app.getAttribute("layout"), "phone");
  viewport.resize(64);
  await app.updateComplete;
  assert.equal(app.getAttribute("layout"), "desktop");
  viewport.resize(47);
  await app.updateComplete;
  assert.equal(app.getAttribute("layout"), "phone");
});

test("an app taken off the page stops following its viewport", async () => {
  const viewport = fakeViewport(64);
  const app = await mountIn(viewport);
  assert.equal(viewport.listening(), 1);
  app.remove();
  assert.equal(viewport.listening(), 0);
  viewport.resize(24);
  await app.updateComplete;
  assert.equal(app.getAttribute("layout"), "desktop");
});

test("a layout that is not one of the two falls back to the phone's", async () => {
  const app = await mountIn(fakeViewport(64));
  app.layout = "television";
  await app.updateComplete;
  assert.equal(app.layout, "phone");
});

test("the navigation is two labelled buttons, and one takes the focus to its region", async () => {
  const app = await mountIn(fakeViewport(24));
  const nav = getByLabel(app, "Sections");
  assert.equal(nav.localName, "nav");
  assert.deepEqual([...nav.querySelectorAll("button")].map((button) => button.textContent), ["Groups", "Rooms"]);

  const rooms = getByLabel(app, "Rooms");
  const groups = getByLabel(app, "Groups");
  const scrolled = [];
  rooms.scrollIntoView = (how) => scrolled.push(["rooms", how]);
  groups.scrollIntoView = (how) => scrolled.push(["groups", how]);

  getByLabel(app, "Go to rooms").click();
  assert.equal(app.shadowRoot.activeElement, rooms);
  getByLabel(app, "Go to groups").click();
  assert.equal(app.shadowRoot.activeElement, groups);
  assert.deepEqual(scrolled, [["rooms", { block: "start" }], ["groups", { block: "start" }]]); // prettier-ignore
});

test("the styles select the layout and the kiosk by attribute and write no breakpoint", () => {
  const sheet = customElements.get("chorus-app").styles.cssText;
  assert.match(sheet, /:host\(\[layout="phone"\]\) nav \{[^}]*position: fixed;[^}]*bottom: var\(--reset-margin\)/);
  assert.match(sheet, /:host\(\[layout="desktop"\]\) \{[^}]*display: grid;[^}]*grid-template-columns:/);
  assert.match(sheet, /:host\(\[mode="kiosk"\]\) \{[^}]*--control-size: var\(--kiosk-control-size\)/);
  assert.doesNotMatch(sheet, /@media|@container/);
});
