// The speakers screen (src/speakers.js) in the shell, over a scripted server:
// a newly adopted speaker is shown as new, each control sends its one command
// as the catalog documents it, a refusal is shown in the server's words, and
// a changed key is shown as the refusal it is, with nothing that accepts it.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, test } from "node:test";

import { createClient, speakerForgetCommand, speakerNameCommand, speakerRoomCommand } from "../src/api.js";
import "../src/chorus-app.js";
import { addressOf, routeOf } from "../src/routes.js";
import { SPEAKERS_SCREEN, SPEAKER_SETUP_SCREEN } from "../src/speakers.js";
import { createStore, keyChangesOf, speakersOf } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
});

const fixture = (name) => readFileSync(new URL(`../../fixtures/control/v2/${name}`, import.meta.url), "utf8").trim();

// The house: two rooms and three speakers. One was adopted a moment ago (the
// server made its name, and it is in no room), one is named and in a room,
// and one is named, in a room and switched off.
const NEW = "chorus-0123456789ab";
const LEFT = "chorus-aaaaaaaaaaaa";
const OFF = "chorus-bbbbbbbbbbbb";
const speaker = (id, more = {}) => ({
  id,
  name: `Speaker ${id.slice(-4)}`,
  named: false,
  room: null,
  present: true,
  software: "chorus-firmware 0.4.0",
  link: "wireless",
  key: `fp-${id.slice(-4)}`,
  roles: ["player"],
  ...more,
});
const SPEAKERS = [
  speaker(NEW),
  speaker(LEFT, { name: "Living left", named: true, room: "living", link: "wired" }),
  speaker(OFF, { name: "Kitchen", named: true, room: "kitchen", present: false, software: "", link: "unknown" }),
];
const house = (serial, speakers = SPEAKERS, more = {}) =>
  stateOf(serial, [zone("living", { name: "Living Room" }), zone("kitchen")], {
    ...(speakers.length > 0 ? { speakers } : {}),
    ...more,
  });

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await app.shadowRoot.querySelector("chorus-speakers")?.updateComplete;
}

async function open(server, address = addressOf(SPEAKERS_SCREEN)) {
  history.replaceState(null, "", `/app/${address}`);
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

// A scripted server that holds the speakers as the real one does.
function speakersServer(start = SPEAKERS) {
  let speakers = start;
  let serial = 1;
  const server = fakeServer(house(serial, speakers));
  server.answer = (body) => {
    const message = JSON.parse(body);
    const change = (fields) => speakers.map((held) => (held.id === message.speaker ? { ...held, ...fields } : held));
    if (message.t === "speaker_name") speakers = change({ name: message.name, named: true });
    if (message.t === "speaker_room") speakers = change({ room: message.room });
    if (message.t === "speaker_forget") speakers = speakers.filter((held) => held.id !== message.speaker);
    serial += 1;
    server.snapshot = house(serial, speakers);
    return { status: 200, body: JSON.stringify(server.snapshot) };
  };
  return server;
}

const screenOf = (app) => app.shadowRoot.querySelector("chorus-speakers");
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const rows = (app) => [...screenOf(app).shadowRoot.querySelectorAll("li[data-speaker]")];
const row = (app, id) => rows(app).find((item) => item.dataset.speaker === id);
const value = (app, id, name) => text(row(app, id).querySelector(`[data-value="${name}"]`));
const alertOf = (app, id) => text(row(app, id).querySelector("[role=alert]"));

function type(field, name) {
  field.value = name;
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

function choose(picker, room) {
  picker.value = room;
  picker.dispatchEvent(new Event("change", { bubbles: true }));
}

test("the three commands are the catalog's own vectors", () => {
  assert.equal(speakerNameCommand("chorus-0123456789ab", "Kitchen left"), fixture("speaker_name.json"));
  assert.equal(speakerRoomCommand("chorus-0123456789ab", "kitchen"), fixture("speaker_room.json"));
  assert.equal(speakerRoomCommand("chorus-0123456789ab", null), fixture("speaker_room-none.json"));
  assert.equal(speakerForgetCommand("chorus-0123456789ab"), fixture("speaker_forget.json"));
});

test("the state's speakers and key changes are read as the catalog writes them", () => {
  const state = JSON.parse(fixture("state-speakers.json"));
  const read = speakersOf(state);
  assert.deepEqual(read.map((held) => held.id), state.speakers.map((held) => held.id)); // prettier-ignore
  for (const [at, held] of state.speakers.entries()) {
    assert.equal(read[at].name, held.name);
    assert.equal(read[at].named, held.named);
    assert.equal(read[at].room, held.room);
    assert.equal(read[at].isNew, !held.named && held.room === null);
    assert.equal(read[at].present, held.present);
    assert.equal(read[at].link, held.link);
  }
  assert.deepEqual(speakersOf(null), []);
  assert.deepEqual(speakersOf({ speakers: [null, { name: "no id" }, { id: "x" }] }).map((held) => [held.id, held.name, held.isNew, held.link]), [["x", "x", true, "unknown"]]); // prettier-ignore
  assert.deepEqual(keyChangesOf({ key_changes: [{ id: "x", pinned: "a", offered: "b" }, { pinned: "no id" }] }), [
    { id: "x", pinned: "a", offered: "b" },
  ]);
  assert.deepEqual(keyChangesOf(null), []);
});

test("the screen has an address of its own and opens from the home", async () => {
  assert.equal(addressOf(SPEAKERS_SCREEN), "#/speakers");
  assert.equal(routeOf("#/speakers").screen, SPEAKERS_SCREEN);
  const app = await open(fakeServer(house(1)), "#/");
  const link = getByLabel(app, "Speakers and their setup");
  assert.equal(link.getAttribute("href"), "#/speakers");
  link.click();
  await rendered(app);
  assert.equal(location.hash, "#/speakers");
  assert.equal(getByLabel(app, "Speakers").localName, "main");
  assert.equal(rows(app).length, 3);
  // The walk-through for a Wi-Fi speaker is linked from it.
  assert.equal(getByLabel(app, "Set up a Wi-Fi speaker").getAttribute("href"), addressOf(SPEAKER_SETUP_SCREEN));
  getByLabel(app, "Back to rooms").click();
  await rendered(app);
  assert.equal(getByLabel(app, "Rooms").localName, "main");
});

test("a speaker with named:false and room:null is shown as new, and the others are not", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  assert.deepEqual(rows(app).map((item) => item.dataset.speaker), [NEW, LEFT, OFF]); // prettier-ignore
  assert.equal(row(app, NEW).hasAttribute("data-new"), true);
  assert.match(text(row(app, NEW).querySelector("[data-new-mark]")), /^New: adopted, not named and in no room yet\.$/);
  for (const id of [LEFT, OFF]) {
    assert.equal(row(app, id).hasAttribute("data-new"), false);
    assert.equal(row(app, id).querySelector("[data-new-mark]"), null);
  }
  // A speaker somebody named but gave no room, or gave a room and no name, has been dealt with.
  server.send(house(2, [speaker(NEW, { named: true, name: "Spare" }), speaker(LEFT, { room: "living" })]));
  await rendered(app);
  assert.deepEqual(rows(app).map((item) => item.hasAttribute("data-new")), [false, false]); // prettier-ignore
  assert.deepEqual(server.commands, [], "showing sends nothing");
});

test("each speaker shows its name, its id, its room, its presence, its link and its software", async () => {
  const app = await open(fakeServer(house(1)));
  assert.equal(text(row(app, NEW).querySelector("h3")), "Speaker 89ab");
  assert.equal(text(row(app, NEW).querySelector("[data-id]")), NEW);
  assert.deepEqual(
    [NEW, LEFT, OFF].map((id) => [value(app, id, "present"), value(app, id, "link"), value(app, id, "software")]),
    [
      ["Connected", "Wi-Fi", "chorus-firmware 0.4.0"],
      ["Connected", "Wired", "chorus-firmware 0.4.0"],
      ["Not connected", "Not reported", "Not said yet"],
    ],
  );
  assert.equal(value(app, LEFT, "key"), "fp-aaaa");
  assert.deepEqual(
    [getByLabel(app, "Room of Speaker 89ab").value, getByLabel(app, "Room of Living left").value, getByLabel(app, "Room of Kitchen").value],
    ["", "living", "kitchen"],
  ); // prettier-ignore
  assert.deepEqual(
    [...getByLabel(app, "Room of Speaker 89ab").querySelectorAll("option")].map((option) => [option.value, text(option)]),
    [
      ["", "No room"],
      ["living", "Living Room"],
      ["kitchen", "kitchen"],
    ],
  );
  assert.equal(getByLabel(app, "Name of Living left").value, "Living left");
});

test("speaker_name is sent as documented, and the screen shows the server's name", async () => {
  const server = speakersServer();
  const app = await open(server);
  const field = getByLabel(app, "Name of Speaker 89ab");
  type(field, "  Kitchen left ");
  await rendered(app);
  assert.deepEqual(server.commands, [], "typing sends nothing");
  getByLabel(app, "Save the name of Speaker 89ab").click();
  await rendered(app);
  assert.deepEqual(server.commands, [`{"v":2,"t":"speaker_name","speaker":"${NEW}","name":"Kitchen left"}`]);
  assert.equal(server.commands[0], fixture("speaker_name.json"));
  assert.equal(text(row(app, NEW).querySelector("h3")), "Kitchen left");
  assert.equal(getByLabel(app, "Name of Kitchen left").value, "Kitchen left");
  // Named, it is no longer new, though it is still in no room.
  assert.equal(row(app, NEW).hasAttribute("data-new"), false);
  // The name it already has sends nothing; Enter in the field sends a new one.
  assert.equal(getByLabel(app, "Save the name of Kitchen left").disabled, true);
  type(getByLabel(app, "Name of Kitchen left"), "Kitchen right");
  getByLabel(app, "Name of Kitchen left").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await rendered(app);
  assert.equal(server.commands.length, 2);
  assert.equal(server.commands[1], `{"v":2,"t":"speaker_name","speaker":"${NEW}","name":"Kitchen right"}`);
  assert.equal(text(row(app, NEW).querySelector("h3")), "Kitchen right");
});

test("the name the server made can be kept as the speaker's name, and an empty one is not sent", async () => {
  const server = speakersServer();
  const app = await open(server);
  type(getByLabel(app, "Name of Speaker 89ab"), "   ");
  await rendered(app);
  assert.equal(getByLabel(app, "Save the name of Speaker 89ab").disabled, true);
  getByLabel(app, "Save the name of Speaker 89ab").click();
  await rendered(app);
  assert.deepEqual(server.commands, []);
  type(getByLabel(app, "Name of Speaker 89ab"), "Speaker 89ab");
  await rendered(app);
  getByLabel(app, "Save the name of Speaker 89ab").click();
  await rendered(app);
  assert.deepEqual(server.commands, [`{"v":2,"t":"speaker_name","speaker":"${NEW}","name":"Speaker 89ab"}`]);
});

test("speaker_room is sent with a room and with null, as documented", async () => {
  const server = speakersServer();
  const app = await open(server);
  choose(getByLabel(app, "Room of Speaker 89ab"), "kitchen");
  await rendered(app);
  assert.deepEqual(server.commands, [`{"v":2,"t":"speaker_room","speaker":"${NEW}","room":"kitchen"}`]);
  assert.equal(server.commands[0], fixture("speaker_room.json"));
  assert.equal(getByLabel(app, "Room of Speaker 89ab").value, "kitchen");
  assert.equal(row(app, NEW).hasAttribute("data-new"), false, "in a room, it is no longer new");

  // Out of every room: `room` is null, said out loud.
  choose(getByLabel(app, "Room of Speaker 89ab"), "");
  await rendered(app);
  assert.equal(server.commands.length, 2);
  assert.equal(server.commands[1], `{"v":2,"t":"speaker_room","speaker":"${NEW}","room":null}`);
  assert.equal(server.commands[1], fixture("speaker_room-none.json"));
  assert.equal(getByLabel(app, "Room of Speaker 89ab").value, "");

  // The room it is already in sends nothing.
  choose(getByLabel(app, "Room of Living left"), "living");
  await rendered(app);
  assert.equal(server.commands.length, 2);
});

test("speaker_forget is sent as documented, after the screen asked again", async () => {
  const server = speakersServer();
  const app = await open(server);
  getByLabel(app, "Forget Living left").click();
  await rendered(app);
  assert.deepEqual(server.commands, [], "the first press only asks");
  assert.match(text(row(app, LEFT).querySelector("[data-forget-question]")), /Forget Living left\? .*pinned key are removed/);
  getByLabel(app, "Keep Living left").click();
  await rendered(app);
  assert.deepEqual(server.commands, []);
  assert.equal(row(app, LEFT).querySelector("[data-forget-question]"), null);

  getByLabel(app, "Forget Living left").click();
  await rendered(app);
  getByLabel(app, "Yes, forget Living left").click();
  await rendered(app);
  assert.deepEqual(server.commands, [`{"v":2,"t":"speaker_forget","speaker":"${LEFT}"}`]);
  assert.equal(speakerForgetCommand("chorus-0123456789ab"), fixture("speaker_forget.json"));
  assert.deepEqual(rows(app).map((item) => item.dataset.speaker), [NEW, OFF]); // prettier-ignore
});

test("a bonded-set refusal is shown with its reason, on the speaker it was for, and nothing changes", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  // The server's own refusal (fixtures/control/v2/error-speaker-room-bonded.json), for this speaker.
  const refused = JSON.parse(fixture("error-speaker-room-bonded.json"));
  const detail = `speaker '${LEFT}' plays in room 'living''s bonded set; unbond room 'living' first`;
  assert.equal(refused.detail, detail.replaceAll(LEFT, "chorus-0123456789ab"));
  server.answer = () => ({ status: 400, body: JSON.stringify({ ...refused, detail }) });

  choose(getByLabel(app, "Room of Living left"), "kitchen");
  await rendered(app);
  assert.deepEqual(server.commands, [`{"v":2,"t":"speaker_room","speaker":"${LEFT}","room":"kitchen"}`]);
  assert.equal(alertOf(app, LEFT), `Refused: ${detail}`);
  assert.equal(getByLabel(app, "Room of Living left").value, "living", "the room is the server's still");
  assert.equal(alertOf(app, NEW), "");

  // Forgetting it is refused the same way, and it stays listed.
  getByLabel(app, "Forget Living left").click();
  await rendered(app);
  getByLabel(app, "Yes, forget Living left").click();
  await rendered(app);
  assert.equal(server.commands[1], `{"v":2,"t":"speaker_forget","speaker":"${LEFT}"}`);
  assert.equal(alertOf(app, LEFT), `Refused: ${detail}`);
  assert.equal(rows(app).length, 3);
});

test("a key change is shown as a refusal with the pinned and the offered key, and nothing accepts it", async () => {
  const change = { id: LEFT, pinned: "fp-aaaa", offered: "fp-ffff" };
  const server = fakeServer(house(1, SPEAKERS, { key_changes: [change] }));
  const app = await open(server);
  const list = getByLabel(app, "Changed keys");
  const entries = [...list.querySelectorAll("li[data-key-change]")];
  assert.deepEqual(entries.map((entry) => entry.dataset.keyChange), [LEFT]); // prettier-ignore
  const [entry] = entries;
  assert.equal(text(entry.querySelector("h3")), "Refused: Living left offered a changed key");
  assert.match(text(entry.querySelector("[role=alert]")), new RegExp(`${LEFT} offered a key that is not the one this id is pinned to\\. The server refused it`));
  assert.equal(text(entry.querySelector('[data-value="pinned"]')), "fp-aaaa");
  assert.equal(text(entry.querySelector('[data-value="offered"]')), "fp-ffff");
  assert.match(text(entry), /Nothing here accepts the offered key\./);
  // The entry has no control at all, and nothing on the screen is an "accept".
  assert.equal(entry.querySelectorAll("button, input, select, a, [role=button], [tabindex]").length, 0);
  const controls = [...screenOf(app).shadowRoot.querySelectorAll("button, a, input, select")];
  assert.deepEqual(
    controls.filter((control) => /accept|trust|approve|allow|adopt/i.test(`${control.getAttribute("aria-label")} ${text(control)}`)),
    [],
  );
  // The speaker's row says so too, and its pinned key is still the one shown.
  assert.match(text(row(app, LEFT).querySelector("[data-key-changed]")), /offered another key and was refused/);
  assert.equal(value(app, LEFT, "key"), "fp-aaaa");
  assert.equal(row(app, NEW).querySelector("[data-key-changed]"), null);
  assert.deepEqual(server.commands, [], "showing a changed key sends nothing");

  // The one way past is the owner's: forgetting the speaker, which asks first.
  getByLabel(app, "Forget Living left").click();
  await rendered(app);
  assert.deepEqual(server.commands, []);

  // Forgotten (by this or another client), the entry leaves with the state.
  server.send(house(2, [SPEAKERS[0], SPEAKERS[2]]));
  await rendered(app);
  assert.equal(queryAllByLabel(app, "Changed keys").length, 0);
});

test("a key change under an id that is not listed says nothing here can forget it", async () => {
  const app = await open(fakeServer(house(1, SPEAKERS, { key_changes: [{ id: "elsewhere", pinned: "p", offered: "o" }] })));
  const entry = getByLabel(app, "Changed keys").querySelector("li");
  assert.equal(text(entry.querySelector("h3")), "Refused: elsewhere offered a changed key");
  assert.match(text(entry), /not among the speakers listed here/);
});

test("a speaker another client adopts, names or moves appears with no reload, and a typed name is kept", async () => {
  const server = fakeServer(house(1, [SPEAKERS[1]]));
  const app = await open(server);
  const field = getByLabel(app, "Name of Living left");
  type(field, "Living le");
  await rendered(app);
  server.send(house(2, [speaker(NEW), { ...SPEAKERS[1], room: "kitchen" }]));
  await rendered(app);
  assert.deepEqual(rows(app).map((item) => [item.dataset.speaker, item.hasAttribute("data-new")]), [[NEW, true], [LEFT, false]]); // prettier-ignore
  assert.equal(getByLabel(app, "Room of Living left").value, "kitchen");
  assert.equal(getByLabel(app, "Name of Living left").value, "Living le", "what a person is typing is theirs");
  // A room the server no longer has is still said.
  server.send(house(3, [{ ...SPEAKERS[1], room: "porch" }]));
  await rendered(app);
  const picker = getByLabel(app, "Room of Living left");
  assert.equal(picker.value, "porch");
  assert.equal(text(picker.selectedOptions[0]), "porch (not on this server now)");
  assert.deepEqual(server.commands, []);
});

test("before the first state the screen says it is reading, and a server with no speaker says so", async () => {
  const server = fakeServer(null);
  const app = await open(server);
  const root = () => screenOf(app).shadowRoot;
  assert.equal(text(root().querySelector("[data-missing]")), "Reading this server's speakers.");
  server.send(house(1, []));
  await rendered(app);
  assert.equal(text(root().querySelector("[data-none]")), "This server has adopted no speaker yet.");
  assert.equal(rows(app).length, 0);
  assert.equal(getByLabel(app, "Set up a Wi-Fi speaker").localName, "a");
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-speakers").styles.cssText;
  assert.match(sheet, /min-height: var\(--control-size\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
