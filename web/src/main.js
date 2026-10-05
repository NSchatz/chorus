// The bundle's entry point: define the app element, give it the mode the
// page was opened in, and hand it the state layer's store, started. A kiosk
// keeps its screen on (wake-lock.js). Then register the service worker, where
// the browser has one to give.

import { createClient } from "./api.js";
import "./chorus-app.js";
import { registerServiceWorker } from "./install.js";
import { resolveMode, storageOf } from "./mode.js";
import { createStore } from "./state.js";
import { keepAwake } from "./wake-lock.js";

const app = document.querySelector("chorus-app");
if (app) {
  app.mode = resolveMode(window.location.search, storageOf(window));
  if (app.mode === "kiosk") keepAwake();
  const store = createStore(createClient());
  app.store = store;
  store.start();
}

registerServiceWorker();
