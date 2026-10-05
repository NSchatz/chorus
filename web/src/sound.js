// A room's sound screen: bass and treble (whole dB), loudness, night mode
// and speech enhancement, the five settings of the catalog's `sound` command
// (docs/control-plane.md, "Per-room sound").
//
// It is the first of the app's further screens and registers itself with the
// navigation (routes.js) at `#/rooms/<room>/sound`; a room's card links to it.
//
// Everything it shows is the room's `sound` as the server last said it
// (state.js); the screen keeps no value of its own. A control asks for its
// change with a `chorus-command` event carrying one `sound` message that
// names the one field that control changes and no other, so a setting another
// client has just moved is never written back from a stale page. The control
// changes when the state that resulted comes back, and a refused command
// shows the field the server named and its words.
//
// As on a room's card, the one thing held back from the server's state is a
// slider's position while a person has hold of it: the figure beside it
// follows the finger during a drag and otherwise says the server's value,
// and the slider takes the server's value again when it loses focus, and at
// once when the server refuses.

import { LitElement, css, html, nothing } from "lit";

import { TONE_DB, soundCommand } from "./api.js";
import { registerScreen } from "./routes.js";
import { roomOf } from "./state.js";

/** The subject a room's sound commands are sent under: its refusals are this screen's, not the card's. */
export const soundSubject = (room) => `sound:${room}`;

// The two tone controls and the three switches, in the catalog's order, with
// the words each is called by.
const TONES = [
  { field: "bass", name: "Bass" },
  { field: "treble", name: "Treble" },
];
const SWITCHES = [
  { field: "loudness", name: "Loudness", says: "Fuller bass and treble at low volume" },
  { field: "night", name: "Night mode", says: "Loud passages held down, quiet ones brought up" },
  { field: "speech", name: "Speech enhancement", says: "Voices brought forward" },
];

/** A tone in words: "+3 dB", "0 dB", "-2 dB". */
export const decibels = (value) => `${value > 0 ? "+" : ""}${value} dB`;

export class ChorusRoomSound extends LitElement {
  static properties = {
    // The room, as state.js reads it (its `sound` is what is shown), or null
    // when the server has no room with the id of the address.
    room: { attribute: false },
    // The id the address names, for the words when there is no such room.
    roomId: { type: String },
    // Whether a state has been read at all: before it, no room is known.
    known: { type: Boolean },
    // The server's words for the last sound command of this room it refused,
    // or "", and the field it named, or "".
    refusal: { type: String },
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
    h2 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
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
    this.refusal = "";
    this.refusalField = "";
    this._dragged = {};
    this._held = new Set();
  }

  _slider(field) {
    return this.renderRoot.querySelector(`input[data-field="${field}"]`);
  }

  // A slider takes the server's value unless a person has hold of it. A
  // refusal overrides that: the command did not happen, and the control goes
  // back to what the server still holds.
  updated(changed) {
    if (!this.room) return;
    const refused = changed.has("refusal") && Boolean(this.refusal);
    if (refused && Object.keys(this._dragged).length > 0) this._dragged = {};
    for (const { field } of TONES) {
      const slider = this._slider(field);
      const value = this.room.sound[field];
      if (!slider || value === null) continue;
      if (!this._held.has(field) || refused) slider.value = String(value);
    }
  }

  // Ask for one field to change, and nothing else.
  _ask(field, value) {
    this.dispatchEvent(
      new CustomEvent("chorus-command", {
        detail: { subject: soundSubject(this.room.id), body: soundCommand(this.room.id, { [field]: value }) },
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

  _onSliderFocus(event) {
    this._held.add(event.target.dataset.field);
  }

  _onSliderBlur(event) {
    const field = event.target.dataset.field;
    this._held.delete(field);
    this._release(field);
    const value = this.room?.sound[field];
    if (value !== null && value !== undefined) event.target.value = String(value);
  }

  _onSliderInput(event) {
    this._dragged = { ...this._dragged, [event.target.dataset.field]: Number(event.target.value) };
  }

  // The gesture is over: ask for the value it ended on.
  _onSliderChange(event) {
    const field = event.target.dataset.field;
    this._release(field);
    this._ask(field, Number(event.target.value));
  }

  _onSwitch(event) {
    const field = event.currentTarget.dataset.field;
    this._ask(field, !this.room.sound[field]);
  }

  _tone({ field, name }) {
    const room = this.room;
    const value = room.sound[field];
    const figure = value === null ? "Unavailable" : decibels(this._dragged[field] ?? value);
    return html`
      <div class="row">
        <label for=${field}>${name}</label>
        ${value === null
          ? nothing
          : html`<input
              id=${field}
              data-field=${field}
              type="range"
              min=${TONE_DB.min}
              max=${TONE_DB.max}
              step="1"
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

  _switch({ field, name, says }) {
    const room = this.room;
    const value = room.sound[field];
    return html`
      <div class="row">
        <button
          type="button"
          data-field=${field}
          aria-label="${name} for ${room.name}"
          aria-pressed=${value === true ? "true" : "false"}
          ?disabled=${value === null}
          @click=${this._onSwitch}
        >
          ${name}
        </button>
        <span data-value=${field}>${value === null ? "Unavailable" : value ? "On" : "Off"}</span>
        <p>${says}</p>
      </div>
    `;
  }

  render() {
    const room = this.room;
    if (!room) {
      return html`<p role="status" data-missing>
        ${this.known ? `This server has no room "${this.roomId}".` : "Reading this server's rooms."}
      </p>`;
    }
    const refused = this.refusal
      ? `Refused${this.refusalField ? ` (${this.refusalField})` : ""}: ${this.refusal}`
      : nothing;
    return html`
      <h2>Sound of ${room.name}</h2>
      ${TONES.map((tone) => this._tone(tone))} ${SWITCHES.map((flag) => this._switch(flag))}
      <p role="alert" data-refusal-field=${this.refusalField || nothing}>${refused}</p>
    `;
  }
}

customElements.define("chorus-room-sound", ChorusRoomSound);

/** The id this screen is registered under (routes.js): `addressOf(SOUND_SCREEN, { room })` is its address. */
export const SOUND_SCREEN = "room-sound";

registerScreen({
  id: SOUND_SCREEN,
  path: "rooms/:room/sound",
  title: ({ room }, view) => `Sound of ${roomOf(view.rooms, room)?.name ?? room}`,
  render: ({ room }, { view, refusals, refusalFields }) => html`
    <chorus-room-sound
      .room=${roomOf(view.rooms, room)}
      .roomId=${room}
      .known=${view.state !== null}
      .refusal=${refusals[soundSubject(room)] ?? ""}
      .refusalField=${refusalFields[soundSubject(room)] ?? ""}
    ></chorus-room-sound>
  `,
});
