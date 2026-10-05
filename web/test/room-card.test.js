// The room card (src/room-card.js) under happy-dom: what it shows of a room,
// what it asks for, and that an update never takes the volume slider out of a
// person's hand.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import "../src/room-card.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

afterEach(() => {
  document.body.replaceChildren();
});

const living = (more = {}) => ({
  id: "living",
  name: "Living Room",
  volume: 400,
  muted: false,
  bond: [
    { endpoint: "endpoint-a", name: "Left of the TV", role: "FL" },
    { endpoint: "endpoint-b", name: "endpoint-b", role: "FR" },
    { endpoint: "endpoint-c", name: "endpoint-c", role: "LFE" },
  ],
  ...more,
});

async function mount(room) {
  const card = document.createElement("chorus-room-card");
  card.room = room;
  document.body.append(card);
  await card.updateComplete;
  return card;
}

const lines = (card, selector) =>
  [...card.shadowRoot.querySelectorAll(selector)].map((node) => node.textContent.replace(/\s+/g, " ").trim());

test("a room card shows the room's name, its bonded set and each member's channel role", async () => {
  const card = await mount(living());
  assert.equal(card.shadowRoot.querySelector("h2").textContent, "Living Room");
  const set = getByLabel(card, "Bonded set");
  assert.equal(set.localName, "ul");
  assert.deepEqual(lines(card, "li"), [
    "Front left: Left of the TV",
    "Front right: endpoint-b",
    "Subwoofer: endpoint-c",
  ]);
  assert.deepEqual(
    [...set.querySelectorAll("li")].map((item) => [item.dataset.endpoint, item.dataset.role]),
    [
      ["endpoint-a", "FL"],
      ["endpoint-b", "FR"],
      ["endpoint-c", "LFE"],
    ],
  );
});

test("a room with no bonded set shows none", async () => {
  const card = await mount(living({ bond: [] }));
  assert.equal(queryAllByLabel(card, "Bonded set").length, 0);
  assert.equal(card.shadowRoot.querySelectorAll("li").length, 0);
});

test("the slider and the figure beside it are the server's volume", async () => {
  const card = await mount(living());
  const slider = getByLabel(card, "Volume for Living Room");
  assert.equal(slider.type, "range");
  assert.equal(slider.value, "400");
  assert.equal(card.shadowRoot.querySelector("[data-volume]").textContent, "40%");

  card.room = living({ volume: 725 });
  await card.updateComplete;
  assert.equal(slider.value, "725");
  assert.equal(card.shadowRoot.querySelector("[data-volume]").textContent, "73%");
});

test("the slider is not replaced or reset while it has focus and an update arrives", async () => {
  const card = await mount(living());
  const slider = getByLabel(card, "Volume for Living Room");
  slider.focus();
  // A drag in progress: the slider is where the finger is, not yet released.
  slider.value = "650";
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  await card.updateComplete;
  assert.equal(card.shadowRoot.querySelector("[data-volume]").textContent, "65%");

  // The server's state arrives with another volume, a new name and a mute.
  card.room = living({ name: "Lounge", volume: 100, muted: true });
  await card.updateComplete;
  const after = getByLabel(card, "Volume for Lounge");
  assert.equal(after, slider, "the same element: the update patched the card and did not replace the slider");
  assert.equal(slider.value, "650", "the update did not move the slider under the finger");
  assert.equal(card.shadowRoot.querySelector("[data-volume]").textContent, "65%");
  // Everything else on the card did take the update.
  assert.equal(card.shadowRoot.querySelector("h2").textContent, "Lounge");
  assert.equal(getByLabel(card, "Mute Lounge").getAttribute("aria-pressed"), "true");

  // More updates, still held.
  card.room = living({ name: "Lounge", volume: 120 });
  await card.updateComplete;
  assert.equal(slider.value, "650");

  // Let go without committing: the slider is the server's again.
  slider.blur();
  await card.updateComplete;
  assert.equal(getByLabel(card, "Volume for Lounge"), slider);
  assert.equal(slider.value, "120");
  assert.equal(card.shadowRoot.querySelector("[data-volume]").textContent, "12%");
});

test("releasing the slider asks for that volume and shows no value the server has not sent", async () => {
  const card = await mount(living());
  const asked = [];
  card.addEventListener("chorus-command", (event) => asked.push(event.detail));
  const slider = getByLabel(card, "Volume for Living Room");
  slider.focus();
  slider.value = "250";
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  await card.updateComplete;
  assert.deepEqual(asked, [{ room: "living", body: '{"v":1,"t":"volume","zone":"living","volume":0.250}' }]);
  // No optimistic value: the figure is the server's until the state comes back.
  assert.equal(card.shadowRoot.querySelector("[data-volume]").textContent, "40%");

  card.room = living({ volume: 250 });
  await card.updateComplete;
  assert.equal(card.shadowRoot.querySelector("[data-volume]").textContent, "25%");
  assert.equal(slider.value, "250");
});

test("the mute button says the server's mute and asks for the other", async () => {
  const card = await mount(living());
  const asked = [];
  card.addEventListener("chorus-command", (event) => asked.push(event.detail.body));
  const mute = getByLabel(card, "Mute Living Room");
  assert.equal(mute.getAttribute("aria-pressed"), "false");
  assert.equal(card.shadowRoot.querySelector("[data-mute]").textContent, "Not muted");
  mute.click();
  await card.updateComplete;
  // Asked for, not assumed.
  assert.equal(mute.getAttribute("aria-pressed"), "false");

  card.room = living({ muted: true });
  await card.updateComplete;
  assert.equal(mute.getAttribute("aria-pressed"), "true");
  assert.equal(card.shadowRoot.querySelector("[data-mute]").textContent, "Muted");
  mute.click();
  assert.deepEqual(asked, [
    '{"v":1,"t":"mute","zone":"living","muted":true}',
    '{"v":1,"t":"mute","zone":"living","muted":false}',
  ]);
});

test("a refusal is shown in the server's words and puts a held slider back", async () => {
  const card = await mount(living());
  const alert = card.shadowRoot.querySelector("[role=alert]");
  assert.equal(alert.textContent.trim(), "");
  const slider = getByLabel(card, "Volume for Living Room");
  slider.focus();
  slider.value = "900";
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));

  card.refusal = "there is no zone 'living'";
  await card.updateComplete;
  assert.equal(alert.textContent.trim(), "Refused: there is no zone 'living'");
  assert.equal(slider.value, "400");
});

test("a volume or mute the app cannot read is said in words, with no control at a made-up value", async () => {
  const card = await mount(living({ volume: null, muted: null }));
  assert.equal(card.shadowRoot.querySelector("input"), null);
  assert.equal(card.shadowRoot.querySelector("[data-volume]").textContent, "Unavailable");
  assert.equal(getByLabel(card, "Mute Living Room").disabled, true);
  assert.equal(card.shadowRoot.querySelector("[data-mute]").textContent, "Unavailable");
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-room-card").styles.cssText;
  assert.match(sheet, /var\(--control-size\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
