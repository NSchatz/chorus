// The rooms screen: every room the server has, one card each
// (room-card.js), in the server's order and in one plain column.
//
// It is given what the state layer holds (state.js) and shows that and
// nothing else: the rooms, whether they are current or last known, and for
// each room the server's words for the last command it refused.

import { LitElement, css, html, nothing } from "lit";
import { repeat } from "lit/directives/repeat.js";

import "./room-card.js";

export class ChorusRooms extends LitElement {
  static properties = {
    // The rooms as state.js reads them, or null before the first state.
    rooms: { attribute: false },
    // "connecting", "live" or "lost" (state.js).
    status: { type: String },
    // Refusal words by room id.
    refusals: { attribute: false },
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
    code {
      font-family: var(--face-figure);
      font-size: var(--code-size);
    }
  `;

  constructor() {
    super();
    this.rooms = null;
    this.status = "connecting";
    this.refusals = {};
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
            html`<li data-room=${room.id}>
              <chorus-room-card .room=${room} .refusal=${this.refusals[room.id] ?? ""}></chorus-room-card>
            </li>`,
        )}
      </ul>
    `;
  }
}

customElements.define("chorus-rooms", ChorusRooms);
