// One room: its name, its bonded set with each member's channel role, its
// volume and its mute, and where it plays: alone, or in a group.
//
// Everything it shows is the room as the server last said it (state.js); the
// card keeps no value of its own. A volume or mute change is asked for with a
// `chorus-command` event carrying the control message, and the card changes
// when the state that resulted comes back.
//
// The one thing held back from the server's state is the slider's position
// while a person has hold of it: Lit patches the card in place, so an update
// never replaces the slider, and while the slider has focus an update does
// not move it either. The figure beside it follows the finger during a drag
// and otherwise says the server's volume, so the two never disagree in
// silence; the slider takes the server's value again when it loses focus, and
// at once when the server refuses a command.

//
// While the room is alone it also shows what it plays and what is playing,
// and offers the server's inputs (playing.js); a room in a group is shown
// that on the group's card, where a choice is the whole group's.
//
// Moving the room has two paths to the same commands (grouping.js). The
// handle beside the name is what the drag gesture picks the room up by
// (drag.js, Pointer Events: `touch-action: none` keeps a finger on it from
// scrolling the page instead). The "Plays with" list is the path with no
// drag, for a keyboard, a switch or a screen reader: it says where the room
// is as the server has it, and choosing another place asks for the move.
// Pressing the handle without dragging it goes to that list.

import { LitElement, css, html, nothing } from "lit";

import { muteCommand, volumeCommand } from "./api.js";
import { placeOfValue } from "./grouping.js";
import "./playing.js";

// The channel roles of docs/protocol.md's channel map, as words.
export const ROLE_NAMES = {
  FL: "Front left",
  FR: "Front right",
  FC: "Centre",
  LFE: "Subwoofer",
  BL: "Rear left",
  BR: "Rear right",
  SL: "Surround left",
  SR: "Surround right",
};

const percent = (thousandths) => `${Math.round(thousandths / 10)}%`;

export class ChorusRoomCard extends LitElement {
  static properties = {
    // The room, as state.js reads it: { id, name, volume, muted, bond,
    // source, nowPlaying }.
    room: { attribute: false },
    // The inputs the server offers, as state.js reads them.
    inputs: { attribute: false },
    // The server's words for the last command of this room it refused, or "".
    refusal: { type: String },
    // The places the room can play, [{ value, label }], and the one it is in
    // now (grouping.js: placesFor, placeOf).
    places: { attribute: false },
    place: { type: String },
    // The slider's position during a drag, in thousandths; null otherwise.
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
    .head {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
    }
    .head h2 {
      flex: 1;
    }
    .handle {
      cursor: grab;
      touch-action: none;
      user-select: none;
    }
    select {
      flex: 1;
      min-width: var(--shrink-min);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    h3 {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
      color: var(--muted);
      font-size: var(--meta-size);
    }
    .row {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label {
      min-width: var(--label-min-width);
    }
    input[type="range"] {
      flex: 1;
      min-width: var(--shrink-min);
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
    select:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      margin: var(--reset-margin);
      color: var(--bad);
    }
  `;

  constructor() {
    super();
    this.room = null;
    this.inputs = [];
    this.refusal = "";
    this.places = [];
    this.place = "alone";
    this._dragged = null;
    this._sliderHeld = false;
  }

  get _slider() {
    return this.renderRoot.querySelector("input[type=range]");
  }

  // The slider takes the server's volume unless a person has hold of it. A
  // refusal overrides that: the command did not happen, and the control goes
  // back to what the server still holds.
  updated(changed) {
    // The list says where the server has the room, after every update.
    const list = this._list;
    if (list) list.value = this.place;
    const slider = this._slider;
    if (!slider || this.room.volume === null) return;
    const refused = changed.has("refusal") && Boolean(this.refusal);
    if (refused) this._dragged = null;
    if (!this._sliderHeld || refused) slider.value = String(this.room.volume);
  }

  _ask(body) {
    this.dispatchEvent(
      new CustomEvent("chorus-command", { detail: { room: this.room.id, body }, bubbles: true, composed: true }),
    );
  }

  _onSliderFocus() {
    this._sliderHeld = true;
  }

  _onSliderBlur() {
    this._sliderHeld = false;
    this._dragged = null;
    if (this.room.volume !== null) this._slider.value = String(this.room.volume);
  }

  _onSliderInput(event) {
    this._dragged = Number(event.target.value);
  }

  // The gesture is over: ask for the volume it ended on.
  _onSliderChange(event) {
    this._dragged = null;
    this._ask(volumeCommand(this.room.id, Number(event.target.value)));
  }

  get _list() {
    return this.renderRoot.querySelector("select");
  }

  // Ask for the move, and put the list back on where the room is: it follows
  // when the state that resulted comes back, and stays if the server refuses.
  _onPlace(event) {
    const value = event.target.value;
    event.target.value = this.place;
    if (value === this.place) return;
    const destination = placeOfValue(value);
    if (!destination) return;
    this.dispatchEvent(
      new CustomEvent("chorus-move", { detail: { room: this.room.id, destination }, bubbles: true, composed: true }),
    );
  }

  // The handle pressed and not dragged (a click, Enter or Space): the path
  // with no drag is the list.
  _onHandle() {
    this._list?.focus();
  }

  _onMute() {
    this._ask(muteCommand(this.room.id, !this.room.muted));
  }

  render() {
    const room = this.room;
    if (!room) return nothing;
    const figure = room.volume === null ? "Unavailable" : percent(this._dragged ?? room.volume);
    return html`
      <div class="head">
        <h2>${room.name}</h2>
        <button
          type="button"
          class="handle"
          data-drag-room=${room.id}
          aria-label="Move ${room.name}"
          title="Drag onto a room or a group, or press to choose from the list"
          @click=${this._onHandle}
        >
          Move
        </button>
      </div>
      ${room.bond.length === 0
        ? nothing
        : html`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${room.bond.map(
                (member) =>
                  html`<li data-endpoint=${member.endpoint} data-role=${member.role}>
                    ${ROLE_NAMES[member.role] ?? member.role}: ${member.name}
                  </li>`,
              )}
            </ul>
          `}
      ${room.source
        ? html`<chorus-playing
            .target=${room.id}
            .name=${room.name}
            .source=${room.source}
            .nowPlaying=${room.nowPlaying}
            .inputs=${this.inputs}
            pick
          ></chorus-playing>`
        : nothing}
      <div class="row">
        <label for="volume">Volume</label>
        ${room.volume === null
          ? nothing
          : html`<input
              id="volume"
              type="range"
              min="0"
              max="1000"
              step="1"
              aria-label="Volume for ${room.name}"
              aria-valuetext=${figure}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-volume>${figure}</span>
      </div>
      <div class="row">
        <button
          type="button"
          aria-label="Mute ${room.name}"
          aria-pressed=${room.muted === true ? "true" : "false"}
          ?disabled=${room.muted === null}
          @click=${this._onMute}
        >
          Mute
        </button>
        <span data-mute>${room.muted === null ? "Unavailable" : room.muted ? "Muted" : "Not muted"}</span>
      </div>
      <div class="row">
        <label for="place">Plays with</label>
        <select id="place" aria-label="Group for ${room.name}" @change=${this._onPlace}>
          ${this.places.map(
            (place) => html`<option value=${place.value} ?selected=${place.value === this.place}>${place.label}</option>`,
          )}
        </select>
      </div>
      <p role="alert">${this.refusal ? `Refused: ${this.refusal}` : nothing}</p>
    `;
  }
}

customElements.define("chorus-room-card", ChorusRoomCard);
