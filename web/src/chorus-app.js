// The app shell: the one element index.html holds. It paints the header and a
// labelled main region, and every screen a later change adds is rendered
// inside that region. It is the one element that holds the state layer's
// store (state.js): it subscribes, passes what the store holds down to the
// screens as properties, and sends the commands the screens ask for with a
// `chorus-command` event. A command the server refuses is shown, in the
// server's words, on the room or the group it was for.
//
// When the login in front of the server has lapsed (the store's status is
// "signed-out") it says so above everything else, with the way back in: a
// link to the page's own address. Following it is a navigation, which the
// login answers with its page and, signed in, sends back here.
//
// It also holds the one drag gesture (drag.js), because a room is dragged
// from one screen's card onto another's: the rooms and the groups region are
// both under it. A drop, the room's "Plays with" list and a group's "Remove"
// button all arrive here as a move (a room and a destination), and
// grouping.js turns a move into its one command.
//
// It lays itself out twice over (the ADR of the layouts and the kiosk):
//   layout   "phone" or "desktop", from the viewport's width (layout.js). A
//            phone is one column, the groups and then the rooms, with the
//            navigation in a bar fixed to the bottom edge, under the thumb. A
//            desktop is two columns, the groups with what each plays beside
//            the rooms, with the navigation in the header.
//   mode     "app" or "kiosk" (mode.js). A kiosk is a wall tablet: no
//            wordmark, every control at the kiosk's larger least size and
//            larger text. It takes the layout its width gives it like any
//            other screen. Keeping its screen on is main.js's (wake-lock.js).
// Both are reflected attributes, and the styles select on them: a breakpoint
// is a length, and no length is written in an element's styles.

import { LitElement, css, html, nothing } from "lit";

import { createDrag } from "./drag.js";
import { groupOfRoom, moveCommand } from "./grouping.js";
import "./groups.js";
import { LAYOUTS, watchLayout } from "./layout.js";
import { MODES } from "./mode.js";
import "./rooms.js";

export class ChorusApp extends LitElement {
  static properties = {
    // "app" or "kiosk" (mode.js). Reflected, so the styles below and a test
    // can read it off the element.
    mode: { type: String, reflect: true },
    // "phone" or "desktop" (layout.js): the element follows the viewport
    // while it is on the page. Reflected, for the same two readers.
    layout: { type: String, reflect: true },
    // The state layer's store (state.js), given by main.js.
    store: { attribute: false },
    // What the store holds: { state, rooms, groups, inputs, status }.
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
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      padding: var(--surface-pad);
      border-bottom: var(--stroke-1) solid var(--border);
    }
    nav {
      display: flex;
      box-sizing: border-box;
      gap: var(--bar-gap-x);
    }
    nav button {
      box-sizing: border-box;
      min-width: var(--control-basis);
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    :focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    /* A region the navigation went to holds the focus without a ring of its
     * own: what is ringed is the control a person then moves to. */
    section:focus,
    main:focus {
      outline: none;
    }
    section,
    main {
      min-width: var(--shrink-min);
    }

    /* The phone: one column, and the navigation fixed to the bottom edge,
     * under the thumb. The host keeps the bar's height free below the last
     * card, so the bar covers nothing. */
    :host([layout="phone"]) {
      padding-bottom: calc(var(--control-size) + var(--bar-pad-y) + var(--bar-pad-y) + var(--hairline));
    }
    :host([layout="phone"]) nav {
      position: fixed;
      right: var(--reset-margin);
      bottom: var(--reset-margin);
      left: var(--reset-margin);
      z-index: 1;
      padding: var(--bar-pad-y) var(--bar-pad-x);
      border-top: var(--hairline) solid var(--border);
      background: var(--panel);
    }
    :host([layout="phone"]) nav button {
      flex: 1 1 var(--control-basis);
    }

    /* The desktop: the groups, with what each plays, beside the rooms; the
     * header and the signed-out words run across both columns. */
    :host([layout="desktop"]) {
      display: grid;
      grid-template-columns: minmax(var(--surface-column-min), 1fr) minmax(var(--surface-column-min), 2fr);
      align-items: start;
    }
    :host([layout="desktop"]) header,
    :host([layout="desktop"]) [data-signed-out] {
      grid-column: 1 / -1;
    }
    :host([layout="desktop"]) nav {
      margin-left: auto;
    }
    :host([layout="desktop"]) section {
      padding-bottom: var(--surface-pad);
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
    [data-signed-out] {
      padding: var(--surface-pad);
      border-bottom: var(--stroke-1) solid var(--border);
      color: var(--warn);
      font-size: var(--body-size);
    }
    [data-signed-out] a {
      color: var(--link);
    }
    /* A wall tablet: every control at the kiosk's least size, larger text,
     * and nothing of the app around the rooms. The sizes are tokens the
     * elements under this one already read, set anew for them here. Wide, it
     * shows both columns and has nothing to navigate between; narrow, it
     * keeps the bar at the bottom edge. */
    :host([mode="kiosk"]) {
      --control-size: var(--kiosk-control-size);
      --control-basis: var(--kiosk-control-size);
      --control-basis-narrow: var(--kiosk-control-size);
      --body-size: var(--kiosk-body-size);
      --meta-size: var(--kiosk-meta-size);
      --heading-size: var(--kiosk-heading-size);
      font-size: var(--body-size);
    }
    :host([mode="kiosk"]) h1 {
      display: none;
    }
    :host([mode="kiosk"][layout="phone"]) header {
      padding: var(--reset-margin);
      border-bottom-width: var(--reset-margin);
    }
    :host([mode="kiosk"][layout="desktop"]) header {
      display: none;
    }
  `;

  constructor() {
    super();
    this.mode = "app";
    this.layout = "phone";
    this.store = null;
    this._view = { state: null, rooms: [], groups: [], inputs: [], status: "connecting" };
    this._refusals = {};
    this._moving = null;
    this._over = null;
    this._unsubscribe = null;
    this._unwatch = null;
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
    if (!LAYOUTS.includes(this.layout)) this.layout = "phone";
    if (changed.has("store")) this._follow();
  }

  connectedCallback() {
    super.connectedCallback();
    this._follow();
    this._unwatch?.();
    this._unwatch = watchLayout((layout) => {
      this.layout = layout;
    });
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._unsubscribe?.();
    this._unsubscribe = null;
    this._unwatch?.();
    this._unwatch = null;
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

  // The navigation: bring the region a button names to the top of the view
  // and put the focus on it, so the keyboard and a screen reader go on from
  // there.
  _onGo(event) {
    const region = this.renderRoot.querySelector(event.currentTarget.dataset.go === "rooms" ? "main" : "section");
    if (!region) return;
    region.scrollIntoView?.({ block: "start" });
    region.focus?.({ preventScroll: true });
  }

  // Signed out: the words and the way to sign in. The link is the page's own
  // address, layout and all, so signing in comes back to the same screen.
  _signedOut() {
    if (this._view.status !== "signed-out") return nothing;
    return html`
      <p role="alert" data-signed-out>
        Signed out. <a href=${globalThis.location?.href ?? "./"} aria-label="Sign in">Sign in</a> to go on.
      </p>
    `;
  }

  render() {
    return html`
      <header>
        <h1>chorus</h1>
        <nav aria-label="Sections">
          <button type="button" data-go="groups" aria-label="Go to groups" @click=${this._onGo}>Groups</button>
          <button type="button" data-go="rooms" aria-label="Go to rooms" @click=${this._onGo}>Rooms</button>
        </nav>
      </header>
      ${this._signedOut()}
      <section
        aria-label="Groups"
        tabindex="-1"
        @chorus-command=${this._onCommand}
        @chorus-move=${this._onMove}
      >
        <chorus-groups
          .groups=${this._view.state === null ? null : this._groups}
          .inputs=${this._view.inputs ?? []}
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
        tabindex="-1"
        @chorus-command=${this._onCommand}
        @chorus-move=${this._onMove}
        @pointerdown=${this._onPointerDown}
      >
        <chorus-rooms
          .rooms=${this._view.state === null ? null : this._view.rooms}
          .status=${this._view.status}
          .inputs=${this._view.inputs ?? []}
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
