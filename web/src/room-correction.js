// A room's correction screen: measure the room with this device's microphone,
// see the filters the server proposes, apply them, switch the correction off
// and on, and undo (docs/room-correction.md, "The measurement in the app";
// docs/control-plane.md, "Room correction: the `measure_sweep` command" and
// "Room correction: a recording, its fit and the undo").
//
// It is one of the app's further screens and registers itself with the
// navigation (routes.js) at `#/rooms/<room>/correction`; a room's card links
// to it.
//
// The measurement is a walk of a few steps, and each is a person's own press:
//
//   1. the guidance: where to hold the phone, a quiet room, what will play
//   2. "Use the microphone": the browser is asked for it with its processing
//      asked off (capture.js), and what it granted is shown, a kind of
//      processing it kept on flagged
//   3. "Play the sweep and record": the server plays the sweep in the room
//      (`measure_sweep`) while the microphone records; when the server says
//      the sweep has ended the recording is sent to it (`POST /api/room-fit`)
//      and the microphone is let go
//   4. the proposed filters, with the fitter's two figures; nothing is
//      applied until "Apply" (`room_eq` with the filters). A recording the
//      fitter refuses is shown under the fitter's own name, in its words, with
//      what to do about it
//
// Below the walk is the room's correction as the server holds it: its
// filters, a switch (`room_eq` with `enabled` alone) and "Undo"
// (`room_eq_undo`), each shown as the server's state says and never as this
// page last asked.
//
// The server measures a room with its correction off (it refuses a recording
// of a room whose correction is on, `correction_on`), so a room that has one
// switched on has it switched off for the sweep and the upload and back on as
// soon as the server has answered, whatever the answer, and if the screen is
// left meanwhile. An apply therefore keeps the earlier correction, switched
// on as it was, as the step "Undo" returns to.
//
// The recording is this page's only while it is measured: it is held in
// memory from the sweep to the upload, sent to this server and nowhere else,
// and dropped. Nothing of it is written to any storage, and the element keeps
// no sample once the server has answered. The microphone is stopped as soon
// as the recording is taken, when a step fails, and when the screen is left.
//
// Nothing here fits anything, and nothing here says how well a room is
// corrected: the figures shown are the server's fitter's own.

import { LitElement, css, html, nothing } from "lit";

import { measureSweepCommand, roomEqCommand, roomEqEnabledCommand, roomEqUndoCommand } from "./api.js";
import { CaptureError, PROCESSING, REASONS, openMicrophone, toUploadRate, wavOf } from "./capture.js";
import { registerScreen } from "./routes.js";
import { measurementOf, roomOf } from "./state.js";

// How long the microphone goes on recording after the server says the
// sweep's program has ended. ASSUMED: the server's "ended" is when it sent
// the last frame, and the room plays it later by its own latency, which
// nothing here knows; a second is the tail the server itself leaves.
export const AFTER_MS = 1_000;
// How long past the program's own length the server's word is waited for
// before the measurement is given up. ASSUMED.
export const GRACE_MS = 5_000;
// The program's length where the state did not say: `measure_sweep`'s
// silence, sweep and silence (docs/control-plane.md).
const PROGRAM_MS = 6_500;

/** What the three kinds of processing are called. */
export const PROCESSING_NAMES = Object.freeze({
  echoCancellation: "Echo cancellation",
  noiseSuppression: "Noise suppression",
  autoGainControl: "Automatic gain control",
});

/**
 * What to do about each recording the fitter refuses, by the fitter's name
 * for it (docs/room-correction.md, "The pipeline", step 2).
 */
export const ADVICE = Object.freeze({
  too_short:
    "The recording ended before the sweep and the room's answer to it did. Keep this screen open and the phone awake until the sweep has finished, then measure again.",
  clipped:
    "The sweep was too loud for the microphone. Turn the room down, or hold the phone further from the speakers, then measure again.",
  too_quiet:
    "The sweep was too quiet at the microphone. Turn the room up, move closer and keep the microphone uncovered, then measure again.",
  too_noisy:
    "The room was too noisy for the sweep to stand clear of it. Pause music elsewhere, quiet the room (voices, fans, a TV), or turn the room up, then measure again.",
});

/** What a refusal with another name, or none, is answered with. */
export const ADVICE_OTHER = "The recording was not fitted. Measure again.";

const signed = (value, places) => `${value > 0 ? "+" : ""}${value.toFixed(places)}`;

/** A filter in words: "45 Hz, -9.32 dB, Q 6.409". */
export const filterWords = (filter) =>
  `${filter.freq_hz} Hz, ${signed(filter.gain_db, 2)} dB, Q ${filter.q.toFixed(3)}`;

const timersOfTheHost = {
  set: (callback, ms) => globalThis.setTimeout(callback, ms),
  clear: (handle) => globalThis.clearTimeout(handle),
};

// A step of the measurement that ended it, with the words to show.
class Stopped extends Error {}
// The screen was left while a step was awaited: nothing more is shown.
class Left extends Error {}

export class ChorusRoomCorrection extends LitElement {
  static properties = {
    // The room, as state.js reads it (its `correction` is what is shown), or
    // null when the server has no room with the id of the address.
    room: { attribute: false },
    // The id the address names, for the words when there is no such room.
    roomId: { type: String },
    // Whether a state has been read at all: before it, no room is known.
    known: { type: Boolean },
    // The state layer's store (state.js): its `command`, `roomFit` and
    // `subscribe` are what the measurement talks to the server with.
    store: { attribute: false },
    // The way to a capture session (capture.js): the microphone, unless a
    // test gives another source.
    capture: { attribute: false },
    // The timers the measurement waits with (a test gives its own).
    timers: { attribute: false },
    // Where the walk stands: "guide", "asking", "ready", "recording",
    // "fitting", "proposed", "refused" or "failed".
    _phase: { state: true },
    // What the browser granted, { settings, kept, sampleRate }, or null.
    _granted: { state: true },
    // The words of a step that failed.
    _message: { state: true },
    // The fit the server proposed, { filters, rmsBeforeDb, rmsAfterDb }.
    _fit: { state: true },
    // The refused recording: { name, words }.
    _refused: { state: true },
    // The server's words for the last refused command of the lower part
    // (apply, the switch, undo), and whether one is being answered.
    _commandRefusal: { state: true },
    _sending: { state: true },
    // What the last apply or undo did, in words.
    _done: { state: true },
  };

  static styles = css`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2,
    h3 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3 {
      margin-top: var(--surface-gap);
      font-size: var(--body-size);
    }
    p,
    li,
    dd,
    dt {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul,
    ol,
    dl {
      margin: var(--reset-margin);
      padding-left: var(--surface-pad);
    }
    dl {
      padding-left: var(--reset-margin);
    }
    .setting {
      display: flex;
      flex-wrap: wrap;
      gap: var(--surface-gap);
    }
    dt {
      min-width: var(--label-min-width);
    }
    .figure {
      font-family: var(--face-figure);
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    button {
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [data-kept],
    [data-flag] {
      color: var(--warn);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
    [role="alert"][data-flag] {
      color: var(--warn);
    }
  `;

  constructor() {
    super();
    this.room = null;
    this.roomId = "";
    this.known = false;
    this.store = null;
    this.capture = openMicrophone;
    this.timers = timersOfTheHost;
    this._session = null;
    this._run = null;
    this._reset();
  }

  _reset() {
    this._phase = "guide";
    this._granted = null;
    this._message = "";
    this._fit = null;
    this._refused = null;
    this._commandRefusal = "";
    this._sending = false;
    this._done = "";
  }

  // Leave the measurement: the microphone is stopped, and a step that is
  // being awaited ends without showing anything.
  _leave() {
    if (this._run) {
      this._run.left = true;
      this._run.wake?.();
    }
    this._run = null;
    this._session?.close();
    this._session = null;
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._leave();
    this._reset();
  }

  // The address now names another room: the walk starts over for it.
  willUpdate(changed) {
    if (changed.has("roomId") && changed.get("roomId") !== undefined) {
      this._leave();
      this._reset();
    }
  }

  get _busy() {
    return this._phase === "asking" || this._phase === "recording" || this._phase === "fitting";
  }

  // Step 2: the microphone, and what the browser granted.
  async _onMicrophone() {
    this._leave();
    const run = (this._run = { left: false });
    this._phase = "asking";
    this._message = "";
    this._fit = null;
    this._refused = null;
    this._done = "";
    let session;
    try {
      session = await this.capture();
    } catch (error) {
      if (run.left) return;
      this._run = null;
      this._phase = "failed";
      this._message = error instanceof CaptureError ? error.message : REASONS.failed;
      return;
    }
    if (run.left) {
      session.close();
      return;
    }
    this._session = session;
    this._granted = { settings: session.settings ?? {}, kept: session.kept ?? [], sampleRate: session.sampleRate ?? null };
    this._phase = "ready";
  }

  _wait(ms) {
    return new Promise((resolve) => this.timers.set(resolve, ms));
  }

  // The end of the sweep the server said it started: its `measurement` in a
  // later state, no longer `playing`; or null when the server says nothing
  // of it by the program's length and GRACE_MS, or the screen is left.
  _sweepEnd(run, mine) {
    const total = mine ? (mine.leadMs ?? 0) + (mine.sweepMs ?? 0) + (mine.tailMs ?? 0) : PROGRAM_MS;
    return new Promise((resolve) => {
      let unsubscribe = () => {};
      const end = (value) => {
        this.timers.clear(timer);
        unsubscribe();
        run.wake = null;
        resolve(value);
      };
      const timer = this.timers.set(() => end(null), total + GRACE_MS);
      run.wake = () => end(null);
      if (!mine) return;
      unsubscribe = this.store.subscribe((view) => {
        const now = measurementOf(view.state);
        if (now && now.id === mine.id && now.state !== "playing") end(now);
      });
    });
  }

  // Step 3: the sweep, the recording and its upload.
  async _onMeasure() {
    const run = this._run;
    const session = this._session;
    const room = this.room;
    if (!run || !session || !room || !this.store || this._phase !== "ready") return;
    const store = this.store;
    const gone = () => {
      if (run.left) throw new Left();
    };
    this._phase = "recording";
    let switchedOff = false;
    let outcome;
    try {
      // The server measures a room with its correction off.
      if (room.correction.enabled !== false && room.correction.filters.length > 0) {
        const off = await store.command(roomEqEnabledCommand(room.id, false));
        if (!off.ok) throw new Stopped(`The room's correction could not be switched off for the sweep: ${off.refusal}`);
        switchedOff = true;
        gone();
      }
      session.start();
      const started = await store.command(measureSweepCommand(room.id));
      gone();
      if (!started.ok) throw new Stopped(`The sweep was not played: ${started.refusal}`);
      const mine = measurementOf(started.state);
      const ended = await this._sweepEnd(run, mine && mine.zone === room.id ? mine : null);
      gone();
      if (!ended) throw new Stopped("The server did not say that the sweep ended, so the recording was not used. Measure again.");
      if (ended.state !== "finished") {
        throw new Stopped(`The sweep was called off${ended.reason ? `: ${ended.reason}` : ""}. Measure again.`);
      }
      await this._wait(AFTER_MS);
      gone();
      const recording = await session.stop();
      gone();
      this._phase = "fitting";
      // The recording lives in this call and in the request, and nowhere else.
      const answer = await store.roomFit(room.id, wavOf(toUploadRate(recording)), recording.sweep ?? {});
      gone();
      outcome = answer.ok
        ? { phase: "proposed", fit: answer.fit }
        : { phase: "refused", refused: { name: answer.name ?? "", words: answer.refusal } };
    } catch (error) {
      if (error instanceof Left) outcome = null;
      else if (error instanceof Stopped) outcome = { phase: "failed", message: error.message };
      else outcome = { phase: "failed", message: "The measurement failed in this page. Measure again." };
    } finally {
      session.close();
      if (this._session === session) this._session = null;
      // The room gets its correction back as it was, whatever happened.
      if (switchedOff) {
        const on = await store.command(roomEqEnabledCommand(room.id, true));
        if (!on.ok && outcome) {
          outcome = {
            phase: "failed",
            message: `The room's correction was switched off for the sweep and could not be switched back on: ${on.refusal}`,
          };
        }
      }
    }
    if (!outcome || run.left) return;
    this._run = null;
    this._phase = outcome.phase;
    this._fit = outcome.fit ?? null;
    this._refused = outcome.refused ?? null;
    this._message = outcome.message ?? "";
  }

  // One command of the lower part, or the apply: the server's state shows
  // what it did, and its words are shown if it refused.
  async _command(body, done = "") {
    if (!this.store || this._sending) return false;
    this._sending = true;
    this._commandRefusal = "";
    this._done = "";
    const result = await this.store.command(body);
    this._sending = false;
    if (!result.ok) {
      this._commandRefusal = result.refusal;
      return false;
    }
    this._done = done;
    return true;
  }

  // Step 4: the apply. Nothing before this press changes the room's filters.
  async _onApply() {
    const fit = this._fit;
    if (!fit || !this.room || this._phase !== "proposed") return;
    const applied = await this._command(roomEqCommand(this.room.id, fit.filters), "Applied. Undo puts back what the room had.");
    if (!applied) return;
    this._fit = null;
    this._granted = null;
    this._phase = "guide";
  }

  _onDiscard() {
    this._leave();
    this._reset();
  }

  _onSwitch() {
    this._command(roomEqEnabledCommand(this.room.id, this.room.correction.enabled !== true));
  }

  _onUndo() {
    this._command(roomEqUndoCommand(this.room.id), "Put back.");
  }

  _guide() {
    return html`
      <ol data-guide>
        <li>
          Where: sit or stand where you usually listen and hold this device at ear height, its microphone uncovered
          and nothing between it and the speakers. Hold it still until the sweep has ended.
        </li>
        <li>
          Quiet: pause what plays in other rooms, close the door, and stop anything that hums or talks. A room that is
          too noisy is refused, not guessed at.
        </li>
        <li>
          What plays: half a second of silence, then one rising tone that sweeps from the lowest bass to the highest
          treble in 5 seconds, then a second of silence, in this room only and at this room's volume. Set the volume
          first: clearly louder than the room's own noise, not uncomfortable.
        </li>
        <li>
          The recording goes to this server to be fitted and is kept nowhere, here or there. Nothing changes in the
          room until you apply what the server proposes.
        </li>
        <li>
          A phone's microphone is not calibrated, least of all in the low bass, and no real phone has been measured
          with this yet: listen to the result, and switch it off or undo it if it is not better.
        </li>
      </ol>
    `;
  }

  // What the browser granted, a kind of processing it kept on flagged.
  _settings() {
    const granted = this._granted;
    if (!granted) return nothing;
    const { settings, kept, sampleRate } = granted;
    const processing = (name) => {
      const value = settings[name];
      if (value === undefined) return html`<dd data-setting=${name}>not reported by this browser</dd>`;
      if (value === false) return html`<dd data-setting=${name}>off, as asked</dd>`;
      return html`<dd data-setting=${name} data-kept>
        on (${String(value)}): asked off, and the browser kept it on
      </dd>`;
    };
    const reported = (value, unit) => (value === undefined || value === null ? "not reported by this browser" : `${value}${unit}`);
    return html`
      <h3 id="granted">What the browser granted</h3>
      <dl aria-labelledby="granted" data-granted>
        ${PROCESSING.map(
          (name) => html`<div class="setting"><dt>${PROCESSING_NAMES[name]}</dt>${processing(name)}</div>`,
        )}
        <div class="setting">
          <dt>Channels</dt>
          <dd data-setting="channelCount">${reported(settings.channelCount, "")}</dd>
        </div>
        <div class="setting">
          <dt>Microphone's sample rate</dt>
          <dd data-setting="sampleRate">${reported(settings.sampleRate, " Hz")}</dd>
        </div>
        <div class="setting">
          <dt>Recorded at</dt>
          <dd data-setting="recordedAt">${reported(sampleRate, " Hz")}</dd>
        </div>
      </dl>
      ${kept.length === 0
        ? nothing
        : html`<p role="alert" data-flag>
            This browser kept ${kept.map((name) => PROCESSING_NAMES[name].toLowerCase()).join(", ")} on. It changes what
            the microphone hears, so a fit of this recording may be wrong.
          </p>`}
    `;
  }

  _walk() {
    const room = this.room;
    const phase = this._phase;
    const microphone = (face) => html`
      <div class="row">
        <button type="button" data-step="microphone" aria-label="Use the microphone to measure ${room.name}" @click=${this._onMicrophone}>
          ${face}
        </button>
      </div>
    `;
    if (phase === "guide") return html`${this._guide()} ${microphone("Use the microphone")}`;
    if (phase === "asking") return html`<p role="status" data-phase="asking">Asking the browser for the microphone.</p>`;
    if (phase === "failed") {
      return html`
        <p role="alert" data-phase="failed">${this._message}</p>
        ${this._settings()} ${microphone("Measure again")}
      `;
    }
    if (phase === "ready") {
      return html`
        ${this._settings()}
        <p>The microphone is open. The sweep plays as soon as you press, at this room's volume.</p>
        <div class="row">
          <button type="button" data-step="measure" aria-label="Play the sweep in ${room.name} and record" @click=${this._onMeasure}>
            Play the sweep and record
          </button>
          <button type="button" data-step="discard" aria-label="Stop measuring ${room.name}" @click=${this._onDiscard}>Stop</button>
        </div>
      `;
    }
    if (phase === "recording") {
      return html`${this._settings()}
        <p role="status" data-phase="recording">The sweep is playing and the microphone is recording. Hold still.</p>`;
    }
    if (phase === "fitting") {
      return html`${this._settings()}
        <p role="status" data-phase="fitting">The recording is with the server, being fitted. The microphone is off.</p>`;
    }
    if (phase === "refused") {
      const { name, words } = this._refused;
      return html`
        <p role="alert" data-phase="refused" data-refusal=${name || nothing}>The server refused the recording: ${words}</p>
        <p data-advice>${ADVICE[name] ?? ADVICE_OTHER}</p>
        ${this._settings()} ${microphone("Measure again")}
      `;
    }
    // "proposed"
    const fit = this._fit;
    const figure = (value) => (value === null ? "not given" : `${value.toFixed(2)} dB`);
    return html`
      <h3 id="proposed">Proposed filters</h3>
      ${fit.filters.length === 0
        ? html`<p data-phase="proposed" data-nothing>The server found nothing to correct in this recording.</p>`
        : html`
            <ul aria-labelledby="proposed" data-phase="proposed">
              ${fit.filters.map((filter) => html`<li class="figure">${filterWords(filter)}</li>`)}
            </ul>
            <p data-rms>
              The fitter's own figure for this recording, the deviation from flat in the band it fits: ${figure(fit.rmsBeforeDb)}
              before, ${figure(fit.rmsAfterDb)} predicted after. It is a prediction from one recording, not a measurement
              of the corrected room.
            </p>
          `}
      ${this._settings()}
      <p>Nothing has changed in the room yet.</p>
      <div class="row">
        ${fit.filters.length === 0
          ? nothing
          : html`<button
              type="button"
              data-step="apply"
              aria-label="Apply the proposed correction to ${room.name}"
              ?disabled=${this._sending}
              @click=${this._onApply}
            >
              Apply
            </button>`}
        <button type="button" data-step="discard" aria-label="Discard the proposed correction for ${room.name}" @click=${this._onDiscard}>
          Discard
        </button>
      </div>
    `;
  }

  // The room's correction as the server holds it.
  _held() {
    const room = this.room;
    const { enabled, filters, undo } = room.correction;
    const locked = this._busy || this._sending;
    return html`
      <h3 id="held">This room's correction</h3>
      ${filters.length === 0
        ? html`<p data-held data-none>This room has no correction.</p>`
        : html`<ul aria-labelledby="held" data-held>
            ${filters.map((filter) => html`<li class="figure">${filterWords(filter)}</li>`)}
          </ul>`}
      <div class="row">
        <button
          type="button"
          data-control="enabled"
          aria-label="Correction for ${room.name}"
          aria-pressed=${enabled === true ? "true" : "false"}
          ?disabled=${enabled === null || filters.length === 0 || locked}
          @click=${this._onSwitch}
        >
          Correction
        </button>
        <span data-value="enabled">${enabled === null ? "Unavailable" : filters.length === 0 ? "Nothing to switch" : enabled ? "On" : "Off"}</span>
        <button
          type="button"
          data-control="undo"
          aria-label="Undo the last correction of ${room.name}"
          ?disabled=${!undo || locked}
          @click=${this._onUndo}
        >
          Undo
        </button>
        <span data-value="undo">${undo ? "Puts back what the room had before the last apply" : "Nothing to undo"}</span>
      </div>
      <p role="status" data-done>${this._done}</p>
      <p role="alert" data-command-refusal>${this._commandRefusal ? `Refused: ${this._commandRefusal}` : nothing}</p>
    `;
  }

  render() {
    const room = this.room;
    if (!room) {
      return html`<p role="status" data-missing>
        ${this.known ? `This server has no room "${this.roomId}".` : "Reading this server's rooms."}
      </p>`;
    }
    return html`
      <h2>Correction of ${room.name}</h2>
      <h3>Measure this room</h3>
      ${this._walk()} ${this._held()}
    `;
  }
}

customElements.define("chorus-room-correction", ChorusRoomCorrection);

/** The id this screen is registered under (routes.js): `addressOf(CORRECTION_SCREEN, { room })` is its address. */
export const CORRECTION_SCREEN = "room-correction";

registerScreen({
  id: CORRECTION_SCREEN,
  path: "rooms/:room/correction",
  title: ({ room }, view) => `Correction of ${roomOf(view.rooms, room)?.name ?? room}`,
  render: ({ room }, { view, store }) => html`
    <chorus-room-correction
      .room=${roomOf(view.rooms, room)}
      .roomId=${room}
      .known=${view.state !== null}
      .store=${store}
    ></chorus-room-correction>
  `,
});
