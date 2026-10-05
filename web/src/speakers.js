// The house's speakers: every adopted speaker with its name, its room, whether
// it is connected, its link and what it runs (docs/control-plane.md,
// "Speakers: adoption, names and rooms"; the state's `speakers` and
// `key_changes`).
//
// Adoption is the server's and is automatic (trust on first use): a speaker's
// first session pins its key and lists it, unnamed and in no room. This
// screen is where that speaker surfaces, marked new, and where a person names
// it (`speaker_name`), puts it in a room or takes it out (`speaker_room`, a
// room or null) and forgets it (`speaker_forget`). Each control sends one
// command and shows the server's state: the name field alone holds what a
// person is typing until the server has it.
//
// A changed key is a refusal, and is shown as one: the server refused a
// session under an adopted id because it offered another key, and the pin did
// not move. The entry says the two fingerprints and has no control. Nothing
// here accepts the offered key; the one way past is to forget the speaker,
// which is a button on the speaker's own row that asks again before it sends,
// and after which the next session under that id is adopted as a new speaker.
//
// It is one of the app's further screens (routes.js) at `#/speakers`, linked
// from the home, and it registers the walk-through for a Wi-Fi speaker
// (speaker-setup.js) at `#/speakers/setup`, which it links to.
//
// A speaker's firmware (the state's `firmware`) is not shown here.

import { LitElement, css, html, nothing } from "lit";

import { speakerForgetCommand, speakerNameCommand, speakerRoomCommand } from "./api.js";
import { registerScreen, addressOf } from "./routes.js";
import "./speaker-setup.js";
import { keyChangesOf, speakersOf } from "./state.js";

/** The subject a speaker's commands are sent under: its refusals are shown on its row. */
export const speakerSubject = (id) => `speaker:${id}`;

const LINKS = { wired: "Wired", wireless: "Wi-Fi" };

export class ChorusSpeakers extends LitElement {
  static properties = {
    // The adopted speakers (state.js, speakersOf), or null before a state has been read.
    speakers: { attribute: false },
    // The refused changed keys (state.js, keyChangesOf).
    keyChanges: { attribute: false },
    // The rooms a speaker can be put in, [{ id, name }].
    rooms: { attribute: false },
    // The server's words for the last refused command, by subject.
    refusals: { attribute: false },
    // The address of the walk-through for a Wi-Fi speaker.
    setup: { type: String },
    // What a person is typing into a speaker's name field, by speaker id.
    _drafts: { state: true },
    // The speaker whose "Forget" was pressed and waits for the second press.
    _forgetting: { state: true },
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
      font-size: var(--body-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    li[data-new],
    li[data-key-change] {
      border-color: var(--warn);
    }
    [data-new-mark],
    [data-key-changed] {
      color: var(--warn);
      font-size: var(--body-size);
    }
    dl {
      display: flex;
      flex-wrap: wrap;
      gap: var(--control-pad-y) var(--surface-gap);
      margin: var(--reset-margin);
      font-size: var(--meta-size);
    }
    dl div {
      display: flex;
      gap: var(--control-pad-y);
    }
    dt {
      color: var(--muted);
    }
    dd {
      margin: var(--reset-margin);
      overflow-wrap: anywhere;
    }
    code,
    [data-id] {
      font-family: var(--font-mono);
      overflow-wrap: anywhere;
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
      margin-top: var(--control-pad-y);
    }
    label {
      min-width: var(--label-min-width);
    }
    button,
    select,
    input,
    a {
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
    a {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      margin-top: var(--surface-gap);
      text-decoration: none;
    }
    select,
    input {
      flex: 1 1 var(--control-basis);
      min-width: var(--shrink-min);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    button[data-forget="yes"] {
      border-color: var(--bad);
      color: var(--bad);
    }
    select:focus-visible,
    input:focus-visible,
    a:focus-visible,
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
    this.speakers = null;
    this.keyChanges = [];
    this.rooms = [];
    this.refusals = {};
    this.setup = "";
    this._drafts = {};
    this._forgetting = null;
  }

  _speaker(id) {
    return (this.speakers ?? []).find((speaker) => speaker.id === id) ?? null;
  }

  // A name the server now holds is no longer a person's to hold: the field
  // follows the server again. A speaker that has gone takes its draft, and
  // its question, with it.
  willUpdate(changed) {
    if (!changed.has("speakers")) return;
    const drafts = Object.entries(this._drafts).filter(([id, draft]) => {
      const speaker = this._speaker(id);
      return speaker && !(speaker.named && speaker.name === draft.trim());
    });
    if (drafts.length !== Object.keys(this._drafts).length) this._drafts = Object.fromEntries(drafts);
    if (this._forgetting !== null && !this._speaker(this._forgetting)) this._forgetting = null;
  }

  _ask(id, body) {
    this.dispatchEvent(
      new CustomEvent("chorus-command", {
        detail: { subject: speakerSubject(id), body },
        bubbles: true,
        composed: true,
      }),
    );
  }

  _onDraft(event) {
    this._drafts = { ...this._drafts, [event.target.dataset.speaker]: event.target.value };
  }

  // The name as typed, without the spaces around it. An empty one, and the
  // name a person already gave, send nothing. The name the server made for an
  // unnamed speaker can be sent as it stands: that is a person naming it.
  _name(id) {
    const speaker = this._speaker(id);
    const name = (this._drafts[id] ?? speaker?.name ?? "").trim();
    if (!speaker || !name || (speaker.named && name === speaker.name)) return;
    this._ask(id, speakerNameCommand(id, name));
  }

  _onName(event) {
    this._name(event.currentTarget.dataset.speaker);
  }

  _onNameKey(event) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    this._name(event.target.dataset.speaker);
  }

  // Ask for the room, and put the list back on what the server holds: it
  // follows when the state that resulted comes back, and stays if the server
  // refuses.
  _onRoom(event) {
    const speaker = this._speaker(event.target.dataset.speaker);
    const value = event.target.value;
    const held = speaker?.room ?? "";
    event.target.value = held;
    if (!speaker || value === held) return;
    this._ask(speaker.id, speakerRoomCommand(speaker.id, value || null));
  }

  // The list takes the server's room whenever the screen is painted: an
  // option's `selected` is only where a list starts.
  updated() {
    for (const list of this.renderRoot.querySelectorAll("select[data-speaker]")) {
      const held = this._speaker(list.dataset.speaker)?.room ?? "";
      if (list.value !== held) list.value = held;
    }
  }

  _onForget(event) {
    const { speaker, forget } = event.currentTarget.dataset;
    if (forget === "ask") {
      this._forgetting = speaker;
      return;
    }
    this._forgetting = null;
    if (forget === "yes") this._ask(speaker, speakerForgetCommand(speaker));
  }

  _rooms(speaker) {
    const rooms = this.rooms ?? [];
    const known = speaker.room === null || rooms.some((room) => room.id === speaker.room);
    return html`
      <option value="" ?selected=${speaker.room === null}>No room</option>
      ${known ? nothing : html`<option value=${speaker.room} selected>${speaker.room} (not on this server now)</option>`}
      ${rooms.map((room) => html`<option value=${room.id} ?selected=${speaker.room === room.id}>${room.name}</option>`)}
    `;
  }

  _forget(speaker) {
    const { id, name } = speaker;
    if (this._forgetting !== id) {
      return html`
        <div class="row">
          <button type="button" data-speaker=${id} data-forget="ask" aria-label="Forget ${name}" @click=${this._onForget}>
            Forget
          </button>
        </div>
      `;
    }
    return html`
      <p data-forget-question>
        Forget ${name}? Its name, its room and its pinned key are removed. If it connects again it is adopted as a new
        speaker, whatever key it then offers.
      </p>
      <div class="row">
        <button type="button" data-speaker=${id} data-forget="yes" aria-label="Yes, forget ${name}" @click=${this._onForget}>
          Yes, forget it
        </button>
        <button type="button" data-speaker=${id} data-forget="no" aria-label="Keep ${name}" @click=${this._onForget}>
          Keep it
        </button>
      </div>
    `;
  }

  _row(speaker) {
    const { id, name } = speaker;
    const refusal = this.refusals?.[speakerSubject(id)] ?? "";
    const changed = (this.keyChanges ?? []).some((change) => change.id === id);
    const draft = this._drafts[id];
    const typed = (draft ?? name).trim();
    return html`
      <li data-speaker=${id} ?data-new=${speaker.isNew}>
        <h3>${name}</h3>
        ${speaker.isNew
          ? html`<p data-new-mark>New: adopted, not named and in no room yet.</p>`
          : nothing}
        ${changed
          ? html`<p data-key-changed>A session under this id offered another key and was refused (above).</p>`
          : nothing}
        <p data-id>${id}</p>
        <dl>
          <div>
            <dt>Now</dt>
            <dd data-value="present">${speaker.present ? "Connected" : "Not connected"}</dd>
          </div>
          <div>
            <dt>Link</dt>
            <dd data-value="link">${LINKS[speaker.link] ?? "Not reported"}</dd>
          </div>
          <div>
            <dt>Software</dt>
            <dd data-value="software">${speaker.software ?? "Not said yet"}</dd>
          </div>
          <div>
            <dt>Key</dt>
            <dd data-value="key">${speaker.key ?? "Not known"}</dd>
          </div>
        </dl>
        <div class="row">
          <label for="name-${id}">Name</label>
          <input
            id="name-${id}"
            type="text"
            autocomplete="off"
            data-speaker=${id}
            aria-label="Name of ${name}"
            .value=${draft ?? name}
            @input=${this._onDraft}
            @keydown=${this._onNameKey}
          />
          <button
            type="button"
            data-speaker=${id}
            aria-label="Save the name of ${name}"
            ?disabled=${!typed || (speaker.named && typed === name)}
            @click=${this._onName}
          >
            Save name
          </button>
        </div>
        <div class="row">
          <label for="room-${id}">Room</label>
          <select id="room-${id}" data-speaker=${id} aria-label="Room of ${name}" @change=${this._onRoom}>
            ${this._rooms(speaker)}
          </select>
        </div>
        ${this._forget(speaker)}
        <p role="alert">${refusal ? `Refused: ${refusal}` : nothing}</p>
      </li>
    `;
  }

  // A refused changed key: what was refused, the two fingerprints, and what a
  // person can do about it. It has no control of its own.
  _keyChange(change) {
    const speaker = this._speaker(change.id);
    return html`
      <li data-key-change=${change.id}>
        <h3>Refused: ${speaker?.name ?? change.id} offered a changed key</h3>
        <p role="alert">
          A session under the id <span data-id>${change.id}</span> offered a key that is not the one this id is pinned
          to. The server refused it, and the pinned key did not move.
        </p>
        <dl>
          <div>
            <dt>Pinned key</dt>
            <dd><code data-value="pinned">${change.pinned ?? "not known"}</code></dd>
          </div>
          <div>
            <dt>Offered key, refused</dt>
            <dd><code data-value="offered">${change.offered ?? "not known"}</code></dd>
          </div>
        </dl>
        <p>
          Nothing here accepts the offered key.
          ${speaker
            ? `If you replaced or wiped this speaker yourself, forget ${speaker.name} below: its next session is then adopted as a new speaker. If you did not, something else is answering under its id.`
            : "This id is not among the speakers listed here, so nothing on this screen can forget it."}
        </p>
      </li>
    `;
  }

  render() {
    if (this.speakers === null) return html`<p role="status" data-missing>Reading this server's speakers.</p>`;
    const changes = this.keyChanges ?? [];
    return html`
      <h2>Speakers</h2>
      <p>A speaker is adopted when it first connects. Name it and put it in a room here.</p>
      ${changes.length === 0
        ? nothing
        : html`<ul aria-label="Changed keys">
            ${changes.map((change) => this._keyChange(change))}
          </ul>`}
      ${this.speakers.length === 0
        ? html`<p role="status" data-none>This server has adopted no speaker yet.</p>`
        : html`<ul aria-label="Adopted speakers">
            ${this.speakers.map((speaker) => this._row(speaker))}
          </ul>`}
      ${this.setup
        ? html`<a href=${this.setup} data-route aria-label="Set up a Wi-Fi speaker">Set up a Wi-Fi speaker</a>`
        : nothing}
    `;
  }
}

customElements.define("chorus-speakers", ChorusSpeakers);

/** The ids the two screens are registered under (routes.js): `addressOf(SPEAKERS_SCREEN)` is `#/speakers`. */
export const SPEAKERS_SCREEN = "speakers";
export const SPEAKER_SETUP_SCREEN = "speaker-setup";

registerScreen({
  id: SPEAKERS_SCREEN,
  path: "speakers",
  title: () => "Speakers",
  render: (_params, { view, refusals }) => html`
    <chorus-speakers
      .speakers=${view.state === null ? null : speakersOf(view.state)}
      .keyChanges=${keyChangesOf(view.state)}
      .rooms=${view.rooms.map(({ id, name }) => ({ id, name }))}
      .refusals=${refusals}
      .setup=${addressOf(SPEAKER_SETUP_SCREEN)}
    ></chorus-speakers>
  `,
});

registerScreen({
  id: SPEAKER_SETUP_SCREEN,
  path: "speakers/setup",
  title: () => "Set up a Wi-Fi speaker",
  render: (_params, { view }) => html`
    <chorus-speaker-setup
      .speakers=${view.state === null ? null : speakersOf(view.state)}
      .status=${view.status}
      .back=${addressOf(SPEAKERS_SCREEN)}
    ></chorus-speaker-setup>
  `,
});
