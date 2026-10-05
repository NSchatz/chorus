// The installable app: the manifest and its icons as committed, the document's
// links to them, and the registration of the service worker (src/install.js).

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";

import { WORKER_SCOPE, WORKER_URL, registerServiceWorker } from "../src/install.js";

const src = (name) => new URL(`../src/${name}`, import.meta.url);
const manifest = JSON.parse(readFileSync(src("manifest.webmanifest"), "utf8"));
const page = readFileSync(src("index.html"), "utf8");

// The width and height a PNG's header declares.
function pngSize(bytes) {
  assert.deepEqual([...bytes.subarray(0, 8)], [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], "a PNG signature");
  assert.equal(bytes.subarray(12, 16).toString("latin1"), "IHDR");
  return `${bytes.readUInt32BE(16)}x${bytes.readUInt32BE(20)}`;
}

test("the manifest names the app, starts and is scoped at /app/ and opens standalone", () => {
  assert.equal(manifest.name, "chorus");
  assert.equal(manifest.short_name, "chorus");
  // The app's permanent path (docs/decisions/0182-the-app-is-served-under-app.md).
  assert.equal(manifest.start_url, "/app/");
  assert.equal(manifest.scope, "/app/");
  assert.equal(manifest.id, "/app/");
  assert.equal(manifest.display, "standalone");
  assert.match(manifest.theme_color, /^#[0-9a-f]{6}$/);
  assert.match(manifest.background_color, /^#[0-9a-f]{6}$/);
});

test("the manifest's icons are PNG files of the sizes it says, one of them maskable", () => {
  assert.ok(manifest.icons.length >= 2);
  for (const icon of manifest.icons) {
    assert.equal(icon.type, "image/png", icon.src);
    // Relative to the manifest, which is beside the document.
    assert.match(icon.src, /^icons\/icon-\d+\.png$/);
    assert.equal(pngSize(readFileSync(src(icon.src))), icon.sizes, icon.src);
  }
  const sizes = (purpose) => manifest.icons.filter((icon) => icon.purpose === purpose).map((icon) => icon.sizes);
  // The two sizes Chromium asks of an installable app.
  assert.deepEqual(sizes("any"), ["192x192", "512x512"]);
  assert.deepEqual(sizes("maskable"), ["512x512"]);
});

test("the document links the manifest with credentials, so it loads behind the login", () => {
  const link = /<link rel="manifest"[^>]*>/.exec(page)?.[0] ?? "";
  assert.match(link, / href="manifest\.webmanifest"/);
  assert.match(link, / crossorigin="use-credentials"/);
  // The icon a phone's home screen takes from the page itself.
  const touch = / rel="apple-touch-icon" href="([^"]+)"/.exec(page)?.[1];
  assert.ok(touch && existsSync(src(touch)), "the apple-touch-icon is a committed file");
  assert.equal(pngSize(readFileSync(src(touch))), "180x180");
});

test("the service worker is registered beside the document, scoped to the app's directory", async () => {
  const calls = [];
  const registration = { scope: "https://house.example/app/" };
  const host = {
    serviceWorker: {
      register: async (...call) => {
        calls.push(call);
        return registration;
      },
    },
  };
  assert.equal(await registerServiceWorker(host), registration);
  assert.deepEqual(calls, [["sw.js", { scope: "./", updateViaCache: "none" }]]);
  assert.equal(WORKER_URL, "sw.js");
  assert.equal(WORKER_SCOPE, "./");
});

test("where the browser gives no service worker, or refuses this one, the app goes on without", async () => {
  // No secure context: `navigator.serviceWorker` is not there.
  assert.equal(await registerServiceWorker({}), null);
  assert.equal(await registerServiceWorker(undefined), null);
  const refusing = {
    serviceWorker: {
      register: async () => {
        throw new Error("SecurityError");
      },
    },
  };
  assert.equal(await registerServiceWorker(refusing), null);
});
