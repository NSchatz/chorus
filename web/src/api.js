// The server's control plane as the app uses it (docs/control-plane.md, "How
// the messages travel"): GET api/state (the state message, once), GET
// api/events (the event stream: one `data: <state message>` per change,
// starting with the state as it stands) and POST api/command (one control
// message; the answer is the resulting state or a refusal).
//
// The event stream is read with fetch and a stream reader, not EventSource,
// so the reader is this module's own code wherever it runs: the browser and
// the live test (`make web-live`, under node) are the same subscriber. What a
// drop looks like and when to open the stream again are decided here, in one
// place, and are testable with a scripted stream.
//
// `fetch` and `base` are given by the caller. `base` is the server's root as
// the page sees it: the app is served under /app/, so from the page that is
// "../"; the live test gives the server's own address.

// The server writes a comment line on a stream that has had nothing to say
// for 15 s (its keepalive). A stream that delivers no byte for this long is
// held by a server that has stopped answering, which the connection alone
// does not show: it is closed and opened again, and the app says so meanwhile.
export const SILENCE_MS = 40_000;
// How long after a stream ends or fails the next one is opened.
export const RETRY_MS = 1_000;

// A volume in thousandths (0 to 1000) as the catalog writes it: exactly three
// fractional digits. JSON.stringify would write 0.5 where the catalog says
// 0.500, so the command bodies below are built by hand around it.
export function volumeLiteral(thousandths) {
  const held = Math.min(1000, Math.max(0, Math.round(Number(thousandths) || 0)));
  return `${Math.floor(held / 1000)}.${String(held % 1000).padStart(3, "0")}`;
}

// The two room commands this change sends, in the catalog's canonical
// encoding (members in the catalog's order, no whitespace).
export function volumeCommand(zone, thousandths) {
  return `{"v":1,"t":"volume","zone":${JSON.stringify(zone)},"volume":${volumeLiteral(thousandths)}}`;
}

export function muteCommand(zone, muted) {
  return `{"v":1,"t":"mute","zone":${JSON.stringify(zone)},"muted":${muted ? "true" : "false"}}`;
}

// What a refused command says, for a person: the server's own `detail` where
// the answer is one of the catalog's refusals, else the answer's text, else
// the status alone.
async function refusalText(response) {
  let text = "";
  try {
    text = (await response.text()).trim();
  } catch {
    text = "";
  }
  try {
    const answer = JSON.parse(text);
    if (answer && typeof answer.detail === "string" && answer.detail) return answer.detail;
  } catch {
    // Not a catalog message (a 403, 415 or 503 says its reason in plain text).
  }
  return text || `the server answered ${response.status}`;
}

const timersOfTheHost = {
  set: (callback, ms) => globalThis.setTimeout(callback, ms),
  clear: (handle) => globalThis.clearTimeout(handle),
};

export function createClient({ fetch = globalThis.fetch.bind(globalThis), base = "../", timers = timersOfTheHost } = {}) {
  // GET api/state: the state message, or a thrown error naming why not.
  async function state() {
    const response = await fetch(`${base}api/state`, { headers: { Accept: "application/json" }, cache: "no-store" });
    if (!response.ok) throw new Error(`the server answered ${response.status}`);
    return response.json();
  }

  // POST api/command with one control message. Resolves, never rejects:
  // { ok: true, state } with the resulting state message, or
  // { ok: false, refusal } with the words to show.
  async function command(body) {
    let response;
    try {
      response = await fetch(`${base}api/command`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body,
      });
    } catch {
      return { ok: false, refusal: "the server could not be reached" };
    }
    if (!response.ok) return { ok: false, refusal: await refusalText(response) };
    try {
      return { ok: true, state: await response.json() };
    } catch {
      // Applied, with an answer this page could not read: the event stream
      // carries the same state.
      return { ok: true, state: null };
    }
  }

  // GET api/events, held open and opened again whenever it ends. `onState`
  // is called with each state message as it arrives; `onStatus` with "live"
  // when a stream delivers its first message and "lost" when a stream ends,
  // fails or goes silent. Returns the function that closes it for good.
  function events({ onState, onStatus = () => {} }) {
    let closed = false;
    let abort = null;
    let retry = null;
    let silence = null;

    const quiet = () => {
      if (silence !== null) timers.clear(silence);
      silence = null;
    };
    const heard = () => {
      quiet();
      silence = timers.set(() => abort?.abort(), SILENCE_MS);
    };

    // One event of the stream: the `data:` lines of a block, joined. Comment
    // lines (the keepalive) and other fields carry nothing this reads.
    const deliver = (block) => {
      const data = block
        .split("\n")
        .filter((line) => line.startsWith("data:"))
        .map((line) => line.slice(5).replace(/^ /, ""))
        .join("\n");
      if (!data) return;
      let message;
      try {
        message = JSON.parse(data);
      } catch {
        return;
      }
      onStatus("live");
      onState(message);
    };

    async function run() {
      abort = new AbortController();
      heard();
      try {
        const response = await fetch(`${base}api/events`, {
          headers: { Accept: "text/event-stream" },
          cache: "no-store",
          signal: abort.signal,
        });
        if (!response.ok || !response.body) throw new Error(`the server answered ${response.status}`);
        const reader = response.body.getReader();
        abort.signal.addEventListener("abort", () => reader.cancel().catch(() => {}));
        const decoder = new TextDecoder();
        let pending = "";
        for (;;) {
          const { done, value } = await reader.read();
          if (done || closed || abort.signal.aborted) break;
          heard();
          pending += decoder.decode(value, { stream: true }).replace(/\r\n?/g, "\n");
          let end;
          while ((end = pending.indexOf("\n\n")) !== -1) {
            deliver(pending.slice(0, end));
            pending = pending.slice(end + 2);
          }
        }
      } catch {
        // A failed or aborted stream ends like one the server closed.
      }
      quiet();
      if (closed) return;
      onStatus("lost");
      retry = timers.set(() => {
        retry = null;
        run();
      }, RETRY_MS);
    }

    run();
    return () => {
      closed = true;
      quiet();
      if (retry !== null) timers.clear(retry);
      abort?.abort();
    };
  }

  return { state, command, events };
}
