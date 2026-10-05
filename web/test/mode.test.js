// Pure logic: the layout a query string asks for.

import assert from "node:assert/strict";
import { test } from "node:test";

import { MODES, displayMode } from "../src/mode.js";

test("a bare or valued kiosk parameter asks for the kiosk layout", () => {
  for (const search of ["?kiosk", "kiosk", "?kiosk=1", "?room=den&kiosk", "?kiosk=true"]) {
    assert.equal(displayMode(search), "kiosk", search);
  }
});

test("no kiosk parameter, or one switched off, is the ordinary layout", () => {
  for (const search of ["", "?", "?room=den", "?kiosk=0", "?kiosk=false", "?kiosks"]) {
    assert.equal(displayMode(search), "app", search);
  }
});

test("every layout displayMode returns is one of MODES", () => {
  assert.deepEqual([...MODES], ["app", "kiosk"]);
  assert.ok(MODES.includes(displayMode("?kiosk")));
  assert.ok(MODES.includes(displayMode("")));
});
