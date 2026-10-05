// The live test (`make web-live`, gate step `web-live`): the app's own
// elements and state layer, in node under happy-dom with no browser, against
// a real chorus-server.
//
// It is the pattern every later screen proves itself with: start the server
// the way a house would be configured, mount `chorus-app` over a store that
// reads that server (the code main.js runs), drive the controls a person
// would, and hold what the page shows to what the server's own
// `GET /api/state` says, in both directions.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run this file without it.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { startHouse, until } from "./house.js";

// The house: two rooms, one of them with a stereo pair bonded and a name its
// id does not hold, so the text asserted below can only be the server's.
const LIVING = { id: "living", name: "Live Test Living Room" };
const DEN = { id: "den" };
const PAIR = [
  { endpoint: "endpoint-a", role: "FL" },
  { endpoint: "endpoint-b", role: "FR" },
];

let house;
let origin;
let store;
let app;

const otherClient = (message) => house.command(message);
const serverRoom = async (id) => (await house.state()).zones.find((zone) => zone.id === id);

// What the page shows, read off the rendered elements.
const cards = () => [
  ...(app.shadowRoot.querySelector("chorus-rooms")?.shadowRoot.querySelectorAll("chorus-room-card") ?? []),
];
const cardOf = (id) => cards().find((card) => card.room.id === id);
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const shown = (id) => {
  const card = cardOf(id);
  if (!card?.shadowRoot.querySelector("h2")) return null;
  const root = card.shadowRoot;
  return {
    name: text(root.querySelector("h2")),
    bond: [...root.querySelectorAll("li")].map(text),
    volume: root.querySelector("input[type=range]").value,
    figure: text(root.querySelector("[data-volume]")),
    muted: root.querySelector("button[aria-pressed]").getAttribute("aria-pressed"),
    refusal: text(root.querySelector("[role=alert]")),
  };
};

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  house = await startHouse([LIVING.id, DEN.id]);
  origin = house.origin;
  // The house, set up the way any client sets it: the living room's name, its
  // two wired endpoints, and the pair bonded.
  await otherClient(JSON.stringify({ v: 1, t: "name", zone: LIVING.id, name: LIVING.name }));
  for (const { endpoint } of PAIR) {
    await otherClient(JSON.stringify({ v: 2, t: "attach", zone: LIVING.id, endpoint, link: "wired" }));
  }
  await otherClient(JSON.stringify({ v: 2, t: "bond", zone: LIVING.id, members: PAIR }));
  await otherClient('{"v":1,"t":"volume","zone":"living","volume":0.400}');
  await otherClient('{"v":1,"t":"volume","zone":"den","volume":0.600}');

  // The app, as main.js makes it, reading that server.
  store = createStore(createClient({ base: `${origin}/` }));
  app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
});

after(async () => {
  store?.stop();
  app?.remove();
  await house?.stop();
});

test("the app renders both rooms of a real server, and the bonded set with its channel roles", async () => {
  await until("the living room's card", () => shown(LIVING.id), {
    name: LIVING.name,
    bond: ["Front left: endpoint-a", "Front right: endpoint-b"],
    volume: "400",
    figure: "40%",
    muted: "false",
    refusal: "",
  });
  await until("the den's card", () => shown(DEN.id), {
    name: "den",
    bond: [],
    volume: "600",
    figure: "60%",
    muted: "false",
    refusal: "",
  });
  assert.deepEqual(cards().map((card) => card.room.id), [LIVING.id, DEN.id]);
  // What the page shows is what the server holds.
  assert.deepEqual((await serverRoom(LIVING.id)).bond, PAIR);
  assert.equal(getByLabel(app, "Bonded set").localName, "ul");
  assert.equal(queryAllByLabel(app, "Bonded set").length, 1);
  // The event stream is delivering, not only the snapshot.
  await until("the store's status", () => store.view().status, "live");
});

test("a volume change made through the slider appears in the server's /api/state", async () => {
  const slider = getByLabel(app, `Volume for ${LIVING.name}`);
  slider.focus();
  slider.value = "250";
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  await until("the living room's volume on the server", async () => (await serverRoom(LIVING.id)).volume, 0.25);
  slider.blur();
  // And comes back down to the page as the server's value.
  await until("the living room's figure", () => shown(LIVING.id).figure, "25%");
  assert.equal(getByLabel(app, `Volume for ${LIVING.name}`), slider, "the slider is the element it was");
  assert.equal((await serverRoom(DEN.id)).volume, 0.6, "the other room was not touched");
});

test("a mute made through the button appears in the server's /api/state, and so does the unmute", async () => {
  const mute = getByLabel(app, `Mute ${LIVING.name}`);
  mute.click();
  await until("the living room's mute on the server", async () => (await serverRoom(LIVING.id)).muted, true);
  await until("the living room's mute button", () => shown(LIVING.id).muted, "true");
  assert.equal((await serverRoom(LIVING.id)).volume, 0.25, "muting kept the volume");
  mute.click();
  await until("the living room's mute on the server", async () => (await serverRoom(LIVING.id)).muted, false);
  await until("the living room's mute button", () => shown(LIVING.id).muted, "false");
});

test("a change made by another client appears in the page without a reload", async () => {
  const den = cardOf(DEN.id);
  const slider = getByLabel(app, "Volume for den");
  await otherClient('{"v":1,"t":"volume","zone":"den","volume":0.125}');
  await otherClient('{"v":1,"t":"mute","zone":"den","muted":true}');
  await otherClient('{"v":1,"t":"name","zone":"den","name":"Renamed Elsewhere"}');
  await until("the den's card", () => shown(DEN.id), {
    name: "Renamed Elsewhere",
    bond: [],
    volume: "125",
    figure: "13%",
    muted: "true",
    refusal: "",
  });
  // The same card and the same slider: the page was patched, not loaded again.
  assert.equal(cardOf(DEN.id), den);
  assert.equal(getByLabel(app, "Volume for Renamed Elsewhere"), slider);

  // The bond dissolved by another client goes from the page too.
  await otherClient(JSON.stringify({ v: 2, t: "unbond", zone: LIVING.id }));
  await until("the living room's bonded set", () => shown(LIVING.id).bond, []);
});
