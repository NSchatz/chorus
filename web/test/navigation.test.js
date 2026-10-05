// The navigation to the app's further screens (src/routes.js): a screen has
// an address of its own in the fragment, opening one is an entry of the
// browser's history, and the browser's back button returns from it. First
// the addresses as pure logic and the navigation over a scripted history,
// then the shell (chorus-app) following happy-dom's own address and history,
// which model a browser's: a pushed entry, `history.back()` and `popstate`.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import { html } from "lit";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { HOME, HOME_ADDRESS, addressOf, createNavigation, registerScreen, routeOf, screenOf } from "../src/routes.js";
import { SOUND_SCREEN } from "../src/sound.js";
import { createStore } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

// A browser's address and session history, as much of them as the navigation
// uses: entries of { hash, state }, `pushState`, `replaceState`, and a `back`
// that says so with `popstate` afterwards, as a browser does.
function fakeBrowser(hash = "") {
  const entries = [{ hash, state: null }];
  let at = 0;
  const listeners = new Map();
  const fire = (type) => {
    for (const listener of [...(listeners.get(type) ?? [])]) listener({ type });
  };
  const hashOf = (address) => (address === "#" ? "" : address);
  const host = {
    location: {
      get hash() {
        return entries[at].hash;
      },
    },
    history: {
      get state() {
        return entries[at].state;
      },
      get length() {
        return entries.length;
      },
      pushState(state, _title, address) {
        entries.splice(at + 1, entries.length, { hash: hashOf(address), state });
        at += 1;
      },
      replaceState(state, _title, address) {
        entries[at] = { hash: hashOf(address), state };
      },
      back() {
        if (at === 0) return;
        at -= 1;
        fire("popstate");
      },
      forward() {
        if (at === entries.length - 1) return;
        at += 1;
        fire("popstate");
      },
    },
    addEventListener(type, listener) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(listener);
    },
    removeEventListener(type, listener) {
      listeners.get(type)?.delete(listener);
    },
    // A person edits the address by hand: a new entry, and `hashchange`.
    type(address) {
      entries.splice(at + 1, entries.length, { hash: address, state: null });
      at += 1;
      fire("hashchange");
    },
    entries: () => entries.map((entry) => entry.hash),
    listening: () => [...listeners.values()].reduce((count, set) => count + set.size, 0),
  };
  return host;
}

const SOUND_OF_LIVING = "#/rooms/living/sound";

test("a screen's address is #/ and its path, and a parameter is written so any id survives it", () => {
  assert.equal(HOME_ADDRESS, "#/");
  assert.equal(addressOf(SOUND_SCREEN, { room: "living" }), SOUND_OF_LIVING);
  assert.equal(addressOf(SOUND_SCREEN, { room: "tv room/2" }), "#/rooms/tv%20room%2F2/sound");
  assert.throws(() => addressOf(SOUND_SCREEN, {}), /needs 'room'/);
  assert.throws(() => addressOf("cinema"), /no screen 'cinema'/);
});

test("a fragment names its screen and parameters, and anything else is the home", () => {
  assert.deepEqual(routeOf(SOUND_OF_LIVING), { screen: SOUND_SCREEN, params: { room: "living" }, address: SOUND_OF_LIVING });
  assert.deepEqual(routeOf("/rooms/living/sound/").params, { room: "living" }, "with or without the # and a last /");
  assert.deepEqual(routeOf("#/rooms/tv%20room%2F2/sound").params, { room: "tv room/2" });
  for (const home of ["", "#", "#/", "#/rooms", "#/rooms/living", "#/rooms/living/sound/more", "#/cinema", "#/rooms/%E0%A4%A/sound", null]) {
    assert.equal(routeOf(home), HOME, `${home} is the home`);
  }
  assert.deepEqual({ ...HOME }, { screen: "home", params: {}, address: "#/" });
});

test("a later screen registers with a path, house-wide or of a room, and is then an address like the first", () => {
  registerScreen({ id: "test-alarms", path: "test-alarms", title: () => "Alarms", render: () => html`<p data-test-alarms>None</p>` });
  registerScreen({
    id: "test-room-settings",
    path: "rooms/:room/test-settings",
    title: ({ room }) => `Settings of ${room}`,
    render: ({ room }) => html`<p data-test-settings>${room}</p>`,
  });
  assert.equal(addressOf("test-alarms"), "#/test-alarms");
  assert.deepEqual(routeOf("#/test-alarms"), { screen: "test-alarms", params: {}, address: "#/test-alarms" });
  assert.deepEqual(routeOf("#/rooms/den/test-settings").params, { room: "den" });
  assert.equal(routeOf(SOUND_OF_LIVING).screen, SOUND_SCREEN, "the first screen is where it was");
  assert.equal(screenOf("test-alarms").title({}, {}), "Alarms");
  assert.equal(screenOf("cinema"), null);

  // What cannot be told apart is refused when it is registered, not found out later.
  const screen = { title: () => "", render: () => html`` };
  assert.throws(() => registerScreen({ ...screen, id: "test-alarms", path: "other" }), /registered twice/);
  assert.throws(() => registerScreen({ ...screen, id: "other", path: "rooms/:id/sound" }), /the same path/);
  assert.throws(() => registerScreen({ ...screen, id: "home", path: "home" }), /not 'home'/);
  assert.throws(() => registerScreen({ ...screen, id: "other", path: "" }), /has a path/);
  assert.throws(() => registerScreen({ id: "other", path: "other" }), /a title and a render/);
});

test("opening a screen is a new entry of the history, and back from it is one step back", () => {
  const browser = fakeBrowser();
  const navigation = createNavigation(browser);
  const seen = [];
  const stop = navigation.watch((route) => seen.push(route.address));
  assert.deepEqual(seen, ["#/"], "the route now, at once");

  navigation.open(SOUND_OF_LIVING);
  assert.deepEqual(browser.entries(), ["", SOUND_OF_LIVING]);
  assert.deepEqual(seen, ["#/", SOUND_OF_LIVING]);
  navigation.open(SOUND_OF_LIVING);
  assert.equal(browser.history.length, 2, "opening the screen that is open adds nothing");

  // The browser's own back button.
  browser.history.back();
  assert.deepEqual(seen, ["#/", SOUND_OF_LIVING, "#/"]);
  // And its forward button: the entry still carries the app's mark, so the
  // app's "Back" is the same one step.
  browser.history.forward();
  navigation.back();
  assert.deepEqual(seen, ["#/", SOUND_OF_LIVING, "#/", SOUND_OF_LIVING, "#/"]);
  assert.deepEqual(browser.entries(), ["", SOUND_OF_LIVING], "nothing was added on the way back");
  navigation.back();
  assert.equal(seen.length, 5, "back at the home does nothing");

  stop();
  assert.equal(browser.listening(), 0);
  browser.type(SOUND_OF_LIVING);
  assert.equal(seen.length, 5, "a stopped watcher hears nothing");
});

test("back from a screen the app was loaded at, or one typed, puts the home in its place and never leaves the app", () => {
  const loaded = fakeBrowser(SOUND_OF_LIVING);
  const navigation = createNavigation(loaded);
  const seen = [];
  navigation.watch((route) => seen.push(route.address));
  assert.deepEqual(seen, [SOUND_OF_LIVING]);
  navigation.back();
  assert.deepEqual(loaded.entries(), ["#/"], "the one entry is now the home");
  assert.deepEqual(seen, [SOUND_OF_LIVING, "#/"]);

  const typed = fakeBrowser();
  const second = createNavigation(typed);
  const heard = [];
  second.watch((route) => heard.push(route.address));
  typed.type(SOUND_OF_LIVING);
  assert.deepEqual(heard, ["#/", SOUND_OF_LIVING], "an address edited by hand is followed");
  second.back();
  assert.deepEqual(typed.entries(), ["", "#/"]);
});

// The shell, over a scripted server and happy-dom's address and history.

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
});

const HOUSE = () =>
  stateOf(1, [
    zone("living", { name: "Living Room", sound: { bass: 3, treble: -2, loudness: true, night: false, speech: false } }),
    zone("den", { sound: { bass: 0, treble: 0, loudness: true, night: false, speech: false } }),
  ]);

async function rendered(app) {
  await settle();
  await app.updateComplete;
  for (const element of app.shadowRoot.querySelectorAll("chorus-rooms, chorus-groups, chorus-room-sound")) {
    await element.updateComplete;
    await Promise.all([...element.shadowRoot.querySelectorAll("chorus-room-card")].map((card) => card.updateComplete));
  }
}

async function mountOver(server, { mode = "app" } = {}) {
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.mode = mode;
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

const screenShown = (app) => app.shadowRoot.querySelector("main").dataset.screen ?? "home";

test("a room's sound screen opens from the room's card at an address of its own, and the browser's back button returns to the rooms", async () => {
  const app = await mountOver(fakeServer(HOUSE()));
  assert.equal(screenShown(app), "home");
  const entries = history.length;

  const link = getByLabel(app, "Sound for Living Room");
  assert.equal(link.localName, "a");
  assert.equal(link.getAttribute("href"), SOUND_OF_LIVING, "the link holds the screen's address");
  link.click();
  await rendered(app);

  assert.equal(location.hash, SOUND_OF_LIVING);
  assert.equal(history.length, entries + 1, "one new entry of the history");
  assert.equal(screenShown(app), SOUND_SCREEN);
  const screen = getByLabel(app, "Sound of Living Room");
  assert.equal(screen.localName, "main");
  assert.equal(app.shadowRoot.activeElement, screen, "the screen's region has the focus");
  assert.equal(getByLabel(app, "Bass for Living Room").value, "3");
  // The screen is alone: the groups and the rooms are not painted under it.
  assert.equal(app.shadowRoot.querySelector("chorus-rooms"), null);
  assert.equal(app.shadowRoot.querySelector("chorus-groups"), null);
  assert.deepEqual(queryAllByLabel(app, "Volume for Living Room"), []);

  // The browser's back button.
  history.back();
  await rendered(app);
  assert.equal(routeOf(location.hash), HOME);
  assert.equal(screenShown(app), "home");
  assert.equal(getByLabel(app, "Rooms").localName, "main");
  assert.equal(getByLabel(app, "Volume for Living Room").value, "500");
  assert.equal(app.shadowRoot.activeElement, getByLabel(app, "Rooms"));

  // And its forward button returns to the screen.
  history.forward();
  await rendered(app);
  assert.equal(screenShown(app), SOUND_SCREEN);
  assert.equal(getByLabel(app, "Treble for Living Room").value, "-2");
});

test("the screen's own Back link is the same one step back, and adds no entry", async () => {
  const app = await mountOver(fakeServer(HOUSE()));
  getByLabel(app, "Sound for den").click();
  await rendered(app);
  const entries = history.length;
  assert.equal(screenShown(app), SOUND_SCREEN);

  const back = getByLabel(app, "Back to rooms");
  assert.equal(back.localName, "a");
  assert.equal(back.getAttribute("href"), "#/");
  back.click();
  await rendered(app);
  assert.equal(screenShown(app), "home");
  assert.equal(history.length, entries);
  assert.equal(routeOf(location.hash), HOME);
});

test("an app loaded at a screen's address shows that screen, and Back from it is the home in its place", async () => {
  history.replaceState(null, "", "/app/?kiosk#/rooms/den/sound");
  const app = await mountOver(fakeServer(HOUSE()));
  const entries = history.length;
  assert.equal(screenShown(app), SOUND_SCREEN);
  assert.equal(getByLabel(app, "Sound of den").localName, "main");

  getByLabel(app, "Back to rooms").click();
  await rendered(app);
  assert.equal(screenShown(app), "home");
  assert.equal(history.length, entries, "the entry was replaced, not left");
  assert.equal(location.pathname + location.search + location.hash, "/app/?kiosk#/", "the kiosk's switch is kept");
});

test("an address edited by hand is followed, and one that names no screen is the home", async () => {
  const app = await mountOver(fakeServer(HOUSE()));
  location.hash = SOUND_OF_LIVING;
  await rendered(app);
  assert.equal(screenShown(app), SOUND_SCREEN);
  location.hash = "#/cinema";
  await rendered(app);
  assert.equal(screenShown(app), "home");
});

test("a navigation button leaves a further screen for the region it names", async () => {
  const app = await mountOver(fakeServer(HOUSE()));
  getByLabel(app, "Sound for den").click();
  await rendered(app);
  getByLabel(app, "Go to groups").click();
  await rendered(app);
  assert.equal(screenShown(app), "home");
  assert.equal(app.shadowRoot.activeElement, getByLabel(app, "Groups"));

  getByLabel(app, "Sound for den").click();
  await rendered(app);
  getByLabel(app, "Go to rooms").click();
  await rendered(app);
  assert.equal(app.shadowRoot.activeElement, getByLabel(app, "Rooms"));
});

test("the screen opens and returns the same way in the desktop layout and in the kiosk, where the way back is in the screen's region", async () => {
  const app = await mountOver(fakeServer(HOUSE()), { mode: "kiosk" });
  app.layout = "desktop";
  await rendered(app);
  assert.equal(app.getAttribute("mode"), "kiosk");
  assert.equal(app.getAttribute("layout"), "desktop");

  getByLabel(app, "Sound for den").click();
  await rendered(app);
  const screen = getByLabel(app, "Sound of den");
  // A wide kiosk paints no header: the link back is inside the screen.
  assert.equal(getByLabel(app, "Back to rooms").closest("main"), screen);
  const sheet = customElements.get("chorus-app").styles.cssText;
  assert.match(sheet, /:host\(\[layout="desktop"\]\) main\[data-screen\] \{[^}]*grid-column: 1 \/ -1/);
  assert.match(sheet, /a\[data-route\] \{[^}]*min-height: var\(--control-size\)/);
  assert.match(customElements.get("chorus-room-card").styles.cssText, /\ba \{[^}]*min-height: var\(--control-size\)/);

  getByLabel(app, "Back to rooms").click();
  await rendered(app);
  assert.equal(screenShown(app), "home");
});

test("a click with a modifier is left to the browser, which opens the link's own address", async () => {
  const app = await mountOver(fakeServer(HOUSE()));
  const link = getByLabel(app, "Sound for den");
  const click = new MouseEvent("click", { bubbles: true, composed: true, cancelable: true, ctrlKey: true });
  // The app's own listener was added first, so this one sees what it decided.
  let prevented = null;
  app.addEventListener("click", (event) => (prevented = event.defaultPrevented), { once: true });
  link.dispatchEvent(click);
  assert.equal(prevented, false);
});

test("an app taken off the page stops following the address", async () => {
  const app = await mountOver(fakeServer(HOUSE()));
  app.remove();
  location.hash = SOUND_OF_LIVING;
  await settle();
  await app.updateComplete;
  assert.equal(screenShown(app), "home");
});
