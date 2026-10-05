// The groups region (src/groups.js) and one group's card (src/group-card.js)
// under happy-dom: every saved group is listed whether it is active or not,
// every live group with its rooms, and the card asks for what its controls
// say and computes nothing.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import "../src/groups.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

afterEach(() => {
  document.body.replaceChildren();
});

const member = (id, name = id) => ({ id, name });
const saved = (more = {}) => ({
  id: "downstairs",
  name: "Downstairs",
  kind: "saved",
  active: true,
  defined: [member("living", "Living Room"), member("kitchen")],
  rooms: [member("living", "Living Room"), member("kitchen")],
  volume: 600,
  ...more,
});
const live = (more = {}) => ({
  id: "live-1",
  name: "study + bedroom",
  kind: "live",
  active: null,
  defined: null,
  rooms: [member("study"), member("bedroom")],
  volume: 450,
  ...more,
});

async function mount(properties) {
  const region = document.createElement("chorus-groups");
  Object.assign(region, properties);
  document.body.append(region);
  await settled(region);
  return region;
}

async function settled(region) {
  await region.updateComplete;
  await Promise.all(cards(region).map((card) => card.updateComplete));
}

const cards = (region) => [...region.shadowRoot.querySelectorAll("chorus-group-card")];
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const members = (card) =>
  [...card.shadowRoot.querySelectorAll("li")].map((item) => [item.dataset.member, item.dataset.playing]);

// What a card asked for, as the app shell would hear it.
function heard(region) {
  const events = [];
  region.addEventListener("chorus-command", (event) => events.push(["command", event.detail]));
  region.addEventListener("chorus-move", (event) => events.push(["move", event.detail]));
  return events;
}

test("a saved group is listed whether it is active or not (K59)", async () => {
  const region = await mount({
    groups: [saved(), saved({ id: "upstairs", name: "Upstairs", active: false, rooms: [], volume: null })],
  });
  const [active, idle] = cards(region);
  assert.deepEqual(cards(region).map((card) => card.shadowRoot.querySelector("h2").textContent), ["Downstairs", "Upstairs"]);
  assert.equal(text(active.shadowRoot.querySelector("[data-kind]")), "Saved group, active");
  assert.equal(active.shadowRoot.querySelector("[data-kind]").dataset.active, "true");
  assert.equal(text(idle.shadowRoot.querySelector("[data-kind]")), "Saved group, not active");
  assert.equal(idle.shadowRoot.querySelector("[data-kind]").dataset.active, "false");
  // The idle one still names its rooms, says none plays in it, and offers to form it.
  assert.equal(getByLabel(region, "Rooms of Upstairs").localName, "ul");
  assert.deepEqual(members(idle), [["living", "false"], ["kitchen", "false"]]);
  assert.equal(getByLabel(region, "Group the rooms of Upstairs").localName, "button");
  // It is not formed, so there is no group volume to show: none is made up.
  assert.deepEqual(queryAllByLabel(region, "Group volume for Upstairs"), []);
  // The active one needs no forming and has the server's group volume.
  assert.deepEqual(queryAllByLabel(region, "Group the rooms of Downstairs"), []);
  assert.equal(getByLabel(region, "Group volume for Downstairs").value, "600");
});

test("a saved group only some of its rooms play in says so, room by room", async () => {
  const region = await mount({
    groups: [saved({ active: false, rooms: [member("kitchen"), member("den")], volume: 300 })],
  });
  const [card] = cards(region);
  assert.equal(text(card.shadowRoot.querySelector("[data-kind]")), "Saved group, partly formed");
  // Its own rooms in its order, then a room that joined it without being one of them.
  assert.deepEqual(members(card), [["living", "false"], ["kitchen", "true"], ["den", "true"]]);
  assert.equal(getByLabel(region, "Group the rooms of Downstairs").localName, "button");
});

test("a live group is listed with its rooms", async () => {
  const region = await mount({ groups: [saved(), live()] });
  const card = cards(region)[1];
  assert.equal(card.shadowRoot.querySelector("h2").textContent, "study + bedroom");
  assert.equal(text(card.shadowRoot.querySelector("[data-kind]")), "Live group");
  assert.deepEqual(members(card), [["study", "true"], ["bedroom", "true"]]);
  assert.deepEqual([...card.shadowRoot.querySelectorAll("li span:first-child")].map(text), ["study", "bedroom"]);
  assert.equal(getByLabel(region, "Group volume for study + bedroom").value, "450");
  assert.equal(text(card.shadowRoot.querySelector("[data-volume]")), "45%");
  assert.equal(region.shadowRoot.querySelector("[data-empty]"), null);
});

test("with no group the region says how to make one, and before the first state it says nothing", async () => {
  const region = await mount({ groups: [] });
  assert.match(text(region.shadowRoot.querySelector("[data-empty]")), /^No groups yet\./);
  region.groups = null;
  await settled(region);
  assert.equal(region.shadowRoot.querySelector("[data-empty]"), null);
});

test("an update keeps each group's card", async () => {
  const region = await mount({ groups: [saved(), live()] });
  const [first, second] = cards(region);
  region.groups = [saved({ volume: 100 }), live({ rooms: [member("study"), member("bedroom"), member("den")] })];
  await settled(region);
  assert.deepEqual(cards(region), [first, second]);
  assert.equal(getByLabel(region, "Group volume for Downstairs").value, "100");
  assert.equal(members(second).length, 3);
});

test("forming a saved group asks the server to take it", async () => {
  const region = await mount({ groups: [saved({ active: false, rooms: [], volume: null })] });
  const events = heard(region);
  getByLabel(region, "Group the rooms of Downstairs").click();
  assert.deepEqual(events, [["command", { subject: "downstairs", body: '{"v":2,"t":"take","target":"downstairs"}' }]]);
});

test("removing a room from a group asks for that room to play alone", async () => {
  const region = await mount({ groups: [live()] });
  const events = heard(region);
  getByLabel(region, "Remove bedroom from study + bedroom").click();
  assert.deepEqual(events, [["move", { room: "bedroom", destination: { kind: "alone" } }]]);
});

test("the group slider asks for a group volume when it is let go, and for nothing while it moves", async () => {
  const region = await mount({ groups: [saved()] });
  const events = heard(region);
  const [card] = cards(region);
  const slider = getByLabel(region, "Group volume for Downstairs");
  slider.focus();
  slider.value = "800";
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  await card.updateComplete;
  // The figure follows the finger; nothing has been asked for yet.
  assert.equal(text(card.shadowRoot.querySelector("[data-volume]")), "80%");
  assert.deepEqual(events, []);
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  assert.deepEqual(events, [
    ["command", { subject: "downstairs", body: '{"v":2,"t":"group_volume","group":"downstairs","volume":0.800}' }],
  ]);
  // Until the server answers, the figure is the server's last word again.
  await card.updateComplete;
  assert.equal(text(card.shadowRoot.querySelector("[data-volume]")), "60%");
});

test("an update does not move the group slider out of a person's hand, and a refusal puts it back", async () => {
  const region = await mount({ groups: [saved()] });
  const [card] = cards(region);
  const slider = getByLabel(region, "Group volume for Downstairs");
  slider.focus();
  slider.value = "800";
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  region.groups = [saved({ volume: 300 })];
  await settled(region);
  assert.equal(slider.value, "800", "held: the update left it where the hand has it");
  assert.equal(getByLabel(region, "Group volume for Downstairs"), slider, "and it is the same element");
  region.refusals = { downstairs: "no room is in a group 'downstairs'" };
  await settled(region);
  assert.equal(slider.value, "300", "refused: back to the server's value");
  assert.equal(text(card.shadowRoot.querySelector("[role=alert]")), "Refused: no room is in a group 'downstairs'");
  slider.blur();
  assert.equal(slider.value, "300");
});

test("each group is a drop target, and the way out of a group is one only while a grouped room is dragged", async () => {
  const region = await mount({ groups: [saved(), live()] });
  const items = [...region.shadowRoot.querySelectorAll("li[data-group]")];
  assert.deepEqual(items.map((item) => [item.dataset.drop, item.dataset.dropId]), [["group", "downstairs"], ["group", "live-1"]]);
  const alone = region.shadowRoot.querySelector('[data-drop="alone"]');
  assert.equal(alone.hidden, true);
  region.moving = { id: "den", name: "den", grouped: false };
  await settled(region);
  assert.equal(alone.hidden, true, "a room that is alone has nowhere to leave");
  region.moving = { id: "study", name: "study", grouped: true };
  region.over = { kind: "group", id: "downstairs" };
  await settled(region);
  assert.equal(alone.hidden, false);
  assert.equal(text(alone), "Drop here to play study alone.");
  assert.deepEqual(items.map((item) => item.hasAttribute("data-over")), [true, false]);
});

test("their styles name tokens, never a literal colour or length", () => {
  for (const name of ["chorus-groups", "chorus-group-card"]) {
    const sheet = customElements.get(name).styles.cssText;
    assert.match(sheet, /var\(--/);
    assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
  }
});
