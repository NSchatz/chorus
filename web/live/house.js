// What every live test (`live/*.live.js`) starts with: a real chorus-server
// for the house the test names, a way to command it as another client would,
// its own `GET /api/state`, and a wait that polls instead of sleeping.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run a live test without it.

import assert from "node:assert/strict";
import { spawn } from "node:child_process";

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

// Wait until `read()` gives what `wanted` describes, and fail saying what it
// gave instead. Nothing here sleeps for a fixed time.
export async function until(what, read, wanted) {
  const deadline = Date.now() + 10_000;
  let got;
  for (;;) {
    got = await read();
    try {
      assert.deepEqual(got, wanted);
      return;
    } catch (error) {
      if (Date.now() > deadline) {
        throw new Error(`${what}: after 10 s it is ${JSON.stringify(got)}, expected ${JSON.stringify(wanted)}`, {
          cause: error,
        });
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}
