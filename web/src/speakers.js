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
// Firmware (docs/firmware-updates.md; the state's `firmware` on a speaker
// and `firmware.images` on the server). A speaker that reported what it runs
// shows the version, the board and the slot, and what it is doing about an
// update or how its last install ended, in words, with the server's reason.
// "Update available" is the server's own `update_available` and is shown for
// no other speaker; the version offered is the staged image's, from
// `firmware.images`.
//
// Nothing here installs by itself (K93). `firmware_install` is sent only when
// a person presses "Install" on one image for one speaker and then confirms
// the question that names both; showing, a new state, a rescan and a reload
// send none. The server decides whether it may: a refusal (a speaker that is
// not on the server's own host without the owner's bench variable is
// `owner-not-at-bench`) is shown on the speaker's row in the server's words.
// "Cancel install" sends `firmware_cancel` while there is something to
// cancel, and "Rescan" sends `firmware_rescan`. A server whose state has no
// `firmware` has no firmware directory: it shows what a speaker runs and no
// update control at all.

import { LitElement, css, html, nothing } from "lit";

import {
  firmwareCancelCommand,
  firmwareInstallCommand,
  firmwareRescanCommand,
  speakerForgetCommand,
  speakerNameCommand,
  speakerRoomCommand,
} from "./api.js";
import { registerScreen, addressOf } from "./routes.js";
import "./speaker-setup.js";
import { firmwareImagesOf, keyChangesOf, speakersOf, updatesFor } from "./state.js";

/** The subject a speaker's commands are sent under: its refusals are shown on its row. */
export const speakerSubject = (id) => `speaker:${id}`;

/** The subject `firmware_rescan` is sent under: its refusal is shown with the staged images. */
export const FIRMWARE_SUBJECT = "firmware";

const LINKS = { wired: "Wired", wireless: "Wi-Fi" };

// The firmware states in which an install is in progress (the server refuses
// another as `busy`), and those of them `firmware_cancel` can still abandon.
const BUSY = ["requested", "receiving", "verified", "pending_verify"];
const CANCELLABLE = ["requested", "receiving"];

// The image a firmware state speaks of, in words: its staged name and its
// version where the server gave them.
function imageWords(firmware) {
  const version = firmware.imageVersion ? `version ${firmware.imageVersion}` : "";
  if (firmware.image) return version ? `image ${firmware.image} (${version})` : `image ${firmware.image}`;
  return version || "an image";
}

/**
 * A speaker's firmware state in words (docs/control-plane.md lists the
 * states). A state this does not know is said in the server's own word.
 */
export function firmwareStateWords(firmware) {
  const what = imageWords(firmware);
  const runs = firmware.version ? `version ${firmware.version}` : "the version it ran before";
  switch (firmware.state) {
    case "idle":
      return "No install is in progress.";
    case "requested":
      return `Install requested: the server is offering ${what} to the speaker.`;
    case "receiving":
      return `Receiving ${what}: ${firmware.received} of ${firmware.size} bytes.`;
    case "verified":
      return `Written and checked: ${what}. The speaker restarts into it.`;
    case "pending_verify":
      return `On trial: the speaker runs ${runs} and has not confirmed it yet.`;
    case "confirmed":
      return `Installed: ${what} confirmed itself, and the speaker runs ${runs}.`;
    case "rolled_back":
      return `Rolled back: ${what} did not confirm, and the speaker runs ${runs} again. Nothing retries it.`;
    case "refused":
      return `Refused by the speaker: ${what} was not installed.`;
    case "interrupted":
      return `Interrupted: the install of ${what} did not finish and is not resumed. Install again to start over.`;
    case "cancelled":
      return `Cancelled: the install of ${what} was abandoned.`;
    default:
      return `Firmware state: ${firmware.state}.`;
  }
}

export class ChorusSpeakers extends LitElement {
  static properties = {
    // The adopted speakers (state.js, speakersOf), or null before a state has been read.
    speakers: { attribute: false },
    // The refused changed keys (state.js, keyChangesOf).
    keyChanges: { attribute: false },
    // The rooms a speaker can be put in, [{ id, name }].
    rooms: { attribute: false },
    // The staged firmware images (state.js, firmwareImagesOf), or null on a
    // server with no firmware directory: no update control is shown then.
    images: { attribute: false },
    // The server's words for the last refused command, by subject.
    refusals: { attribute: false },
    // The address of the walk-through for a Wi-Fi speaker.
    setup: { type: String },
    // What a person is typing into a speaker's name field, by speaker id.
    _drafts: { state: true },
    // The speaker whose "Forget" was pressed and waits for the second press.
    _forgetting: { state: true },
    // The install that was asked for and waits for its confirmation:
    // { speaker, image }, the speaker's id and the staged image's name.
    _installing: { state: true },
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
    h3,
    h4 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3,
    h4 {
      font-size: var(--body-size);
    }
    section {
      margin-top: var(--surface-gap);
    }
    [data-update-available] {
      color: var(--warn);
      font-size: var(--body-size);
    }
    progress {
      flex: 1 1 var(--control-basis);
      min-width: var(--shrink-min);
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
    this.images = null;
    this._drafts = {};
    this._forgetting = null;
    this._installing = null;
  }

  _speaker(id) {
    return (this.speakers ?? []).find((speaker) => speaker.id === id) ?? null;
  }

  // A name the server now holds is no longer a person's to hold: the field
  // follows the server again. A speaker that has gone takes its draft, and
  // its question, with it.
  willUpdate(changed) {
    // An install that is waiting for its confirmation is asked about what the
    // server says now: when the speaker has gone, or the image is no longer
    // an update for it, the question goes and nothing is sent.
    if (this._installing !== null && (changed.has("speakers") || changed.has("images"))) {
      const speaker = this._speaker(this._installing.speaker);
      const offered = speaker && this._offers(speaker).some((image) => image.name === this._installing.image);
      if (!offered) this._installing = null;
    }
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

  // The staged images a person can install on this speaker now: the ones the
  // server's `update_available` is about, and none while an install is in
  // progress or on a server with no firmware directory.
  _offers(speaker) {
    const firmware = speaker.firmware;
    if (!firmware || this.images === null || BUSY.includes(firmware.state)) return [];
    return updatesFor(firmware, this.images);
  }

  // "Install" only asks; the command is sent by "Yes, install it" alone, and
  // names the image and the speaker the question named.
  _onInstall(event) {
    const { speaker, image, install } = event.currentTarget.dataset;
    if (install === "ask") {
      this._installing = { speaker, image };
      return;
    }
    const asked = this._installing;
    this._installing = null;
    if (install !== "yes" || !asked || asked.speaker !== speaker || asked.image !== image) return;
    this._ask(speaker, firmwareInstallCommand(speaker, image));
  }

  _onCancelInstall(event) {
    const { speaker } = event.currentTarget.dataset;
    this._ask(speaker, firmwareCancelCommand(speaker));
  }

  _onRescan() {
    this.dispatchEvent(
      new CustomEvent("chorus-command", {
        detail: { subject: FIRMWARE_SUBJECT, body: firmwareRescanCommand() },
        bubbles: true,
        composed: true,
      }),
    );
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

  // One staged image that is an update for this speaker: what it is, and the
  // button that asks. While it is asked about, the question and its two
  // answers instead.
  _offer(speaker, image) {
    const { id, name } = speaker;
    const version = image.version ?? "not said";
    const asked = this._installing?.speaker === id && this._installing?.image === image.name;
    if (!asked) {
      return html`
        <div class="row" data-update=${image.name}>
          <span data-update-version>Version ${version}, image ${image.name}</span>
          <button
            type="button"
            data-speaker=${id}
            data-image=${image.name}
            data-install="ask"
            aria-label="Install image ${image.name} (version ${version}) on ${name}"
            ?disabled=${!speaker.present}
            @click=${this._onInstall}
          >
            Install
          </button>
        </div>
      `;
    }
    return html`
      <div data-update=${image.name}>
        <p data-install-question>
          Install image ${image.name} (version ${version}) on ${name} (${id})? It runs version
          ${speaker.firmware.version ?? "not said"} now. The image is sent to the speaker, which restarts into it and
          stops playing while it does. If the new version does not confirm itself, the speaker goes back to the one it
          runs now.
        </p>
        <div class="row">
          <button
            type="button"
            data-speaker=${id}
            data-image=${image.name}
            data-install="yes"
            aria-label="Yes, install image ${image.name} on ${name}"
            @click=${this._onInstall}
          >
            Yes, install it
          </button>
          <button
            type="button"
            data-speaker=${id}
            data-image=${image.name}
            data-install="no"
            aria-label="Do not install image ${image.name} on ${name}"
            @click=${this._onInstall}
          >
            Not now
          </button>
        </div>
      </div>
    `;
  }

  // A speaker's firmware: what it runs, what it is doing about an update or
  // how the last install ended (with the server's reason), and, on a server
  // that stages images, the update the server says is available and the
  // controls for it. A speaker that reported no firmware has none of this.
  _firmware(speaker) {
    const firmware = speaker.firmware;
    if (!firmware) return nothing;
    const { id, name } = speaker;
    const staged = this.images !== null;
    const offers = this._offers(speaker);
    const receiving = firmware.state === "receiving" && firmware.size > 0;
    return html`
      <section data-firmware aria-label="Firmware of ${name}">
        <h4>Firmware</h4>
        <dl>
          <div>
            <dt>Runs</dt>
            <dd data-value="firmware-version">${firmware.version ?? "Not said"}</dd>
          </div>
          <div>
            <dt>Board</dt>
            <dd data-value="firmware-board">${firmware.board ?? "Not said"}</dd>
          </div>
          <div>
            <dt>Slot</dt>
            <dd data-value="firmware-slot">${firmware.slot ?? "Not said"}</dd>
          </div>
        </dl>
        <p role="status" data-firmware-state=${firmware.state}>
          ${firmwareStateWords(firmware)}
          ${firmware.reason ? html`<span data-firmware-reason>Reason: ${firmware.reason}.</span>` : nothing}
        </p>
        ${receiving
          ? html`<div class="row">
              <progress
                max=${firmware.size}
                value=${Math.min(firmware.received, firmware.size)}
                aria-label="Install progress of ${name}"
              ></progress>
            </div>`
          : nothing}
        ${staged && CANCELLABLE.includes(firmware.state)
          ? html`<div class="row">
              <button
                type="button"
                data-speaker=${id}
                data-cancel-install
                aria-label="Cancel the install on ${name}"
                @click=${this._onCancelInstall}
              >
                Cancel install
              </button>
            </div>`
          : nothing}
        ${staged && firmware.updateAvailable
          ? html`
              <p data-update-available>Update available</p>
              ${offers.map((image) => this._offer(speaker, image))}
              ${offers.length > 0 && !speaker.present
                ? html`<p data-update-absent>The speaker is not connected: it can be installed when it is.</p>`
                : nothing}
              ${offers.length === 0 && !BUSY.includes(firmware.state)
                ? html`<p data-update-unlisted>
                    The server lists no verified image for this board with another version. Rescan the staged images.
                  </p>`
                : nothing}
            `
          : nothing}
      </section>
    `;
  }

  // The staged images as the server graded them, and the rescan. Only on a
  // server that has a firmware directory.
  _images() {
    if (this.images === null) return nothing;
    const refusal = this.refusals?.[FIRMWARE_SUBJECT] ?? "";
    return html`
      <section data-firmware-images aria-label="Firmware images">
        <h3>Firmware images</h3>
        <p>
          Images are staged as files in the server's firmware directory. Nothing is installed until you press Install
          on a speaker and confirm it.
        </p>
        ${this.images.length === 0
          ? html`<p role="status" data-no-images>No image is staged.</p>`
          : html`<ul aria-label="Staged images">
              ${this.images.map(
                (image) => html`
                  <li data-image=${image.name}>
                    <h4>${image.name}</h4>
                    <dl>
                      <div>
                        <dt>Version</dt>
                        <dd data-value="version">${image.version ?? "Not read"}</dd>
                      </div>
                      <div>
                        <dt>Board</dt>
                        <dd data-value="board">${image.board ?? "Not read"}</dd>
                      </div>
                      <div>
                        <dt>Verdict</dt>
                        <dd data-value="verdict">
                          ${image.verified
                            ? "Verified"
                            : `Refused: ${image.reason ?? "no reason given"}. It is never offered to a speaker.`}
                        </dd>
                      </div>
                    </dl>
                  </li>
                `,
              )}
            </ul>`}
        <div class="row">
          <button type="button" data-rescan aria-label="Rescan the staged firmware images" @click=${this._onRescan}>
            Rescan
          </button>
        </div>
        <p role="alert">${refusal ? `Refused: ${refusal}` : nothing}</p>
      </section>
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
        ${this._firmware(speaker)}
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
      ${this._images()}
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
      .images=${firmwareImagesOf(view.state)}
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
