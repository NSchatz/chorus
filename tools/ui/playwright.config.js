// The rendered grader's configuration.
//
// It drives the Chromium that is already in this container rather than one
// Playwright downloads, which is why the install is
// `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 pnpm install --ignore-scripts`. Point
// CHORUS_BROWSER somewhere else to use a different engine.
//
// Nothing here ships. The Cargo workspace has no dependency on any of it, the
// server serves no file from this directory, and tools/ui-render-run.sh is the
// only thing that runs it.

const path = require("path");

module.exports = {
  testDir: __dirname,
  testMatch: /.*\.spec\.js/,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: {
    launchOptions: {
      executablePath: process.env.CHORUS_BROWSER || "/usr/bin/chromium",
      // This container runs unprivileged and has no user namespace to give the
      // renderer, which is what the sandbox needs. The page being rendered is
      // served by a process this script started, on loopback, from bytes in
      // this repository.
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    },
    // A fixed viewport, because a criterion about a rendered box is a claim
    // about a rendered box at a stated size.
    viewport: { width: 1280, height: 900 },
    deviceScaleFactor: 1,
  },
  outputDir: path.join(__dirname, ".playwright"),
};
