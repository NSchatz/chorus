// The house's alarms, its stored sources and its sleep timers
// (docs/control-plane.md: `alarm_set`, `alarm_delete`, `alarm_stop`, `sleep`,
// `source_store`, `source_forget`; docs/inputs.md, "The four alarm sources").
//
// It is one of the app's further screens and registers itself with the
// navigation (routes.js) at `#/alarms`; the home links to it.
//
// Three parts, each of them the server's state and nothing else:
//
//   Alarms          every alarm the state lists: when it rings, where, what
//                   it plays, its switch, "Stop" while it rings, "Edit" and
//                   "Delete". Under them the alarm being written.
//   Stored sources  the stream URLs and Spotify URIs the server keeps for an
//                   alarm to play, each with "Forget", and the one being
//                   written.
//   Sleep timers    every timer the state lists, with the time left and
//                   "Cancel", and the one being asked for.
//
// An alarm's source is one of four kinds (K80), and the picker offers what
// the state names of each: the chimes (`chimes`; the app has no list of its
// own), the inputs offered now (`inputs`), and the stored sources of kind
// `url` and of kind `spotify` (`stored_sources`). Where the state says the
// server cannot play a source, the screen says why: the server falls back to
// its bell chime then (an alarm must still wake), and a person should know
// before the morning.
//
// The catalog's `alarm_set` creates or replaces a whole alarm, so the alarm
// being written is a draft: nothing of it is sent until "Save alarm". "Edit"
// copies an alarm into the draft; saving under the same name replaces it, and
// under another name makes a second alarm. An alarm's switch sends the alarm
// as the server holds it with `enabled` the opposite.
//
// A sleep timer's time left is the server's (`remaining_s`). The server sends
// a state for the countdown alone only when the whole minutes change, so the
// screen counts the seconds down on its own between two states, on the
// monotonic clock, and takes every state's value as the truth.

import { LitElement, css, html, nothing } from "lit";

import {
  MAX_DURATION_MIN,
  MAX_RAMP_S,
  MAX_SLEEP_MIN,
  WEEK,
  alarmDeleteCommand,
  alarmSetCommand,
  alarmStopCommand,
  sleepCommand,
  sourceForgetCommand,
  sourceStoreCommand,
} from "./api.js";
import { registerScreen } from "./routes.js";
import { alarmsOf, chimesOf, receiversOf, sleepOf, storedSourcesOf } from "./state.js";

/** The subjects this screen's commands are sent under: a refusal is shown where it was asked. */
export const alarmSubject = (alarm) => `alarm:${alarm}`;
export const ALARM_DRAFT_SUBJECT = "alarms:draft";
export const storedSubject = (id) => `stored:${id}`;
export const STORED_DRAFT_SUBJECT = "stored:draft:";
export const sleepSubject = (target) => `sleep:${target}`;
export const SLEEP_DRAFT_SUBJECT = "sleep:draft:";

const DAY_NAMES = {
  mon: ["Mon", "Monday"],
  tue: ["Tue", "Tuesday"],
  wed: ["Wed", "Wednesday"],
  thu: ["Thu", "Thursday"],
  fri: ["Fri", "Friday"],
  sat: ["Sat", "Saturday"],
  sun: ["Sun", "Sunday"],
};

const KIND_NAMES = { url: "Stream URL", spotify: "Spotify URI" };

// The drafts a person starts from. An alarm's target and source are chosen
// from what the server has, so they start empty and take the first offered.
const ALARM_DRAFT = Object.freeze({
  alarm: "",
  target: "",
  time: "07:00",
  days: Object.freeze(["mon", "tue", "wed", "thu", "fri"]),
  source: "",
  volume: 300,
  rampS: 30,
  durationMin: 60,
  enabled: true,
});
const STORED_DRAFT = Object.freeze({ id: "", name: "", kind: "url", value: "" });
const SLEEP_DRAFT = Object.freeze({ target: "", minutes: 30 });

const percent = (thousandths) => `${Math.round(thousandths / 10)}%`;
const isTime = (value) => /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
const bounded = (value, max) => Math.min(max, Math.max(0, Math.round(Number(value) || 0)));

/**
 * The time left of a sleep timer, in words.
 *
 * @param {number} seconds whole seconds left
 * @returns {string} for example "17 min 22 s left"
 */
export function leftWords(seconds) {
  const left = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(left / 3600);
  const minutes = Math.floor((left % 3600) / 60);
  if (hours > 0) return `${hours} h ${minutes} min left`;
  if (minutes > 0) return `${minutes} min ${left % 60} s left`;
  return `${left} s left`;
}

export class ChorusAlarms extends LitElement {
  static properties = {
    // Whether a state has been read at all.
    known: { type: Boolean },
    // The state message what is shown was read from, for its identity alone:
    // a new one is a new reading of every sleep timer. (Not its serial: a
    // stream opened again says the same serial with a later count.)
    heard: { attribute: false },
    // The server's alarms, stored sources and sleep timers (state.js).
    alarms: { attribute: false },
    stored: { attribute: false },
    sleep: { attribute: false },
    // The chimes the state names, or null when it names none; the targets
    // whose Spotify receiver is running, or null on a server with none.
    chimes: { attribute: false },
    receivers: { attribute: false },
    // The inputs offered now, [{ id, source, label }].
    inputs: { attribute: false },
    // Where an alarm can ring (the rooms and the saved groups) and what a
    // sleep timer can be for (the rooms and the groups formed now), each
    // [{ id, name }].
    rooms: { attribute: false },
    savedGroups: { attribute: false },
    formedGroups: { attribute: false },
    // The server's words for the last refused command, and the field it
    // named, by subject.
    refusals: { attribute: false },
    refusalFields: { attribute: false },
    // What is being written, not yet sent.
    _alarm: { state: true },
    _source: { state: true },
    _timer: { state: true },
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
    h3,
    h4 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3,
    h4 {
      padding-top: var(--surface-gap);
      font-size: var(--body-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li,
    .draft {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    li[data-ringing] {
      border-color: var(--accent);
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label {
      min-width: var(--label-min-width);
    }
    input[type="range"] {
      flex: 1 1 var(--slider-min-width);
      min-width: var(--slider-min-width);
      height: var(--control-size);
      margin: var(--reset-margin);
      accent-color: var(--accent);
    }
    button,
    select,
    input:not([type="range"]) {
      box-sizing: border-box;
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    select,
    input[type="text"],
    input[type="url"] {
      flex: 1 1 var(--control-basis);
      min-width: var(--shrink-min);
    }
    .figure {
      min-width: var(--figure-min-width);
      font-family: var(--face-figure);
      text-align: end;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    input:focus-visible,
    select:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [data-value="ringing"] {
      color: var(--accent);
    }
    [data-fallback],
    [data-unavailable] {
      color: var(--warn);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
  `;

  constructor() {
    super();
    this.known = false;
    this.heard = null;
    this.alarms = [];
    this.stored = [];
    this.sleep = [];
    this.chimes = null;
    this.receivers = null;
    this.inputs = [];
    this.rooms = [];
    this.savedGroups = [];
    this.formedGroups = [];
    this.refusals = {};
    this.refusalFields = {};
    this._alarm = { ...ALARM_DRAFT };
    this._source = { ...STORED_DRAFT };
    this._timer = { ...SLEEP_DRAFT };
    // The monotonic clock the countdown reads, in milliseconds, and when the
    // state now shown was heard on it. A test gives its own clock.
    this.clock = () => globalThis.performance.now();
    this._heardAt = 0;
    this._ticker = null;
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._tickEvery(false);
  }

  willUpdate(changed) {
    if (changed.has("heard")) this._heardAt = this.clock();
  }

  // The lists take the draft's values whenever the screen is painted (an
  // option's `selected` is only where a list starts), and the countdown runs
  // only while there is a timer to count.
  updated() {
    for (const list of this.renderRoot.querySelectorAll("select[data-holds]")) {
      const held = list.dataset.holds;
      if (list.value !== held) list.value = held;
    }
    this._tickEvery(this.isConnected && (this.sleep ?? []).some((timer) => timer.remainingS !== null));
  }

  _tickEvery(on) {
    if (on === (this._ticker !== null)) return;
    if (on) this._ticker = globalThis.setInterval(() => this.tick(), 1000);
    else {
      globalThis.clearInterval(this._ticker);
      this._ticker = null;
    }
  }

  /** Paint the countdown again from the clock: the ticker's one job. */
  tick() {
    this.requestUpdate();
  }

  // The seconds a timer has left now: the server's count, less what has
  // passed here since it was heard. It never goes below zero, and the entry
  // leaves the screen when it leaves the state, not when this reaches zero.
  _left(timer) {
    const passed = Math.floor(Math.max(0, this.clock() - this._heardAt) / 1000);
    return Math.max(0, timer.remainingS - passed);
  }

  _ask(subject, body) {
    this.dispatchEvent(
      new CustomEvent("chorus-command", { detail: { subject, body }, bubbles: true, composed: true }),
    );
  }

  _refusal(subject) {
    const words = this.refusals?.[subject] ?? "";
    if (!words) return html`<p role="alert"></p>`;
    const field = this.refusalFields?.[subject] ?? "";
    return html`<p role="alert" data-refusal-field=${field || nothing}>Refused${field ? ` (${field})` : ""}: ${words}</p>`;
  }

  // --- names ---------------------------------------------------------------

  _place(id) {
    const all = [...(this.rooms ?? []), ...(this.savedGroups ?? []), ...(this.formedGroups ?? [])];
    return all.find((place) => place.id === id)?.name ?? id;
  }

  _storedOf(source) {
    if (!source.startsWith("stored:")) return null;
    return (this.stored ?? []).find((stored) => stored.id === source.slice(7)) ?? null;
  }

  // What a source is, for a person: its kind and its name.
  _sourceName(source) {
    if (source.startsWith("chime:")) return `Chime: ${source.slice(6)}`;
    if (source.startsWith("line-in:")) {
      const input = (this.inputs ?? []).find((offered) => offered.source === source);
      return `Input: ${input?.label ?? source.slice(8)}`;
    }
    const stored = this._storedOf(source);
    if (stored) return `${KIND_NAMES[stored.kind] ?? stored.kind}: ${stored.name}`;
    return source;
  }

  // Why the server cannot play this source for this target, as far as the
  // state says; "" when the state says nothing against it. The server rings
  // its bell chime in its place.
  _unplayable(source, target) {
    if (source.startsWith("chime:")) {
      if (this.chimes !== null && !this.chimes.includes(source.slice(6))) return `This server has no chime "${source.slice(6)}".`;
      return "";
    }
    if (source.startsWith("line-in:")) {
      if (!(this.inputs ?? []).some((offered) => offered.source === source)) {
        return `The input ${source.slice(8)} is not offered now: its speaker is not connected.`;
      }
      return "";
    }
    if (source.startsWith("stored:")) {
      const stored = this._storedOf(source);
      if (!stored) return `This server has no stored source "${source.slice(7)}".`;
      if (stored.kind !== "spotify") return "";
      if (this.receivers === null) return "This server runs no Spotify receiver.";
      const saved = (this.savedGroups ?? []).some((group) => group.id === target);
      if (!this.receivers.includes(`${saved ? "group" : "room"}:${target}`)) {
        return `No Spotify receiver is running for ${this._place(target)}.`;
      }
      return "";
    }
    return "This is not a source an alarm plays.";
  }

  // --- the alarms ----------------------------------------------------------

  _alarmOf(id) {
    return (this.alarms ?? []).find((alarm) => alarm.id === id) ?? null;
  }

  // An alarm as the server holds it, as the command takes it.
  _sendable(alarm, change = {}) {
    return alarmSetCommand({ ...alarm, alarm: alarm.id, ...change });
  }

  _onSwitch(event) {
    const alarm = this._alarmOf(event.currentTarget.dataset.alarm);
    if (alarm) this._ask(alarmSubject(alarm.id), this._sendable(alarm, { enabled: !alarm.enabled }));
  }

  _onStop(event) {
    const id = event.currentTarget.dataset.alarm;
    this._ask(alarmSubject(id), alarmStopCommand(id));
  }

  _onDelete(event) {
    const id = event.currentTarget.dataset.alarm;
    this._ask(alarmSubject(id), alarmDeleteCommand(id));
  }

  _onEdit(event) {
    const alarm = this._alarmOf(event.currentTarget.dataset.alarm);
    if (!alarm) return;
    const { id, ringing: _ringing, ...fields } = alarm;
    this._alarm = { alarm: id, ...fields };
  }

  _alarmRow(alarm) {
    const days = alarm.days.length === 0 ? "once" : WEEK.filter((day) => alarm.days.includes(day)).map((day) => DAY_NAMES[day][0]).join(" "); // prettier-ignore
    const length = alarm.durationMin === 0 ? "until stopped" : `for ${alarm.durationMin} min`;
    const why = this._unplayable(alarm.source, alarm.target);
    return html`
      <li data-alarm=${alarm.id} ?data-ringing=${alarm.ringing}>
        <h4>${alarm.id}</h4>
        <p data-value="when">${alarm.time}, ${days}</p>
        <p data-value="what">
          ${this._sourceName(alarm.source)} in ${this._place(alarm.target)}, to ${percent(alarm.volume)} over ${alarm.rampS} s,
          ${length}
        </p>
        ${why ? html`<p data-fallback>${why} The alarm rings the bell chime instead.</p>` : nothing}
        <div class="row">
          <button
            type="button"
            data-alarm=${alarm.id}
            aria-label="Alarm ${alarm.id}"
            aria-pressed=${alarm.enabled ? "true" : "false"}
            @click=${this._onSwitch}
          >
            Alarm
          </button>
          <span data-value="enabled">${alarm.enabled ? "On" : "Off"}</span>
          ${alarm.ringing
            ? html`<span data-value="ringing">Ringing now</span>
                <button type="button" data-alarm=${alarm.id} aria-label="Stop alarm ${alarm.id}" @click=${this._onStop}>
                  Stop
                </button>`
            : nothing}
          <button type="button" data-alarm=${alarm.id} aria-label="Edit alarm ${alarm.id}" @click=${this._onEdit}>Edit</button>
          <button type="button" data-alarm=${alarm.id} aria-label="Delete alarm ${alarm.id}" @click=${this._onDelete}>
            Delete
          </button>
        </div>
        ${this._refusal(alarmSubject(alarm.id))}
      </li>
    `;
  }

  // --- the alarm being written ---------------------------------------------

  // Every source the picker offers, by kind: [{ kind, label, options: [{ value, name }] }].
  _offeredSources() {
    const stored = (kind) =>
      (this.stored ?? []).filter((source) => source.kind === kind).map((source) => ({ value: `stored:${source.id}`, name: source.name })); // prettier-ignore
    return [
      { kind: "chime", label: "Chimes", options: (this.chimes ?? []).map((name) => ({ value: `chime:${name}`, name })) },
      { kind: "line-in", label: "Inputs", options: (this.inputs ?? []).map((input) => ({ value: input.source, name: input.label })) }, // prettier-ignore
      { kind: "url", label: "Stored stream URLs", options: stored("url") },
      { kind: "spotify", label: "Stored Spotify URIs", options: stored("spotify") },
    ];
  }

  // The draft with its target and its source filled in from what the server
  // has: the first room and the first source offered, until a person chooses.
  _alarmDraft() {
    const draft = this._alarm;
    const places = [...(this.rooms ?? []), ...(this.savedGroups ?? [])];
    const first = this._offeredSources().flatMap((group) => group.options)[0];
    return { ...draft, target: draft.target || (places[0]?.id ?? ""), source: draft.source || (first?.value ?? "") };
  }

  _setAlarm(change) {
    this._alarm = { ...this._alarm, ...change };
  }

  _onAlarmText(event) {
    this._setAlarm({ alarm: event.target.value.trim() });
  }

  _onAlarmChoice(event) {
    this._setAlarm({ [event.target.dataset.field]: event.target.value });
  }

  _onAlarmTime(event) {
    if (!isTime(event.target.value)) {
      event.target.value = this._alarm.time;
      return;
    }
    this._setAlarm({ time: event.target.value });
  }

  _onAlarmDay(event) {
    const day = event.currentTarget.dataset.day;
    const days = this._alarm.days.includes(day)
      ? this._alarm.days.filter((other) => other !== day)
      : WEEK.filter((other) => other === day || this._alarm.days.includes(other));
    this._setAlarm({ days });
  }

  _onAlarmVolume(event) {
    this._setAlarm({ volume: Number(event.target.value) });
  }

  _onAlarmCount(event) {
    const { field, max } = event.target.dataset;
    const held = bounded(event.target.value, Number(max));
    event.target.value = String(held);
    this._setAlarm({ [field]: held });
  }

  _onAlarmEnabled() {
    this._setAlarm({ enabled: !this._alarm.enabled });
  }

  _onSave() {
    this._ask(ALARM_DRAFT_SUBJECT, alarmSetCommand(this._alarmDraft()));
  }

  // Why a kind has nothing to offer, or cannot be played, as far as the state
  // says: one line for each kind there is something to say about.
  _kindNotes(draft) {
    const notes = [];
    if (this.chimes === null) notes.push(["chime", "This server does not say which chimes it has, so none is offered here."]);
    if ((this.inputs ?? []).length === 0) notes.push(["line-in", "No input is offered now: no speaker with a line-in is connected."]); // prettier-ignore
    const kinds = new Set((this.stored ?? []).map((source) => source.kind));
    if (!kinds.has("url")) notes.push(["url", "No stream URL is stored: add one under Stored sources."]);
    if (!kinds.has("spotify")) notes.push(["spotify", "No Spotify URI is stored: add one under Stored sources."]);
    else if (this.receivers === null) {
      notes.push(["spotify", "This server runs no Spotify receiver: an alarm with a Spotify URI rings the bell chime instead."]);
    }
    const chosen = draft.source ? this._unplayable(draft.source, draft.target) : "";
    const about = this._storedOf(draft.source)?.kind === "spotify" ? "spotify" : "chosen";
    if (chosen && !(about === "spotify" && this.receivers === null)) {
      notes.push([about, `${chosen} The alarm would ring the bell chime instead.`]);
    }
    return notes.map(([kind, words]) => html`<p data-unavailable=${kind}>${words}</p>`);
  }

  _alarmForm() {
    const draft = this._alarmDraft();
    const rooms = this.rooms ?? [];
    const groups = this.savedGroups ?? [];
    const places = [...rooms, ...groups];
    const sources = this._offeredSources();
    const option = (item) => html`<option value=${item.value}>${item.name}</option>`;
    const place = (item) => html`<option value=${item.id}>${item.name}</option>`;
    const listed = sources.some((group) => group.options.some((item) => item.value === draft.source));
    const replaces = this._alarmOf(draft.alarm) !== null;
    const ready = draft.alarm !== "" && draft.target !== "" && draft.source !== "";
    return html`
      <div class="draft" data-draft="alarm">
        <div class="row">
          <label for="alarm-name">Name</label>
          <input
            id="alarm-name"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            .value=${draft.alarm}
            aria-label="Alarm name"
            @input=${this._onAlarmText}
          />
          <p>Lower-case letters, digits and "-". An alarm with this name is replaced.</p>
        </div>
        <div class="row">
          <label for="alarm-target">Rings in</label>
          <select id="alarm-target" data-field="target" data-holds=${draft.target} aria-label="Alarm target" @change=${this._onAlarmChoice}>
            ${places.some((item) => item.id === draft.target) || !draft.target
              ? nothing
              : html`<option value=${draft.target}>${draft.target} (not on this server now)</option>`}
            ${rooms.length === 0 ? nothing : html`<optgroup label="Rooms">${rooms.map(place)}</optgroup>`}
            ${groups.length === 0 ? nothing : html`<optgroup label="Saved groups">${groups.map(place)}</optgroup>`}
          </select>
          <label for="alarm-time">At</label>
          <input id="alarm-time" type="time" .value=${draft.time} aria-label="Alarm time" @change=${this._onAlarmTime} />
        </div>
        <div class="row" role="group" aria-label="Days of the alarm">
          ${WEEK.map(
            (day) =>
              html`<button
                type="button"
                data-day=${day}
                aria-label="${DAY_NAMES[day][1]}, the alarm"
                aria-pressed=${draft.days.includes(day) ? "true" : "false"}
                @click=${this._onAlarmDay}
              >
                ${DAY_NAMES[day][0]}
              </button>`,
          )}
          <p data-value="days">${draft.days.length === 0 ? "No day: it rings once, at the next such time." : "It rings on these days."}</p>
        </div>
        <div class="row">
          <label for="alarm-source">Plays</label>
          <select id="alarm-source" data-field="source" data-holds=${draft.source} aria-label="Alarm source" @change=${this._onAlarmChoice}>
            ${listed || !draft.source ? nothing : html`<option value=${draft.source}>${draft.source} (not on this server now)</option>`}
            ${sources.map((group) =>
              group.options.length === 0
                ? nothing
                : html`<optgroup label=${group.label} data-kind=${group.kind}>${group.options.map(option)}</optgroup>`,
            )}
          </select>
        </div>
        ${this._kindNotes(draft)}
        <div class="row">
          <label for="alarm-volume">Volume</label>
          <input
            id="alarm-volume"
            type="range"
            min="0"
            max="1000"
            step="1"
            .value=${String(draft.volume)}
            aria-label="Alarm volume"
            aria-valuetext=${percent(draft.volume)}
            @input=${this._onAlarmVolume}
          />
          <span class="figure" data-value="volume">${percent(draft.volume)}</span>
        </div>
        <div class="row">
          <label for="alarm-ramp">Rises over, seconds</label>
          <input
            id="alarm-ramp"
            type="number"
            min="0"
            max=${MAX_RAMP_S}
            step="1"
            data-field="rampS"
            data-max=${MAX_RAMP_S}
            .value=${String(draft.rampS)}
            aria-label="Alarm ramp, seconds"
            @change=${this._onAlarmCount}
          />
          <label for="alarm-duration">Plays for, minutes</label>
          <input
            id="alarm-duration"
            type="number"
            min="0"
            max=${MAX_DURATION_MIN}
            step="1"
            data-field="durationMin"
            data-max=${MAX_DURATION_MIN}
            .value=${String(draft.durationMin)}
            aria-label="Alarm duration, minutes"
            @change=${this._onAlarmCount}
          />
          <p>0 minutes plays until it is stopped.</p>
        </div>
        <div class="row">
          <button type="button" aria-label="Alarm switched on" aria-pressed=${draft.enabled ? "true" : "false"} @click=${this._onAlarmEnabled}>
            Switched on
          </button>
          <button type="button" aria-label="Save alarm" ?disabled=${!ready} @click=${this._onSave}>Save alarm</button>
          <p>${replaces ? `Saving replaces the alarm "${draft.alarm}".` : "Nothing is sent until it is saved."}</p>
        </div>
        ${this._refusal(ALARM_DRAFT_SUBJECT)}
      </div>
    `;
  }

  // --- the stored sources --------------------------------------------------

  _onForget(event) {
    const id = event.currentTarget.dataset.stored;
    this._ask(storedSubject(id), sourceForgetCommand(id));
  }

  _onSourceField(event) {
    this._source = { ...this._source, [event.target.dataset.field]: event.target.value.trim() };
  }

  _onStore() {
    const { id, kind, value, name } = this._source;
    this._ask(STORED_DRAFT_SUBJECT, sourceStoreCommand(id, kind, value, name || id));
  }

  _storedRow(source) {
    return html`
      <li data-stored=${source.id}>
        <h4>${source.name}</h4>
        <p><span data-value="kind">${KIND_NAMES[source.kind] ?? source.kind}</span>, <span data-id>${source.id}</span></p>
        <p data-value="value">${source.value}</p>
        <div class="row">
          <button type="button" data-stored=${source.id} aria-label="Forget stored source ${source.name}" @click=${this._onForget}>
            Forget
          </button>
        </div>
        ${this._refusal(storedSubject(source.id))}
      </li>
    `;
  }

  _storedForm() {
    const draft = this._source;
    const spotify = draft.kind === "spotify";
    return html`
      <div class="draft" data-draft="stored">
        <div class="row">
          <label for="stored-kind">Kind</label>
          <select id="stored-kind" data-field="kind" data-holds=${draft.kind} aria-label="Stored source kind" @change=${this._onSourceField}>
            <option value="url">${KIND_NAMES.url}</option>
            <option value="spotify">${KIND_NAMES.spotify}</option>
          </select>
          <label for="stored-id">Id</label>
          <input
            id="stored-id"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            data-field="id"
            .value=${draft.id}
            aria-label="Stored source id"
            @input=${this._onSourceField}
          />
        </div>
        <div class="row">
          <label for="stored-name">Name</label>
          <input id="stored-name" type="text" data-field="name" .value=${draft.name} aria-label="Stored source name" @input=${this._onSourceField} />
        </div>
        <div class="row">
          <label for="stored-value">${spotify ? "Spotify URI" : "Address"}</label>
          <input
            id="stored-value"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            data-field="value"
            .value=${draft.value}
            placeholder=${spotify ? "spotify:playlist:..." : "https://..."}
            aria-label="Stored source address"
            @input=${this._onSourceField}
          />
        </div>
        <p>
          ${spotify
            ? "A playlist, an album, a track or an episode, as the Spotify app shares it: spotify:playlist:<id>."
            : "An http:// or https:// address of a stream. Everyone who can open this app can read it: do not store one with a password in it."}
        </p>
        <div class="row">
          <button type="button" aria-label="Store source" ?disabled=${!draft.id || !draft.value} @click=${this._onStore}>
            Store source
          </button>
          <p>A source stored under an id already taken replaces it.</p>
        </div>
        ${this._refusal(STORED_DRAFT_SUBJECT)}
      </div>
    `;
  }

  // --- the sleep timers ----------------------------------------------------

  _sleepTargets() {
    return [...(this.rooms ?? []), ...(this.formedGroups ?? [])];
  }

  _onCancel(event) {
    const target = event.currentTarget.dataset.target;
    this._ask(sleepSubject(target), sleepCommand(target, 0));
  }

  _onSleepTarget(event) {
    this._timer = { ...this._timer, target: event.target.value };
  }

  _onSleepMinutes(event) {
    const minutes = bounded(event.target.value, MAX_SLEEP_MIN);
    event.target.value = String(minutes);
    this._timer = { ...this._timer, minutes };
  }

  _onSleep() {
    const target = this._timer.target || (this._sleepTargets()[0]?.id ?? "");
    if (target) this._ask(SLEEP_DRAFT_SUBJECT, sleepCommand(target, this._timer.minutes));
  }

  _sleepRow(timer) {
    const name = this._place(timer.target);
    const left = timer.remainingS === null ? `${timer.minutes ?? "?"} min asked for` : leftWords(this._left(timer));
    return html`
      <li data-sleep=${timer.target}>
        <h4>${name}</h4>
        <div class="row">
          <span class="figure" data-value="left">${left}</span>
          <button type="button" data-target=${timer.target} aria-label="Cancel sleep timer for ${name}" @click=${this._onCancel}>
            Cancel
          </button>
        </div>
        ${this._refusal(sleepSubject(timer.target))}
      </li>
    `;
  }

  _sleepForm() {
    const rooms = this.rooms ?? [];
    const groups = this.formedGroups ?? [];
    const target = this._timer.target || (this._sleepTargets()[0]?.id ?? "");
    const place = (item) => html`<option value=${item.id}>${item.name}</option>`;
    return html`
      <div class="draft" data-draft="sleep">
        <div class="row">
          <label for="sleep-target">For</label>
          <select id="sleep-target" data-holds=${target} aria-label="Sleep timer target" @change=${this._onSleepTarget}>
            ${rooms.length === 0 ? nothing : html`<optgroup label="Rooms">${rooms.map(place)}</optgroup>`}
            ${groups.length === 0 ? nothing : html`<optgroup label="Groups playing now">${groups.map(place)}</optgroup>`}
          </select>
          <label for="sleep-minutes">Minutes</label>
          <input
            id="sleep-minutes"
            type="number"
            min="0"
            max=${MAX_SLEEP_MIN}
            step="1"
            .value=${String(this._timer.minutes)}
            aria-label="Sleep timer minutes"
            @change=${this._onSleepMinutes}
          />
          <button type="button" aria-label="Start sleep timer" ?disabled=${!target} @click=${this._onSleep}>Start</button>
        </div>
        <p>It fades the room out and stops it when the time is up. 0 minutes cancels the timer it has.</p>
        ${this._refusal(SLEEP_DRAFT_SUBJECT)}
      </div>
    `;
  }

  render() {
    if (!this.known) return html`<p role="status" data-missing>Reading this server's alarms.</p>`;
    const alarms = this.alarms ?? [];
    const stored = this.stored ?? [];
    const sleep = this.sleep ?? [];
    return html`
      <h2>Alarms</h2>
      <p>An alarm rings in its room or its saved group on the server's own clock, and rises from silence to its volume.</p>
      ${alarms.length === 0
        ? html`<p role="status" data-none="alarms">This server has no alarm.</p>`
        : html`<ul aria-label="Alarms">
            ${alarms.map((alarm) => this._alarmRow(alarm))}
          </ul>`}
      <h3>Set an alarm</h3>
      ${this._alarmForm()}

      <h2>Stored sources</h2>
      <p>A stream URL or a Spotify URI the server keeps for an alarm to play.</p>
      ${stored.length === 0
        ? html`<p role="status" data-none="stored">This server has no stored source.</p>`
        : html`<ul aria-label="Stored sources">
            ${stored.map((source) => this._storedRow(source))}
          </ul>`}
      <h3>Store a source</h3>
      ${this._storedForm()}

      <h2>Sleep timers</h2>
      ${sleep.length === 0
        ? html`<p role="status" data-none="sleep">No sleep timer is running.</p>`
        : html`<ul aria-label="Sleep timers">
            ${sleep.map((timer) => this._sleepRow(timer))}
          </ul>`}
      <h3>Start a sleep timer</h3>
      ${this._sleepForm()}
    `;
  }
}

customElements.define("chorus-alarms", ChorusAlarms);

/** The id this screen is registered under (routes.js): `addressOf(ALARMS_SCREEN)` is its address. */
export const ALARMS_SCREEN = "alarms";

const places = (list) => list.map(({ id, name }) => ({ id, name }));

registerScreen({
  id: ALARMS_SCREEN,
  path: "alarms",
  title: () => "Alarms and sleep timers",
  render: (_params, { view, refusals, refusalFields }) => {
    const groups = view.groups ?? [];
    return html`
      <chorus-alarms
        .known=${view.state !== null}
        .heard=${view.state}
        .alarms=${alarmsOf(view.state)}
        .stored=${storedSourcesOf(view.state)}
        .sleep=${sleepOf(view.state)}
        .chimes=${chimesOf(view.state)}
        .receivers=${receiversOf(view.state)}
        .inputs=${view.inputs ?? []}
        .rooms=${places(view.rooms)}
        .savedGroups=${places(groups.filter((group) => group.kind === "saved"))}
        .formedGroups=${places(groups.filter((group) => group.rooms.length > 0))}
        .refusals=${refusals}
        .refusalFields=${refusalFields}
      ></chorus-alarms>
    `;
  },
});
