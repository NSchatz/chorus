// The live test of a room's volume limit and quiet hours (`make web-live`):
// the app's own elements, navigation and state layer, in node under happy-dom
// with no browser, against a real chorus-server whose civil clock is held
// (`--civil-time`), so whether a window is active is the server's own answer
// and not the hour this test happens to run at.
//
// The screen is opened the way a person opens it, from the room's card.
// Everything is then set through the screen and read back from the server's
// own `GET /api/state`: the limit, a window that covers the held time (which
// the server says is active, and which lowers `effective_limit`), the switch,
// and a volume asked for above the limit, which the page shows as the server
// clamps it.
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

const BEDROOM = { id: "bedroom", name: "Live Test Bedroom" };
const DEN = { id: "den" };
// The server's civil clock, held: a Wednesday, half past eleven at night.
const HELD = "wed-23:30";
const WEEK = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

let house;
let store;
let app;

const otherClient = (message) => house.command(message);
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const control = (label) => queryAllByLabel(app, `${label} for ${BEDROOM.name}`)[0] ?? null;
const screen = () => app.shadowRoot.querySelector("chorus-room-limits");
const value = (name) => {
  const node = screen()?.shadowRoot.querySelector(`[data-value="${name}"]`);
  return node ? text(node) : null;
};
const items = () => [...(screen()?.shadowRoot.querySelectorAll("li[data-window]") ?? [])];
const activeWords = () => items().map((item) => text(item.querySelector('[data-value="active"]')));

// What the server's own state says of the room's limits.
const serverRoom = async (id = BEDROOM.id) => {
  const { volume, limit, effective_limit, quiet, quiet_enabled } = (await house.state()).zones.find((zone) => zone.id === id);
  return { volume, limit, effective_limit, quiet, quiet_enabled };
};

// A person moves a slider to `to` and lets go of it.
function slide(slider, to) {
  slider.focus();
  slider.value = String(to);
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  slider.blur();
}

// A person sets a time field.
function setTime(field, to) {
  field.focus();
  field.value = to;
  field.dispatchEvent(new Event("change", { bubbles: true }));
  field.blur();
}

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  house = await startHouse([BEDROOM.id, DEN.id], { extra: ["--civil-time", HELD] });
  await otherClient(JSON.stringify({ v: 1, t: "name", zone: BEDROOM.id, name: BEDROOM.name }));
  await otherClient('{"v":1,"t":"volume","zone":"bedroom","volume":0.800}');

  // The app, as main.js makes it, reading that server.
  store = createStore(createClient({ base: `${house.origin}/` }));
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

test("the limits screen opens from the room's card and shows what a real server holds", async () => {
  await until("the link on the bedroom's card", () => queryAllByLabel(app, `Limits for ${BEDROOM.name}`).length, 1);
  assert.deepEqual(await serverRoom(), { volume: 0.8, limit: 1, effective_limit: 1, quiet: [], quiet_enabled: true });
  getByLabel(app, `Limits for ${BEDROOM.name}`).click();
  await until("the limit slider", () => control("Volume limit")?.value ?? null, "1000");
  assert.equal(getByLabel(app, `Volume limits of ${BEDROOM.name}`).localName, "main");
  assert.equal(location.hash, "#/rooms/bedroom/limits", "the screen has an address of its own");
  assert.deepEqual([value("limit"), value("effective"), value("volume"), value("enabled")], ["100%", "100%", "80%", "On"]);
  assert.deepEqual(items(), []);
  await until("the store's status", () => store.view().status, "live");
});

test("a limit set through the screen is the server's, and it pulls the room's volume down to it", async () => {
  slide(control("Volume limit"), 600);
  await until("the server's limit", async () => {
    const { volume, limit, effective_limit } = await serverRoom();
    return { volume, limit, effective_limit };
  }, { volume: 0.6, limit: 0.6, effective_limit: 0.6 }); // prettier-ignore
  await until("the screen", () => [control("Volume limit").value, value("limit"), value("effective"), value("volume")], ["600", "60%", "60%", "60%"]); // prettier-ignore
  assert.equal((await serverRoom(DEN.id)).limit, 1, "the other room was not touched");
});

test("a window set through the screen that covers the held time shows active and lowers effective_limit", async () => {
  // The draft as it stands: every day, 22:00 to 07:00, a quarter of full
  // scale. The held clock, Wednesday 23:30, is inside it.
  assert.deepEqual(
    [control("Start of the new window").value, control("End of the new window").value, control("Limit of the new window").value],
    ["22:00", "07:00", "250"],
  );
  control("Add window").click();
  await until("the server's room", () => serverRoom(), {
    volume: 0.25,
    limit: 0.6,
    effective_limit: 0.25,
    quiet: [{ days: WEEK, start: "22:00", end: "07:00", limit: 0.25, active: true }],
    quiet_enabled: true,
  });
  await until("the screen", () => [activeWords(), value("limit"), value("effective"), value("volume")], [["Active now"], "60%", "25%", "25%"]); // prettier-ignore
  assert.equal(items()[0].hasAttribute("data-active"), true);

  // A second window, of a day the held clock is not in: listed, not active.
  for (const day of ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Sunday"]) {
    control(`${day}, the new window`).click();
  }
  setTime(control("Start of the new window"), "13:00");
  setTime(control("End of the new window"), "15:00");
  await until("the draft", () => control("Start of the new window").value, "13:00");
  control("Add window").click();
  await until("the server's windows", async () => (await serverRoom()).quiet.map((window) => [window.days, window.start, window.active]), [
    [WEEK, "22:00", true],
    [["sat"], "13:00", false],
  ]); // prettier-ignore
  await until("the screen", () => activeWords(), ["Active now", "Not active now"]);
  assert.equal((await serverRoom()).effective_limit, 0.25);
});

test("a volume above the limit is shown as the server clamps it", async () => {
  // Another client asks for more than the window allows.
  await otherClient('{"v":1,"t":"volume","zone":"bedroom","volume":0.900}');
  assert.equal((await serverRoom()).volume, 0.25, "the server clamped it");
  await until("the volume on the limits screen", () => value("volume"), "25%");

  // And a person does, on the room's card: the slider comes back to the clamp.
  getByLabel(app, "Back to rooms").click();
  await until("the room's card", () => queryAllByLabel(app, `Volume for ${BEDROOM.name}`).length, 1);
  const volume = getByLabel(app, `Volume for ${BEDROOM.name}`);
  await until("the card's volume", () => volume.value, "250");
  const before = (await house.state()).serial;
  slide(volume, 900);
  await until("the server's answer", async () => (await house.state()).serial > before, true);
  assert.equal((await serverRoom()).volume, 0.25);
  await until("the card's volume after asking for 90%", () => volume.value, "250");
  assert.equal(volume.getAttribute("aria-valuetext"), "25%");

  getByLabel(app, `Limits for ${BEDROOM.name}`).click();
  await until("the limits screen again", () => value("volume"), "25%");
});

test("editing the window so it no longer covers the held time ends it, and the switch turns the cap off and on", async () => {
  // From a quarter to midnight: the held 23:30 is before it.
  setTime(control("Start of window 1"), "23:45");
  await until("the server's window", async () => {
    const { quiet, effective_limit } = await serverRoom();
    return [quiet[0].start, quiet[0].active, effective_limit];
  }, ["23:45", false, 0.6]); // prettier-ignore
  await until("the screen", () => [activeWords()[0], value("effective")], ["Not active now", "60%"]);
  assert.equal((await serverRoom()).volume, 0.25, "a window ending raises nothing");

  // Its cap, a day off and its end, each through the screen.
  slide(control("Limit of window 1"), 200);
  control("Sunday, window 1").click();
  await until("the server's window", async () => {
    const [{ days, limit }] = (await serverRoom()).quiet;
    return [days, limit];
  }, [["mon", "tue", "wed", "thu", "fri", "sat"], 0.2]); // prettier-ignore
  setTime(control("Start of window 1"), "23:00");
  await until("the server's room", async () => {
    const { quiet, effective_limit, volume } = await serverRoom();
    return [quiet[0].active, effective_limit, volume];
  }, [true, 0.2, 0.2]); // prettier-ignore
  await until("the screen", () => [activeWords()[0], value("effective"), value("volume")], ["Active now", "20%", "20%"]);

  // Switched off: the window is kept and the clock is still inside it, but
  // it caps nothing.
  await until("the switch", () => control("Quiet hours").getAttribute("aria-pressed"), "true");
  control("Quiet hours").click();
  await until("the server's room", async () => {
    const { quiet, quiet_enabled, effective_limit } = await serverRoom();
    return [quiet.length, quiet[0].active, quiet_enabled, effective_limit];
  }, [2, true, false, 0.6]); // prettier-ignore
  await until("the screen", () => [value("enabled"), activeWords()[0], value("effective")], ["Off", "Inside it now, and quiet hours are off", "60%"]); // prettier-ignore
  control("Quiet hours").click();
  await until("the server's room", async () => {
    const { quiet_enabled, effective_limit } = await serverRoom();
    return [quiet_enabled, effective_limit];
  }, [true, 0.2]); // prettier-ignore
  await until("the screen", () => [value("enabled"), activeWords()[0], value("effective")], ["On", "Active now", "20%"]);
});

test("a window the real server refuses is shown with its field and its words, and removing the windows leaves none", async () => {
  // A window that starts and ends at the same minute is the server's to refuse.
  setTime(control("End of window 2"), "13:00");
  const alert = screen().shadowRoot.querySelector("[role=alert]");
  await until("the refusal's field", () => alert.getAttribute("data-refusal-field"), "windows");
  assert.match(text(alert), /^Refused \(windows\): .*13:00.*ambiguous/);
  assert.equal(control("End of window 2").value, "15:00", "the field is the server's value again");
  assert.equal((await serverRoom()).quiet[1].end, "15:00", "nothing of it was applied");

  control("Remove window 1").click();
  await until("the server's windows", async () => (await serverRoom()).quiet.map((window) => window.start), ["13:00"]);
  await until("the screen", () => items().length, 1);
  assert.equal(text(alert), "", "an accepted command clears the refusal");
  control("Remove window 1").click();
  await until("the server's windows", async () => (await serverRoom()).quiet, []);
  await until("the screen", () => [items().length, value("effective")], [0, "60%"]);
});

test("a change a second client makes appears on the screen with no reload", async () => {
  const limit = control("Volume limit");
  await otherClient('{"v":2,"t":"limit","zone":"bedroom","limit":0.900}');
  await otherClient('{"v":2,"t":"quiet_hours","zone":"bedroom","windows":[{"days":["wed"],"start":"23:00","end":"23:59","limit":0.100}]}');
  await otherClient('{"v":2,"t":"quiet_hours_enabled","zone":"bedroom","enabled":false}');
  await until("the screen", () => [limit.value, value("effective"), value("enabled"), activeWords()], ["900", "90%", "Off", ["Inside it now, and quiet hours are off"]]); // prettier-ignore
  assert.equal(control("Volume limit"), limit, "the page was patched, not loaded again");
  assert.equal(control("Start of window 1").value, "23:00");
});
