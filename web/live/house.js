// What every live test (`live/*.live.js`) starts with: a real chorus-server
// for the house the test names, a way to command it as another client would,
// its own `GET /api/state`, and a wait that polls instead of sleeping.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run a live test without it.
//
// It also starts what an alarm's sources need beyond an endpoint's line-in
// (endpoint.js), each the way crates/server/tests does it: a stream on
// loopback for a stored stream URL (`startStream`, as
// alarm_stored_sources.rs's endless route) and the fake Soloist under the
// real receiver supervisor for a stored Spotify URI (`startReceiver`, as
// soloist_receivers.rs's bench).

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { connect } from "node:net";
import { dirname, join } from "node:path";

const LISTENING = /control listening on=\S*?:(\d+)/;

// Start chorus-server with one room per id, no audio device, on loopback
// ports of its own choosing. Resolves to { origin, command, state, said, stop }.
//
// `extra` is more of the server's own arguments, for a house that needs more
// than rooms (players, renderers). `identityDir` names a directory for the
// server's key where the house needs one that is kept (the renderers refuse
// an ephemeral identity); without it the identity is ephemeral.
export async function startHouse(roomIds, { extra = [], identityDir = null } = {}) {
  const args = [
    "--listen", "127.0.0.1:0",
    "--control-listen", "127.0.0.1:0",
    ...(identityDir ? ["--identity-dir", identityDir] : ["--ephemeral-identity"]),
    "--allow-non-realtime",
    "--allow-unlocked-memory",
    "--source", "tone",
    "--serve-forever",
    ...extra,
    ...roomIds.flatMap((id) => ["--zone", id]),
  ]; // prettier-ignore
  let log = "";
  const server = spawn(process.env.CHORUS_SERVER_BIN, args, { stdio: ["ignore", "pipe", "pipe"] });
  const port = await new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`chorus-server did not say where it listens within 30 s:\n${log}`)),
      30_000,
    );
    const read = (chunk) => {
      log += chunk;
      const found = LISTENING.exec(log);
      if (found) {
        clearTimeout(timer);
        resolve(Number(found[1]));
      }
    };
    server.stdout.setEncoding("utf8").on("data", read);
    server.stderr.setEncoding("utf8").on("data", read);
    server.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    server.once("exit", (code, signal) => {
      clearTimeout(timer);
      reject(new Error(`chorus-server exited (${code ?? signal}) before it listened:\n${log}`));
    });
  });
  const origin = `http://127.0.0.1:${port}`;

  return {
    origin,
    // Another client of the same server: a command sent straight to it, the
    // way a script, the control page or a second phone would.
    async command(message) {
      const response = await fetch(`${origin}/api/command`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: message,
      });
      const text = await response.text();
      assert.equal(response.status, 200, `the server took ${message}: ${text}`);
    },
    // The server's own GET /api/state, now.
    async state() {
      const response = await fetch(`${origin}/api/state`);
      assert.equal(response.status, 200);
      return response.json();
    },
    // Everything the server has printed so far.
    log: () => log,
    // What the server has printed that matches `pattern` (a line that says
    // where something else of it listens): waits for it, and resolves to the
    // match.
    async said(pattern) {
      const deadline = Date.now() + 30_000;
      for (;;) {
        const found = pattern.exec(log);
        if (found) return found;
        if (Date.now() > deadline || server.exitCode !== null) {
          throw new Error(`chorus-server did not print ${pattern} within 30 s:\n${log}`);
        }
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
    },
    async stop() {
      if (server.exitCode !== null) return;
      const gone = new Promise((resolve) => server.once("exit", resolve));
      server.kill("SIGTERM");
      await gone;
    },
  };
}

// A stream on loopback for a stored stream URL: an Icecast-style answer with
// no length, 16-bit stereo at `rateHz`, for as long as the peer reads (a
// station, not a file: a file that ends while the alarm rings is a fallback
// to the chime). No two frames are alike and none is silence. The server
// fetches from loopback only when started with `--media-allow-loopback`.
// Resolves to { url, opened, closed, stop }: how many fetches began, and how
// many of them the peer has closed.
export async function startStream(rateHz) {
  let opened = 0;
  let closed = 0;
  const sockets = new Set();
  const server = createServer((request, response) => {
    if (request.url !== "/stream") {
      response.writeHead(404, { "Content-Length": "0" }).end();
      return;
    }
    opened += 1;
    response.writeHead(200, { "Content-Type": `audio/L16;rate=${rateHz};channels=2`, "icy-name": "Live Test Radio" });
    let frame = 0;
    const block = () => {
      const bytes = Buffer.alloc(4_800 * 4);
      for (let at = 0; at < 4_800; at += 1, frame += 1) {
        bytes.writeInt16BE(1 + (frame % 30_011), at * 4);
        bytes.writeInt16BE(-(1 + Math.floor(frame / 30_011)), at * 4 + 2);
      }
      return bytes;
    };
    // As fast as the peer takes it: the player reads at the pace it plays.
    const pump = () => {
      while (!response.destroyed && response.write(block()));
    };
    response.on("drain", pump);
    response.once("close", () => {
      closed += 1;
    });
    pump();
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  return {
    url: `http://127.0.0.1:${server.address().port}/stream`,
    opened: () => opened,
    closed: () => closed,
    async stop() {
      for (const socket of sockets) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}

// The two programs a Spotify receiver is tested with, built beside the
// server by any build of every target (`cargo build --workspace
// --all-targets`, which the gate runs before this; `cargo build -p
// chorus-server --examples` alone): the real receiver supervisor and the
// fake Soloist (crates/soloist-fake). No real Soloist, account or network.
function receiverPrograms() {
  const examples = join(dirname(process.env.CHORUS_SERVER_BIN), "examples");
  const programs = { supervisor: join(examples, "server-test-soloistd"), soloist: join(examples, "server-test-fake-soloist") };
  for (const program of Object.values(programs)) {
    assert.ok(existsSync(program), `${program} is not built; build it with \`cargo build -p chorus-server --examples\``);
  }
  return programs;
}

// Receiver 0 of one: the real supervisor, which starts the fake Soloist when
// the server assigns the receiver to a room. `root` is a scratch directory
// (short: a Unix socket's path is at most 107 bytes); the server is started
// with `--soloist-dir <root>/recv --soloist-receivers 1`.
// Resolves to { dir, login, commands, stop }: `login()` is a person choosing
// the device in the Spotify app, without which Soloist has no session to
// play in; `commands()` is every control command the fake has accepted.
export async function startReceiver(root) {
  const { supervisor, soloist } = receiverPrograms();
  const dir = join(root, "recv");
  await mkdir(dir, { recursive: true });
  await writeFile(join(root, "key"), "not-a-real-key-chorus-test\n");
  await writeFile(join(root, "fake0.conf"), "");
  const control = join(root, "app0.sock");
  const child = spawn(
    supervisor,
    [
      "--soloist-dir", dir,
      "--api-key-file", join(root, "key"),
      "--state-dir", join(root, "state"),
      "--cache-dir", join(root, "cache"),
      "--soloist-bin", soloist,
      "--receivers", "1",
      "--receiver", "0",
      "--pipewire", "none",
      "--backoff-min-ms", "40",
      "--backoff-max-ms", "160",
      "--stop-timeout-ms", "3000",
    ],
    {
      stdio: "ignore",
      env: {
        ...process.env,
        FAKE_SOLOIST_CONF: join(root, "fake0.conf"),
        FAKE_SOLOIST_ARGV_LOG: join(root, "argv0.log"),
        FAKE_SOLOIST_COMMAND_LOG: join(root, "commands0.log"),
        FAKE_SOLOIST_CONTROL: control,
        FAKE_SOLOIST_PIPE_DIR: dir,
      },
    },
  ); // prettier-ignore
  await once(child, "spawn");

  // One line to the fake as "the Spotify app": its answer, or null while the
  // fake is not there yet (it is started when the receiver is assigned).
  const says = (line) =>
    new Promise((resolve) => {
      let answer = "";
      const socket = connect(control);
      socket.setEncoding("utf8");
      socket.once("connect", () => socket.write(`${line}\n`));
      socket.on("data", (chunk) => {
        answer += chunk;
        if (answer.includes("\n")) socket.destroy();
      });
      socket.once("error", () => resolve(null));
      socket.once("close", () => resolve(answer.trim() || null));
    });

  return {
    dir,
    login: () => until("the fake Soloist takes the app's login", () => says("login"), "ok", 30_000),
    async commands() {
      return readFile(join(root, "commands0.log"), "utf8").then(
        (text) => text.split("\n").filter(Boolean),
        () => [],
      );
    },
    // The normal way (SIGTERM), so the supervisor stops its Soloist too.
    async stop() {
      if (child.exitCode !== null || child.signalCode !== null) return;
      const gone = once(child, "exit");
      child.kill("SIGTERM");
      await gone;
    },
  };
}

// Wait until `read()` gives what `wanted` describes, and fail saying what it
// gave instead. Nothing here sleeps for a fixed time. `limitMs` is how long
// it may take: longer for what waits on the server's schedule (an alarm's
// minute, a sleep timer's end).
export async function until(what, read, wanted, limitMs = 10_000) {
  const deadline = Date.now() + limitMs;
  let got;
  for (;;) {
    got = await read();
    try {
      assert.deepEqual(got, wanted);
      return;
    } catch (error) {
      if (Date.now() > deadline) {
        throw new Error(`${what}: after ${limitMs / 1000} s it is ${JSON.stringify(got)}, expected ${JSON.stringify(wanted)}`, {
          cause: error,
        });
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}
