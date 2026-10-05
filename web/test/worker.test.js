// The service worker's rules (src/worker.js) as pure logic, over a scripted
// network and a scripted cache store: no browser and no worker is started.
// What they hold: the API is never the worker's to answer or to keep, nothing
// but the app's own plain `200` is ever written to a cache, and a navigation
// asks the network first and falls back to the shell.

import assert from "node:assert/strict";
import { test } from "node:test";

import { CACHE_PREFIX, createWorker, mayStore, route } from "../src/worker.js";

const ORIGIN = "https://house.example";
const SCOPE = `${ORIGIN}/app/`;
const SHELL = ["", "assets/app-7ZM4XPDE.css", "assets/main-65UAVUNQ.js", "manifest.webmanifest", "icons/icon-192.png"];

// The server's API routes the app or any page of this origin may ask for
// (crates/server/src/control.rs): the state, the command, the artwork and
// every event stream.
const API = [
  ["GET", "/api/state"],
  ["POST", "/api/command"],
  ["GET", "/api/events"],
  ["GET", "/api/controller-events"],
  ["GET", "/api/voice-events"],
  ["GET", "/api/visualizer"],
  ["GET", "/api/artwork?group=kitchen"],
  ["POST", "/api/leaving"],
  ["GET", "/api"],
];

const request = (path, { method = "GET", mode = "cors" } = {}) => ({ method, mode, url: new URL(path, ORIGIN).href });
const navigate = (path) => request(path, { mode: "navigate" });

// A response as the worker reads one. `body` tells one from another.
function answer(body, { status = 200, type = "basic", redirected = false, url = "" } = {}) {
  const response = { body, status, type, redirected, url, ok: status >= 200 && status < 300 };
  response.clone = () => ({ ...response });
  return response;
}

// A CacheStorage that remembers every write.
function fakeCaches(initial = {}) {
  const stores = new Map(Object.entries(initial).map(([name, entries]) => [name, new Map(Object.entries(entries))]));
  const caches = {
    puts: [],
    opened: [],
    async open(name) {
      caches.opened.push(name);
      if (!stores.has(name)) stores.set(name, new Map());
      const entries = stores.get(name);
      return {
        match: async (url) => entries.get(String(url)),
        put: async (url, response) => {
          caches.puts.push(String(url));
          entries.set(String(url), response);
        },
        keys: async () => [...entries.keys()],
      };
    },
    keys: async () => [...stores.keys()],
    delete: async (name) => stores.delete(name),
    entries: (name) => Object.fromEntries([...(stores.get(name) ?? [])].map(([url, response]) => [url, response.body])),
  };
  return caches;
}

// A network that answers by path, and remembers what it was asked.
function fakeNetwork(answers = {}) {
  const network = {
    asked: [],
    options: [],
    answers,
    async fetch(target, options) {
      const url = new URL(typeof target === "string" ? target : target.url);
      const path = url.pathname + url.search;
      network.asked.push(path);
      network.options.push(options);
      const found = network.answers[path];
      if (found === undefined) throw new TypeError("fetch failed");
      return typeof found === "function" ? found() : found;
    },
  };
  return network;
}

function workerOver(network, caches, version = "new") {
  return createWorker({ caches, fetch: network.fetch, scope: SCOPE, version, shell: SHELL });
}

// The answers that are never kept, each as the browser hands it to a worker.
const NEVER_KEPT = {
  "a redirect that was not followed": () => answer("", { status: 0, type: "opaqueredirect" }),
  "the page a followed redirect ended on": () => answer("the login page", { redirected: true, url: `${ORIGIN}/login?rd=/app/` }),
  "a redirect's own status": () => answer("", { status: 302 }),
  "a refusal by the login": () => answer("not signed in", { status: 401 }),
  "a missing file": () => answer("no such route", { status: 404 }),
  "a partial answer": () => answer("part", { status: 206 }),
  "a gateway with no server": () => answer("bad gateway", { status: 502 }),
  "another origin's readable response": () => answer("theirs", { type: "cors", url: "https://login.example/" }),
  "another origin's opaque response": () => answer("", { status: 0, type: "opaque" }),
};

test("every API route, the event streams included, is the browser's own request", () => {
  for (const [method, path] of API) {
    assert.equal(route(request(path, { method }), SCOPE), "api", `${method} ${path}`);
    // Asked for as a page, too: an API address typed into the bar.
    assert.equal(route(navigate(path), SCOPE), "api", `navigate ${path}`);
  }
  // Not the API: a name that only begins like it.
  assert.equal(route(request("/apiary"), SCOPE), "other");
  assert.equal(route(request("/app/api/state"), SCOPE), "file");
});

test("the worker does not answer an API request, fetch it or touch a cache for it", async () => {
  // A cache that already holds an API answer, as no worker of this code
  // writes one: even then it is not served.
  const caches = fakeCaches({ [`${CACHE_PREFIX}new`]: { [`${ORIGIN}/api/state`]: answer("stale state") } });
  const network = fakeNetwork(Object.fromEntries(API.map(([, path]) => [path, answer("from the server")])));
  const worker = workerOver(network, caches);
  for (const [method, path] of API) {
    assert.equal(worker.respond(request(path, { method })), null, `${method} ${path}`);
    assert.equal(worker.respond(navigate(path)), null, `navigate ${path}`);
  }
  assert.deepEqual(network.asked, [], "the worker made no request of its own");
  assert.deepEqual(caches.opened, [], "no cache was opened");
  assert.deepEqual(caches.puts, [], "nothing was written");
});

test("an API answer may not be stored, whatever it is", () => {
  for (const [, path] of API) {
    const url = `${ORIGIN}${path}`;
    assert.equal(mayStore(url, answer("{}", { url }), SCOPE), false, path);
  }
  // The same answer for a file of the app may.
  const file = `${SCOPE}manifest.webmanifest`;
  assert.equal(mayStore(file, answer("{}", { url: file }), SCOPE), true);
});

test("a redirect, a status that is not 200 and another origin's response may not be stored", () => {
  const url = `${SCOPE}assets/main-65UAVUNQ.js`;
  for (const [what, make] of Object.entries(NEVER_KEPT)) {
    assert.equal(mayStore(url, make(), SCOPE), false, what);
    assert.equal(mayStore(SCOPE, make(), SCOPE), false, `${what}, as the document`);
  }
  assert.equal(mayStore(url, answer("the file", { url }), SCOPE), true);
  assert.equal(mayStore(url, null, SCOPE), false);
  // Nothing outside the app is stored either: the server's other pages,
  // another origin.
  assert.equal(mayStore(`${ORIGIN}/`, answer("the control page"), SCOPE), false);
  assert.equal(mayStore("https://login.example/app/", answer("theirs"), SCOPE), false);
});

test("none of those answers is written to a cache on any path, and each is handed on as it came", async () => {
  const paths = {
    navigation: navigate("/app/"),
    asset: request("/app/assets/main-65UAVUNQ.js"),
    file: request("/app/manifest.webmanifest"),
  };
  for (const [kind, asked] of Object.entries(paths)) {
    for (const [what, make] of Object.entries(NEVER_KEPT)) {
      const caches = fakeCaches();
      const given = make();
      const network = fakeNetwork({ [new URL(asked.url).pathname]: given });
      const response = await workerOver(network, caches).respond(asked);
      assert.equal(response, given, `${kind}: ${what} is handed on`);
      assert.deepEqual(caches.puts, [], `${kind}: ${what} is not written`);
    }
  }
});

test("what the worker answers and what it leaves to the browser", () => {
  assert.equal(route(navigate("/app/"), SCOPE), "navigation");
  assert.equal(route(navigate("/app/index.html"), SCOPE), "navigation");
  assert.equal(route(navigate("/app/?mode=kiosk"), SCOPE), "navigation");
  assert.equal(route(request("/app/assets/main-65UAVUNQ.js"), SCOPE), "asset");
  assert.equal(route(request("/app/assets/app-7ZM4XPDE.css"), SCOPE), "asset");
  assert.equal(route(request("/app/manifest.webmanifest"), SCOPE), "file");
  assert.equal(route(request("/app/icons/icon-192.png"), SCOPE), "file");
  assert.equal(route(request("/app/sw.js"), SCOPE), "file");
  // Not named by a hash: never answered from the cache first.
  assert.equal(route(request("/app/assets/main.js"), SCOPE), "file");
  // The browser's own: another origin (the login), the server's other pages,
  // a request that is not a GET.
  assert.equal(route({ method: "GET", mode: "navigate", url: "https://login.example/app/" }, SCOPE), "other");
  assert.equal(route(navigate("/"), SCOPE), "other");
  assert.equal(route(navigate("/login?rd=/app/"), SCOPE), "other");
  assert.equal(route(navigate("/application/"), SCOPE), "other");
  assert.equal(route(request("/app/", { method: "POST" }), SCOPE), "other");
  const worker = workerOver(fakeNetwork(), fakeCaches());
  assert.equal(worker.respond(navigate("/")), null);
  assert.equal(worker.respond({ method: "GET", mode: "no-cors", url: "https://login.example/logo.png" }), null);
});

test("a navigation asks the network first and keeps the document as the shell", async () => {
  const caches = fakeCaches({ [`${CACHE_PREFIX}new`]: { [SCOPE]: answer("the old document") } });
  const network = fakeNetwork({ "/app/?mode=kiosk": answer("the new document", { url: `${SCOPE}?mode=kiosk` }) });
  const worker = workerOver(network, caches);
  const response = await worker.respond(navigate("/app/?mode=kiosk"));
  assert.equal(response.body, "the new document");
  assert.deepEqual(network.asked, ["/app/?mode=kiosk"]);
  // Kept under the app's own address, whatever the navigation's query.
  assert.deepEqual(caches.entries(worker.cacheName), { [SCOPE]: "the new document" });
});

test("a navigation the network cannot answer gets the cached shell", async () => {
  const caches = fakeCaches({ [`${CACHE_PREFIX}new`]: { [SCOPE]: answer("the shell") } });
  const network = fakeNetwork();
  const worker = workerOver(network, caches);
  assert.equal((await worker.respond(navigate("/app/"))).body, "the shell");
  assert.equal((await worker.respond(navigate("/app/index.html?mode=kiosk"))).body, "the shell");
  assert.deepEqual(network.asked, ["/app/", "/app/index.html?mode=kiosk"], "the network was asked first each time");
  assert.deepEqual(caches.puts, []);

  // A gateway that answers for a server that is not there: the shell too,
  // and the gateway's page is not kept.
  network.answers["/app/"] = answer("bad gateway", { status: 502 });
  assert.equal((await worker.respond(navigate("/app/"))).body, "the shell");
  assert.deepEqual(caches.puts, []);
});

test("with no shell cached, a navigation the network cannot answer fails as the network did", async () => {
  const worker = workerOver(fakeNetwork(), fakeCaches());
  await assert.rejects(worker.respond(navigate("/app/")), /fetch failed/);
  const gateway = answer("bad gateway", { status: 503 });
  const behind = workerOver(fakeNetwork({ "/app/": gateway }), fakeCaches());
  assert.equal(await behind.respond(navigate("/app/")), gateway);
});

test("a lapsed login's redirect reaches the browser and leaves the shell as it was", async () => {
  const caches = fakeCaches({ [`${CACHE_PREFIX}new`]: { [SCOPE]: answer("the shell") } });
  const redirect = answer("", { status: 0, type: "opaqueredirect" });
  const worker = workerOver(fakeNetwork({ "/app/": redirect }), caches);
  // Not the shell: the browser must follow the redirect to the login.
  assert.equal(await worker.respond(navigate("/app/")), redirect);
  assert.deepEqual(caches.entries(worker.cacheName), { [SCOPE]: "the shell" });
});

test("a file named by its content is answered from the cache, and fetched once when it is not there", async () => {
  const path = "/app/assets/main-65UAVUNQ.js";
  const caches = fakeCaches();
  const network = fakeNetwork({ [path]: () => answer("the bundle", { url: `${ORIGIN}${path}` }) });
  const worker = workerOver(network, caches);
  assert.equal((await worker.respond(request(path))).body, "the bundle");
  assert.equal((await worker.respond(request(path))).body, "the bundle");
  assert.deepEqual(network.asked, [path], "the second answer is the cache's");
});

test("any other file of the app asks the network first, the cache when the network fails", async () => {
  const path = "/app/manifest.webmanifest";
  const caches = fakeCaches();
  const network = fakeNetwork({ [path]: answer("manifest 1", { url: `${ORIGIN}${path}` }) });
  const worker = workerOver(network, caches);
  assert.equal((await worker.respond(request(path))).body, "manifest 1");
  network.answers[path] = answer("manifest 2", { url: `${ORIGIN}${path}` });
  assert.equal((await worker.respond(request(path))).body, "manifest 2");
  delete network.answers[path];
  assert.equal((await worker.respond(request(path))).body, "manifest 2");
  await assert.rejects(worker.respond(request("/app/icons/icon-512.png")), /fetch failed/);
});

test("installing caches the whole shell, each file asked of the server without following a redirect", async () => {
  const caches = fakeCaches();
  const network = fakeNetwork(
    Object.fromEntries(SHELL.map((file) => [`/app/${file}`, answer(`<${file}>`, { url: `${SCOPE}${file}` })])),
  );
  const worker = workerOver(network, caches);
  await worker.install();
  assert.equal(worker.cacheName, `${CACHE_PREFIX}new`);
  assert.deepEqual(Object.keys(caches.entries(worker.cacheName)), SHELL.map((file) => `${SCOPE}${file}`));
  for (const options of network.options) {
    assert.deepEqual(options, { cache: "no-cache", credentials: "same-origin", redirect: "manual" });
  }
});

test("installing while the login has lapsed caches nothing at all", async () => {
  for (const [what, make] of Object.entries(NEVER_KEPT)) {
    const caches = fakeCaches();
    const answers = Object.fromEntries(SHELL.map((file) => [`/app/${file}`, answer(`<${file}>`)]));
    // The last file of the shell is the one the login answers for.
    answers[`/app/${SHELL.at(-1)}`] = make();
    await assert.rejects(workerOver(fakeNetwork(answers), caches).install(), /nothing was cached/, what);
    assert.deepEqual(caches.puts, [], what);
  }
});

test("activating deletes the caches of earlier builds and nothing that is not the worker's", async () => {
  const caches = fakeCaches({
    [`${CACHE_PREFIX}old`]: { [SCOPE]: answer("older") },
    [`${CACHE_PREFIX}new`]: { [SCOPE]: answer("this build's") },
    "somebody-else": { [`${ORIGIN}/`]: answer("theirs") },
  });
  await workerOver(fakeNetwork(), caches).activate();
  assert.deepEqual(await caches.keys(), [`${CACHE_PREFIX}new`, "somebody-else"]);
});
