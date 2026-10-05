// One group: a saved group (always shown, active or not: K59) or a live one.
// Its name, its rooms, a way to take each room out, for a saved group that
// is not active a way to form it, and the group volume (K77, Sonos-style).
//
// The group volume is the server's: `volume` is the figure the state message
// gives for the group (the average of its rooms as the server rounds it),
// and moving the slider asks for a group volume with `group_volume` and
// nothing more. The server scales every room, clamps each to its own limit
// and says what resulted; the card and the room cards then show that. The
// app scales no room and averages nothing, so it has no figure that could
// disagree with the server's.
//
// A group that is formed also shows what it plays and what is playing
// (playing.js). The inputs can be chosen for a live group and for a saved
// group that is active: a `take` naming a saved group that is only partly
// formed would also pull the rest of its rooms in, which is "Group these
// rooms", not a choice of input.
//
// The command is sent when the slider is let go, not while it moves: the
// server scales from the rooms' volumes as they stand, so a run of commands
// during one gesture would round the balance away step by step, and passing
// through zero would lose it. As on a room's card, the slider is not moved
// by an update while a person has hold of it, and the figure follows the
// finger meanwhile.

import { LitElement, css, html, nothing } from "lit";

import { groupVolumeCommand, takeCommand } from "./api.js";
import "./playing.js";

const percent = (thousandths) => `${Math.round(thousandths / 10)}%`;

export class ChorusGroupCard extends LitElement {
  static properties = {
    // The group, as state.js reads it: { id, name, kind, active, defined,
    // rooms, volume, source, nowPlaying }.
    group: { attribute: false },
    // The inputs the server offers, as state.js reads them.
    inputs: { attribute: false },
    // The server's words for the last command of this group it refused, or "".
    refusal: { type: String },
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
    p,
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
      color: var(--muted);
      font-size: var(--meta-size);
    }
    li,
    .row {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    li span:first-child {
      flex: 1;
      color: var(--fg);
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
    input:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      color: var(--bad);
    }
  `;

  constructor() {
    super();
    this.group = null;
    this.inputs = [];
    this.refusal = "";
    this._dragged = null;
    this._sliderHeld = false;
  }

  get _slider() {
    return this.renderRoot.querySelector("input[type=range]");
  }

  // The slider takes the server's group volume unless a person has hold of
  // it; a refusal puts it back at once.
  updated(changed) {
    const slider = this._slider;
    if (!slider || !this.group || this.group.volume === null) return;
    const refused = changed.has("refusal") && Boolean(this.refusal);
    if (refused) this._dragged = null;
    if (!this._sliderHeld || refused) slider.value = String(this.group.volume);
  }

  _ask(body) {
    this.dispatchEvent(
      new CustomEvent("chorus-command", { detail: { subject: this.group.id, body }, bubbles: true, composed: true }),
    );
  }

  _onSliderFocus() {
    this._sliderHeld = true;
  }

  _onSliderBlur() {
    this._sliderHeld = false;
    this._dragged = null;
    if (this._slider && this.group.volume !== null) this._slider.value = String(this.group.volume);
  }

  _onSliderInput(event) {
    this._dragged = Number(event.target.value);
  }

  // The gesture is over: ask the server for the group volume it ended on.
  _onSliderChange(event) {
    this._dragged = null;
    this._ask(groupVolumeCommand(this.group.id, Number(event.target.value)));
  }

  // Form a saved group: `take` on it moves every one of its rooms into it.
  _onActivate() {
    this._ask(takeCommand(this.group.id));
  }

  _onRemove(room) {
    this.dispatchEvent(
      new CustomEvent("chorus-move", {
        detail: { room: room.id, destination: { kind: "alone" } },
        bubbles: true,
        composed: true,
      }),
    );
  }

  // What the line under the name says.
  _kindText() {
    const group = this.group;
    if (group.kind === "live") return "Live group";
    if (group.active) return "Saved group, active";
    return group.rooms.length > 0 ? "Saved group, partly formed" : "Saved group, not active";
  }

  // The rooms listed: a saved group's own rooms in its order, then any other
  // room playing in it now; a live group's rooms.
  _listed() {
    const group = this.group;
    const playing = new Set(group.rooms.map((room) => room.id));
    const defined = group.defined ?? [];
    const definedIds = new Set(defined.map((room) => room.id));
    return [
      ...defined.map((room) => ({ ...room, playing: playing.has(room.id) })),
      ...group.rooms.filter((room) => !definedIds.has(room.id)).map((room) => ({ ...room, playing: true })),
    ];
  }

  render() {
    const group = this.group;
    if (!group) return nothing;
    const figure = group.volume === null ? "" : percent(this._dragged ?? group.volume);
    return html`
      <h2>${group.name}</h2>
      <p data-kind=${group.kind} data-active=${group.active === null ? nothing : String(group.active)}>
        ${this._kindText()}
      </p>
      <ul aria-label="Rooms of ${group.name}">
        ${this._listed().map(
          (room) =>
            html`<li data-member=${room.id} data-playing=${String(room.playing)}>
              <span>${room.name}</span>
              ${room.playing
                ? html`<button
                    type="button"
                    aria-label="Remove ${room.name} from ${group.name}"
                    @click=${() => this._onRemove(room)}
                  >
                    Remove
                  </button>`
                : html`<span>Not in the group now</span>`}
            </li>`,
        )}
      </ul>
      ${group.source
        ? html`<chorus-playing
            .target=${group.id}
            .name=${group.name}
            .source=${group.source}
            .nowPlaying=${group.nowPlaying}
            .inputs=${this.inputs}
            ?pick=${group.kind === "live" || group.active === true}
          ></chorus-playing>`
        : nothing}
      ${group.kind === "saved" && !group.active
        ? html`<div class="row">
            <button type="button" aria-label="Group the rooms of ${group.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`
        : nothing}
      ${group.volume === null
        ? nothing
        : html`<div class="row">
            <label for="volume">Group volume</label>
            <input
              id="volume"
              type="range"
              min="0"
              max="1000"
              step="1"
              aria-label="Group volume for ${group.name}"
              aria-valuetext=${figure}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />
            <span class="figure" data-volume>${figure}</span>
          </div>`}
      <p role="alert">${this.refusal ? `Refused: ${this.refusal}` : nothing}</p>
    `;
  }
}

customElements.define("chorus-group-card", ChorusGroupCard);
