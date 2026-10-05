// Registering the service worker (sw.js, beside the document) from the page.
//
// The worker's address and scope are relative to the document, like every
// other link of the app: `sw.js` under the app's directory, with that
// directory as its scope. `updateViaCache: "none"` asks the server for the
// worker at every update check, whatever a cache in between holds; the server
// answers `no-cache` for it besides (crates/server/src/app.rs).
//
// A browser gives a page a service worker only in a secure context (HTTPS,
// or the loopback address). On the server's plain HTTP port there is none,
// and the app works without: this then does nothing.

export const WORKER_URL = "sw.js";
export const WORKER_SCOPE = "./";

// Resolves to the registration, or to null where there is no service worker
// to register or the browser refused it. Never rejects: the app does not
// depend on its worker.
export async function registerServiceWorker(host = globalThis.navigator) {
  const container = host?.serviceWorker;
  if (!container || typeof container.register !== "function") return null;
  try {
    return await container.register(WORKER_URL, { scope: WORKER_SCOPE, updateViaCache: "none" });
  } catch {
    return null;
  }
}
