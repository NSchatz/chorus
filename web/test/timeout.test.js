// The per-test limit: a test past it fails, named, under `node --test --test-timeout`, as
// tools/web.sh runs the unit tests (60 s there; 1 s here, on a test that sleeps for 5).

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

test("test_web_sleeping_test_fails_with_its_name", () => {
  const dir = mkdtempSync(join(tmpdir(), "chorus-web-timeout-"));
  try {
    const file = join(dir, "sleeps.test.mjs");
    writeFileSync(
      file,
      'import { test } from "node:test";\n\ntest("sleeps", () => new Promise((resolve) => setTimeout(resolve, 5000)));\n',
    );
    // NODE_TEST_CONTEXT tells a node started under `node --test` to report to its parent: the
    // child here is a run of its own.
    const env = { ...process.env };
    delete env.NODE_TEST_CONTEXT;
    const child = spawnSync(process.execPath, ["--test", "--test-timeout=1000", "--test-reporter=spec", file], {
      env,
      encoding: "utf8",
      timeout: 30000,
    });
    const output = `${child.stdout}${child.stderr}`;
    assert.notEqual(child.status, 0, output);
    assert.match(output, /sleeps/);
    assert.match(output, /timed out/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
