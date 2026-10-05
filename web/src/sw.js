// The app's service worker, written by hand: it hands the browser's three
// events to the rules of worker.js and decides nothing itself. The build
// bundles the two into dist/sw.js, at the app's root so that the worker's
// scope is the app and nothing else, and fills in the two names below: the
// build's version and the files of the shell.
//
// A fetch handler is kept even where every request would reach the network
// anyway: Chromium still ties its install prompt to one.

/* global __CHORUS_BUILD__, __CHORUS_SHELL__ */

import { createWorker } from "./worker.js";

const worker = createWorker({
  caches: self.caches,
  fetch: (request, options) => self.fetch(request, options),
  scope: self.registration.scope,
  version: __CHORUS_BUILD__,
  shell: __CHORUS_SHELL__,
});

self.addEventListener("install", (event) => event.waitUntil(worker.install()));
self.addEventListener("activate", (event) => event.waitUntil(worker.activate()));
self.addEventListener("fetch", (event) => {
  const answer = worker.respond(event.request);
  // No answer: the request is the browser's own (the API, another origin).
  if (answer) event.respondWith(answer);
});
