// The live test of inputs and what is playing (`make web-live`, gate step
// `web-live`): the app's own elements and state layer, in node under
// happy-dom with no browser, against a real chorus-server.
//
// The server is given what a house has, from outside, the way the things in
// a house give it: an endpoint's session offers a line-in (endpoint.js), and
// a UPnP control point tells a room's renderer to play a track that has a
// title, an artist and a cover (control-point.js). The app is then held to
// the server's own `GET /api/state`: the picker lists exactly the inputs the
// server offers, choosing one through it changes the group's `source`, and a
// player's now-playing record shows in the room's card with an artwork image
// whose address the server answers with the cover.

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { openDiscovery, playOnRenderer, startOrigin } from "./control-point.js";
import { offerLineIn } from "./endpoint.js";
import { startHouse, until } from "./house.js";

const ROOMS = ["kitchen", "den"];
// The endpoint with something wired to its line-in, and what a person
// called that input.
const ENDPOINT = "live-test-amp";
const INPUT = `${ENDPOINT}/line-1`;
const LABEL = "Live Test Record Deck";
const TRACK = { title: "Live Test Ferry", artist: "The Live Test Engines", album: "Live Test Crossings" };

let scratch;
let origin;
let discovery;
let house;
let endpoint;
let store;
let app;

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const roomCard = (id) =>
  app.shadowRoot.querySelector("chorus-rooms").shadowRoot.querySelector(`li[data-room="${id}"] chorus-room-card`);
const playingOf = (id) => roomCard(id)?.shadowRoot.querySelector("chorus-playing")?.shadowRoot ?? null;
const groupOf = (state, id) => state.groups.find((group) => group.id === id);

// The picker of a room as the page shows it: each input's id, its button's
// words and whether it is the one marked.
const picker = (id) =>
  [...(playingOf(id)?.querySelectorAll("li[data-input]") ?? [])].map((item) => {
    const button = item.querySelector("button");
    return { input: item.dataset.input, label: text(button), current: button.getAttribute("aria-pressed") === "true" };
  });

// What the page says a room is playing: the record's words, or null.
function shownRecord(id) {
  const root = playingOf(id);
  if (!root?.querySelector("[data-now-playing]")) return null;
  const words = (selector) => {
    const node = root.querySelector(selector);
    return node ? text(node) : null;
  };
  return {
    title: words("[data-title]"),
    artist: words("[data-artist]"),
    album: words("[data-album]"),
    state: root.querySelector("[data-now-playing]").dataset.nowPlaying,
  };
}

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  scratch = await mkdtemp(join(tmpdir(), "chorus-web-live-"));
  origin = await startOrigin();
  discovery = await openDiscovery();
  // One network player and the renderers that give it work, with discovery
  // kept on loopback; the players may fetch from loopback, where the test's
  // origin is.
  house = await startHouse(ROOMS, {
    identityDir: scratch,
    extra: [
      "--state-file", join(scratch, "state"),
      "--slots", "4",
      "--players", "1",
      "--media-allow-loopback",
      "--upnp",
      "--upnp-listen", "127.0.0.1:0",
      "--upnp-ssdp-port", "0",
      "--upnp-ssdp-group", `127.0.0.1:${discovery.port}`,
      "--upnp-openhome", "off",
    ],
  }); // prettier-ignore

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
  await discovery?.close();
  await origin?.stop();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

test("with no input offered the page offers none, and says what each room plays", async () => {
  await until("the den's source line", () => text(playingOf("den")?.querySelector("[data-source]") ?? document.body), "Source: The server's stream");
  const state = await house.state();
  assert.deepEqual(state.inputs, []);
  assert.equal(groupOf(state, "den").source, "stream");
  assert.deepEqual(picker("den"), []);
  assert.equal(shownRecord("den"), null);
  await until("the store's status", () => store.view().status, "live");
});

test("the picker lists exactly the inputs the server offers, by their labels", async () => {
  const [, port] = await house.said(/chorus-server: listening on=\S*?:(\d+)/);
  endpoint = await offerLineIn({ host: "127.0.0.1", port: Number(port), endpoint: ENDPOINT });
  await until("the inputs the server offers", async () => (await house.state()).inputs, [INPUT]);
  // Unlabelled, an input is called by its id.
  for (const room of ROOMS) {
    await until(`the ${room}'s picker`, () => picker(room), [{ input: INPUT, label: INPUT, current: false }]);
  }
  // A person names it, as any client may; the page follows.
  await house.command(JSON.stringify({ v: 2, t: "input_label", input: INPUT, name: LABEL, role: "line-in" }));
  const state = await house.state();
  assert.deepEqual(state.inputs, [INPUT]);
  assert.deepEqual(state.input_labels, [{ input: INPUT, name: LABEL, role: "line-in" }]);
  for (const room of ROOMS) {
    await until(`the ${room}'s picker`, () => picker(room), [{ input: INPUT, label: LABEL, current: false }]);
  }
  assert.equal(getByLabel(app, "Inputs for den").localName, "ul");
});

test("choosing an input through the app changes the group's source in /api/state", async () => {
  assert.equal(groupOf(await house.state(), "den").source, "stream");
  getByLabel(app, `Play ${LABEL} in den`).click();
  await until("the den's source on the server", async () => groupOf(await house.state(), "den").source, `line-in:${INPUT}`);
  // The mark and the source line follow the state that came back.
  await until("the den's picker", () => picker("den"), [{ input: INPUT, label: LABEL, current: true }]);
  assert.equal(text(playingOf("den").querySelector("[data-source]")), `Source: ${LABEL}`);
  assert.equal(text(roomCard("den").shadowRoot.querySelector("[role=alert]")), "");
  // The other room was not touched.
  const state = await house.state();
  assert.equal(groupOf(state, "kitchen").source, "stream");
  assert.deepEqual(picker("kitchen"), [{ input: INPUT, label: LABEL, current: false }]);
  assert.deepEqual(state.zones.map((room) => [room.id, room.group]), ROOMS.map((id) => [id, id]));
});

test("a now-playing record set on a player shows its title and artist, and its artwork answers 200 from the server", async () => {
  assert.equal(shownRecord("kitchen"), null);
  const [, ssdp] = await house.said(/upnp renderers listening on=\S+ .*?ssdp_port=(\d+)/);
  await playOnRenderer(discovery, Number(ssdp), "kitchen", { ...TRACK, track: origin.track, cover: origin.cover });
  await until("the kitchen's record on the server", async () => groupOf(await house.state(), "kitchen").now_playing?.state, "playing");
  const state = await house.state();
  const record = groupOf(state, "kitchen").now_playing;
  assert.equal(groupOf(state, "kitchen").source, "player:p0");
  assert.deepEqual(
    { title: record.title, artist: record.artist, album: record.album, art: record.art_url },
    { ...TRACK, art: origin.cover },
  );
  // The component shows the record as the server has it.
  await until("the kitchen's now-playing view", () => shownRecord("kitchen"), { ...TRACK, state: "playing" });
  assert.equal(text(playingOf("kitchen").querySelector("[data-source]")), "Source: Network player p0");

  // Its artwork is an image on the server's own route, not the cover's
  // address, and the server answers that address with the cover.
  const image = playingOf("kitchen").querySelector("img");
  const address = new URL(image.getAttribute("src"));
  assert.equal(address.origin, house.origin);
  assert.equal(address.pathname, "/api/artwork");
  assert.deepEqual([...address.searchParams], [["group", "kitchen"]]);
  const answer = await fetch(image.getAttribute("src"));
  assert.equal(answer.status, 200);
  assert.equal(answer.headers.get("content-type"), "image/png");
  assert.equal(Buffer.from(await answer.arrayBuffer()).subarray(1, 4).toString("latin1"), "PNG");
  assert.ok(origin.asked.includes("/cover.png"), "the server fetched the cover from where the record says it is");

  // The room that plays the line-in has no record, and shows none.
  assert.equal(groupOf(state, "den").now_playing, undefined);
  assert.equal(shownRecord("den"), null);
  assert.deepEqual(queryAllByLabel(app, "Artwork for den"), []);
});
