// Keeping the kiosk's screen on: the Screen Wake Lock is asked for while the
// page is visible and asked for again when it becomes visible again, and
// where there is none, or the browser refuses, nothing throws.

import assert from "node:assert/strict";
import { test } from "node:test";

import { keepAwake } from "../src/wake-lock.js";

// A document with a visibility the test sets, as the browser does.
function fakeDocument(visibilityState = "visible") {
  const listeners = new Set();
  return {
    visibilityState,
    addEventListener: (type, listener) => type === "visibilitychange" && listeners.add(listener),
    removeEventListener: (type, listener) => type === "visibilitychange" && listeners.delete(listener),
    listening: () => listeners.size,
    show() {
      this.visibilityState = "visible";
      for (const listener of [...listeners]) listener();
    },
    // Hidden: the browser first takes back every wake lock of the page.
    hide(wakeLock) {
      this.visibilityState = "hidden";
      wakeLock?.takeBack();
      for (const listener of [...listeners]) listener();
    },
  };
}

// navigator.wakeLock: every request is recorded and answered with a sentinel
// that says when it is released, or refused when `refuse` is set.
function fakeWakeLock() {
  const sentinels = [];
  return {
    requests: [],
    refuse: null,
    sentinels,
    async request(type) {
      this.requests.push(type);
      if (this.refuse) throw this.refuse;
      const listeners = new Set();
      const sentinel = {
        released: false,
        type,
        addEventListener: (name, listener) => name === "release" && listeners.add(listener),
        async release() {
          if (this.released) return;
          this.released = true;
          for (const listener of [...listeners]) listener();
        },
      };
      sentinels.push(sentinel);
      return sentinel;
    },
    takeBack() {
      for (const sentinel of sentinels) sentinel.release();
    },
  };
}

test("kiosk mode degrades without error where Wake Lock is absent", async () => {
  const document = fakeDocument();
  for (const navigator of [undefined, null, {}, { wakeLock: undefined }, { wakeLock: {} }]) {
    const awake = keepAwake({ navigator, document });
    assert.equal(awake.supported, false);
    assert.equal(awake.held(), false);
    await awake.settled();
    document.hide();
    document.show();
    await awake.stop();
  }
  assert.equal(document.listening(), 0, "with nothing to ask for, nothing listens");

  // A navigator whose property throws, and no document at all.
  const hostile = Object.defineProperty({}, "wakeLock", {
    get() {
      throw new Error("no wake lock here");
    },
  });
  assert.equal(keepAwake({ navigator: hostile, document }).supported, false);
  assert.equal(keepAwake({ navigator: { wakeLock: fakeWakeLock() }, document: null }).supported, false);
});

test("a visible page asks for a screen wake lock once and holds it", async () => {
  const wakeLock = fakeWakeLock();
  const awake = keepAwake({ navigator: { wakeLock }, document: fakeDocument() });
  assert.equal(awake.supported, true);
  await awake.settled();
  assert.deepEqual(wakeLock.requests, ["screen"]);
  assert.equal(awake.held(), true);
});

test("the wake lock the browser took back while the page was hidden is asked for again when the page is visible again", async () => {
  const wakeLock = fakeWakeLock();
  const document = fakeDocument();
  const awake = keepAwake({ navigator: { wakeLock }, document });
  await awake.settled();

  document.hide(wakeLock);
  await awake.settled();
  assert.equal(awake.held(), false);
  assert.deepEqual(wakeLock.requests, ["screen"], "a hidden page asks for nothing");

  document.show();
  await awake.settled();
  assert.deepEqual(wakeLock.requests, ["screen", "screen"]);
  assert.equal(awake.held(), true);

  // Visible again with the lock still held: it is not asked for twice.
  document.show();
  await awake.settled();
  assert.deepEqual(wakeLock.requests, ["screen", "screen"]);
});

test("a page that starts hidden asks when it first becomes visible", async () => {
  const wakeLock = fakeWakeLock();
  const document = fakeDocument("hidden");
  const awake = keepAwake({ navigator: { wakeLock }, document });
  await awake.settled();
  assert.deepEqual(wakeLock.requests, []);
  document.show();
  await awake.settled();
  assert.deepEqual(wakeLock.requests, ["screen"]);
});

test("a refused request does not throw, and the next time the page is visible it is asked again", async () => {
  const wakeLock = fakeWakeLock();
  wakeLock.refuse = new DOMException("the battery is low", "NotAllowedError");
  const document = fakeDocument();
  const awake = keepAwake({ navigator: { wakeLock }, document });
  await awake.settled();
  assert.equal(awake.held(), false);
  assert.deepEqual(wakeLock.requests, ["screen"]);

  wakeLock.refuse = null;
  document.hide(wakeLock);
  document.show();
  await awake.settled();
  assert.deepEqual(wakeLock.requests, ["screen", "screen"]);
  assert.equal(awake.held(), true);
});

test("two visibility changes in a row make one request, not two", async () => {
  const wakeLock = fakeWakeLock();
  const document = fakeDocument("hidden");
  const awake = keepAwake({ navigator: { wakeLock }, document });
  document.show();
  document.show();
  await awake.settled();
  assert.deepEqual(wakeLock.requests, ["screen"]);
});

test("stopped, it gives the wake lock back and asks no more", async () => {
  const wakeLock = fakeWakeLock();
  const document = fakeDocument();
  const awake = keepAwake({ navigator: { wakeLock }, document });
  await awake.settled();
  await awake.stop();
  assert.equal(wakeLock.sentinels[0].released, true);
  assert.equal(awake.held(), false);
  assert.equal(document.listening(), 0);
  document.show();
  await awake.settled();
  assert.deepEqual(wakeLock.requests, ["screen"]);
});

test("stopped while a request is still unanswered, the lock it is then given is given back", async () => {
  const wakeLock = fakeWakeLock();
  const awake = keepAwake({ navigator: { wakeLock }, document: fakeDocument() });
  await awake.stop();
  assert.deepEqual(wakeLock.requests, ["screen"]);
  assert.equal(wakeLock.sentinels[0].released, true);
  assert.equal(awake.held(), false);
});
