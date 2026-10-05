// The service worker's rules as pure logic: which request the worker answers
// and how, and which response may be kept. sw.js, the worker itself, only
// hands the browser's events to this; the unit tests give it a scripted
// network and a scripted cache store (docs/decisions/0190-the-app-installs-behind-the-login.md).
//
// The app sits behind a household login (a reverse proxy with forward
// authentication). When the login has lapsed the proxy answers any request
// with a redirect to the login page or with a refusal, and a worker that kept
// such an answer would serve the login page as the app from then on. So:
//
//   the API (the server's `/api/`, every event stream included) is the
//     browser's own request. The worker does not answer it, does not fetch it
//     and never writes it to a cache.
//   a navigation to the app is asked of the network first. The document it
//     answers with is kept as the shell, and the shell is what a navigation
//     gets when the network fails or the server cannot be reached.
//   a file named by the hash of its content is answered from the cache, and
//     from the network when it is not there yet.
//   any other file of the app is asked of the network first, the cache when
//     the network fails.
//   everything else (another origin, the server's other pages, a request that
//     is not a GET) is the browser's own request.
//
// One rule decides what is kept, for every path above: `mayStore`.

// The caches of this worker are named with this prefix and the build's
// version; a new build's worker deletes the others it finds.
export const CACHE_PREFIX = "chorus-app-";

// The document inside the scope, beside the scope's own URL.
const DOCUMENT = "index.html";
// `assets/<name>-<hash>.<ext>`, the name the build gives a file by its content
// (crates/server/src/app.rs serves exactly these as immutable).
const HASHED = /^assets\/[^/]+-[A-Z0-9]{8}\.[A-Za-z0-9]+$/;

// Where the server's API is, seen from the scope: the app is served under
// /app/ and the API beside it, at the root (api.js reaches it as "../api/").
const apiPath = (scope) => new URL("../api/", scope).pathname;

// What the worker does with a request, by name:
//   "api"         the API: the browser's own request, never cached
//   "other"       not the app's: the browser's own request
//   "navigation"  the app's document: network first, the shell as fallback
//   "asset"       a file named by its content: the cache first
//   "file"        any other file of the app: network first
// `request` is what a Request holds: { method, url, mode }. `scope` is the
// worker's scope, the app's own directory.
export function route(request, scope) {
  const home = new URL(scope);
  const target = new URL(request.url, home);
  if (target.origin !== home.origin) return "other";
  const api = apiPath(home);
  if (target.pathname === api.slice(0, -1) || target.pathname.startsWith(api)) return "api";
  if (request.method !== "GET") return "other";
  if (!target.pathname.startsWith(home.pathname)) return "other";
  const path = target.pathname.slice(home.pathname.length);
  if (path === "" || path === DOCUMENT) return request.mode === "navigate" ? "navigation" : "file";
  return HASHED.test(path) ? "asset" : "file";
}

// Whether `response`, the answer to a GET of `url`, may be written to a cache.
// Only a plain `200` that the app's own origin gave for a file of the app:
//   not the API, whatever it answered;
//   not a redirect the browser did not follow (`opaqueredirect`) and not the
//     page a followed redirect ended on (`redirected`): the login;
//   not a `401`, a `404`, a `502` or any other status that is not `200`;
//   not a response of another origin (`cors`, `opaque`).
export function mayStore(url, response, scope) {
  const kind = route({ method: "GET", url, mode: "same-origin" }, scope);
  if (kind !== "asset" && kind !== "file") return false;
  if (!response || response.status !== 200 || response.type !== "basic" || response.redirected) return false;
  // A basic response is this origin's; one that names where it came from
  // names the address that was asked for.
  if (response.url && new URL(response.url).origin !== new URL(scope).origin) return false;
  return true;
}

// The worker over what it is given:
//   caches   the CacheStorage
//   fetch    the network
//   scope    the worker's scope, the URL of the app's directory
//   version  the build's version, which names this worker's cache
//   shell    the files a page of the app needs, relative to the scope, the
//            document as "" (the build lists them)
// `install` and `activate` are the two lifecycle steps; `respond(request)`
// is the answer to a fetch event: a promise of a response, or null when the
// request is the browser's own to make.
export function createWorker({ caches, fetch, scope, version, shell }) {
  const cacheName = `${CACHE_PREFIX}${version}`;
  // The shell document's key: the scope itself, whatever query a navigation
  // carried (the page reads its own address).
  const documentUrl = new URL(scope).href;
  const absolute = (url) => new URL(url, scope).href;

  // The one place a response is written to a cache.
  async function keep(url, response) {
    if (!mayStore(url, response, scope)) return false;
    const cache = await caches.open(cacheName);
    await cache.put(url, response.clone());
    return true;
  }

  async function kept(url) {
    const cache = await caches.open(cacheName);
    return (await cache.match(url)) ?? null;
  }

  // Fetch every file of the shell, and keep them only when every one may be
  // kept: a worker installed while the login has lapsed is not installed,
  // and the browser tries again at the next load.
  async function install() {
    const urls = shell.map(absolute);
    const answers = [];
    for (const url of urls) {
      const response = await fetch(url, { cache: "no-cache", credentials: "same-origin", redirect: "manual" });
      if (!mayStore(url, response, scope)) {
        throw new Error(`${url} did not answer with the file (status ${response.status}, ${response.type}): nothing was cached`);
      }
      answers.push(response);
    }
    const cache = await caches.open(cacheName);
    for (const [index, url] of urls.entries()) await cache.put(url, answers[index]);
  }

  // Delete the caches of earlier builds, and nothing that is not this worker's.
  async function activate() {
    for (const name of await caches.keys()) {
      if (name.startsWith(CACHE_PREFIX) && name !== cacheName) await caches.delete(name);
    }
  }

  // A navigation: the network's answer, whatever it is (a redirect to the
  // login is handed to the browser, which follows it), and the shell when
  // there is no answer or the answer is a gateway's `5xx` for a server that
  // is not there.
  async function navigation(request) {
    let response;
    try {
      response = await fetch(request);
    } catch (error) {
      const shellDocument = await kept(documentUrl);
      if (shellDocument) return shellDocument;
      throw error;
    }
    if (response.status >= 500) return (await kept(documentUrl)) ?? response;
    await keep(documentUrl, response);
    return response;
  }

  async function asset(request) {
    const url = absolute(request.url);
    const cached = await kept(url);
    if (cached) return cached;
    const response = await fetch(request);
    await keep(url, response);
    return response;
  }

  async function file(request) {
    const url = absolute(request.url);
    let response;
    try {
      response = await fetch(request);
    } catch (error) {
      const cached = await kept(url);
      if (cached) return cached;
      throw error;
    }
    await keep(url, response);
    return response;
  }

  function respond(request) {
    switch (route(request, scope)) {
      case "navigation":
        return navigation(request);
      case "asset":
        return asset(request);
      case "file":
        return file(request);
      default:
        // "api" and "other": the worker does not answer.
        return null;
    }
  }

  return { cacheName, install, activate, respond };
}
