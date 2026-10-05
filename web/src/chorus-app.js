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
// Beyond the groups and the rooms there are further screens (routes.js), each
// with an address of its own in the fragment. The shell follows the address:
// at the home it paints the two regions, and at a screen's address it paints
// that screen alone in the main region, across both columns, under a "Back"
// link. A link with `data-route` anywhere under it opens its address as an
// entry of the browser's history, so the browser's back button returns; the
// link is in the screen's region and not the header, which a wide kiosk does
// not show. The navigation's two buttons leave a further screen for the
// region they name. A room's screens are linked from the room's card; a
// screen of the whole house (autoplay.js, alarms.js, speakers.js) is linked
// from the home, under the rooms.
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
import { ALARMS_SCREEN } from "./alarms.js";
import { groupOfRoom, moveCommand } from "./grouping.js";
import { AUTOPLAY_SCREEN } from "./autoplay.js";
import "./groups.js";
import { LAYOUTS, watchLayout } from "./layout.js";
import "./limits.js";
import { MODES } from "./mode.js";
import "./room-correction.js";
import "./rooms.js";
import { HOME, HOME_ADDRESS, addressOf, createNavigation, screenOf } from "./routes.js";
import "./sound.js";
import { SPEAKERS_SCREEN } from "./speakers.js";
import "./theater.js";

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
    // The field the server named in that refusal, by the same ids.
    _refusalFields: { state: true },
    // The route the address names (routes.js): { screen, params, address }.
    _route: { state: true },
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
    /* A further screen: alone on the page, across both columns, under the
     * way back. The link is a control like the navigation's buttons. */
    :host([layout="desktop"]) main[data-screen] {
      grid-column: 1 / -1;
    }
    main[data-screen] {
      display: flex;
      flex-direction: column;
      align-items: stretch;
      gap: var(--surface-gap);
    }
    a[data-route] {
      display: inline-flex;
      box-sizing: border-box;
      align-items: center;
      justify-content: center;
      align-self: start;
      min-width: var(--control-basis);
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      text-decoration: none;
    }
    section {
      padding-bottom: var(--reset-margin);
    }
    /* The home's links to the house-wide screens, under the rooms. */
    a.more {
      margin-top: var(--surface-gap);
    }
    a.more + a.more {
      margin-left: var(--surface-gap);
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
    this._refusalFields = {};
    this._navigation = createNavigation();
    this._route = this._navigation.route();
    this._unroute = null;
    // The region a navigation button asked for while a further screen was
    // open: it takes the focus once the home is painted again.
    this._goingTo = null;
    this.addEventListener("click", (event) => this._onLink(event));
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
    this._unroute?.();
    this._unroute = this._navigation.watch((route) => {
      if (route.address !== this._route.address) this._route = route;
    });
  }

  // A new screen is where the keyboard and a screen reader go on from: its
  // region takes the focus, as a page's start does after a navigation.
  updated(changed) {
    if (!changed.has("_route") || changed.get("_route") === undefined) return;
    const going = this._goingTo;
    this._goingTo = null;
    const region = this.renderRoot.querySelector(going === "groups" ? "section" : "main");
    if (!region) return;
    if (going) region.scrollIntoView?.({ block: "start" });
    region.focus?.({ preventScroll: !going });
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._unsubscribe?.();
    this._unsubscribe = null;
    this._unwatch?.();
    this._unwatch = null;
    this._unroute?.();
    this._unroute = null;
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
    this._refusalFields = { ...this._refusalFields, [subject]: "" };
    const result = await this.store.command(body);
    if (result.ok) return;
    this._refusals = { ...this._refusals, [subject]: result.refusal };
    this._refusalFields = { ...this._refusalFields, [subject]: result.field ?? "" };
  }

  // A screen that has to know when its command was answered (limits.js) says
  // so with a `done` in the event's detail: it is called once the server has
  // answered, accepted or refused, and what it answered is held.
  _onCommand(event) {
    const { subject, room, body, done } = event.detail;
    this._send(subject ?? room, body).then(() => done?.());
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
    const go = event.currentTarget.dataset.go;
    if (this._route.screen !== "home") {
      // A further screen is open: leave it, and go on when the home is back.
      this._goingTo = go;
      this._navigation.back();
      return;
    }
    const region = this.renderRoot.querySelector(go === "rooms" ? "main" : "section");
    if (!region) return;
    region.scrollIntoView?.({ block: "start" });
    region.focus?.({ preventScroll: true });
  }

  // A click on a link to a screen (`data-route`), anywhere under the shell:
  // the app opens the address itself, as an entry of the history it marks
  // (routes.js), and `data-route="back"` leaves the screen. A click with a
  // modifier or another button is the browser's (a new tab, a new window).
  _onLink(event) {
    if (event.defaultPrevented || event.button > 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const link = event.composedPath().find((node) => node?.localName === "a" && node.hasAttribute("data-route"));
    if (!link) return;
    event.preventDefault();
    if (link.dataset.route === "back") this._navigation.back();
    else this._navigation.open(link.getAttribute("href"));
  }

  // A further screen, alone in the main region. An address that names a
  // screen this build does not have is the home (routes.js), so there is
  // always one to paint.
  _screen(route) {
    const screen = screenOf(route.screen);
    const context = { view: this._view, refusals: this._refusals, refusalFields: this._refusalFields, store: this.store };
    return html`
      <main
        aria-label=${screen.title(route.params, this._view)}
        data-screen=${screen.id}
        tabindex="-1"
        @chorus-command=${this._onCommand}
      >
        <a href=${HOME_ADDRESS} data-route="back" aria-label="Back to rooms">Back</a>
        ${screen.render(route.params, context)}
      </main>
    `;
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
      ${this._signedOut()} ${this._route.screen === HOME.screen ? this._home() : this._screen(this._route)}
    `;
  }

  // The home: the groups, then the rooms.
  _home() {
    return html`
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
        <a class="more" href=${addressOf(AUTOPLAY_SCREEN)} data-route aria-label="Autoplay rules">Autoplay</a>
        <a class="more" href=${addressOf(ALARMS_SCREEN)} data-route aria-label="Alarms and sleep timers">Alarms</a>
        <a class="more" href=${addressOf(SPEAKERS_SCREEN)} data-route aria-label="Speakers and their setup">Speakers</a>
        <slot></slot>
      </main>
    `;
  }
}

customElements.define("chorus-app", ChorusApp);
