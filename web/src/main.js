// The bundle's entry point: define the app element, give it the layout the
// page was opened with, and hand it the state layer's store, started. Then
// register the service worker, where the browser has one to give.

import { createClient } from "./api.js";
import "./chorus-app.js";
import { registerServiceWorker } from "./install.js";
import { displayMode } from "./mode.js";
import { createStore } from "./state.js";

const app = document.querySelector("chorus-app");
if (app) {
  app.mode = displayMode(window.location.search);
  const store = createStore(createClient());
  app.store = store;
  store.start();
}

registerServiceWorker();
