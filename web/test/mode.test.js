// The mode a query string asks for, and the kiosk choice kept in storage.

import assert from "node:assert/strict";
import { test } from "node:test";

import { KIOSK_KEY, MODES, displayMode, kioskSwitch, resolveMode, storageOf } from "../src/mode.js";

test("a bare or valued kiosk parameter asks for the kiosk layout", () => {
  for (const search of ["?kiosk", "kiosk", "?kiosk=1", "?room=den&kiosk", "?kiosk=true"]) {
    assert.equal(displayMode(search), "kiosk", search);
  }
});

test("no kiosk parameter, or one switched off, is the ordinary layout", () => {
  for (const search of ["", "?", "?room=den", "?kiosk=0", "?kiosk=false", "?kiosks"]) {
    assert.equal(displayMode(search), "app", search);
  }
});

test("every layout displayMode returns is one of MODES", () => {
  assert.deepEqual([...MODES], ["app", "kiosk"]);
  assert.ok(MODES.includes(displayMode("?kiosk")));
  assert.ok(MODES.includes(displayMode("")));
});

// A Storage that holds what it is given.
function fakeStorage(held = {}) {
  const items = new Map(Object.entries(held));
  return {
    items,
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => items.set(key, String(value)),
    removeItem: (key) => items.delete(key),
  };
}

test("a query string switches the kiosk on, switches it off, or says nothing about it", () => {
  assert.equal(kioskSwitch("?kiosk"), "kiosk");
  assert.equal(kioskSwitch("?kiosk=1"), "kiosk");
  assert.equal(kioskSwitch("?kiosk=0"), "app");
  assert.equal(kioskSwitch("?kiosk=false"), "app");
  assert.equal(kioskSwitch(""), null);
  assert.equal(kioskSwitch("?room=den"), null);
});

test("the kiosk switch is kept: opened once with ?kiosk, the app is a kiosk at its plain address too", () => {
  const storage = fakeStorage();
  assert.equal(resolveMode("", storage), "app");
  assert.equal(resolveMode("?kiosk", storage), "kiosk");
  assert.equal(storage.getItem(KIOSK_KEY), "1");
  // A reload of the same address, and the installed app's start address.
  assert.equal(resolveMode("?kiosk", storage), "kiosk");
  assert.equal(resolveMode("", storage), "kiosk");
  assert.equal(resolveMode("?room=den", storage), "kiosk");
});

test("?kiosk=0 leaves the kiosk, and that is kept too", () => {
  const storage = fakeStorage({ [KIOSK_KEY]: "1" });
  assert.equal(resolveMode("?kiosk=0", storage), "app");
  assert.equal(storage.getItem(KIOSK_KEY), null);
  assert.equal(resolveMode("", storage), "app");
  assert.equal(resolveMode("?kiosk=false", fakeStorage({ [KIOSK_KEY]: "1" })), "app");
});

test("a value in storage that is not the kiosk's is the ordinary app", () => {
  assert.equal(resolveMode("", fakeStorage({ [KIOSK_KEY]: "yes" })), "app");
  assert.equal(resolveMode("", fakeStorage({ other: "1" })), "app");
});

test("without storage, or with one that throws, the query string still decides the page", () => {
  const refusing = {
    getItem() {
      throw new Error("storage is off");
    },
    setItem() {
      throw new Error("storage is off");
    },
    removeItem() {
      throw new Error("storage is off");
    },
  };
  for (const storage of [null, undefined, refusing]) {
    assert.equal(resolveMode("?kiosk", storage), "kiosk");
    assert.equal(resolveMode("?kiosk=0", storage), "app");
    assert.equal(resolveMode("", storage), "app");
  }
});

test("storageOf gives the host's localStorage, or null where reading it throws", () => {
  const storage = fakeStorage();
  assert.equal(storageOf({ localStorage: storage }), storage);
  assert.equal(storageOf({}), null);
  const hostile = Object.defineProperty({}, "localStorage", {
    get() {
      throw new Error("denied");
    },
  });
  assert.equal(storageOf(hostile), null);
});
