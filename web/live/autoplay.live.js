// The live test of the autoplay screen (`make web-live`): the app's own
// elements, navigation and state layer, in node under happy-dom with no
// browser, against a real chorus-server.
//
// The input is a real one: an endpoint's session offers its line-in
// (endpoint.js), so the server's state lists an input no flag could put
// there. The screen is opened from the home, the input's rule is made and
// changed through the screen, and each step is read back from the server's
// own `GET /api/state`.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run this file without it.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { offerLineIn } from "./endpoint.js";
import { startHouse, until } from "./house.js";

const ROOMS = ["kitchen", "den", "study"];
const GROUP = { id: "downstairs", name: "Live Test Downstairs" };
// The endpoint with something wired to its line-in.
const ENDPOINT = "live-test-amp";
const INPUT = `${ENDPOINT}/line-1`;

let house;
let endpoint;
let store;
let app;

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const screen = () => app.shadowRoot.querySelector("chorus-autoplay");
const rows = () => [...(screen()?.shadowRoot.querySelectorAll("li[data-input]") ?? [])].map((row) => row.dataset.input);
const toggle = () => queryAllByLabel(app, `Autoplay for ${INPUT}`)[0] ?? null;
const picker = () => queryAllByLabel(app, `Autoplay target for ${INPUT}`)[0] ?? null;
// What the screen shows of the input's rule: its target and whether it is on.
const shown = () => (picker() && toggle() ? [picker().value, toggle().getAttribute("aria-pressed") === "true"] : null);
const serverRules = async () => (await house.state()).autoplay;

// A person chooses a target from the list.
function choose(target) {
  const list = picker();
  list.value = target;
  list.dispatchEvent(new Event("change", { bubbles: true }));
}

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  // `--slots`: a server lists an endpoint's line-in only when it has slots
  // to play one in (as live/playing.live.js starts it).
  house = await startHouse(ROOMS, { extra: ["--slots", "4"] });
  await house.command(JSON.stringify({ v: 2, t: "group_save", group: GROUP.id, name: GROUP.name, zones: ["kitchen", "den"] }));

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
});

test("the autoplay screen opens from the home and lists the input a real endpoint offers", async () => {
  await until("the link on the home", () => queryAllByLabel(app, "Autoplay rules").length, 1);
  getByLabel(app, "Autoplay rules").click();
  await until("the autoplay screen", () => Boolean(screen()), true);
  assert.equal(location.hash, "#/autoplay", "the screen has an address of its own");
  assert.equal(getByLabel(app, "Autoplay").localName, "main");
  assert.deepEqual((await house.state()).inputs, []);
  await until("the screen with no input", () => rows(), []);
  await until("the store's status", () => store.view().status, "live");

  const [, port] = await house.said(/chorus-server: listening on=\S*?:(\d+)/);
  endpoint = await offerLineIn({ host: "127.0.0.1", port: Number(port), endpoint: ENDPOINT });
  await until("the inputs the server offers", async () => (await house.state()).inputs, [INPUT]);
  await until("the screen's inputs", () => rows(), [INPUT]);
  assert.deepEqual(await serverRules(), [], "the input has no rule yet");
  assert.deepEqual(shown(), ["", false]);
  assert.equal(toggle().disabled, true, "a rule with no target cannot be switched on");
  // The targets are the server's rooms and its saved group, by their names.
  assert.deepEqual(
    [...picker().querySelectorAll("optgroup")].map((group) => [group.label, [...group.querySelectorAll("option")].map((option) => [option.value, text(option)])]),
    [
      ["Rooms", ROOMS.map((room) => [room, room])],
      ["Saved groups", [[GROUP.id, GROUP.name]]],
    ],
  ); // prettier-ignore
});

test("a rule set through the screen is read back from the server's /api/state", async () => {
  // A target makes the rule, switched off.
  choose("study");
  await until("the server's rule", () => serverRules(), [{ input: INPUT, target: "study", enabled: false }]);
  await until("the screen", () => shown(), ["study", false]);

  // The switch turns it on.
  toggle().click();
  await until("the server's rule", () => serverRules(), [{ input: INPUT, target: "study", enabled: true }]);
  await until("the screen", () => shown(), ["study", true]);

  // A saved group as its target, with the switch as it was.
  choose(GROUP.id);
  await until("the server's rule", () => serverRules(), [{ input: INPUT, target: GROUP.id, enabled: true }]);
  await until("the screen", () => shown(), [GROUP.id, true]);

  // And off again.
  toggle().click();
  await until("the server's rule", () => serverRules(), [{ input: INPUT, target: GROUP.id, enabled: false }]);
  await until("the screen", () => shown(), [GROUP.id, false]);
  const alert = screen().shadowRoot.querySelector("li[data-input] [role=alert]");
  assert.equal(text(alert), "", "nothing was refused");
});

test("a rule a second client sets appears on the screen, and the rule outlives its input's offer", async () => {
  const list = picker();
  await house.command(JSON.stringify({ v: 2, t: "autoplay", input: INPUT, target: "den", enabled: true }));
  await until("the screen", () => shown(), ["den", true]);
  assert.equal(picker(), list, "the page was patched, not loaded again");

  // The endpoint goes: the server offers no input, and keeps the rule.
  await endpoint.stop();
  endpoint = null;
  await until("the inputs the server offers", async () => (await house.state()).inputs, []);
  assert.deepEqual(await serverRules(), [{ input: INPUT, target: "den", enabled: true }]);
  await until(
    "the screen says the input is not offered now",
    () => Boolean(screen().shadowRoot.querySelector(`li[data-input="${INPUT}"] [data-absent]`)),
    true,
  );
  assert.deepEqual(shown(), ["den", true]);
});
