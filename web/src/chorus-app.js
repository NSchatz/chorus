// The app shell: the one element index.html holds. It paints the header and a
// labelled main region, and every screen a later change adds is rendered
// inside that region. It is the one element that holds the state layer's
// store (state.js): it subscribes, passes what the store holds down to the
// screens as properties, and sends the commands the screens ask for with a
// `chorus-command` event. A command the server refuses is shown, in the
// server's words, on the room it was for.

import { LitElement, css, html } from "lit";

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
    // The server's words for the last refused command, by room id.
    _refusals: { state: true },
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
    main {
      padding: var(--surface-pad);
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
    this._view = { state: null, rooms: [], status: "connecting" };
    this._refusals = {};
    this._unsubscribe = null;
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

  async _onCommand(event) {
    const { room, body } = event.detail;
    if (!this.store) return;
    this._refusals = { ...this._refusals, [room]: "" };
    const result = await this.store.command(body);
    if (!result.ok) this._refusals = { ...this._refusals, [room]: result.refusal };
  }

  render() {
    return html`
      <header>
        <h1>chorus</h1>
      </header>
      <main aria-label="Rooms" @chorus-command=${this._onCommand}>
        <chorus-rooms
          .rooms=${this._view.state === null ? null : this._view.rooms}
          .status=${this._view.status}
          .refusals=${this._refusals}
        ></chorus-rooms>
        <slot></slot>
      </main>
    `;
  }
}

customElements.define("chorus-app", ChorusApp);
