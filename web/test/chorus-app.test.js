// The app shell as a component: defined, rendered into its shadow root and
// updated by Lit, under happy-dom (test/setup.js), with no browser.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import "../src/chorus-app.js";
import { getByLabel } from "./label-query.js";

afterEach(() => {
  document.body.replaceChildren();
});

async function mount() {
  const app = document.createElement("chorus-app");
  document.body.append(app);
  await app.updateComplete;
  return app;
}

test("the shell renders its wordmark and a labelled main region", async () => {
  const app = await mount();
  assert.equal(app.shadowRoot.querySelector("h1").textContent, "chorus");
  const main = getByLabel(app, "Rooms");
  assert.equal(main.localName, "main");
  assert.ok(main.querySelector("slot"), "screens are slotted into the main region");
});

test("the shell lists the server's rooms by name, in the server's order", async () => {
  const app = await mount();
  assert.equal(getByLabel(app, "Rooms").querySelectorAll("li").length, 0);
  app.rooms = [
    { id: "kitchen", name: "The Kitchen" },
    { id: "den", name: "den" },
  ];
  await app.updateComplete;
  const items = [...getByLabel(app, "Rooms").querySelectorAll("li")];
  assert.deepEqual(items.map((item) => item.textContent.trim()), ["The Kitchen", "den"]);
  assert.deepEqual(items.map((item) => item.dataset.room), ["kitchen", "den"]);
});

test("the layout is a reflected property, ordinary by default", async () => {
  const app = await mount();
  assert.equal(app.getAttribute("mode"), "app");
  app.mode = "kiosk";
  await app.updateComplete;
  assert.equal(app.getAttribute("mode"), "kiosk");
});

test("a layout that is not one of the two falls back to the ordinary one", async () => {
  const app = await mount();
  app.mode = "cinema";
  await app.updateComplete;
  assert.equal(app.mode, "app");
  assert.equal(app.getAttribute("mode"), "app");
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-app").styles.cssText;
  assert.match(sheet, /var\(--wordmark-ink\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
