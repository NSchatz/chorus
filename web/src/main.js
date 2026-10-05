// The bundle's entry point: define the app element, give it the layout the
// page was opened with, and hand it the server's rooms.

import "./chorus-app.js";
import { displayMode } from "./mode.js";
import { loadRooms } from "./rooms.js";

const app = document.querySelector("chorus-app");
if (app) {
  app.mode = displayMode(window.location.search);
  loadRooms().then((rooms) => {
    app.rooms = rooms;
  });
}
