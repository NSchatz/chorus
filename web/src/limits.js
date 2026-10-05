// A room's volume limit and quiet hours: the room's maximum volume, the
// switch that says whether its quiet-hours windows cap it, and the windows
// themselves, each the days it starts on, a start, an end and a cap
// (docs/control-plane.md: `limit`, `quiet_hours`, `quiet_hours_enabled`;
// docs/decisions/0150-quiet-hours-switched-off-and-on-per-room.md).
//
// It is one of the app's further screens and registers itself with the
// navigation (routes.js) at `#/rooms/<room>/limits`; a room's card links to
// it.
//
// Everything it shows of the room is the server's (state.js, `limitsOf`):
// the limit, the limit in force now (`effective_limit`, which the server
// computes and the app never does), the room's volume as the server holds it
// under that limit, and for each window whether the server's clock is inside
// it. A control asks for its change with a `chorus-command` event and changes
// when the state that resulted comes back; a refused command shows the field
// the server named and its words.
//
// The catalog has one command for a room's windows and it replaces them all,
// so a change to one window (a day, a time, its cap), a removal and an
// addition each send the whole list as the server last said it with that one
// change made. A second change made before the server has answered the first
// (three days tapped one after another) is made to the list the first one
// asked for, not to the state the first has yet to reach, or it would undo
// it; that list is kept only until every command of this screen has been
// answered, and is never shown.
//
// Two things here are the page's own and not the server's. A slider or a
// time held by a person keeps its position until it is let go, as on a
// room's card. And the window being written under "Add a window" is a draft:
// nothing of it is sent until "Add window" is pressed, so no cap comes into
// force while a person is still choosing its hours.

import { LitElement, css, html, nothing } from "lit";

import { MAX_QUIET_WINDOWS, WEEK, limitCommand, quietHoursCommand, quietHoursEnabledCommand } from "./api.js";
import { registerScreen } from "./routes.js";
import { roomOf } from "./state.js";

/** The subject a room's limit and quiet-hours commands are sent under: its refusals are this screen's. */
export const limitsSubject = (room) => `limits:${room}`;

const DAY_NAMES = {
  mon: ["Mon", "Monday"],
  tue: ["Tue", "Tuesday"],
  wed: ["Wed", "Wednesday"],
  thu: ["Thu", "Thursday"],
  fri: ["Fri", "Friday"],
  sat: ["Sat", "Saturday"],
  sun: ["Sun", "Sunday"],
};

// The draft a person starts from: every night, at a quarter of full scale.
const DRAFT = Object.freeze({ days: WEEK, start: "22:00", end: "07:00", limit: 250 });

const percent = (thousandths) => `${Math.round(thousandths / 10)}%`;
const isTime = (value) => /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
// A window as the command takes it: what the server said, less `active`.
const sendable = ({ days, start, end, limit }) => ({ days, start, end, limit });

export class ChorusRoomLimits extends LitElement {
  static properties = {
    // The room, as state.js reads it (its `limits` and its `volume` are what
    // is shown), or null when the server has no room with the address's id.
    room: { attribute: false },
    // The id the address names, for the words when there is no such room.
    roomId: { type: String },
    // Whether a state has been read at all: before it, no room is known.
    known: { type: Boolean },
    // The server's words for the last command of this screen it refused, or
    // "", and the field it named, or "".
    refusal: { type: String },
    refusalField: { type: String },
    // A slider's position during a drag, by the control's key; absent otherwise.
    _dragged: { state: true },
    // The window being written, not yet sent.
    _draft: { state: true },
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
      padding-top: var(--surface-gap);
      font-size: var(--body-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ol {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li,
    .draft {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    li[data-active] {
      border-color: var(--accent);
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
    input[type="time"] {
      box-sizing: border-box;
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
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
    [data-value="active"][data-active] {
      color: var(--accent);
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
    this._draft = { ...DRAFT };
    this._held = new Set();
    // The windows the last unanswered `quiet_hours` asked for, { room,
    // windows }, and how many of this screen's are unanswered.
    this._asked = null;
    this._unanswered = 0;
  }

  // A control that shows a value of the server's (`data-server`) takes it
  // unless a person has hold of it. A refusal overrides that: the command did
  // not happen, and the control goes back to what the server still holds.
  updated(changed) {
    if (!this.room) return;
    const refused = changed.has("refusal") && Boolean(this.refusal);
    if (refused && Object.keys(this._dragged).length > 0) this._dragged = {};
    for (const control of this.renderRoot.querySelectorAll("input[data-server]")) {
      if (!this._held.has(control.dataset.key) || refused) control.value = control.dataset.server;
    }
  }

  _ask(body, done) {
    this.dispatchEvent(
      new CustomEvent("chorus-command", {
        detail: { subject: limitsSubject(this.room.id), body, done },
        bubbles: true,
        composed: true,
      }),
    );
  }

  // The room's windows with one change made: the windows as the server last
  // said them, or, while a `quiet_hours` of this screen is unanswered, the
  // windows that one asked for.
  _askWindows(change) {
    const room = this.room.id;
    const windows = (this._asked?.room === room ? this._asked.windows : this.room.limits.windows).map(sendable);
    change(windows);
    this._asked = { room, windows };
    this._unanswered += 1;
    this._ask(quietHoursCommand(room, windows), () => {
      this._unanswered -= 1;
      if (this._unanswered === 0) this._asked = null;
    });
  }

  _release(key) {
    if (!(key in this._dragged)) return;
    const { [key]: _, ...rest } = this._dragged;
    this._dragged = rest;
  }

  _onFocus(event) {
    this._held.add(event.target.dataset.key);
  }

  _onBlur(event) {
    const { key, server } = event.target.dataset;
    this._held.delete(key);
    this._release(key);
    event.target.value = server;
  }

  _onSliderInput(event) {
    this._dragged = { ...this._dragged, [event.target.dataset.key]: Number(event.target.value) };
  }

  // The gesture is over: ask for the value it ended on.
  _onLimitChange(event) {
    this._release(event.target.dataset.key);
    this._ask(limitCommand(this.room.id, Number(event.target.value)));
  }

  _onEnabled() {
    this._ask(quietHoursEnabledCommand(this.room.id, !this.room.limits.quietEnabled));
  }

  _onWindowLimit(event) {
    const at = Number(event.target.dataset.window);
    const limit = Number(event.target.value);
    this._release(event.target.dataset.key);
    this._askWindows((windows) => {
      windows[at] = { ...windows[at], limit };
    });
  }

  // A time that is not one (the field was cleared) is not asked for: the
  // field takes the server's value again.
  _onWindowTime(event) {
    const { window: at, edge, server } = event.target.dataset;
    const value = event.target.value;
    if (!isTime(value)) {
      event.target.value = server;
      return;
    }
    if (value === server) return;
    this._askWindows((windows) => {
      windows[Number(at)] = { ...windows[Number(at)], [edge]: value };
    });
  }

  // A day of a window, switched to the opposite of what the server holds. A
  // window with no day left is the server's to refuse, in its own words.
  _onWindowDay(event) {
    const { window: at, day } = event.currentTarget.dataset;
    this._askWindows((windows) => {
      const window = windows[Number(at)];
      const days = window.days.includes(day) ? window.days.filter((other) => other !== day) : [...window.days, day];
      windows[Number(at)] = { ...window, days };
    });
  }

  _onRemove(event) {
    const at = Number(event.currentTarget.dataset.window);
    this._askWindows((windows) => windows.splice(at, 1));
  }

  _onDraftDay(event) {
    const day = event.currentTarget.dataset.day;
    const days = this._draft.days.includes(day)
      ? this._draft.days.filter((other) => other !== day)
      : WEEK.filter((other) => other === day || this._draft.days.includes(other));
    this._draft = { ...this._draft, days };
  }

  _onDraftTime(event) {
    const edge = event.target.dataset.edge;
    if (!isTime(event.target.value)) {
      event.target.value = this._draft[edge];
      return;
    }
    this._draft = { ...this._draft, [edge]: event.target.value };
  }

  _onDraftLimit(event) {
    this._draft = { ...this._draft, limit: Number(event.target.value) };
  }

  // Add the draft after the room's windows. The draft stays as written: it
  // is the server's answer, a new window in the list or a refusal, that says
  // what became of it.
  _onAdd() {
    this._askWindows((windows) => windows.push({ ...this._draft }));
  }

  _days(pressed, which, onClick, at) {
    const room = this.room;
    return html`
      <div class="row" role="group" aria-label="Days of ${which} for ${room.name}">
        ${WEEK.map(
          (day) =>
            html`<button
              type="button"
              data-day=${day}
              data-window=${at ?? nothing}
              aria-label="${DAY_NAMES[day][1]}, ${which} for ${room.name}"
              aria-pressed=${pressed.includes(day) ? "true" : "false"}
              @click=${onClick}
            >
              ${DAY_NAMES[day][0]}
            </button>`,
        )}
      </div>
    `;
  }

  _window(window, at, readable) {
    const room = this.room;
    const which = `window ${at + 1}`;
    const on = room.limits.quietEnabled !== false;
    const active = window.active ? (on ? "Active now" : "Inside it now, and quiet hours are off") : "Not active now";
    if (!readable) {
      return html`<li data-window=${at}><p data-value="active">Unavailable</p></li>`;
    }
    const key = `window-${at}`;
    return html`
      <li data-window=${at} ?data-active=${window.active}>
        <div class="row">
          <strong>Window ${at + 1}</strong>
          <span data-value="active" ?data-active=${window.active && on}>${active}</span>
        </div>
        ${this._days(window.days, which, this._onWindowDay, at)}
        <div class="row">
          <label for="${key}-start">From</label>
          <input
            id="${key}-start"
            type="time"
            data-key="${key}-start"
            data-window=${at}
            data-edge="start"
            data-server=${window.start}
            aria-label="Start of ${which} for ${room.name}"
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @change=${this._onWindowTime}
          />
          <label for="${key}-end">Until</label>
          <input
            id="${key}-end"
            type="time"
            data-key="${key}-end"
            data-window=${at}
            data-edge="end"
            data-server=${window.end}
            aria-label="End of ${which} for ${room.name}"
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @change=${this._onWindowTime}
          />
        </div>
        <div class="row">
          <label for="${key}-limit">Limit</label>
          <input
            id="${key}-limit"
            type="range"
            min="0"
            max="1000"
            step="1"
            data-key="${key}-limit"
            data-window=${at}
            data-server=${window.limit}
            aria-label="Limit of ${which} for ${room.name}"
            aria-valuetext=${percent(this._dragged[`${key}-limit`] ?? window.limit)}
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @input=${this._onSliderInput}
            @change=${this._onWindowLimit}
          />
          <span class="figure" data-value="window-limit">${percent(this._dragged[`${key}-limit`] ?? window.limit)}</span>
        </div>
        <div class="row">
          <button type="button" data-window=${at} aria-label="Remove ${which} for ${room.name}" @click=${this._onRemove}>
            Remove
          </button>
        </div>
      </li>
    `;
  }

  _adding(count) {
    const room = this.room;
    if (count >= MAX_QUIET_WINDOWS) {
      return html`<p data-full>A room has at most ${MAX_QUIET_WINDOWS} windows. Remove one to add another.</p>`;
    }
    const draft = this._draft;
    const which = "the new window";
    return html`
      <div class="draft" data-draft>
        ${this._days(draft.days, which, this._onDraftDay)}
        <div class="row">
          <label for="draft-start">From</label>
          <input
            id="draft-start"
            type="time"
            data-edge="start"
            .value=${draft.start}
            aria-label="Start of ${which} for ${room.name}"
            @change=${this._onDraftTime}
          />
          <label for="draft-end">Until</label>
          <input
            id="draft-end"
            type="time"
            data-edge="end"
            .value=${draft.end}
            aria-label="End of ${which} for ${room.name}"
            @change=${this._onDraftTime}
          />
        </div>
        <div class="row">
          <label for="draft-limit">Limit</label>
          <input
            id="draft-limit"
            type="range"
            min="0"
            max="1000"
            step="1"
            .value=${String(draft.limit)}
            aria-label="Limit of ${which} for ${room.name}"
            aria-valuetext=${percent(draft.limit)}
            @input=${this._onDraftLimit}
          />
          <span class="figure" data-value="draft-limit">${percent(draft.limit)}</span>
        </div>
        <div class="row">
          <button
            type="button"
            aria-label="Add window for ${room.name}"
            ?disabled=${draft.days.length === 0}
            @click=${this._onAdd}
          >
            Add window
          </button>
          <p>${draft.days.length === 0 ? "A window starts on at least one day." : "Nothing is sent until it is added."}</p>
        </div>
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
    const { limit, effectiveLimit, quietEnabled, windows } = room.limits;
    // A list the page cannot read whole is not one it can send back whole.
    const readable = windows.every((w) => w.start && w.end && w.limit !== null && w.days.length > 0);
    const refused = this.refusal
      ? `Refused${this.refusalField ? ` (${this.refusalField})` : ""}: ${this.refusal}`
      : nothing;
    const figure = limit === null ? "Unavailable" : percent(this._dragged.limit ?? limit);
    return html`
      <h2>Volume limits of ${room.name}</h2>
      <div class="row">
        <label for="limit">Volume limit</label>
        ${limit === null
          ? nothing
          : html`<input
              id="limit"
              type="range"
              min="0"
              max="1000"
              step="1"
              data-key="limit"
              data-server=${limit}
              aria-label="Volume limit for ${room.name}"
              aria-valuetext=${figure}
              @focus=${this._onFocus}
              @blur=${this._onBlur}
              @input=${this._onSliderInput}
              @change=${this._onLimitChange}
            />`}
        <span class="figure" data-value="limit">${figure}</span>
      </div>
      <div class="row">
        <span>Limit in force now</span>
        <span class="figure" data-value="effective">${effectiveLimit === null ? "Unavailable" : percent(effectiveLimit)}</span>
        <span>Volume now</span>
        <span class="figure" data-value="volume">${room.volume === null ? "Unavailable" : percent(room.volume)}</span>
      </div>
      <h3>Quiet hours</h3>
      <div class="row">
        <button
          type="button"
          aria-label="Quiet hours for ${room.name}"
          aria-pressed=${quietEnabled === true ? "true" : "false"}
          ?disabled=${quietEnabled === null}
          @click=${this._onEnabled}
        >
          Quiet hours
        </button>
        <span data-value="enabled">${quietEnabled === null ? "Unavailable" : quietEnabled ? "On" : "Off"}</span>
        <p>Off, no window caps the room, and every window is kept.</p>
      </div>
      ${windows.length === 0 ? html`<p data-none>This room has no quiet-hours window.</p>` : nothing}
      <ol aria-label="Quiet-hours windows of ${room.name}">
        ${windows.map((window, at) => this._window(window, at, readable))}
      </ol>
      ${readable
        ? html`<h3>Add a window</h3>
            ${this._adding(windows.length)}`
        : html`<p data-unreadable>This server's windows cannot be read here, so they cannot be changed here.</p>`}
      <p role="alert" data-refusal-field=${this.refusalField || nothing}>${refused}</p>
    `;
  }
}

customElements.define("chorus-room-limits", ChorusRoomLimits);

/** The id this screen is registered under (routes.js): `addressOf(LIMITS_SCREEN, { room })` is its address. */
export const LIMITS_SCREEN = "room-limits";

registerScreen({
  id: LIMITS_SCREEN,
  path: "rooms/:room/limits",
  title: ({ room }, view) => `Volume limits of ${roomOf(view.rooms, room)?.name ?? room}`,
  render: ({ room }, { view, refusals, refusalFields }) => html`
    <chorus-room-limits
      .room=${roomOf(view.rooms, room)}
      .roomId=${room}
      .known=${view.state !== null}
      .refusal=${refusals[limitsSubject(room)] ?? ""}
      .refusalField=${refusalFields[limitsSubject(room)] ?? ""}
    ></chorus-room-limits>
  `,
});
