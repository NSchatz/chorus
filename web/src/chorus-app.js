// The app shell: the one element index.html holds. It paints the header and a
// labelled main region, and every screen a later change adds is rendered
// inside that region. It is the one element that holds the state layer's
// store (state.js): it subscribes, passes what the store holds down to the
// screens as properties, and sends the commands the screens ask for with a
// `chorus-command` event. A command the server refuses is shown, in the
// server's words, on the room or the group it was for.
//
// It also holds the one drag gesture (drag.js), because a room is dragged
// from one screen's card onto another's: the rooms and the groups region are
// both under it. A drop, the room's "Plays with" list and a group's "Remove"
// button all arrive here as a move (a room and a destination), and
// grouping.js turns a move into its one command.

import { LitElement, css, html, nothing } from "lit";

import { createDrag } from "./drag.js";
import { groupOfRoom, moveCommand } from "./grouping.js";
import "./groups.js";
import { MODES } from "./mode.js";
import "./rooms.js";

export class ChorusApp extends LitElement {
  static properties = {
    // "app" or "kiosk" (mode.js). Reflected, so the styles below and a test
    // can read it off the element.
    mode: { type: String, reflect: true },
    // The state layer's store (state.js), given by main.js.
    store: { attribute: false },
    // What the store holds: { state, rooms, status }.
    _view: { state: true },
    // The server's words for the last refused command, by room or group id.
    _refusals: { state: true },
    // The room being dragged, { id, name, grouped }, and the destination
    // under the pointer; null when no drag is on.
    _moving: { state: true },
    _over: { state: true },
  };

  // Adopted as a constructable stylesheet, which chorus-server's
  // Content-Security-Policy (style-src 'self', no inline style) allows.
  static styles = css`
    :host {
      display: block;
    }
    header {
      display: flex;
      align-items: baseline;
      gap: var(--surface-gap);
      padding: var(--surface-pad);
      border-bottom: var(--stroke-1) solid var(--border);
    }
    h1 {
      margin: var(--reset-margin);
      color: var(--wordmark-ink);
      font-size: var(--wordmark-size);
      letter-spacing: var(--tracking-wordmark);
    }
    section,
    main {
      padding: var(--surface-pad);
    }
    section {
      padding-bottom: var(--reset-margin);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    /* A wall tablet shows the rooms and nothing of the app around them. */
    :host([mode="kiosk"]) header {
      display: none;
    }
  `;

  constructor() {
    super();
    this.mode = "app";
    this.store = null;
    this._view = { state: null, rooms: [], groups: [], status: "connecting" };
    this._refusals = {};
    this._moving = null;
    this._over = null;
    this._unsubscribe = null;
    this._drag = createDrag({
      onStart: (id) => {
        const room = this._room(id);
        if (!room) return;
        this._moving = { id, name: room.name, grouped: Boolean(groupOfRoom(room, this._groups)) };
      },
      onOver: (destination) => {
        const now = this._over;
        if (now?.kind === destination?.kind && now?.id === destination?.id) return;
        this._over = destination;
      },
      onEnd: (id, destination) => {
        this._moving = null;
        this._over = null;
        if (destination) this._move(id, destination);
      },
    });
  }

  get _groups() {
    return this._view.groups ?? [];
  }

  _room(id) {
    return this._view.rooms.find((room) => room.id === id) ?? null;
  }

  willUpdate(changed) {
    if (!MODES.includes(this.mode)) this.mode = "app";
    if (changed.has("store")) this._follow();
  }

  connectedCallback() {
    super.connectedCallback();
    this._follow();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._unsubscribe?.();
    this._unsubscribe = null;
    this._drag.cancel();
  }

  // Subscribe to the store this element has now, and to no other.
  _follow() {
    this._unsubscribe?.();
    this._unsubscribe = null;
    if (!this.store || !this.isConnected) return;
    this._unsubscribe = this.store.subscribe((view) => {
      this._view = view;
    });
  }

  // Send one command; `subject` is the room or the group whose card shows
  // the server's words if it is refused.
  async _send(subject, body) {
    if (!this.store) return;
    this._refusals = { ...this._refusals, [subject]: "" };
    const result = await this.store.command(body);
    if (!result.ok) this._refusals = { ...this._refusals, [subject]: result.refusal };
  }

  _onCommand(event) {
    const { subject, room, body } = event.detail;
    this._send(subject ?? room, body);
  }

  // A move: the room goes to the destination, if that is somewhere else.
  _move(id, destination) {
    const room = this._room(id);
    const body = moveCommand(room, destination, this._groups);
    if (body) this._send(id, body);
  }

  _onMove(event) {
    this._move(event.detail.room, event.detail.destination);
  }

  _onPointerDown(event) {
    this._drag.begin(event);
  }

  render() {
    return html`
      <header>
        <h1>chorus</h1>
      </header>
      <section
        aria-label="Groups"
        @chorus-command=${this._onCommand}
        @chorus-move=${this._onMove}
      >
        <chorus-groups
          .groups=${this._view.state === null ? null : this._groups}
          .refusals=${this._refusals}
          .moving=${this._moving}
          .over=${this._over}
        ></chorus-groups>
        <p role="status" data-drag>
          ${this._moving ? `Moving ${this._moving.name}. Drop it on a room or a group.` : nothing}
        </p>
      </section>
      <main
        aria-label="Rooms"
        @chorus-command=${this._onCommand}
        @chorus-move=${this._onMove}
        @pointerdown=${this._onPointerDown}
      >
        <chorus-rooms
          .rooms=${this._view.state === null ? null : this._view.rooms}
          .status=${this._view.status}
          .refusals=${this._refusals}
          .groups=${this._groups}
          .moving=${this._moving}
          .over=${this._over}
        ></chorus-rooms>
        <slot></slot>
      </main>
    `;
  }
}

customElements.define("chorus-app", ChorusApp);
