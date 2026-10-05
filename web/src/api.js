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
// The app sits behind a household login (a reverse proxy with forward
// authentication), and a login lapses. The proxy then answers a request with
// a redirect to its login page, or refuses it with 401. Every request here is
// made with `redirect: "manual"`, so the redirect is not followed: the page
// would be given the login page's HTML where it asked for JSON, and could not
// tell that from a broken server. What comes back instead is an
// `opaqueredirect`, which `signedOut` reads, and the app says "Signed out"
// (docs/decisions/0190-the-app-installs-behind-the-login.md).
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

// The grouping commands of catalog version 2 (docs/control-plane.md, "The
// commands catalog version 2 adds"), in the same canonical encoding;
// fixtures/control/v2 has the vector of each.
//
// `join`: the room plays in the target's group; the target is a room, or a
// formed or saved group.
export function joinCommand(zone, target) {
  return `{"v":2,"t":"join","zone":${JSON.stringify(zone)},"target":${JSON.stringify(target)}}`;
}

// `take` (K78): every room of the target plays in the target's group. On a
// saved group that makes it active; on a room it is how the room leaves the
// group it is in, to play alone in the group named for it.
export function takeCommand(target) {
  return `{"v":2,"t":"take","target":${JSON.stringify(target)}}`;
}

// `take` with a source: the target's group then plays that (an offered input
// is `line-in:<endpoint>/<input>`). The target here is always a group that is
// formed, named by its own id, so no room moves.
export function takeSourceCommand(target, source) {
  return `{"v":2,"t":"take","target":${JSON.stringify(target)},"source":${JSON.stringify(source)}}`;
}

// `group_volume` (K77): the server scales every room of the group; this
// names the group volume asked for and nothing else.
export function groupVolumeCommand(group, thousandths) {
  return `{"v":2,"t":"group_volume","group":${JSON.stringify(group)},"volume":${volumeLiteral(thousandths)}}`;
}

// `sound` (docs/control-plane.md, "Per-room sound"): a room's tone (`bass`
// and `treble`, whole dB), loudness, night mode and speech enhancement. It is
// a partial update, and only the members of `changes` this names are written,
// in the catalog's order: a field that is absent keeps what the room had.
export const TONE_DB = Object.freeze({ min: -10, max: 10 });
const SOUND_TONES = ["bass", "treble"];
const SOUND_SWITCHES = ["loudness", "night", "speech"];

export function soundCommand(zone, changes = {}) {
  let body = `{"v":2,"t":"sound","zone":${JSON.stringify(zone)}`;
  for (const field of SOUND_TONES) {
    if (changes[field] === undefined) continue;
    const held = Math.min(TONE_DB.max, Math.max(TONE_DB.min, Math.round(Number(changes[field]) || 0)));
    body += `,"${field}":${held}`;
  }
  for (const field of SOUND_SWITCHES) {
    if (changes[field] === undefined) continue;
    body += `,"${field}":${changes[field] ? "true" : "false"}`;
  }
  // `tv_upmix` ("The TV path"): what a theater set's surround members play
  // from a stream with no surround channel, one of TV_UPMIXES.
  if (changes.tv_upmix !== undefined) body += `,"tv_upmix":${JSON.stringify(String(changes.tv_upmix))}`;
  return `${body}}`;
}

/** The catalog's words for a room's TV upmix: silence, or the passive matrix surround. */
export const TV_UPMIXES = Object.freeze(["off", "ambient"]);

// `av_trim` (docs/control-plane.md, "The TV path"): a room's signed A/V trim,
// a whole number of milliseconds inside the catalog's range. Positive delays
// the room's TV audio; negative brings it earlier.
export const AV_TRIM_MS = Object.freeze({ min: -100, max: 200 });

const within = (value, { min, max }) => Math.min(max, Math.max(min, Math.round(Number(value) || 0)));

export function avTrimCommand(zone, ms) {
  return `{"v":2,"t":"av_trim","zone":${JSON.stringify(zone)},"av_trim_ms":${within(ms, AV_TRIM_MS)}}`;
}

// `bass_management` ("Per-room sound"): the crossover in Hz, the sub's level
// and its polarity. A partial update like `sound`: only the members of
// `changes` this names are written, in the catalog's order. The level is in
// hundredths of a dB here and is written with exactly two places, as the
// catalog wants it (JSON.stringify would write -3.5 for -3.50).
export const CROSSOVER_HZ = Object.freeze({ min: 40, max: 200 });
export const SUB_LEVEL = Object.freeze({ min: -1200, max: 600 });
export const SUB_POLARITIES = Object.freeze(["normal", "inverted"]);

/** Hundredths of a dB as the catalog writes them: "-3.50", "0.00", "6.00". */
export function decibelLiteral(hundredths) {
  const held = within(hundredths, SUB_LEVEL);
  const size = Math.abs(held);
  return `${held < 0 ? "-" : ""}${Math.floor(size / 100)}.${String(size % 100).padStart(2, "0")}`;
}

export function bassManagementCommand(zone, changes = {}) {
  let body = `{"v":2,"t":"bass_management","zone":${JSON.stringify(zone)}`;
  if (changes.crossover_hz !== undefined) body += `,"crossover_hz":${within(changes.crossover_hz, CROSSOVER_HZ)}`;
  if (changes.sub_level_db !== undefined) body += `,"sub_level_db":${decibelLiteral(changes.sub_level_db)}`;
  if (changes.sub_polarity !== undefined) body += `,"sub_polarity":${JSON.stringify(String(changes.sub_polarity))}`;
  return `${body}}`;
}

// The volume limits and quiet hours of a room (docs/control-plane.md, "The
// commands catalog version 2 adds"), in the same canonical encoding;
// fixtures/control/v2 has the vector of each.
//
// `limit`: the room's maximum volume, in thousandths. The server pulls the
// room's volume down to it.
export function limitCommand(zone, thousandths) {
  return `{"v":2,"t":"limit","zone":${JSON.stringify(zone)},"limit":${volumeLiteral(thousandths)}}`;
}

// The days of a week as the catalog names them, in the order it wants them.
export const WEEK = Object.freeze(["mon", "tue", "wed", "thu", "fri", "sat", "sun"]);
// Most quiet-hours windows one room may have (the catalog's bound).
export const MAX_QUIET_WINDOWS = 8;

// `quiet_hours`: the room's windows, every one of them, replacing what it
// had; `[]` removes them. A window is { days, start, end, limit }: the days
// it starts on, "HH:MM" twice, and its cap in thousandths. The days are
// written once each and in week order, whatever order they were given in.
export function quietHoursCommand(zone, windows = []) {
  const written = windows.map((window) => {
    const days = WEEK.filter((day) => (window.days ?? []).includes(day));
    return `{"days":${JSON.stringify(days)},"start":${JSON.stringify(String(window.start))},"end":${JSON.stringify(
      String(window.end),
    )},"limit":${volumeLiteral(window.limit)}}`;
  });
  return `{"v":2,"t":"quiet_hours","zone":${JSON.stringify(zone)},"windows":[${written.join(",")}]}`;
}

// `quiet_hours_enabled`: whether the room's windows cap it. Its windows stay.
export function quietHoursEnabledCommand(zone, enabled) {
  return `{"v":2,"t":"quiet_hours_enabled","zone":${JSON.stringify(zone)},"enabled":${enabled ? "true" : "false"}}`;
}

// `autoplay`: the rule of one input (`<endpoint>/<input>`), created or
// replaced whole: where it plays (a room or a saved group) and whether it is
// in force. A rule's two TV fields are the catalog's to default: each is
// written only when `false`, so a rule that has one switched off keeps it
// through a change of its target or its switch, and a rule that has neither
// has the bytes of fixtures/control/v2/autoplay.json.
export function autoplayCommand(input, target, enabled, { stopOnStandby = true, lowLatency = true } = {}) {
  return `{"v":2,"t":"autoplay","input":${JSON.stringify(input)},"target":${JSON.stringify(target)},"enabled":${
    enabled ? "true" : "false"
  }${stopOnStandby === false ? ',"stop_on_standby":false' : ""}${lowLatency === false ? ',"low_latency":false' : ""}}`;
}

// Alarms, sleep timers and stored sources (docs/control-plane.md, "The
// commands catalog version 2 adds" and "Stored sources and input labels"), in
// the same canonical encoding; fixtures/control/v2 has the vector of each.
//
// The catalog's bounds: an alarm's ramp in seconds, how long it plays by
// itself in minutes (0 plays until stopped), and a sleep timer's minutes.
export const MAX_RAMP_S = 600;
export const MAX_DURATION_MIN = 720;
export const MAX_SLEEP_MIN = 720;

const whole = (value, max) => Math.min(max, Math.max(0, Math.round(Number(value) || 0)));

// `alarm_set`: an alarm, created or replaced whole. `target` is a room or a
// saved group; `time` is "HH:MM" on the server's civil clock; `days` are the
// days it rings on, written once each and in week order, and none is once, at
// the next `time`; `source` is one of the four spellings (`chime:<name>`,
// `line-in:<endpoint>/<input>`, `stored:<id>` for a stored stream URL or a
// stored Spotify URI); `volume` is in thousandths.
export function alarmSetCommand({ alarm, target, time, days = [], source, volume, rampS, durationMin, enabled }) {
  const written = WEEK.filter((day) => days.includes(day));
  return `{"v":2,"t":"alarm_set","alarm":${JSON.stringify(alarm)},"target":${JSON.stringify(target)},"time":${JSON.stringify(
    String(time),
  )},"days":${JSON.stringify(written)},"source":${JSON.stringify(source)},"volume":${volumeLiteral(volume)},"ramp_s":${whole(
    rampS,
    MAX_RAMP_S,
  )},"duration_min":${whole(durationMin, MAX_DURATION_MIN)},"enabled":${enabled ? "true" : "false"}}`;
}

// `alarm_delete`: the alarm is forgotten (and stopped, if it rings).
export function alarmDeleteCommand(alarm) {
  return `{"v":2,"t":"alarm_delete","alarm":${JSON.stringify(alarm)}}`;
}

// `alarm_stop`: the alarm stops ringing and is kept.
export function alarmStopCommand(alarm) {
  return `{"v":2,"t":"alarm_stop","alarm":${JSON.stringify(alarm)}}`;
}

// `sleep`: a sleep timer for a room or a formed group; 0 minutes cancels it.
export function sleepCommand(target, minutes) {
  return `{"v":2,"t":"sleep","target":${JSON.stringify(target)},"minutes":${whole(minutes, MAX_SLEEP_MIN)}}`;
}

// `source_store`: a stored source, stored or replaced: a stream URL (`kind`
// "url") or a Spotify URI ("spotify"), which an alarm names as `stored:<id>`.
export function sourceStoreCommand(id, kind, value, name) {
  return `{"v":2,"t":"source_store","id":${JSON.stringify(id)},"kind":${JSON.stringify(kind)},"value":${JSON.stringify(
    value,
  )},"name":${JSON.stringify(name)}}`;
}

// `source_forget`: the stored source is forgotten; the server refuses while
// an alarm plays it.
export function sourceForgetCommand(id) {
  return `{"v":2,"t":"source_forget","id":${JSON.stringify(id)}}`;
}

// Speakers (docs/control-plane.md, "Speakers: adoption, names and rooms"), in
// the same canonical encoding; fixtures/control/v2 has the vector of each. A
// speaker is named by the id its sessions authenticate as.
//
// `speaker_name`: the name a person gives an adopted speaker.
export function speakerNameCommand(speaker, name) {
  return `{"v":2,"t":"speaker_name","speaker":${JSON.stringify(speaker)},"name":${JSON.stringify(name)}}`;
}

// `speaker_room`: the speaker becomes a member of the room and leaves every
// other; `null` takes it out of every room. The member is always written: the
// server refuses a `room` left out, so that no room is said out loud.
export function speakerRoomCommand(speaker, room) {
  return `{"v":2,"t":"speaker_room","speaker":${JSON.stringify(speaker)},"room":${
    typeof room === "string" && room ? JSON.stringify(room) : "null"
  }}`;
}

// `speaker_forget`: the speaker's record, its place in any room and its
// pinned key are removed; its next session is adopted afresh.
export function speakerForgetCommand(speaker) {
  return `{"v":2,"t":"speaker_forget","speaker":${JSON.stringify(speaker)}}`;
}

// Firmware (docs/control-plane.md, "Firmware: staged images and explicit
// installs"), in the same canonical encoding; fixtures/control/v2 has the
// vector of each. Nothing installs without `firmware_install` (K93).
//
// `firmware_install`: one staged image onto one speaker, both named. The app
// never sends the catalog's `"all": true` nor its `"force": true`.
export function firmwareInstallCommand(speaker, image) {
  return `{"v":2,"t":"firmware_install","speaker":${JSON.stringify(speaker)},"image":${JSON.stringify(image)}}`;
}

// `firmware_cancel`: abandon a speaker's install that is not verified yet.
export function firmwareCancelCommand(speaker) {
  return `{"v":2,"t":"firmware_cancel","speaker":${JSON.stringify(speaker)}}`;
}

// `firmware_rescan`: the server reads its firmware directory again.
export function firmwareRescanCommand() {
  return '{"v":2,"t":"firmware_rescan"}';
}

// Where a group's now-playing artwork is, for an <img>: the server's own
// route (docs/control-plane.md, "Now-playing artwork"), which names the group
// and nothing else. The record's own artwork address is somebody else's, and
// the page's Content-Security-Policy would not load it; it is never requested
// from here. It only tells one cover from the next: the route's address is
// the same for every track of a group, so a short tag of the record's
// address rides in the fragment, which the server is never sent, and a new
// track is a new address to the page.
export function artworkUrl(base, group, art = "") {
  let tag = 5381;
  for (const unit of String(art)) tag = (Math.imul(tag, 33) ^ unit.codePointAt(0)) >>> 0;
  return `${base}api/artwork?group=${encodeURIComponent(group)}${art ? `#${tag.toString(36)}` : ""}`;
}

// Whether a response is the login's and not the server's: a redirect the
// browser was told not to follow (to the login page), or a 401. chorus-server
// has no authentication and answers neither on any route, so both can only
// come from the login in front of it.
export function signedOut(response) {
  return Boolean(response) && (response.type === "opaqueredirect" || response.status === 401);
}

// What a command that met the login says.
export const SIGNED_OUT = "Signed out";

// What a refused command says, for a person: the server's own `detail` where
// the answer is one of the catalog's refusals, else the answer's text, else
// the status alone. A catalog refusal also names the `field` it refused,
// which comes back beside the words: { refusal, field } or { refusal }.
async function refusalOf(response) {
  let text = "";
  try {
    text = (await response.text()).trim();
  } catch {
    text = "";
  }
  try {
    const answer = JSON.parse(text);
    if (answer && typeof answer.detail === "string" && answer.detail) {
      const field = typeof answer.field === "string" && answer.field ? { field: answer.field } : {};
      return { refusal: answer.detail, ...field };
    }
  } catch {
    // Not a catalog message (a 403, 415 or 503 says its reason in plain text).
  }
  return { refusal: text || `the server answered ${response.status}` };
}

const timersOfTheHost = {
  set: (callback, ms) => globalThis.setTimeout(callback, ms),
  clear: (handle) => globalThis.clearTimeout(handle),
};

export function createClient({ fetch = globalThis.fetch.bind(globalThis), base = "../", timers = timersOfTheHost } = {}) {
  // GET api/state: the state message, or a thrown error naming why not.
  async function state() {
    const response = await fetch(`${base}api/state`, {
      headers: { Accept: "application/json" },
      cache: "no-store",
      redirect: "manual",
    });
    if (signedOut(response)) throw Object.assign(new Error(SIGNED_OUT), { signedOut: true });
    if (!response.ok) throw new Error(`the server answered ${response.status}`);
    return response.json();
  }

  // POST api/command with one control message. Resolves, never rejects:
  // { ok: true, state } with the resulting state message, or
  // { ok: false, refusal } with the words to show, and `field` where the
  // server named the field it refused; a command that met the login is
  // { ok: false, refusal, signedOut: true }.
  async function command(body) {
    let response;
    try {
      response = await fetch(`${base}api/command`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body,
        redirect: "manual",
      });
    } catch {
      return { ok: false, refusal: "the server could not be reached" };
    }
    if (signedOut(response)) return { ok: false, refusal: SIGNED_OUT, signedOut: true };
    if (!response.ok) return { ok: false, ...(await refusalOf(response)) };
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
  // when a stream delivers its first message, "lost" when a stream ends,
  // fails or goes silent, and "signed-out" when the login answered where the
  // stream should have opened. It is opened again after either, so a person
  // who signs in elsewhere (another tab) is live again with no reload.
  // Returns the function that closes it for good.
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
      let login = false;
      try {
        const response = await fetch(`${base}api/events`, {
          headers: { Accept: "text/event-stream" },
          cache: "no-store",
          redirect: "manual",
          signal: abort.signal,
        });
        login = signedOut(response);
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
      onStatus(login ? "signed-out" : "lost");
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

  // The address of a group's artwork as this page reaches the server.
  const artwork = (group, art) => artworkUrl(base, group, art);

  return { state, command, events, artwork };
}
