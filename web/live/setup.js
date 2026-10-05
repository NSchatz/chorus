// Loaded before the live test (`node --import ./live/setup.js --test`): happy-dom's
// window, document and customElements become globals, as in test/setup.js, so the
// app's elements render in node. Unlike the unit tests, the live test talks to a
// real server, so the network stays node's own: happy-dom's registration replaces
// fetch and the classes around it with a browser's (same-origin rules against the
// page's address included), and they are put back here.

import { GlobalRegistrator } from "@happy-dom/global-registrator";

const NETWORK = ["fetch", "Headers", "Request", "Response", "AbortController", "AbortSignal", "ReadableStream", "TextDecoder", "TextEncoder"]; // prettier-ignore
const nodes = Object.fromEntries(NETWORK.map((name) => [name, globalThis[name]]));

GlobalRegistrator.register({ url: "http://chorus.test/" });

for (const [name, value] of Object.entries(nodes)) globalThis[name] = value;
