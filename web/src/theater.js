// A room's theater screen: the A/V trim in milliseconds, TV autoplay for the
// room's TV inputs with its two options, the TV upmix, and the sub's bass
// management (docs/control-plane.md, "The TV path" and "Per-room sound").
//
// It is one of the app's further screens and registers itself with the
// navigation (routes.js) at `#/rooms/<room>/theater`. A room's card links to
// it where it applies and nowhere else: a room that has a TV input (an input
// the state says is a TV's, `input_kinds`, offered by an endpoint of the room
// or playing in the room by its autoplay rule), or whose bonded set is more
// than a pair (state.js, `theaterOf`).
//
// Everything it shows is the server's (state.js); the screen keeps no value
// of its own. A control asks for its change with a `chorus-command` event
// carrying one message that names the one thing it changes:
//
//   the A/V trim           `av_trim`
//   the TV upmix           `sound`, with `tv_upmix` and no other field
//   the sub's three        `bass_management`, with the one field
//   TV autoplay            `autoplay` for the TV input, its whole rule: the
//                          autoplay screen's own element (autoplay.js), in
//                          its TV form
//
// The control changes when the state that resulted comes back, and a refused
// command shows the field the server named and its words. As on the sound
// screen, the one thing held back from the server's state is a slider's
// position while a person has hold of it.
//
// Bass management is the sub's: a room whose set has no sub (the server's
// `active`) shows no control for it, and says why. Nothing here says how well
// picture and sound line up: the trim is a number a person sets by eye and
// ear, and the screen shows the number.

import { LitElement, css, html, nothing } from "lit";

import {
  AV_TRIM_MS,
  CROSSOVER_HZ,
  SUB_LEVEL,
  SUB_POLARITIES,
  TV_UPMIXES,
  avTrimCommand,
  bassManagementCommand,
  decibelLiteral,
  soundCommand,
} from "./api.js";
import { autoplayRows } from "./autoplay.js";
import { registerScreen } from "./routes.js";
import { roomOf } from "./state.js";

/** The subject a room's theater commands are sent under: its refusals are this screen's. */
export const theaterSubject = (room) => `theater:${room}`;

/** A trim in words: "+40 ms", "0 ms", "-25 ms". */
export const milliseconds = (value) => `${value > 0 ? "+" : ""}${value} ms`;

/** A sub level (hundredths of a dB) in words: "+1.50 dB", "0.00 dB", "-3.25 dB". */
export const subLevel = (hundredths) => `${hundredths > 0 ? "+" : ""}${decibelLiteral(hundredths)} dB`;

// The three sliders, by the catalog's field. Each reads the server's value
// off the room, in the unit its command carries (the sub's level in
// hundredths of a dB), says it in words and makes its command. `scale` is how
// many of that unit one of the slider's own is: the level's slider is in dB.
const SLIDERS = {
  av_trim_ms: {
    name: "A/V trim",
    range: AV_TRIM_MS,
    scale: 1,
    step: 1,
    held: (room) => room.theater.avTrimMs,
    words: milliseconds,
    command: (room, value) => avTrimCommand(room.id, value),
  },
  crossover_hz: {
    name: "Crossover",
    range: CROSSOVER_HZ,
    scale: 1,
    step: 1,
    held: (room) => room.theater.bass.crossoverHz,
    words: (value) => `${value} Hz`,
    command: (room, value) => bassManagementCommand(room.id, { crossover_hz: value }),
  },
  sub_level_db: {
    name: "Sub level",
    range: SUB_LEVEL,
    scale: 100,
    step: 0.5,
    held: (room) => room.theater.bass.subLevel,
    words: subLevel,
    command: (room, value) => bassManagementCommand(room.id, { sub_level_db: value }),
  },
};

// The two choices, each a row of buttons with the server's one pressed.
const UPMIX_NAMES = { off: "Off", ambient: "Ambient" };
const POLARITY_NAMES = { normal: "Normal", inverted: "Inverted" };

export class ChorusRoomTheater extends LitElement {
  static properties = {
    // The room, as state.js reads it (its `theater` is what is shown), or
    // null when the server has no room with the id of the address.
    room: { attribute: false },
    // The id the address names, for the words when there is no such room.
    roomId: { type: String },
    // Whether a state has been read at all: before it, no room is known.
    known: { type: Boolean },
    // The room's TV inputs as the autoplay element shows them (autoplayRows).
    inputs: { attribute: false },
    // Where a TV input can play: the rooms and the saved groups, [{ id, name }].
    rooms: { attribute: false },
    groups: { attribute: false },
    // The server's words for the last refused command, by subject (the
    // autoplay element reads its inputs' out of it).
    refusals: { attribute: false },
    // The field the server named in the last refused theater command, or "".
    refusalField: { type: String },
    // A slider's position during a drag, by field; absent otherwise.
    _dragged: { state: true },
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
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    chorus-autoplay {
      margin-top: var(--surface-gap);
    }
    /* A slider keeps a width a finger can travel: in a narrow screen, or at
     * the kiosk's sizes, the row wraps and the slider takes a line. */
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label,
    .name {
      min-width: var(--label-min-width);
    }
    input[type="range"] {
      flex: 1 1 var(--slider-min-width);
      min-width: var(--slider-min-width);
      height: var(--control-size);
      margin: var(--reset-margin);
      accent-color: var(--accent);
    }
    .figure {
      min-width: var(--figure-min-width);
      font-family: var(--face-figure);
      text-align: end;
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
    input:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
  `;

  constructor() {
    super();
    this.room = null;
    this.roomId = "";
    this.known = false;
    this.inputs = [];
    this.rooms = [];
    this.groups = [];
    this.refusals = {};
    this.refusalField = "";
    this._dragged = {};
    this._held = new Set();
    this._shownRefusal = "";
  }

  get _refusal() {
    return (this.room && this.refusals?.[theaterSubject(this.room.id)]) || "";
  }

  // A slider's own figure for a value in the command's unit.
  _position(field, value) {
    return String(value / SLIDERS[field].scale);
  }

  // A slider takes the server's value unless a person has hold of it. A
  // refusal overrides that: the command did not happen, and the control goes
  // back to what the server still holds.
  // (The shell clears a subject's refusal when it sends the next command, so
  // a refusal that is new is one that was not there at the last paint.)
  updated() {
    if (!this.room) return;
    const refusal = this._refusal;
    const refused = Boolean(refusal) && refusal !== this._shownRefusal;
    this._shownRefusal = refusal;
    if (refused && Object.keys(this._dragged).length > 0) this._dragged = {};
    for (const slider of this.renderRoot.querySelectorAll("input[data-field]")) {
      const field = slider.dataset.field;
      const value = SLIDERS[field].held(this.room);
      if (value === null) continue;
      if (!this._held.has(field) || refused) slider.value = this._position(field, value);
    }
  }

  _send(body) {
    this.dispatchEvent(
      new CustomEvent("chorus-command", {
        detail: { subject: theaterSubject(this.room.id), body },
        bubbles: true,
        composed: true,
      }),
    );
  }

  _release(field) {
    if (!(field in this._dragged)) return;
    const { [field]: _, ...rest } = this._dragged;
    this._dragged = rest;
  }

  // What a slider stands at, in the command's unit.
  _read(slider) {
    return Math.round(Number(slider.value) * SLIDERS[slider.dataset.field].scale);
  }

  _onSliderFocus(event) {
    this._held.add(event.target.dataset.field);
  }

  _onSliderBlur(event) {
    const field = event.target.dataset.field;
    this._held.delete(field);
    this._release(field);
    const value = this.room ? SLIDERS[field].held(this.room) : null;
    if (value !== null) event.target.value = this._position(field, value);
  }

  _onSliderInput(event) {
    this._dragged = { ...this._dragged, [event.target.dataset.field]: this._read(event.target) };
  }

  // The gesture is over: ask for the value it ended on.
  _onSliderChange(event) {
    const field = event.target.dataset.field;
    this._release(field);
    this._send(SLIDERS[field].command(this.room, this._read(event.target)));
  }

  // One millisecond earlier or later than the server holds: a slider over
  // three hundred of them is coarse under a finger.
  _onNudge(event) {
    const held = this.room.theater.avTrimMs;
    if (held === null) return;
    this._send(avTrimCommand(this.room.id, held + Number(event.currentTarget.dataset.nudge)));
  }

  _onUpmix(event) {
    this._send(soundCommand(this.room.id, { tv_upmix: event.currentTarget.dataset.choice }));
  }

  _onPolarity(event) {
    this._send(bassManagementCommand(this.room.id, { sub_polarity: event.currentTarget.dataset.choice }));
  }

  _slider(field) {
    const room = this.room;
    const { name, range, scale, step, held, words } = SLIDERS[field];
    const value = held(room);
    const figure = value === null ? "Unavailable" : words(this._dragged[field] ?? value);
    return html`
      <div class="row">
        <label for=${field}>${name}</label>
        ${value === null
          ? nothing
          : html`<input
              id=${field}
              data-field=${field}
              type="range"
              min=${range.min / scale}
              max=${range.max / scale}
              step=${step}
              aria-label="${name} for ${room.name}"
              aria-valuetext=${figure}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-value=${field}>${figure}</span>
      </div>
    `;
  }

  _nudges() {
    const room = this.room;
    const held = room.theater.avTrimMs;
    const nudge = (by, name, face) => html`
      <button
        type="button"
        data-nudge=${by}
        aria-label="A/V trim 1 ms ${name} for ${room.name}"
        ?disabled=${held === null || held + by < AV_TRIM_MS.min || held + by > AV_TRIM_MS.max}
        @click=${this._onNudge}
      >
        ${face}
      </button>
    `;
    return html`<div class="row">${nudge(-1, "earlier", "1 ms earlier")} ${nudge(1, "later", "1 ms later")}</div>`;
  }

  // A choice among the catalog's words: a button each, the server's pressed.
  // A word the server holds that this build does not know is shown as it is.
  _choice({ field, name, words, names, held, onChoose }) {
    const room = this.room;
    return html`
      <div class="row" role="group" aria-label="${name} for ${room.name}">
        <span class="name">${name}</span>
        ${words.map(
          (word) => html`
            <button
              type="button"
              data-choice=${word}
              data-of=${field}
              aria-label="${name} ${names[word].toLowerCase()} for ${room.name}"
              aria-pressed=${held === word ? "true" : "false"}
              ?disabled=${held === null}
              @click=${onChoose}
            >
              ${names[word]}
            </button>
          `,
        )}
        <span data-value=${field}>${held === null ? "Unavailable" : (names[held] ?? held)}</span>
      </div>
    `;
  }

  _bass() {
    const { bass } = this.room.theater;
    if (!bass.active) {
      return html`<p data-no-sub>This room's set has no sub, so there is no bass management to set.</p>`;
    }
    return html`
      ${this._slider("crossover_hz")} ${this._slider("sub_level_db")}
      ${this._choice({
        field: "sub_polarity",
        name: "Sub polarity",
        words: SUB_POLARITIES,
        names: POLARITY_NAMES,
        held: bass.subPolarity,
        onChoose: this._onPolarity,
      })}
    `;
  }

  render() {
    const room = this.room;
    if (!room) {
      return html`<p role="status" data-missing>
        ${this.known ? `This server has no room "${this.roomId}".` : "Reading this server's rooms."}
      </p>`;
    }
    const theater = room.theater;
    if (!theater.offered) {
      return html`
        <h2>Theater of ${room.name}</h2>
        <p role="status" data-none>This room has no TV input and no theater set, so it has no theater settings.</p>
      `;
    }
    const refusal = this._refusal;
    const refused = refusal ? `Refused${this.refusalField ? ` (${this.refusalField})` : ""}: ${refusal}` : nothing;
    return html`
      <h2>Theater of ${room.name}</h2>
      <h3>A/V trim</h3>
      <p>Later delays this room's TV sound; earlier brings it forward.</p>
      ${this._slider("av_trim_ms")} ${this._nudges()}
      <chorus-autoplay
        tv
        .rows=${this.inputs ?? []}
        .rooms=${this.rooms}
        .groups=${this.groups}
        .refusals=${this.refusals}
        .home=${{ id: room.id, name: room.name }}
      ></chorus-autoplay>
      <h3>TV upmix</h3>
      <p>
        What the surround speakers of a theater set play from a TV in stereo: nothing, or an ambient
        surround.${theater.surrounds ? nothing : " This room's set has no surround speakers now."}
      </p>
      ${this._choice({
        field: "tv_upmix",
        name: "TV upmix",
        words: TV_UPMIXES,
        names: UPMIX_NAMES,
        held: theater.tvUpmix,
        onChoose: this._onUpmix,
      })}
      <h3>Bass management</h3>
      ${this._bass()}
      <p role="alert" data-refusal-field=${this.refusalField || nothing}>${refused}</p>
    `;
  }
}

customElements.define("chorus-room-theater", ChorusRoomTheater);

/** The id this screen is registered under (routes.js): `addressOf(THEATER_SCREEN, { room })` is its address. */
export const THEATER_SCREEN = "room-theater";

const places = (list) => list.map(({ id, name }) => ({ id, name }));

/**
 * The room's TV inputs as rows of the autoplay element: the house's rows
 * (autoplayRows) that are this room's TV inputs, each with its kind.
 */
export function theaterInputs(view, room) {
  if (!room) return [];
  const kinds = new Map(room.theater.tvInputs.map(({ input, kind }) => [input, kind]));
  return autoplayRows(view)
    .filter((row) => kinds.has(row.input))
    .map((row) => ({ ...row, kind: kinds.get(row.input) }));
}

registerScreen({
  id: THEATER_SCREEN,
  path: "rooms/:room/theater",
  title: ({ room }, view) => `Theater of ${roomOf(view.rooms, room)?.name ?? room}`,
  render: ({ room }, { view, refusals, refusalFields }) => html`
    <chorus-room-theater
      .room=${roomOf(view.rooms, room)}
      .roomId=${room}
      .known=${view.state !== null}
      .inputs=${theaterInputs(view, roomOf(view.rooms, room))}
      .rooms=${places(view.rooms)}
      .groups=${places((view.groups ?? []).filter((group) => group.kind === "saved"))}
      .refusals=${refusals}
      .refusalField=${refusalFields[theaterSubject(room)] ?? ""}
    ></chorus-room-theater>
  `,
});
