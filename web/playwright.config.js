// The browser smoke test's configuration (`make web-smoke`, tools/web.sh):
// the one file under smoke/, in headless Chromium, once, with nothing kept.
// tools/web.sh passes --output, so a run writes nothing into the checkout.

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./smoke",
  testMatch: "*.spec.js",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: true,
  reporter: "list",
  timeout: 60_000,
  use: {
    browserName: "chromium",
    headless: true,
    trace: "off",
    screenshot: "off",
    video: "off",
  },
});
