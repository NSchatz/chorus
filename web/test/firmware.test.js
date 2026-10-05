// Firmware on the speakers screen (src/speakers.js) in the shell, over a
// scripted server: what each speaker runs, "update available" for exactly
// the speakers the server says it of, each firmware state with its reason,
// and the three commands. The point of the file is K93: nothing is installed
// without an explicit action that names the image and the speaker, so the
// scripted server's command log is held empty until "Install" has been
// pressed and confirmed.
//
// Nothing here is a speaker or an install: the server is scripted.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, test } from "node:test";

import { createClient, firmwareCancelCommand, firmwareInstallCommand, firmwareRescanCommand } from "../src/api.js";
import "../src/chorus-app.js";
import { addressOf } from "../src/routes.js";
import { SPEAKERS_SCREEN, firmwareStateWords } from "../src/speakers.js";
import { createStore, firmwareImagesOf, speakersOf, updatesFor } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
});

const fixture = (name) => readFileSync(new URL(`../../fixtures/control/v2/${name}`, import.meta.url), "utf8").trim();

// The house: one room and four speakers. Two run 1.0.0 on the brick board,
// for which 2.0.0 is staged and verified; one runs the compact board's only
// staged version; one is a speaker that takes no updates (no `firmware`).
const BRICK = "chorus-0123456789ab";
const SECOND = "chorus-ba9876543210";
const COMPACT = "chorus-cccccccccccc";
const PLAIN = "chorus-dddddddddddd";
const IDLE = { version: "1.0.0", board: "brick-s3-wired", slot: 0, state: "idle", reason: "none", update_available: true, image: null, image_version: "", received: 0, size: 0 }; // prettier-ignore
const speaker = (id, name, firmware, more = {}) => ({
  id,
  name,
  named: true,
  room: "living",
  present: true,
  software: "chorus-endpoint 0.1.0",
  link: "wired",
  key: `fp-${id.slice(-4)}`,
  roles: ["player"],
  ...(firmware ? { firmware } : {}),
  ...more,
});
const SPEAKERS = [
  speaker(BRICK, "Living left", IDLE),
  speaker(SECOND, "Living right", { ...IDLE, slot: 1 }),
  speaker(COMPACT, "Hall", { ...IDLE, version: "2.0.0", board: "compact-s3-wifi", update_available: false }),
  speaker(PLAIN, "Old box", null),
];
const image = (name, version, board, more = {}) => ({ name, version, board, size: 1536000, sha256: "0f".repeat(32), verdict: "verified", ...more }); // prettier-ignore
const IMAGES = [
  image("brick-2-0-0", "2.0.0", "brick-s3-wired"),
  image("brick-tampered", "3.0.0", "brick-s3-wired", { verdict: "refused", reason: "digest-mismatch" }),
  image("compact-2-0-0", "2.0.0", "compact-s3-wifi"),
  image("brick-1-0-0", "1.0.0", "brick-s3-wired"),
];
// `images` null is a server started without a firmware directory: its state
// has no `firmware` key at all.
const house = (serial, speakers = SPEAKERS, images = IMAGES) =>
  stateOf(serial, [zone("living", { name: "Living Room" })], {
    speakers,
    ...(images === null ? {} : { firmware: { images } }),
  });

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await app.shadowRoot.querySelector("chorus-speakers")?.updateComplete;
}

async function open(server) {
  history.replaceState(null, "", `/app/${addressOf(SPEAKERS_SCREEN)}`);
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

// A scripted server that holds firmware as the real one does: an install is
// `requested` on the speaker it named, a cancel is `cancelled`, and a rescan
// lists what `staged` now holds.
function firmwareServer({ speakers = SPEAKERS, images = IMAGES, staged = images } = {}) {
  // Its serials start above the ones a test sends on the event stream by
  // hand, so that its answer is the newer state.
  let serial = 100;
  const server = fakeServer(house(serial, speakers, images));
  server.answer = (body) => {
    const message = JSON.parse(body);
    const change = (fields) =>
      speakers.map((held) => (held.id === message.speaker ? { ...held, firmware: { ...held.firmware, ...fields } } : held));
    if (message.t === "firmware_install") {
      const found = images.find((held) => held.name === message.image);
      speakers = change({ state: "requested", image: found.name, image_version: found.version, size: found.size });
    }
    if (message.t === "firmware_cancel") speakers = change({ state: "cancelled" });
    if (message.t === "firmware_rescan") images = staged;
    serial += 1;
    server.snapshot = house(serial, speakers, images);
    return { status: 200, body: JSON.stringify(server.snapshot) };
  };
  return server;
}

const screenOf = (app) => app.shadowRoot.querySelector("chorus-speakers");
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const row = (app, id) => screenOf(app).shadowRoot.querySelector(`li[data-speaker="${id}"]`);
const firmwareOf = (app, id) => row(app, id).querySelector("[data-firmware]");
const value = (app, id, name) => text(row(app, id).querySelector(`[data-value="${name}"]`));
const stateOfRow = (app, id) => row(app, id).querySelector("[data-firmware-state]");
const alertOf = (app, id) => text(row(app, id).querySelector(":scope > [role=alert]"));
const updates = (app, id) => [...row(app, id).querySelectorAll("[data-update]")].map((entry) => entry.dataset.update);
// Every control on the screen that would start, stop or look for an install.
const updateControls = (app) => [
  ...screenOf(app).shadowRoot.querySelectorAll("[data-install], [data-cancel-install], [data-rescan]"),
];

const INSTALL = "Install image brick-2-0-0 (version 2.0.0) on Living left";
const YES = "Yes, install image brick-2-0-0 on Living left";
const NO = "Do not install image brick-2-0-0 on Living left";

test("the three commands are the catalog's own vectors", () => {
  assert.equal(firmwareInstallCommand("chorus-0123456789ab", "brick-2-0-0"), fixture("firmware_install.json"));
  assert.equal(firmwareCancelCommand("chorus-0123456789ab"), fixture("firmware_cancel.json"));
  assert.equal(firmwareRescanCommand(), fixture("firmware_rescan.json"));
});

test("the state's firmware is read as the catalog writes it", () => {
  const state = JSON.parse(fixture("state-firmware.json"));
  const images = firmwareImagesOf(state);
  assert.deepEqual(images.map((held) => [held.name, held.version, held.board, held.verified, held.reason]), [
    ["brick-2-0-0", "2.0.0", "brick-s3-wired", true, null],
    ["brick-tampered", "2.0.0", "brick-s3-wired", false, "digest-mismatch"],
    ["compact-2-0-0", "2.0.0", "compact-s3-wifi", true, null],
    ["brick-1-0-0", "1.0.0", "brick-s3-wired", true, null],
  ]); // prettier-ignore
  const [first, second] = speakersOf(state);
  assert.deepEqual(first.firmware, {
    version: "1.0.0",
    board: "brick-s3-wired",
    slot: 0,
    state: "requested",
    reason: null,
    updateAvailable: true,
    image: "brick-2-0-0",
    imageVersion: "2.0.0",
    received: 0,
    size: 1536000,
  });
  assert.deepEqual([second.firmware.state, second.firmware.reason, second.firmware.image, second.firmware.imageVersion], ["rolled_back", "not_confirmed", null, "3.0.0"]); // prettier-ignore
  // The update the server's flag is about: the verified image of the board with another version.
  assert.deepEqual(updatesFor(first.firmware, images).map((held) => held.name), ["brick-2-0-0"]); // prettier-ignore
  // The flag is the server's: without it there is no update, whatever is staged.
  assert.deepEqual(updatesFor({ ...first.firmware, updateAvailable: false }, images), []);
  assert.deepEqual(updatesFor(null, images), []);
  // A state with no `firmware`, and a speaker with none.
  assert.equal(firmwareImagesOf({ speakers: [] }), null);
  assert.equal(firmwareImagesOf(null), null);
  assert.deepEqual(firmwareImagesOf({ firmware: {} }), []);
  assert.equal(speakersOf({ speakers: [{ id: "x" }] })[0].firmware, null);
  assert.equal(speakersOf({ speakers: [{ id: "x", firmware: { update_available: "yes" } }] })[0].firmware.updateAvailable, false);
});

test("each speaker shows the firmware it runs, and one that reported none shows no firmware", async () => {
  const app = await open(fakeServer(house(1)));
  assert.deepEqual(
    [BRICK, SECOND, COMPACT].map((id) => [value(app, id, "firmware-version"), value(app, id, "firmware-board"), value(app, id, "firmware-slot")]),
    [
      ["1.0.0", "brick-s3-wired", "0"],
      ["1.0.0", "brick-s3-wired", "1"],
      ["2.0.0", "compact-s3-wifi", "0"],
    ],
  ); // prettier-ignore
  assert.equal(getByLabel(app, "Firmware of Living left"), firmwareOf(app, BRICK));
  assert.equal(firmwareOf(app, PLAIN), null);
  assert.equal(text(stateOfRow(app, BRICK)), "No install is in progress.");
});

test("update available is shown only where firmware.update_available is true, with the image's version from firmware.images", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  for (const id of [BRICK, SECOND]) {
    assert.equal(text(row(app, id).querySelector("[data-update-available]")), "Update available");
    // The one image the flag is about: not the refused one, not the other board's, not the version it runs.
    assert.deepEqual(updates(app, id), ["brick-2-0-0"]);
    assert.equal(text(row(app, id).querySelector("[data-update-version]")), "Version 2.0.0, image brick-2-0-0");
  }
  for (const id of [COMPACT, PLAIN]) {
    assert.equal(row(app, id).querySelector("[data-update-available]"), null);
    assert.deepEqual(updates(app, id), []);
    assert.equal(row(app, id).querySelectorAll("[data-install]").length, 0);
  }
  assert.equal(queryAllByLabel(app, INSTALL).length, 1);

  // The version shown is the staged image's: another image staged, another version shown.
  server.send(house(2, SPEAKERS, [image("brick-next", "2.1.0-rc1", "brick-s3-wired")]));
  await rendered(app);
  assert.equal(text(row(app, BRICK).querySelector("[data-update-version]")), "Version 2.1.0-rc1, image brick-next");
  // The flag is the server's. Staged images that look like an update are not one without it.
  server.send(house(3, [speaker(BRICK, "Living left", { ...IDLE, update_available: false })]));
  await rendered(app);
  assert.equal(row(app, BRICK).querySelector("[data-update-available]"), null);
  assert.equal(row(app, BRICK).querySelectorAll("[data-install]").length, 0);
  // And with it but no image to name, there is nothing to install: the screen says so.
  server.send(house(4, [speaker(BRICK, "Living left", IDLE)], [IMAGES[1], IMAGES[2], IMAGES[3]]));
  await rendered(app);
  assert.equal(text(row(app, BRICK).querySelector("[data-update-available]")), "Update available");
  assert.match(text(row(app, BRICK).querySelector("[data-update-unlisted]")), /lists no verified image for this board/);
  assert.equal(row(app, BRICK).querySelectorAll("[data-install]").length, 0);
  assert.deepEqual(server.commands, [], "showing sends nothing");
});

test("nothing is sent until Install is pressed and confirmed; then exactly one firmware_install names that image and speaker", async () => {
  const server = firmwareServer();
  const app = await open(server);
  assert.deepEqual(server.commands, [], "opening the screen sends nothing");

  // New states, a rescan by somebody else and the screen being opened again send nothing.
  server.send(house(2));
  server.send(house(3, SPEAKERS, [...IMAGES, image("brick-3-0-0", "3.0.0", "brick-s3-wired")]));
  await rendered(app);
  assert.deepEqual(updates(app, BRICK), ["brick-2-0-0", "brick-3-0-0"]);
  server.send(house(4));
  await rendered(app);
  assert.deepEqual(server.commands, []);

  // The first press only asks, and the question names the image and the speaker.
  getByLabel(app, INSTALL).click();
  await rendered(app);
  assert.deepEqual(server.commands, [], "Install only asks");
  const question = text(row(app, BRICK).querySelector("[data-install-question]"));
  assert.match(question, new RegExp(`^Install image brick-2-0-0 \\(version 2\\.0\\.0\\) on Living left \\(${BRICK}\\)\\? It runs version 1\\.0\\.0 now\\.`));
  assert.equal(row(app, SECOND).querySelector("[data-install-question]"), null, "the other speaker is not asked about");

  // "Not now" sends nothing and takes the question away.
  getByLabel(app, NO).click();
  await rendered(app);
  assert.deepEqual(server.commands, []);
  assert.equal(row(app, BRICK).querySelector("[data-install-question]"), null);
  assert.equal(queryAllByLabel(app, YES).length, 0);

  // Asked again and confirmed: one command, with that image and that speaker.
  getByLabel(app, INSTALL).click();
  await rendered(app);
  assert.deepEqual(server.commands, []);
  getByLabel(app, YES).click();
  await rendered(app);
  assert.deepEqual(server.commands, [`{"v":2,"t":"firmware_install","speaker":"${BRICK}","image":"brick-2-0-0"}`]);
  assert.equal(server.commands[0], fixture("firmware_install.json"));

  // The screen shows what the server now says, and offers no second install while this one runs.
  assert.equal(stateOfRow(app, BRICK).dataset.firmwareState, "requested");
  assert.equal(text(stateOfRow(app, BRICK)), "Install requested: the server is offering image brick-2-0-0 (version 2.0.0) to the speaker.");
  assert.equal(row(app, BRICK).querySelectorAll("[data-install]").length, 0);
  assert.equal(stateOfRow(app, SECOND).dataset.firmwareState, "idle", "the other speaker was not touched");
  assert.equal(queryAllByLabel(app, "Install image brick-2-0-0 (version 2.0.0) on Living right").length, 1);
  // The update is still staged afterwards, and looking at it again sends nothing.
  server.send(house(200));
  await rendered(app);
  assert.equal(queryAllByLabel(app, INSTALL).length, 1);
  assert.equal(server.commands.length, 1, "exactly one install was sent");
});

test("a question about an update that has gone is withdrawn, and a speaker that is not connected cannot be installed", async () => {
  const server = firmwareServer();
  const app = await open(server);
  getByLabel(app, INSTALL).click();
  await rendered(app);
  assert.equal(queryAllByLabel(app, YES).length, 1);
  // The image is replaced on the server while the question is up: the question is about nothing now.
  server.send(house(2, SPEAKERS, [image("brick-2-0-1", "2.0.1", "brick-s3-wired")]));
  await rendered(app);
  assert.equal(queryAllByLabel(app, YES).length, 0);
  assert.equal(row(app, BRICK).querySelector("[data-install-question]"), null);
  assert.deepEqual(updates(app, BRICK), ["brick-2-0-1"]);

  server.send(house(3, [speaker(BRICK, "Living left", IDLE, { present: false })]));
  await rendered(app);
  const button = getByLabel(app, INSTALL);
  assert.equal(button.disabled, true);
  assert.match(text(row(app, BRICK).querySelector("[data-update-absent]")), /not connected/);
  button.click();
  await rendered(app);
  assert.equal(row(app, BRICK).querySelector("[data-install-question]"), null);
  assert.deepEqual(server.commands, []);
});

test("firmware_cancel and firmware_rescan are sent from their controls", async () => {
  const receiving = { ...IDLE, state: "receiving", image: "brick-2-0-0", image_version: "2.0.0", received: 384000, size: 1536000 };
  const next = image("brick-3-0-0", "3.0.0", "brick-s3-wired");
  const server = firmwareServer({ speakers: [speaker(BRICK, "Living left", receiving), SPEAKERS[1]], staged: [...IMAGES, next] });
  const app = await open(server);
  // Only an install that can still be abandoned has the control.
  assert.equal(queryAllByLabel(app, "Cancel the install on Living right").length, 0);
  assert.equal(text(stateOfRow(app, BRICK)), "Receiving image brick-2-0-0 (version 2.0.0): 384000 of 1536000 bytes.");
  const progress = getByLabel(app, "Install progress of Living left");
  assert.deepEqual([progress.getAttribute("value"), progress.getAttribute("max")], ["384000", "1536000"]);
  getByLabel(app, "Cancel the install on Living left").click();
  await rendered(app);
  assert.deepEqual(server.commands, [`{"v":2,"t":"firmware_cancel","speaker":"${BRICK}"}`]);
  assert.equal(server.commands[0], fixture("firmware_cancel.json"));
  assert.equal(stateOfRow(app, BRICK).dataset.firmwareState, "cancelled");
  assert.equal(queryAllByLabel(app, "Cancel the install on Living left").length, 0);

  const staged = () => [...getByLabel(app, "Staged images").querySelectorAll("li[data-image]")].map((entry) => entry.dataset.image);
  assert.deepEqual(staged(), IMAGES.map((held) => held.name)); // prettier-ignore
  getByLabel(app, "Rescan the staged firmware images").click();
  await rendered(app);
  assert.deepEqual(server.commands.slice(1), ['{"v":2,"t":"firmware_rescan"}']);
  assert.equal(server.commands[1], fixture("firmware_rescan.json"));
  assert.deepEqual(staged(), [...IMAGES.map((held) => held.name), "brick-3-0-0"]); // prettier-ignore
  // What the rescan found is an update to look at, and nothing more was sent for it.
  assert.deepEqual(updates(app, SECOND), ["brick-2-0-0", "brick-3-0-0"]);
  assert.equal(server.commands.length, 2);
});

test("the staged images are listed with the server's verdict, and a refused one is never offered", async () => {
  const app = await open(fakeServer(house(1)));
  const entry = (name) => getByLabel(app, "Staged images").querySelector(`li[data-image="${name}"]`);
  const shown = (name) => ["version", "board", "verdict"].map((field) => text(entry(name).querySelector(`[data-value="${field}"]`)));
  assert.deepEqual(shown("brick-2-0-0"), ["2.0.0", "brick-s3-wired", "Verified"]);
  assert.deepEqual(shown("brick-tampered"), ["3.0.0", "brick-s3-wired", "Refused: digest-mismatch. It is never offered to a speaker."]);
  assert.equal(screenOf(app).shadowRoot.querySelectorAll('[data-update="brick-tampered"]').length, 0);
  // A server with a directory and nothing in it says so, and can still be asked to look again.
  const empty = await open(fakeServer(house(1, [speaker(BRICK, "Living left", { ...IDLE, update_available: false })], [])));
  assert.equal(text(screenOf(empty).shadowRoot.querySelector("[data-no-images]")), "No image is staged.");
  assert.equal(getByLabel(empty, "Rescan the staged firmware images").localName, "button");
});

// Every state of docs/control-plane.md ("A speaker's `firmware.state` is what
// it is doing ... or how the last install this server process saw ended"),
// each with a reason the server gives with it, and what the screen says.
const INSTALLING = { image: "brick-2-0-0", image_version: "2.0.0", received: 0, size: 1536000 };
const STATES = [
  ["idle", "none", {}, "No install is in progress."],
  ["requested", "none", INSTALLING, "Install requested: the server is offering image brick-2-0-0 (version 2.0.0) to the speaker."],
  ["receiving", "bad_offset", { ...INSTALLING, received: 65536 }, "Receiving image brick-2-0-0 (version 2.0.0): 65536 of 1536000 bytes. Reason: bad_offset."],
  ["verified", "none", { ...INSTALLING, received: 1536000 }, "Written and checked: image brick-2-0-0 (version 2.0.0). The speaker restarts into it."],
  ["pending_verify", "not_confirmed", { ...INSTALLING, version: "2.0.0", slot: 1 }, "On trial: the speaker runs version 2.0.0 and has not confirmed it yet. Reason: not_confirmed."],
  ["confirmed", "none", { ...INSTALLING, version: "2.0.0", slot: 1, update_available: false }, "Installed: image brick-2-0-0 (version 2.0.0) confirmed itself, and the speaker runs version 2.0.0."],
  ["rolled_back", "not_confirmed", { image: null, image_version: "3.0.0" }, "Rolled back: version 3.0.0 did not confirm, and the speaker runs version 1.0.0 again. Nothing retries it. Reason: not_confirmed."],
  ["refused", "too_large", INSTALLING, "Refused by the speaker: image brick-2-0-0 (version 2.0.0) was not installed. Reason: too_large."],
  ["interrupted", "session_ended", INSTALLING, "Interrupted: the install of image brick-2-0-0 (version 2.0.0) did not finish and is not resumed. Install again to start over. Reason: session_ended."],
  ["cancelled", "none", INSTALLING, "Cancelled: the install of image brick-2-0-0 (version 2.0.0) was abandoned."],
]; // prettier-ignore

test("each firmware state the catalog lists is rendered in words, with its reason", async () => {
  // The catalog's own list (crates/control/src/firmware.rs names the same ten).
  const catalog = readFileSync(new URL("../../docs/control-plane.md", import.meta.url), "utf8");
  const listed = /A speaker's `firmware\.state` is what it is doing \(([^)]*)\) or how the last install this\s+server process saw ended \(([^)]*)\)/.exec(catalog);
  assert.ok(listed, "docs/control-plane.md lists the firmware states");
  const names = `${listed[1]}, ${listed[2]}`.match(/`[a-z_]+`/g).map((name) => name.slice(1, -1));
  assert.deepEqual(STATES.map(([state]) => state), names); // prettier-ignore

  const server = fakeServer(house(1));
  const app = await open(server);
  let serial = 1;
  for (const [state, reason, more, words] of STATES) {
    serial += 1;
    server.send(house(serial, [speaker(BRICK, "Living left", { ...IDLE, state, reason, ...more })]));
    await rendered(app);
    const shown = stateOfRow(app, BRICK);
    assert.equal(shown.dataset.firmwareState, state);
    assert.equal(shown.getAttribute("role"), "status");
    assert.equal(text(shown), words, state);
    assert.equal(text(shown.querySelector("[data-firmware-reason]") ?? document.createElement("i")), reason === "none" ? "" : `Reason: ${reason}.`);
    // An install can be abandoned only while it is requested or being received.
    assert.equal(queryAllByLabel(app, "Cancel the install on Living left").length, ["requested", "receiving"].includes(state) ? 1 : 0, state);
    // And none is offered while one is in progress.
    const busy = ["requested", "receiving", "verified", "pending_verify"].includes(state);
    assert.equal(row(app, BRICK).querySelectorAll("[data-install]").length, busy || state === "confirmed" ? 0 : 1, state);
  }
  // The reasons a speaker and the server give are the server's words, whichever they are.
  for (const reason of ["bad_digest", "write_failed", "busy", "wrong_board", "medium_refused", "not_resumed"]) {
    serial += 1;
    server.send(house(serial, [speaker(BRICK, "Living left", { ...IDLE, state: "refused", reason, ...INSTALLING })]));
    await rendered(app);
    assert.equal(text(stateOfRow(app, BRICK).querySelector("[data-firmware-reason]")), `Reason: ${reason}.`);
  }
  // A state a later server adds is said in its own word, not hidden.
  assert.equal(firmwareStateWords({ state: "paused", version: "1.0.0" }), "Firmware state: paused.");
  assert.deepEqual(server.commands, [], "showing a state sends nothing");
});

// The refusals of `firmware_install` by name (docs/control-plane.md,
// "Firmware: staged images and explicit installs"), each with the server's
// detail: the catalog's own vector where fixtures/control/v2 has one.
const REFUSALS = [
  ["unknown-image", "image", fixture("error-firmware-install-unknown-image.json")],
  ["image-not-verified", "image", fixture("error-firmware-install-not-verified.json")],
  ["speaker-absent", "speaker", fixture("error-firmware-install-absent.json")],
  ["not-updatable", "speaker", `{"v":2,"t":"error","field":"speaker","detail":"not-updatable: speaker '${BRICK}' has not said what firmware it runs, so it takes no updates"}`],
  ["busy", "speaker", fixture("error-firmware-install-busy.json")],
  ["wrong-board", "speaker", fixture("error-firmware-install-wrong-board.json")],
  ["already-running", "speaker", fixture("error-firmware-install-already-running.json")],
  ["owner-not-at-bench", "speaker", `{"v":2,"t":"error","field":"speaker","detail":"owner-not-at-bench: speaker '${BRICK}' is not on this host, and an install to a real device is the owner's"}`],
]; // prettier-ignore

test("each install refusal is shown by the name the server gave it, on the speaker it was for, and nothing changes", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  for (const [name, field, answer] of REFUSALS) {
    const refused = JSON.parse(answer);
    assert.equal(refused.field, field);
    assert.ok(refused.detail.startsWith(`${name}: `), name);
    server.answer = () => ({ status: 400, body: answer });
    getByLabel(app, INSTALL).click();
    await rendered(app);
    getByLabel(app, YES).click();
    await rendered(app);
    assert.equal(alertOf(app, BRICK), `Refused: ${refused.detail}`);
    assert.equal(alertOf(app, SECOND), "");
    assert.equal(stateOfRow(app, BRICK).dataset.firmwareState, "idle", "a refused install changed nothing");
    assert.equal(queryAllByLabel(app, INSTALL).length, 1);
  }
  assert.equal(server.commands.length, REFUSALS.length);
  assert.deepEqual([...new Set(server.commands)], [`{"v":2,"t":"firmware_install","speaker":"${BRICK}","image":"brick-2-0-0"}`]);
  // An unknown speaker is the catalog's usual refusal, with no name of its own: shown as it stands.
  const unknown = JSON.parse(fixture("error-firmware-install-unknown-speaker.json"));
  server.answer = () => ({ status: 400, body: JSON.stringify(unknown) });
  getByLabel(app, INSTALL).click();
  await rendered(app);
  getByLabel(app, YES).click();
  await rendered(app);
  assert.equal(alertOf(app, BRICK), `Refused: ${unknown.detail}`);
});

test("a refused cancel and a refused rescan are shown by name too", async () => {
  const requested = { ...IDLE, state: "requested", ...INSTALLING };
  const server = fakeServer(house(1, [speaker(BRICK, "Living left", requested)]));
  const app = await open(server);
  const nothing = JSON.parse(fixture("error-firmware-cancel-nothing.json"));
  server.answer = () => ({ status: 400, body: JSON.stringify(nothing) });
  getByLabel(app, "Cancel the install on Living left").click();
  await rendered(app);
  assert.match(alertOf(app, BRICK), /^Refused: nothing-to-cancel: /);
  assert.equal(alertOf(app, BRICK), `Refused: ${nothing.detail}`);

  const noDir = JSON.parse(fixture("error-firmware-rescan-no-dir.json"));
  server.answer = () => ({ status: 400, body: JSON.stringify(noDir) });
  getByLabel(app, "Rescan the staged firmware images").click();
  await rendered(app);
  const images = getByLabel(app, "Firmware images");
  assert.equal(text(images.querySelector("[role=alert]")), `Refused: ${noDir.detail}`);
  assert.match(noDir.detail, /^no-firmware-dir: /);
});

test("a server with no firmware key in its state shows no update controls", async () => {
  // Its speakers may still say what they run, and even carry the flag: with
  // no staged images there is nothing to install and nothing to rescan.
  const installing = speaker(SECOND, "Living right", { ...IDLE, state: "receiving", ...INSTALLING, received: 1024 });
  const server = fakeServer(house(1, [SPEAKERS[0], installing, SPEAKERS[3]], null));
  const app = await open(server);
  assert.equal(server.snapshot.firmware, undefined);
  assert.deepEqual(updateControls(app), []);
  assert.equal(screenOf(app).shadowRoot.querySelector("[data-firmware-images]"), null);
  assert.equal(screenOf(app).shadowRoot.querySelector("[data-update-available]"), null);
  assert.equal(screenOf(app).shadowRoot.querySelectorAll("[data-update]").length, 0);
  for (const label of [INSTALL, "Cancel the install on Living right", "Rescan the staged firmware images"]) {
    assert.equal(queryAllByLabel(app, label).length, 0, label);
  }
  // What a speaker runs is information, and is still shown.
  assert.equal(value(app, BRICK, "firmware-version"), "1.0.0");
  assert.equal(stateOfRow(app, SECOND).dataset.firmwareState, "receiving");
  // The rest of the screen is what it was.
  assert.equal(getByLabel(app, "Forget Living left").localName, "button");

  // The controls come with the key, with no reload, and go with it.
  server.send(house(2));
  await rendered(app);
  assert.equal(updateControls(app).length > 0, true);
  assert.equal(queryAllByLabel(app, INSTALL).length, 1);
  server.send(house(3, SPEAKERS, null));
  await rendered(app);
  assert.deepEqual(updateControls(app), []);
  assert.deepEqual(server.commands, []);
});

test("the screen has no control that installs on every speaker, forces an install or uploads an image", async () => {
  const app = await open(fakeServer(house(1)));
  const controls = [...screenOf(app).shadowRoot.querySelectorAll("button, a, input, select")];
  assert.deepEqual(
    controls.filter((control) => /\ball\b|every|force|upload|automatic/i.test(`${control.getAttribute("aria-label")} ${text(control)}`)),
    [],
  );
  assert.equal(screenOf(app).shadowRoot.querySelectorAll('input[type="file"]').length, 0);
  const source = readFileSync(new URL("../src/api.js", import.meta.url), "utf8");
  assert.doesNotMatch(source.replace(/^\s*\/\/.*$/gm, ""), /"all"|"force"/);
});
