// A scripted chorus-server for the unit tests: a `fetch` that answers the
// three routes the app uses, with the test deciding what each says and when.
//
//   server.snapshot        what GET api/state answers (a state message), or
//                          null for a server that cannot be reached
//   server.send(state)     one `data:` event on every open event stream
//   server.write(text)     raw bytes on every open event stream
//   server.drop()          the server closes every open event stream
//   server.answer          what POST api/command answers: (body) =>
//                          { status, body } with `body` a string
//   server.commands        the bodies posted so far
//   server.streams         how many event streams have been opened
//   server.login           how the household login in front of the server
//                          answers in the server's place: null (signed in:
//                          the server answers), "redirect" (a redirect to the
//                          login page, which a request made with
//                          `redirect: "manual"` sees as an `opaqueredirect`)
//                          or "refuse" (a 401)
//   server.requests        every request so far: { route, redirect }
//
// The state messages are shaped like the catalog's v2 state
// (docs/control-plane.md, "The state message"), with the members the app reads.

const encoder = new TextEncoder();

export function zone(id, more = {}) {
  return { id, name: id, group: id, volume: 0.5, muted: false, endpoints: [], present: [], bond: [], ...more };
}

export function stateOf(serial, zones, more = {}) {
  return { v: 2, t: "state", serial, zones, groups: [], ...more };
}

// The server's group volume rule, as docs/control-plane.md documents it
// ("Group volume (K77, Sonos-style)"), for the scripted server to answer a
// `group_volume` with. It is the test's model of the SERVER and is nowhere in
// the app: the group volume G is the average of the rooms' volumes in
// thousandths, rounded half up; setting G' scales every room by G'/G (rounded
// half up) and clamps each to its own effective limit, with no shortfall
// redistributed; from G = 0 every room is set to G'.
const halfUp = (value) => Math.floor(value + 0.5);
const inThousandths = (volume) => Math.round(volume * 1000);

export function groupVolumeOf(state, groupId) {
  const rooms = state.zones.filter((room) => room.group === groupId);
  return halfUp(rooms.reduce((sum, room) => sum + inThousandths(room.volume), 0) / rooms.length);
}

/** The state after `group_volume` on `groupId` asking for `wanted` thousandths. */
export function afterGroupVolume(state, groupId, wanted) {
  const before = groupVolumeOf(state, groupId);
  const zones = state.zones.map((room) => {
    if (room.group !== groupId) return room;
    const scaled = before === 0 ? wanted : halfUp((inThousandths(room.volume) * wanted) / before);
    const limit = inThousandths(room.effective_limit ?? 1);
    return { ...room, volume: Math.min(scaled, limit) / 1000 };
  });
  const next = { ...state, serial: state.serial + 1, zones };
  next.groups = state.groups.map((group) =>
    group.id === groupId ? { ...group, volume: groupVolumeOf(next, groupId) / 1000 } : group,
  );
  return next;
}

function answerOf(status, body) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => body,
    json: async () => JSON.parse(body),
  };
}

// What a browser hands a page for a redirect it was told not to follow.
const opaqueRedirect = () => ({
  ok: false,
  status: 0,
  type: "opaqueredirect",
  body: null,
  text: async () => "",
  json: async () => JSON.parse(""),
});

export function fakeServer(snapshot = null) {
  const open = new Set();
  const server = {
    snapshot,
    commands: [],
    streams: 0,
    login: null,
    requests: [],
    answer: () => ({ status: 200, body: JSON.stringify(server.snapshot) }),
    base: "http://chorus.test/",

    write(text) {
      for (const stream of open) stream.push({ done: false, value: encoder.encode(text) });
    },
    send(state) {
      server.write(`data: ${JSON.stringify(state)}\n\n`);
    },
    drop() {
      for (const stream of [...open]) stream.push({ done: true, value: undefined });
    },

    async fetch(url, options = {}) {
      const route = String(url).slice(server.base.length);
      server.requests.push({ route, redirect: options.redirect });
      if (server.login === "redirect") {
        // A browser follows the redirect unless told not to, and the page is
        // then given the login page where it asked for the server.
        if (options.redirect !== "manual") return { ...answerOf(200, "<!DOCTYPE html><title>Sign in</title>"), redirected: true };
        return opaqueRedirect();
      }
      if (server.login === "refuse") return answerOf(401, "not signed in\n");
      if (route === "api/state") {
        if (server.snapshot === null) throw new TypeError("fetch failed");
        return answerOf(200, JSON.stringify(server.snapshot));
      }
      if (route === "api/command") {
        server.commands.push(options.body);
        const { status, body } = server.answer(options.body);
        return answerOf(status, body);
      }
      if (route === "api/events") {
        server.streams += 1;
        // One stream: chunks queue until read, and a read waits for a chunk.
        const queued = [];
        const waiting = [];
        const stream = {
          push(chunk) {
            if (chunk.done) open.delete(stream);
            const reader = waiting.shift();
            if (reader) reader.resolve(chunk);
            else queued.push(chunk);
          },
        };
        open.add(stream);
        options.signal?.addEventListener("abort", () => {
          open.delete(stream);
          for (const reader of waiting.splice(0)) reader.reject(new Error("aborted"));
        });
        return {
          ok: true,
          status: 200,
          body: {
            getReader: () => ({
              read: () =>
                queued.length > 0
                  ? Promise.resolve(queued.shift())
                  : new Promise((resolve, reject) => waiting.push({ resolve, reject })),
              cancel: async () => {},
            }),
          },
        };
      }
      return answerOf(404, "no such route");
    },
  };
  return server;
}

// Timers a test fires by hand: `timers.fire(ms)` runs every callback set
// for exactly that delay and not yet cleared.
export function fakeTimers() {
  let next = 1;
  const set = new Map();
  return {
    set(callback, ms) {
      set.set(next, { callback, ms });
      return next++;
    },
    clear(handle) {
      set.delete(handle);
    },
    pending: (ms) => [...set.values()].filter((timer) => timer.ms === ms).length,
    fire(ms) {
      for (const [handle, timer] of [...set]) {
        if (timer.ms !== ms) continue;
        set.delete(handle);
        timer.callback();
      }
    },
  };
}

// Let everything already queued (promise reactions, stream reads) run.
export async function settle() {
  for (let turn = 0; turn < 20; turn += 1) await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}
