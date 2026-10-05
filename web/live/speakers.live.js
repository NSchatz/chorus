// The live test of the speakers screen and of the walk-through for a Wi-Fi
// speaker (`make web-live`): the app's own elements, navigation and state
// layer, in node under happy-dom with no browser, against a real
// chorus-server.
//
// The speaker is a real session: a scripted endpoint (endpoint.js) opens an
// audio session under an id the server has never seen, which is what
// adoption is. The walk-through is open when it arrives and completes on
// that arrival by itself; the speaker is then shown as new on the speakers
// screen, named and given a room through the screen, and each step is read
// back from the server's own `GET /api/state`.
//
// What this does not run is a speaker: no access point, no join page, no
// radio. The walk-through's steps at the speaker have never run against a
// real one (docs/bench-packet.md S9 is the owner's step).
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run this file without it.

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { offerLineIn } from "./endpoint.js";
import { startHouse, until } from "./house.js";

const ROOMS = ["kitchen", "den"];
// The id the endpoint's session authenticates as, and the name a person gives it.
const SPEAKER = "live-test-speaker";
const NAME = "Live Test Left";

let scratch;
let house;
let endpoint;
let store;
let app;
// The name the server made for the speaker when it adopted it.
let made;

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const screen = () => app.shadowRoot.querySelector("chorus-speakers");
const walk = () => app.shadowRoot.querySelector("chorus-speaker-setup");
const rows = () => [...(screen()?.shadowRoot.querySelectorAll("li[data-speaker]") ?? [])];
const row = () => rows().find((item) => item.dataset.speaker === SPEAKER) ?? null;
const one = (label) => queryAllByLabel(app, label)[0] ?? null;
const serverSpeaker = async () => ((await house.state()).speakers ?? []).find((speaker) => speaker.id === SPEAKER) ?? null;
// What the server says of the speaker that this screen changes or shows.
const held = async () => {
  const speaker = await serverSpeaker();
  return speaker && [speaker.name, speaker.named, speaker.room, speaker.present];
};

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  // The pins are a file of the server's identity directory, so the house has
  // one that is kept.
  scratch = await mkdtemp(join(tmpdir(), "chorus-web-live-"));
  house = await startHouse(ROOMS, { identityDir: scratch });

  store = createStore(createClient({ base: `${house.origin}/` }));
  app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
});

after(async () => {
  store?.stop();
  app?.remove();
  await endpoint?.stop();
  await house?.stop();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

test("the speakers screen opens from the home, and the walk-through from it", async () => {
  await until("the link on the home", () => queryAllByLabel(app, "Speakers and their setup").length, 1);
  getByLabel(app, "Speakers and their setup").click();
  await until("the speakers screen", () => Boolean(screen()), true);
  assert.equal(location.hash, "#/speakers", "the screen has an address of its own");
  await until("the store's status", () => store.view().status, "live");
  assert.equal((await house.state()).speakers, undefined, "the server has adopted nothing yet");
  await until("the screen with no speaker", () => Boolean(screen().shadowRoot.querySelector("[data-none]")), true);
  assert.deepEqual(rows(), []);

  getByLabel(app, "Set up a Wi-Fi speaker").click();
  await until("the walk-through", () => Boolean(walk()), true);
  assert.equal(location.hash, "#/speakers/setup");
  await until("the walk-through waits", () => Boolean(walk().shadowRoot.querySelector("[data-waiting]")), true);
  assert.equal(walk().shadowRoot.querySelectorAll("li[data-step]").length, 4);
  assert.equal(walk().shadowRoot.querySelector("[data-done]"), null);
});

test("an open walk-through completes when a real session is adopted", async () => {
  const [, port] = await house.said(/chorus-server: listening on=\S*?:(\d+)/);
  endpoint = await offerLineIn({ host: "127.0.0.1", port: Number(port), endpoint: SPEAKER });

  // The server adopted it: listed, unnamed, in no room, and present.
  await until("the server's speakers", async () => ((await house.state()).speakers ?? []).map((speaker) => speaker.id), [SPEAKER]);
  const adopted = await serverSpeaker();
  made = adopted.name;
  assert.deepEqual([adopted.named, adopted.room, adopted.present], [false, null, true]);
  await house.said(new RegExp(`speaker listed id=${SPEAKER} `));

  // The walk-through says so by itself: nothing was pressed.
  await until(
    "the walk-through's arrival",
    () => [...walk().shadowRoot.querySelectorAll("[data-arrived]")].map((entry) => entry.dataset.arrived),
    [SPEAKER],
  );
  assert.equal(text(walk().shadowRoot.querySelector("[data-arrived]")), `${made} (${SPEAKER}) joined and was adopted.`);
  assert.equal(walk().shadowRoot.querySelector("[data-waiting]"), null);

  // From it to the speakers screen.
  getByLabel(app, "Name the new speaker and give it a room").click();
  await until("the speakers screen", () => Boolean(screen()), true);
  assert.equal(location.hash, "#/speakers");
});

test("the adopted speaker appears on the screen as new, with what the server says of it", async () => {
  await until("the screen's speakers", () => rows().map((item) => item.dataset.speaker), [SPEAKER]);
  assert.equal(row().hasAttribute("data-new"), true);
  assert.match(text(row().querySelector("[data-new-mark]")), /^New: adopted, not named and in no room yet\.$/);
  assert.equal(text(row().querySelector("h3")), made);
  const adopted = await serverSpeaker();
  const shown = (name) => text(row().querySelector(`[data-value="${name}"]`));
  assert.equal(shown("present"), "Connected");
  assert.equal(shown("software"), adopted.software);
  assert.equal(adopted.software, "chorus-web-live-endpoint", "the software is the session's own hello");
  assert.equal(shown("key"), adopted.key);
  assert.equal(shown("link"), "Not reported", "an endpoint with no control client reports no link");
  assert.equal(adopted.link, "unknown");
  assert.equal(one(`Room of ${made}`).value, "");
  assert.deepEqual(
    [...one(`Room of ${made}`).querySelectorAll("option")].map((option) => option.value),
    ["", ...ROOMS],
  );
});

test("it is named and given a room through the screen, and GET /api/state agrees", async () => {
  const field = getByLabel(app, `Name of ${made}`);
  field.value = NAME;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await screen().updateComplete;
  getByLabel(app, `Save the name of ${made}`).click();
  await until("the server's speaker", () => held(), [NAME, true, null, true]);
  await until("the screen's name", () => text(row().querySelector("h3")), NAME);
  assert.equal(row().hasAttribute("data-new"), false, "named, it is no longer new");

  // Into a room: the server makes it a member, and present there.
  const choose = (room) => {
    const list = getByLabel(app, `Room of ${NAME}`);
    list.value = room;
    list.dispatchEvent(new Event("change", { bubbles: true }));
  };
  choose("den");
  await until("the server's speaker", () => held(), [NAME, true, "den", true]);
  await until("the screen's room", () => one(`Room of ${NAME}`)?.value, "den");
  const den = async () => (await house.state()).zones.find((zone) => zone.id === "den");
  await until("the room's members", async () => (await den()).endpoints, [SPEAKER]);
  await until("the room's present speakers", async () => (await den()).present, [SPEAKER]);

  // And out of every room again.
  choose("");
  await until("the server's speaker", () => held(), [NAME, true, null, true]);
  await until("the screen's room", () => one(`Room of ${NAME}`)?.value, "");
  await until("the room's members", async () => (await den()).endpoints, []);
  choose("kitchen");
  await until("the server's speaker", () => held(), [NAME, true, "kitchen", true]);
  await until("the screen's room", () => one(`Room of ${NAME}`)?.value, "kitchen");
  assert.equal(text(row().querySelector("[role=alert]")), "", "nothing was refused");
});

test("a change a second client makes appears, and a speaker whose session ends is not connected", async () => {
  const before = row();
  await house.command(JSON.stringify({ v: 2, t: "speaker_room", speaker: SPEAKER, room: "den" }));
  await until("the screen's room", () => one(`Room of ${NAME}`)?.value, "den");
  assert.equal(row(), before, "the page was patched, not loaded again");

  await endpoint.stop();
  endpoint = null;
  await until("the server's speaker", () => held(), [NAME, true, "den", false]);
  await until("the screen's presence", () => text(row().querySelector('[data-value="present"]')), "Not connected");
});

test("forgetting it through the screen removes it from the server, and asks first", async () => {
  getByLabel(app, `Forget ${NAME}`).click();
  await until("the question", () => Boolean(row().querySelector("[data-forget-question]")), true);
  assert.deepEqual(await held(), [NAME, true, "den", false], "the first press sent nothing");
  getByLabel(app, `Yes, forget ${NAME}`).click();
  await until("the server's speakers", async () => (await house.state()).speakers ?? [], []);
  await until("the screen", () => rows().length, 0);
  assert.equal((await house.state()).zones.find((zone) => zone.id === "den").endpoints.length, 0);
});
