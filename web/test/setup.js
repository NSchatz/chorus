// Loaded before every test file (`node --import ./test/setup.js --test`): happy-dom's window,
// document, HTMLElement and customElements become globals, so Lit elements define, render and
// update in node with no browser. It has to run before Lit is imported, which is why it is a
// preload and not an import of the tests.

import { GlobalRegistrator } from "@happy-dom/global-registrator";

GlobalRegistrator.register({ url: "http://chorus.test/" });
