// The live test of firmware on the speakers screen (`make web-live`): the
// app's own elements, navigation and state layer, in node under happy-dom
// with no browser, against a real chorus-server started with
// `--firmware-dir`, holding an image staged with the server's own
// `chorus-server stage-firmware`.
//
// The speaker is a real session on loopback that reports an older version: a
// scripted endpoint (endpoint.js, `updatableSpeaker`). The screen shows the
// update; nothing changes while it is only shown, rescanned or asked about
// (the server logs no offer and the endpoint is sent none); the install
// action, pressed and confirmed on the screen, is what makes the server's own
// `GET /api/state` leave `idle`. The install is then cancelled from the
// screen, started again, taken to `verified` by the endpoint and confirmed by
// its next session.
//
// What this is not: an install. The endpoint is a script that keeps the bytes
// in memory and checks their digest; there is no flash, no bootloader and no
// board, and its "restart" is a socket closing. The update unit the board
// runs, against the real server, is crates/server/tests/firmware_install.rs
// (the host build of the C endpoint), which this step of the gate cannot have:
// it runs before the gate builds the C endpoint. Nothing here ran on a board.
//
// Loopback only. This file never sets the owner's bench variable
// (CHORUS_OWNER_AT_BENCH) and refuses to run where something else has: the
// server's guard does not apply to a speaker on the server's own host.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run this file without it.

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { promisify } from "node:util";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { capabilities, firmwareStatus, updatableSpeaker } from "./endpoint.js";
import { startHouse, until } from "./house.js";

const run = promisify(execFile);

// The id the endpoint's session authenticates as, the board it says it is,
// and what it runs before and after.
const SPEAKER = "live-test-updatable";
const BOARD = "live-test-board";
const RUNS = "1.0.0";
const NEXT = "2.0.0";
const IMAGE = "live-2-0-0";
const TAMPERED = "live-tampered";
const IMAGE_BYTES = 200_000;
// Where the endpoint stops taking chunks, so a transfer under way can be
// looked at: 32 of the server's 1024-byte chunks, two acknowledgements.
const HOLD_AT = 32_768;

let scratch;
let house;
let port;
let speaker;
let store;
let app;
// The endpoint's static key, the same on its next boot.
const secret = randomBytes(32);
// The staged image's bytes, and the name the server made for the speaker.
let staged;
let made;

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const screen = () => app.shadowRoot.querySelector("chorus-speakers");
const row = () => screen()?.shadowRoot.querySelector(`li[data-speaker="${SPEAKER}"]`) ?? null;
const shownState = () => row()?.querySelector("[data-firmware-state]")?.dataset.firmwareState ?? null;
const shown = (name) => text(row().querySelector(`[data-value="${name}"]`));
const installLabel = () => `Install image ${IMAGE} (version ${NEXT}) on ${made}`;
const serverSpeaker = async () => ((await house.state()).speakers ?? []).find((held) => held.id === SPEAKER) ?? null;
const firmware = async () => (await serverSpeaker())?.firmware ?? null;
// How many `firmware_offer`s the server says it has sent: one log line each.
const offers = () => house.log().split("\n").filter((line) => line.includes("firmware offer ")).length;

// An image the server's staging accepts: the ESP image magic, and an
// application description (its magic word, then the version at offset 16 of
// it) where the server reads the version from. The rest is not a program.
function imageOf(version, bytes) {
  const image = Buffer.alloc(bytes);
  for (let at = 0; at < bytes; at += 1) image[at] = (at * 31 + (at >> 8)) & 0xff;
  image[0] = 0xe9;
  image.writeUInt32LE(0xabcd5432, 32);
  image.fill(0, 48, 80);
  image.write(version, 48, "utf8");
  return image;
}

async function stage(dir, name, version) {
  const built = join(scratch, `build-${name}.bin`);
  await writeFile(built, imageOf(version, IMAGE_BYTES));
  await run(process.env.CHORUS_SERVER_BIN, ["stage-firmware", "--image", built, "--board", BOARD, "--name", name, "--firmware-dir", dir]);
  return readFile(join(dir, `${name}.bin`));
}

const vector = (name) =>
  readFile(new URL(`../../fixtures/protocol/v2/${name}.hex`, import.meta.url), "utf8").then((hex) =>
    Buffer.from(hex.replace(/#.*$/gm, "").replace(/\s+/g, ""), "hex"),
  );

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  assert.equal(process.env.CHORUS_OWNER_AT_BENCH, undefined, "the owner's bench variable is not set here, and this test never sets it");
  scratch = await mkdtemp(join(tmpdir(), "chorus-web-live-"));
  const dir = join(scratch, "firmware");
  await mkdir(dir);
  staged = await stage(dir, IMAGE, NEXT);
  // A second image whose bytes changed after its manifest was written.
  const tampered = await stage(dir, TAMPERED, "3.0.0");
  tampered[70_000] ^= 0x01;
  await writeFile(join(dir, `${TAMPERED}.bin`), tampered);

  house = await startHouse(["den"], { identityDir: scratch, extra: ["--firmware-dir", dir] });
  [, port] = await house.said(/chorus-server: listening on=\S*?:(\d+)/);

  store = createStore(createClient({ base: `${house.origin}/` }));
  app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
});

after(async () => {
  store?.stop();
  app?.remove();
  await speaker?.stop();
  await house?.stop();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

test("the scripted endpoint's firmware messages are the protocol's own vectors", async () => {
  const brick = { version: "0.2.0", board: "brick-s3-wired", slot: 0 };
  assert.deepEqual(firmwareStatus(brick), await vector("firmware_status"));
  assert.deepEqual(
    firmwareStatus({ ...brick, transfer: 7, state: "receiving", received: 65_536, imageVersion: "0.3.0" }),
    await vector("firmware_status_receiving"),
  );
  assert.deepEqual(
    capabilities({ codecs: 0b111, sampleFormats: 0b011, maxChannels: 2, ratesHz: [44_100, 48_000], bufferMs: 300, latencyNs: 0, leds: 0, bands: 0, features: 0b10 }),
    await vector("capabilities_ota"),
  ); // prettier-ignore
});

test("the screen shows the update: what the speaker runs, and the staged image's version", async () => {
  await until("the link on the home", () => queryAllByLabel(app, "Speakers and their setup").length, 1);
  getByLabel(app, "Speakers and their setup").click();
  await until("the speakers screen", () => Boolean(screen()), true);
  await until("the store's status", () => store.view().status, "live");

  // The server lists what was staged, each with its verdict.
  const images = (await house.state()).firmware.images;
  assert.deepEqual(images.map((image) => [image.name, image.version, image.board, image.size, image.verdict, image.reason]), [
    [IMAGE, NEXT, BOARD, IMAGE_BYTES, "verified", undefined],
    [TAMPERED, "3.0.0", BOARD, IMAGE_BYTES, "refused", "digest-mismatch"],
  ]); // prettier-ignore
  assert.equal(images[0].sha256, createHash("sha256").update(staged).digest("hex"));

  speaker = await updatableSpeaker({ host: "127.0.0.1", port: Number(port), endpoint: SPEAKER, secret, version: RUNS, board: BOARD, holdAt: HOLD_AT });
  await until("the server's word on the speaker's firmware", async () => {
    const held = await firmware();
    return held && [held.version, held.board, held.slot, held.state, held.update_available, held.image];
  }, [RUNS, BOARD, 0, "idle", true, null]); // prettier-ignore
  made = (await serverSpeaker()).name;

  await until("the screen's speaker", () => Boolean(row()), true);
  await until("the screen's update", () => [...row().querySelectorAll("[data-update]")].map((entry) => entry.dataset.update), [IMAGE]);
  assert.equal(text(row().querySelector("[data-update-available]")), "Update available");
  assert.equal(text(row().querySelector("[data-update-version]")), `Version ${NEXT}, image ${IMAGE}`);
  assert.deepEqual([shown("firmware-version"), shown("firmware-board"), shown("firmware-slot")], [RUNS, BOARD, "0"]);
  assert.equal(shownState(), "idle");
  // The refused image is listed with its reason and is not offered.
  const entry = getByLabel(app, "Staged images").querySelector(`li[data-image="${TAMPERED}"]`);
  assert.match(text(entry.querySelector('[data-value="verdict"]')), /^Refused: digest-mismatch\./);
  assert.equal(row().querySelectorAll(`[data-update="${TAMPERED}"]`).length, 0);
});

test("nothing changes until the install action: showing, a rescan and the question send no offer", async () => {
  const quiet = async (when) => {
    const held = await firmware();
    assert.deepEqual([held.state, held.version, held.image, held.update_available], ["idle", RUNS, null, true], when);
    assert.equal(offers(), 0, `the server sent no offer ${when}`);
    assert.deepEqual(speaker.statuses(), ["idle"], `the endpoint was offered nothing ${when}`);
  };
  for (let read = 0; read < 10; read += 1) await quiet("while the update is shown");

  // A rescan from the screen is not an install.
  const serial = (await house.state()).serial;
  getByLabel(app, "Rescan the staged firmware images").click();
  await until("the rescan is applied", async () => (await house.state()).serial > serial, true);
  await quiet("after a rescan");
  assert.equal(text(getByLabel(app, "Firmware images").querySelector("[role=alert]")), "", "the rescan was not refused");

  // The first press only asks; "Not now" sends nothing.
  getByLabel(app, installLabel()).click();
  await until("the question", () => Boolean(row().querySelector("[data-install-question]")), true);
  assert.match(text(row().querySelector("[data-install-question]")), new RegExp(`^Install image ${IMAGE} \\(version 2\\.0\\.0\\) on ${made} \\(${SPEAKER}\\)\\?`));
  await quiet("while the question is up");
  getByLabel(app, `Do not install image ${IMAGE} on ${made}`).click();
  await until("the question is gone", () => Boolean(row().querySelector("[data-install-question]")), false);
  for (let read = 0; read < 10; read += 1) await quiet("after the question was declined");
  const report = await (await fetch(`${house.origin}/api/report`)).text();
  assert.match(report, /control applied=1 refused=0/, "the rescan is the one command this server has been sent");
});

test("the install action, pressed and confirmed, makes the server's firmware state leave idle", async () => {
  getByLabel(app, installLabel()).click();
  await until("the question", () => queryAllByLabel(app, `Yes, install image ${IMAGE} on ${made}`).length, 1);
  assert.equal((await firmware()).state, "idle", "the question changed nothing");
  getByLabel(app, `Yes, install image ${IMAGE} on ${made}`).click();

  // GET /api/state: the speaker's firmware state has left idle, for that image.
  await until("the server's firmware state leaves idle", async () => (await firmware()).state !== "idle", true);
  // The endpoint holds after 32 chunks, so the transfer is under way and stays there.
  await until("the transfer under way", async () => {
    const held = await firmware();
    return [held.state, held.image, held.image_version, held.received, held.size, held.version];
  }, ["receiving", IMAGE, NEXT, HOLD_AT, IMAGE_BYTES, RUNS]); // prettier-ignore
  assert.equal(offers(), 1, "one install action, one offer");
  await house.said(new RegExp(`firmware offer .*${SPEAKER}.*image=${IMAGE}`));
  assert.equal(text(row().querySelector(":scope > [role=alert]")), "", "the install was not refused");

  // The screen shows the progress, and offers no second install while this one runs.
  await until("the screen's firmware state", () => text(row().querySelector("[data-firmware-state]")), `Receiving image ${IMAGE} (version ${NEXT}): ${HOLD_AT} of ${IMAGE_BYTES} bytes.`);
  const progress = getByLabel(app, `Install progress of ${made}`);
  assert.deepEqual([progress.getAttribute("value"), progress.getAttribute("max")], [String(HOLD_AT), String(IMAGE_BYTES)]);
  assert.equal(row().querySelectorAll("[data-install]").length, 0);
});

test("cancel from the screen abandons the transfer, and the screen says cancelled", async () => {
  getByLabel(app, `Cancel the install on ${made}`).click();
  await until("the server's firmware state", async () => (await firmware()).state, "cancelled");
  await until("the screen's firmware state", () => shownState(), "cancelled");
  assert.equal(text(row().querySelector("[data-firmware-state]")), `Cancelled: the install of image ${IMAGE} (version ${NEXT}) was abandoned.`);
  // The endpoint was told to drop it, and did.
  await until("the endpoint's last status", () => speaker.statuses().at(-1), "idle");
  assert.equal((await firmware()).version, RUNS);
  // The update is still staged: it can be installed again, and only by asking again.
  await until("the install is offered again", () => queryAllByLabel(app, installLabel()).length, 1);
  assert.equal(offers(), 1);
  // With nothing to cancel there is no control for it.
  assert.equal(queryAllByLabel(app, `Cancel the install on ${made}`).length, 0);
});

test("installed again and let through, the image arrives whole and the speaker's next session confirms it", async () => {
  speaker.release();
  getByLabel(app, installLabel()).click();
  await until("the question", () => queryAllByLabel(app, `Yes, install image ${IMAGE} on ${made}`).length, 1);
  getByLabel(app, `Yes, install image ${IMAGE} on ${made}`).click();

  // The endpoint took every byte, found the digest good, said so and "restarted".
  const arrived = await speaker.restarted;
  assert.equal(arrived.version, NEXT);
  assert.equal(arrived.image.equals(staged), true, "the staged image arrived byte for byte");
  assert.equal(offers(), 2, "one offer for each install action");
  await until("the server's speaker", async () => {
    const held = await serverSpeaker();
    return [held.present, held.firmware.state, held.firmware.received];
  }, [false, "verified", IMAGE_BYTES]); // prettier-ignore
  await until("the screen's firmware state", () => shownState(), "verified");
  await until("the screen's presence", () => shown("present"), "Not connected");

  // Its next boot: the same key, the new version from the other slot, on
  // trial and then confirmed (the script says so; no image booted).
  await speaker.stop();
  speaker = await updatableSpeaker({ host: "127.0.0.1", port: Number(port), endpoint: SPEAKER, secret, version: NEXT, board: BOARD, slot: 1, trial: arrived.transfer });
  await until("the server's word on the speaker's firmware", async () => {
    const held = await firmware();
    return [held.state, held.version, held.slot, held.image, held.update_available];
  }, ["confirmed", NEXT, 1, IMAGE, false]); // prettier-ignore
  await until("the screen's firmware state", () => shownState(), "confirmed");
  assert.equal(text(row().querySelector("[data-firmware-state]")), `Installed: image ${IMAGE} (version ${NEXT}) confirmed itself, and the speaker runs version ${NEXT}.`);
  assert.equal(shown("firmware-version"), NEXT);
  // It runs the staged version now: no update is shown, and nothing more was offered.
  assert.equal(row().querySelector("[data-update-available]"), null);
  assert.equal(row().querySelectorAll("[data-install]").length, 0);
  assert.equal(offers(), 2);
});
