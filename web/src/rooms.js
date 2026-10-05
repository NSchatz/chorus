// The rooms screen: every room the server has, one card each
// (room-card.js), in the server's order and in one plain column.
//
// Each room's list item is a drop target of the drag gesture (drag.js): a
// room dropped on it plays with that room.
//
// It is given what the state layer holds (state.js) and shows that and
// nothing else: the rooms, whether they are current or last known, and for
// each room the server's words for the last command it refused.

import { LitElement, css, html, nothing } from "lit";
import { repeat } from "lit/directives/repeat.js";

import { placeOf, placesFor } from "./grouping.js";
import "./room-card.js";

export class ChorusRooms extends LitElement {
  static properties = {
    // The rooms as state.js reads them, or null before the first state.
    rooms: { attribute: false },
    // "connecting", "live" or "lost" (state.js).
    status: { type: String },
    // The inputs the server offers, as state.js reads them.
    inputs: { attribute: false },
    // Refusal words by room id.
    refusals: { attribute: false },
    // The saved and live groups as state.js reads them.
    groups: { attribute: false },
    // The room being dragged, { id, name, grouped }, or null.
    moving: { attribute: false },
    // The destination under the pointer during a drag (grouping.js), or null.
    over: { attribute: false },
  };

  static styles = css`
    :host {
      display: block;
    }
    ul {
      display: flex;
      flex-direction: column;
      gap: var(--surface-gap);
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    p[data-status="lost"] {
      color: var(--warn);
    }
    li[data-moving] {
      border-radius: var(--surface-radius);
      outline: var(--stroke-1) dashed var(--border);
      outline-offset: var(--focus-ring-offset);
    }
    li[data-over] {
      border-radius: var(--surface-radius);
      outline: var(--focus-ring-width) solid var(--accent);
      outline-offset: var(--focus-ring-offset);
    }
    code {
      font-family: var(--face-figure);
      font-size: var(--code-size);
    }
  `;

  constructor() {
    super();
    this.rooms = null;
    this.inputs = [];
    this.status = "connecting";
    this.refusals = {};
    this.groups = [];
    this.moving = null;
    this.over = null;
  }

  // What the connection line says. Live says nothing: the rooms are the
  // message. A dropped feed says that what is shown is last known.
  _statusText() {
    if (this.status === "lost") {
      return this.rooms === null
        ? "The server cannot be reached."
        : "Connection lost. This is the last known state.";
    }
    return this.rooms === null ? "Reading this server's rooms." : "";
  }

  render() {
    const rooms = this.rooms;
    const groups = this.groups ?? [];
    const over = this.over;
    return html`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${rooms !== null && rooms.length === 0
        ? html`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`
        : nothing}
      <ul>
        ${repeat(
          rooms ?? [],
          (room) => room.id,
          (room) =>
            html`<li
              data-room=${room.id}
              data-drop="room"
              data-drop-id=${room.id}
              ?data-moving=${this.moving?.id === room.id}
              ?data-over=${over?.kind === "room" && over.id === room.id && this.moving?.id !== room.id}
            >
              <chorus-room-card
                .room=${room}
                .inputs=${this.inputs}
                .refusal=${this.refusals[room.id] ?? ""}
                .places=${placesFor(room, rooms, groups)}
                .place=${placeOf(room, groups)}
              ></chorus-room-card>
            </li>`,
        )}
      </ul>
    `;
  }
}

customElements.define("chorus-rooms", ChorusRooms);
