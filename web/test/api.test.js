// The API client (src/api.js) against a scripted server: the command bodies
// are the catalog's bytes, a refusal is the server's words, and the event
// stream is read whole whatever way its bytes arrive and opened again when
// it ends or goes silent.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { RETRY_MS, SILENCE_MS, createClient, muteCommand, volumeCommand, volumeLiteral } from "../src/api.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";

const fixture = (name) => readFileSync(new URL(`../../fixtures/control/${name}`, import.meta.url), "utf8").trim();

function clientOf(server, timers = fakeTimers()) {
  return createClient({ fetch: server.fetch, base: server.base, timers });
}

test("a volume is written with exactly three fractional digits", () => {
  assert.equal(volumeLiteral(500), "0.500");
  assert.equal(volumeLiteral(0), "0.000");
  assert.equal(volumeLiteral(1000), "1.000");
  assert.equal(volumeLiteral(7), "0.007");
  assert.equal(volumeLiteral(1200), "1.000");
  assert.equal(volumeLiteral(-3), "0.000");
});

test("the volume and mute commands are the catalog's own bytes", () => {
  assert.equal(volumeCommand("kitchen", 500), fixture("volume.json"));
  assert.equal(muteCommand("kitchen", true), fixture("mute.json"));
  assert.equal(muteCommand("den", false), '{"v":1,"t":"mute","zone":"den","muted":false}');
});

test("an accepted command resolves to the state that resulted", async () => {
  const server = fakeServer(stateOf(4, [zone("kitchen")]));
  const result = await clientOf(server).command(volumeCommand("kitchen", 250));
  assert.deepEqual(server.commands, ['{"v":1,"t":"volume","zone":"kitchen","volume":0.250}']);
  assert.equal(result.ok, true);
  assert.equal(result.state.serial, 4);
});

test("a refused command resolves to the server's refusal text", async () => {
  const server = fakeServer(stateOf(1, []));
  const detail = "there is no zone 'attic'; the zones configured on this server are kitchen";
  server.answer = () => ({ status: 400, body: JSON.stringify({ v: 1, t: "error", field: "zone", detail }) });
  assert.deepEqual(await clientOf(server).command(muteCommand("attic", true)), { ok: false, refusal: detail });

  // A refusal that is not a catalog message says its own text, and one with
  // no text its status.
  server.answer = () => ({ status: 403, body: "the Origin header names another server\n" });
  assert.equal((await clientOf(server).command("{}")).refusal, "the Origin header names another server");
  server.answer = () => ({ status: 503, body: "" });
  assert.equal((await clientOf(server).command("{}")).refusal, "the server answered 503");
});

test("a command to a server that cannot be reached says so and does not throw", async () => {
  const client = createClient({
    fetch: async () => {
      throw new TypeError("fetch failed");
    },
    base: "http://chorus.test/",
    timers: fakeTimers(),
  });
  assert.deepEqual(await client.command("{}"), { ok: false, refusal: "the server could not be reached" });
});

test("the event stream delivers each state message, however its bytes are cut", async () => {
  const server = fakeServer();
  const seen = [];
  const statuses = [];
  const close = clientOf(server).events({
    onState: (state) => seen.push(state.serial),
    onStatus: (status) => statuses.push(status),
  });
  await settle();
  assert.equal(server.streams, 1);
  assert.deepEqual(statuses, []);

  const first = `data: ${JSON.stringify(stateOf(1, [zone("kitchen")]))}\n\n`;
  server.write(first.slice(0, 20));
  await settle();
  assert.deepEqual(seen, []);
  server.write(first.slice(20));
  // The keepalive comment, then two events in one chunk, one with CRLF.
  server.write(`: keepalive\n\ndata: ${JSON.stringify(stateOf(2, []))}\r\n\r\ndata: ${JSON.stringify(stateOf(3, []))}\n\n`);
  await settle();
  assert.deepEqual(seen, [1, 2, 3]);
  assert.deepEqual([...new Set(statuses)], ["live"]);
  close();
});

test("a stream the server closes is reported lost and opened again", async () => {
  const server = fakeServer();
  const timers = fakeTimers();
  const seen = [];
  const statuses = [];
  const close = clientOf(server, timers).events({
    onState: (state) => seen.push(state.serial),
    onStatus: (status) => statuses.push(status),
  });
  await settle();
  server.send(stateOf(1, []));
  await settle();
  server.drop();
  await settle();
  assert.deepEqual(statuses, ["live", "lost"]);
  assert.equal(server.streams, 1);

  timers.fire(RETRY_MS);
  await settle();
  assert.equal(server.streams, 2);
  server.send(stateOf(2, []));
  await settle();
  assert.deepEqual(seen, [1, 2]);
  assert.deepEqual(statuses, ["live", "lost", "live"]);

  // Closed for good: nothing more is opened.
  close();
  await settle();
  timers.fire(RETRY_MS);
  await settle();
  assert.equal(server.streams, 2);
  assert.equal(timers.pending(RETRY_MS), 0);
  assert.equal(timers.pending(SILENCE_MS), 0);
});

test("a stream that goes silent is reported lost and opened again", async () => {
  const server = fakeServer();
  const timers = fakeTimers();
  const statuses = [];
  const close = clientOf(server, timers).events({ onState: () => {}, onStatus: (status) => statuses.push(status) });
  await settle();
  server.send(stateOf(1, []));
  await settle();
  assert.equal(timers.pending(SILENCE_MS), 1);

  // Nothing arrives, not even the keepalive, for the whole of the bound.
  timers.fire(SILENCE_MS);
  await settle();
  assert.deepEqual(statuses, ["live", "lost"]);
  timers.fire(RETRY_MS);
  await settle();
  assert.equal(server.streams, 2);
  close();
});
