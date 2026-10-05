// The rooms screen (src/rooms.js) under happy-dom: one card per room in the
// server's order, kept across updates, and words for the states with no room.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import "../src/rooms.js";

afterEach(() => {
  document.body.replaceChildren();
});

const room = (id, more = {}) => ({ id, name: id, volume: 500, muted: false, bond: [], ...more });

async function mount(properties) {
  const screen = document.createElement("chorus-rooms");
  Object.assign(screen, properties);
  document.body.append(screen);
  await screen.updateComplete;
  return screen;
}

const cards = (screen) => [...screen.shadowRoot.querySelectorAll("chorus-room-card")];
const status = (screen) => screen.shadowRoot.querySelector("[role=status]").textContent.trim();

test("the screen shows one card per room, in the server's order", async () => {
  const screen = await mount({ rooms: [room("kitchen", { name: "The Kitchen" }), room("den")], status: "live" });
  assert.deepEqual(cards(screen).map((card) => card.room.name), ["The Kitchen", "den"]);
  assert.deepEqual(
    [...screen.shadowRoot.querySelectorAll("li")].map((item) => item.dataset.room),
    ["kitchen", "den"],
  );
  assert.equal(status(screen), "");
  assert.equal(screen.shadowRoot.querySelector("[data-empty]"), null);
});

test("an update keeps each room's card, whatever order the rooms come in", async () => {
  const screen = await mount({ rooms: [room("kitchen"), room("den")], status: "live" });
  const [kitchen, den] = cards(screen);
  screen.rooms = [room("den", { volume: 100 }), room("attic"), room("kitchen")];
  await screen.updateComplete;
  const after = cards(screen);
  assert.deepEqual(after.map((card) => card.room.id), ["den", "attic", "kitchen"]);
  assert.equal(after[0], den);
  assert.equal(after[2], kitchen);
  assert.equal(den.room.volume, 100);
});

test("a refusal is handed to the card of the room it was for", async () => {
  const screen = await mount({ rooms: [room("kitchen"), room("den")], status: "live", refusals: { den: "no" } });
  assert.deepEqual(cards(screen).map((card) => card.refusal), ["", "no"]);
});

test("before the first state, with no rooms and with a lost connection it says so in words", async () => {
  const screen = await mount({});
  assert.equal(status(screen), "Reading this server's rooms.");
  assert.equal(cards(screen).length, 0);

  screen.status = "lost";
  await screen.updateComplete;
  assert.equal(status(screen), "The server cannot be reached.");

  screen.rooms = [];
  screen.status = "live";
  await screen.updateComplete;
  assert.equal(status(screen), "");
  assert.match(screen.shadowRoot.querySelector("[data-empty]").textContent, /No rooms yet/);

  screen.rooms = [room("kitchen")];
  screen.status = "lost";
  await screen.updateComplete;
  assert.equal(status(screen), "Connection lost. This is the last known state.");
  assert.equal(cards(screen).length, 1);
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-rooms").styles.cssText;
  assert.match(sheet, /var\(--surface-gap\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
