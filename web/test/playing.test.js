// What a room or a group plays (src/playing.js) under happy-dom, and the
// state layer's reading of it (src/state.js): the input picker lists exactly
// the inputs the server offers, by their labels, and marks the one playing;
// the now-playing view shows the record and loads its artwork from this
// server's own route; a group with no record shows none; and an artwork that
// fails to load gives way to the placeholder.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, test } from "node:test";

import { artworkUrl, createClient } from "../src/api.js";
import "../src/group-card.js";
import "../src/playing.js";
import { sourceText } from "../src/playing.js";
import "../src/room-card.js";
import { createStore, groupsOf, inputsOf, playingOf, roomsOf } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

afterEach(() => {
  document.body.replaceChildren();
});

const fixture = (name) =>
  JSON.parse(readFileSync(new URL(`../../fixtures/control/v2/${name}.json`, import.meta.url), "utf8"));

const INPUTS = [
  { id: "endpoint-a/line-1", source: "line-in:endpoint-a/line-1", label: "endpoint-a/line-1" },
  { id: "endpoint-c/line-1", source: "line-in:endpoint-c/line-1", label: "Kitchen streamer" },
  { id: "endpoint-c/optical-1", source: "line-in:endpoint-c/optical-1", label: "Television" },
];

const song = (more = {}) => ({
  title: "Morning Light",
  artist: "The Example Quartet",
  album: "First Takes",
  state: "playing",
  via: "upnp",
  artwork: "../api/artwork?group=kitchen#abc",
  ...more,
});

async function mount(properties) {
  const playing = document.createElement("chorus-playing");
  Object.assign(playing, { target: "kitchen", name: "Kitchen", source: "stream", pick: true, ...properties });
  document.body.append(playing);
  await playing.updateComplete;
  return playing;
}

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const part = (playing, selector) => playing.shadowRoot.querySelector(selector);
const buttons = (playing) => [...playing.shadowRoot.querySelectorAll("li[data-input] button")];

function heard(element) {
  const events = [];
  element.addEventListener("chorus-command", (event) => events.push(event.detail));
  return events;
}

test("the input picker lists exactly the offered inputs, by their labels, and marks the current source", async () => {
  const playing = await mount({ inputs: INPUTS, source: "line-in:endpoint-c/line-1" });
  assert.equal(getByLabel(playing, "Inputs for Kitchen").localName, "ul");
  // One button for each offered input, in the server's order, and no other:
  // the stream, a player or a chime is not something a person picks here.
  assert.deepEqual(
    [...playing.shadowRoot.querySelectorAll("li[data-input]")].map((item) => item.dataset.input),
    INPUTS.map((input) => input.id),
  );
  assert.deepEqual(buttons(playing).map(text), ["endpoint-a/line-1", "Kitchen streamer", "Television"]);
  assert.deepEqual(playing.shadowRoot.querySelectorAll("button").length, INPUTS.length);
  for (const input of INPUTS) {
    assert.equal(getByLabel(playing, `Play ${input.label} in Kitchen`).localName, "button");
  }
  // The one the group plays is the pressed one, and the source line names it.
  assert.deepEqual(buttons(playing).map((button) => button.getAttribute("aria-pressed")), ["false", "true", "false"]);
  assert.equal(text(part(playing, "[data-source]")), "Source: Kitchen streamer");
  assert.equal(part(playing, "[data-source]").dataset.source, "line-in:endpoint-c/line-1");
});

test("a source that is not an offered input marks none of them, and no inputs means no picker", async () => {
  const playing = await mount({ inputs: INPUTS, source: "player:p0" });
  assert.deepEqual(buttons(playing).map((button) => button.getAttribute("aria-pressed")), ["false", "false", "false"]);
  assert.equal(text(part(playing, "[data-source]")), "Source: Network player p0");
  playing.inputs = [];
  await playing.updateComplete;
  assert.deepEqual(queryAllByLabel(playing, "Inputs for Kitchen"), []);
  assert.equal(playing.shadowRoot.querySelectorAll("button").length, 0);
});

test("choosing an input asks for a take of the group with that input, and the mark waits for the server", async () => {
  const playing = await mount({ inputs: INPUTS, source: "stream" });
  const events = heard(playing);
  const television = getByLabel(playing, "Play Television in Kitchen");
  television.click();
  assert.deepEqual(events, [
    { subject: "kitchen", body: '{"v":2,"t":"take","target":"kitchen","source":"line-in:endpoint-c/optical-1"}' },
  ]);
  // No optimistic value: it is marked when the state that resulted says so.
  await playing.updateComplete;
  assert.equal(television.getAttribute("aria-pressed"), "false");
  playing.source = "line-in:endpoint-c/optical-1";
  await playing.updateComplete;
  assert.equal(getByLabel(playing, "Play Television in Kitchen"), television, "the button is the element it was");
  assert.equal(television.getAttribute("aria-pressed"), "true");
  // The input it plays already is not asked for again.
  television.click();
  assert.equal(events.length, 1);
});

test("where the inputs cannot be chosen the source is still said", async () => {
  const playing = await mount({ inputs: INPUTS, source: "line-in:endpoint-c/line-1", pick: false });
  assert.equal(playing.shadowRoot.querySelectorAll("button").length, 0);
  assert.equal(text(part(playing, "[data-source]")), "Source: Kitchen streamer");
});

test("the now-playing view shows title, artist, album and state, and its artwork is the same-origin artwork route", async () => {
  const art = "http://192.0.2.10:8200/art/42.jpg";
  const playing = await mount({ source: "player:p0", nowPlaying: song({ artwork: artworkUrl("../", "kitchen", art) }) });
  assert.equal(text(part(playing, "[data-title]")), "Morning Light");
  assert.equal(text(part(playing, "[data-artist]")), "The Example Quartet");
  assert.equal(text(part(playing, "[data-album]")), "First Takes");
  assert.equal(text(part(playing, "[data-state]")), "Playing");
  assert.equal(part(playing, "[data-now-playing]").dataset.nowPlaying, "playing");
  // The artwork is an <img> on this origin's artwork route, never on the
  // record's own address.
  const image = part(playing, "img");
  assert.equal(image.getAttribute("alt"), "Artwork for Kitchen");
  assert.match(image.getAttribute("src"), /^\.\.\/api\/artwork\?group=kitchen(#[0-9a-z]+)?$/);
  // Resolved from the page (the app is served under /app/), it is this origin's route.
  const resolved = new URL(image.getAttribute("src"), "http://chorus.test/app/");
  assert.equal(resolved.origin, "http://chorus.test");
  assert.equal(resolved.pathname, "/api/artwork");
  assert.deepEqual([...resolved.searchParams], [["group", "kitchen"]]);
  assert.doesNotMatch(playing.shadowRoot.innerHTML, /192\.0\.2\.10/);
  assert.equal(part(playing, '[data-artwork="placeholder"]'), null);
});

test("the state is said in words: playing, paused or buffering", async () => {
  const playing = await mount({ source: "player:p0", nowPlaying: song() });
  for (const [state, words] of [["paused", "Paused"], ["buffering", "Buffering"], ["playing", "Playing"]]) {
    playing.nowPlaying = song({ state });
    await playing.updateComplete;
    assert.equal(text(part(playing, "[data-state]")), words);
    assert.equal(part(playing, "[data-now-playing]").dataset.nowPlaying, state);
  }
});

test("what the record does not know is left out, not made up", async () => {
  const playing = await mount({
    source: "player:p1",
    nowPlaying: song({ title: "Evening news", artist: null, album: null, state: "paused", artwork: null }),
  });
  assert.equal(text(part(playing, "[data-title]")), "Evening news");
  assert.equal(part(playing, "[data-artist]"), null);
  assert.equal(part(playing, "[data-album]"), null);
  assert.equal(text(part(playing, "[data-state]")), "Paused");
  // No artwork in the record: the placeholder, and no request for one.
  assert.equal(part(playing, "img"), null);
  assert.equal(getByLabel(playing, "No artwork for Kitchen").dataset.artwork, "placeholder");
});

test("without a now-playing record the view shows the source and no record", async () => {
  const playing = await mount({ source: "stream", nowPlaying: null, inputs: INPUTS });
  assert.equal(part(playing, "[data-now-playing]"), null);
  assert.equal(part(playing, "[data-title]"), null);
  assert.equal(part(playing, "[data-state]"), null);
  assert.equal(part(playing, "img"), null);
  assert.equal(part(playing, "[data-artwork]"), null);
  assert.equal(text(part(playing, "[data-source]")), "Source: The server's stream");
  // The inputs are offered all the same.
  assert.equal(buttons(playing).length, INPUTS.length);
  // A record arriving and going is shown and taken away.
  playing.nowPlaying = song();
  await playing.updateComplete;
  assert.equal(text(part(playing, "[data-title]")), "Morning Light");
  playing.nowPlaying = null;
  await playing.updateComplete;
  assert.equal(part(playing, "[data-now-playing]"), null);
});

test("an artwork that fails to load falls back to the placeholder, and the next cover is tried again", async () => {
  const first = artworkUrl("../", "kitchen", "http://192.0.2.10:8200/art/42.jpg");
  const playing = await mount({ source: "player:p0", nowPlaying: song({ artwork: first }) });
  const image = part(playing, "img");
  assert.equal(part(playing, '[data-artwork="placeholder"]'), null);
  image.dispatchEvent(new Event("error"));
  await playing.updateComplete;
  assert.equal(part(playing, "img"), null);
  assert.equal(getByLabel(playing, "No artwork for Kitchen").dataset.artwork, "placeholder");
  // The record is still shown beside it.
  assert.equal(text(part(playing, "[data-title]")), "Morning Light");
  // The same record again (every state message carries it) does not ask again.
  playing.nowPlaying = song({ artwork: first });
  await playing.updateComplete;
  assert.equal(part(playing, "img"), null);
  // Another track has another cover: its image is asked for.
  const second = artworkUrl("../", "kitchen", "http://192.0.2.10:8200/art/43.jpg");
  assert.notEqual(second, first);
  playing.nowPlaying = song({ title: "Second", artwork: second });
  await playing.updateComplete;
  assert.equal(part(playing, "img").getAttribute("src"), second);
  assert.equal(part(playing, '[data-artwork="placeholder"]'), null);
});

test("a source is said in words", () => {
  assert.equal(sourceText("stream"), "The server's stream");
  assert.equal(sourceText("none"), "Nothing");
  assert.equal(sourceText("line-in:endpoint-c/line-1", INPUTS), "Kitchen streamer");
  assert.equal(sourceText("line-in:gone/line-9", INPUTS), "Input gone/line-9");
  assert.equal(sourceText("player:p1"), "Network player p1");
  assert.equal(sourceText("chime:doorbell"), "Chime doorbell");
  assert.equal(sourceText("soloist:r0"), "Spotify");
  assert.equal(sourceText("something-new"), "something-new");
  assert.equal(sourceText(null), "Unavailable");
});

test("the state layer reads the offered inputs with their labels, in the server's order", () => {
  const state = fixture("state-inputs");
  assert.deepEqual(
    inputsOf(state),
    state.inputs.map((id) => ({
      id,
      source: `line-in:${id}`,
      label: state.input_labels.find((label) => label.input === id)?.name ?? id,
    })),
  );
  // A label for an input that is not offered now adds nothing.
  assert.deepEqual(
    inputsOf({ inputs: ["a/line-1", "b/line-1"], input_labels: [{ input: "b/line-1", name: "Deck", role: "line-in" }, { input: "z/line-1", name: "Gone", role: "line-in" }] }),
    [
      { id: "a/line-1", source: "line-in:a/line-1", label: "a/line-1" },
      { id: "b/line-1", source: "line-in:b/line-1", label: "Deck" },
    ],
  );
  assert.deepEqual(inputsOf({ zones: [] }), []);
  assert.deepEqual(inputsOf(null), []);
});

test("the state layer reads each formed group's source and now-playing record", () => {
  const state = fixture("state-playing");
  const playing = playingOf(state);
  for (const group of state.groups) {
    const read = playing.get(group.id);
    assert.equal(read.source, group.source);
    if (!group.now_playing) {
      assert.equal(read.nowPlaying, null);
      continue;
    }
    const { title, artist, album, state: playState, via, art_url: art } = group.now_playing;
    assert.deepEqual(read.nowPlaying, {
      title,
      artist,
      album,
      state: playState,
      via,
      artwork: art ? artworkUrl("../", group.id, art) : null,
    });
  }
  const withRecord = state.groups.filter((group) => group.now_playing);
  assert.ok(withRecord.length >= 2, "the fixture has groups with a record");
  assert.ok(withRecord.some((group) => group.now_playing.art_url), "and one of them has artwork");
  assert.ok(withRecord.some((group) => !group.now_playing.art_url), "and one has none");

  // A room alone carries what its own group plays; a room in a group does
  // not, because the group's card shows it.
  const rooms = roomsOf(state);
  for (const room of rooms) {
    if (room.group === room.id) {
      assert.deepEqual({ source: room.source, nowPlaying: room.nowPlaying }, playing.get(room.id));
    } else {
      assert.deepEqual({ source: room.source, nowPlaying: room.nowPlaying }, { source: null, nowPlaying: null });
    }
  }
  assert.ok(rooms.some((room) => room.group !== room.id) && rooms.some((room) => room.nowPlaying));
  // A saved or live group that is formed carries its own.
  for (const group of groupsOf(state)) {
    const formed = state.groups.some((now) => now.id === group.id);
    assert.deepEqual(
      { source: group.source, nowPlaying: group.nowPlaying },
      formed ? playing.get(group.id) : { source: null, nowPlaying: null },
    );
  }
  assert.ok(groupsOf(state).some((group) => group.nowPlaying));
});

test("the artwork address is the server's route as the page reaches it, and the store uses the client's", async () => {
  assert.equal(artworkUrl("../", "kitchen"), "../api/artwork?group=kitchen");
  assert.equal(artworkUrl("http://127.0.0.1:9/", "live 1&x"), "http://127.0.0.1:9/api/artwork?group=live%201%26x");
  const one = artworkUrl("../", "kitchen", "http://192.0.2.10/a.jpg");
  assert.match(one, /^\.\.\/api\/artwork\?group=kitchen#[0-9a-z]+$/);
  assert.equal(artworkUrl("../", "kitchen", "http://192.0.2.10/a.jpg"), one);
  assert.notEqual(artworkUrl("../", "kitchen", "http://192.0.2.10/b.jpg"), one);

  const state = fixture("state-playing");
  const client = createClient({
    base: "http://127.0.0.1:9/",
    fetch: async () => new Response(JSON.stringify(state), { status: 200 }),
  });
  const store = createStore({ ...client, events: () => () => {} });
  store.start();
  await new Promise((resolve) => setTimeout(resolve, 0));
  const view = store.view();
  store.stop();
  assert.deepEqual(view.inputs, inputsOf(state));
  const shown = view.groups.find((group) => group.nowPlaying?.artwork);
  assert.match(shown.nowPlaying.artwork, new RegExp(`^http://127\\.0\\.0\\.1:9/api/artwork\\?group=${shown.id}#`));
});

test("a room alone shows what it plays on its card, and a room in a group does not", async () => {
  const room = { id: "kitchen", name: "Kitchen", volume: 400, muted: false, bond: [], group: "kitchen" };
  const card = document.createElement("chorus-room-card");
  Object.assign(card, { room: { ...room, source: "player:p0", nowPlaying: song() }, inputs: INPUTS });
  document.body.append(card);
  await card.updateComplete;
  const playing = card.shadowRoot.querySelector("chorus-playing");
  await playing.updateComplete;
  assert.equal(text(part(playing, "[data-title]")), "Morning Light");
  const events = heard(card);
  getByLabel(card, "Play Television in Kitchen").click();
  assert.deepEqual(events, [
    { subject: "kitchen", body: '{"v":2,"t":"take","target":"kitchen","source":"line-in:endpoint-c/optical-1"}' },
  ]);
  card.room = { ...room, group: "live-1", source: null, nowPlaying: null };
  await card.updateComplete;
  assert.equal(card.shadowRoot.querySelector("chorus-playing"), null);
});

test("a formed group shows what it plays on its card; a partly formed saved group offers no choice of input", async () => {
  const group = {
    id: "downstairs",
    name: "Downstairs",
    kind: "saved",
    active: true,
    defined: [{ id: "living", name: "living" }, { id: "kitchen", name: "kitchen" }],
    rooms: [{ id: "living", name: "living" }, { id: "kitchen", name: "kitchen" }],
    volume: 600,
    source: "player:p0",
    nowPlaying: song(),
  };
  const card = document.createElement("chorus-group-card");
  Object.assign(card, { group, inputs: INPUTS });
  document.body.append(card);
  await card.updateComplete;
  const playing = () => card.shadowRoot.querySelector("chorus-playing");
  await playing().updateComplete;
  assert.equal(text(part(playing(), "[data-artist]")), "The Example Quartet");
  const events = heard(card);
  getByLabel(card, "Play Kitchen streamer in Downstairs").click();
  assert.deepEqual(events, [
    { subject: "downstairs", body: '{"v":2,"t":"take","target":"downstairs","source":"line-in:endpoint-c/line-1"}' },
  ]);
  card.group = { ...group, active: false, rooms: [{ id: "kitchen", name: "kitchen" }] };
  await card.updateComplete;
  await playing().updateComplete;
  assert.deepEqual(queryAllByLabel(card, "Play Kitchen streamer in Downstairs"), []);
  assert.equal(text(part(playing(), "[data-title]")), "Morning Light");
  // Not formed: nothing is playing in it, and nothing is said.
  card.group = { ...group, active: false, rooms: [], volume: null, source: null, nowPlaying: null };
  await card.updateComplete;
  assert.equal(playing(), null);
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-playing").styles.cssText;
  assert.match(sheet, /var\(--/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
  const tokens = readFileSync(new URL("../src/tokens.css", import.meta.url), "utf8");
  for (const [, name] of sheet.matchAll(/var\((--[a-z0-9-]+)\)/g)) {
    assert.ok(tokens.includes(`${name}:`), `${name} is a token of tokens.css`);
  }
});
