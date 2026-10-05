// The bundle's entry point: define the app element and give it the layout the
// page was opened with.

import "./chorus-app.js";
import { displayMode } from "./mode.js";

const app = document.querySelector("chorus-app");
if (app) app.mode = displayMode(window.location.search);
