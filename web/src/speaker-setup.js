// The walk-through for a compact Wi-Fi speaker: how a speaker that knows no
// network learns the house's, from a phone
// (docs/decisions/0103-wifi-provisioning-over-softap.md, decisions 2 to 4;
// the owner's bench step is docs/bench-packet.md S9).
//
// It is guidance, and a watch on the state. The speaker raises its own access
// point and serves its own join page there; a secure page cannot portably
// fetch a plain-HTTP device, so this app does not, and cannot, drive that
// page (0103, decision 3). The server has no
// route for any of it. What the app does is say the steps and wait: adoption
// on the audio network is the signal that it worked (K92), so the
// walk-through ends by itself when a speaker that was not adopted when it
// began appears in the state's `speakers`.
//
// The network's passphrase is typed into the speaker's page and nowhere else.
// This element has no field at all: it asks for nothing and holds nothing but
// the ids of the speakers that were adopted when it began.
//
// The phone leaves the house's network to reach the speaker, so the app loses
// the server while the walk-through is open. That is the store's "lost" (or
// "signed-out") status: the steps stay, the element says the server cannot be
// reached and that this is expected, and the store's own retry brings the
// state back. A phone may also drop the page while its owner is in the Wi-Fi
// settings and load it again afterwards, so the ids it began with are kept in
// the tab's session storage until the screen is left.
//
// The screen is registered by speakers.js, which links to it.

import { LitElement, css, html, nothing } from "lit";

/** What the access point's name starts with; six characters follow (0103, decision 4: `chorus-setup-<6>`). */
export const ACCESS_POINT_PREFIX = "chorus-setup-";
/** How many characters follow the prefix, and how long the setup secret is (0103, decision 4). */
export const ACCESS_POINT_SUFFIX_LENGTH = 6;
export const SETUP_SECRET_LENGTH = 12;
/** The speaker's own join page (0103, decision 3): `GET /` is the form and `POST /join` takes it. */
export const JOIN_PAGE = Object.freeze({ form: "GET /", takes: "POST /join", title: "chorus speaker setup" });

/**
 * The steps, in order: { id, title, text }, `text` one paragraph each. They
 * are the same whatever the state says: nothing the app knows changes what a
 * person does at the speaker.
 */
export const SETUP_STEPS = Object.freeze([
  {
    id: "power",
    title: "Switch the speaker on",
    text: [
      `A Wi-Fi speaker that knows no network raises a Wi-Fi access point of its own, named ${ACCESS_POINT_PREFIX} and ${ACCESS_POINT_SUFFIX_LENGTH} characters (${ACCESS_POINT_PREFIX}<${ACCESS_POINT_SUFFIX_LENGTH} characters>).`,
      `Its setup secret is ${SETUP_SECRET_LENGTH} characters and is the access point's password. The speaker prints it, and the address of its join page, on its serial console when the access point comes up.`,
    ],
  },
  {
    id: "access-point",
    title: "Join the speaker's access point",
    text: [
      `In this phone's Wi-Fi settings, join the network ${ACCESS_POINT_PREFIX}<${ACCESS_POINT_SUFFIX_LENGTH} characters> with the setup secret as its password. Accept that it has no internet.`,
      "The phone is then off the house's network, and this page cannot reach the chorus server until it is back. That is expected. Leave this page open.",
    ],
  },
  {
    id: "join-page",
    title: "Open the speaker's join page",
    text: [
      `In the phone's browser, open the address the speaker printed (http://<address>/). The speaker serves the page itself, on its access point: it is titled "${JOIN_PAGE.title}" and is a form with two fields.`,
      "Type the house network's name and its passphrase into that page, and press Join. They go to the speaker and nowhere else: this app never asks for them. The network has to be on 2.4 GHz and have a passphrase; the speaker refuses an open network.",
      'The page answers "Received". If the join fails the access point stays up: join it again and load the page again, and it says why above the form (auth-error for a wrong passphrase, network-not-found for a name it cannot see).',
    ],
  },
  {
    id: "return",
    title: "Come back to the house's network",
    text: [
      "The speaker takes its access point down and joins the house's network. The phone goes back to the house's network on its own, or join it again in the Wi-Fi settings.",
      "When the speaker reaches the chorus server it is adopted, and this page says so by itself. There is nothing to press.",
    ],
  },
]);

/**
 * The speakers that have arrived: those of `speakers` whose id is not one the
 * walk-through began with. Pure.
 *
 * @param {string[]} baseline the ids adopted when the walk-through began
 * @param {{ id: string }[]} speakers the adopted speakers now
 */
export function arrivals(baseline, speakers) {
  const known = new Set(baseline ?? []);
  return (speakers ?? []).filter((speaker) => !known.has(speaker.id));
}

// Where the ids the walk-through began with are kept while its screen is
// open: the tab's session storage, which a reload of the page keeps.
const STORAGE_KEY = "chorus-speaker-setup";

function heldBaseline(storage) {
  try {
    const held = JSON.parse(storage?.getItem(STORAGE_KEY) ?? "null");
    return Array.isArray(held) && held.every((id) => typeof id === "string") ? held : null;
  } catch {
    return null;
  }
}

function holdBaseline(storage, baseline) {
  try {
    if (baseline === null) storage?.removeItem(STORAGE_KEY);
    else storage?.setItem(STORAGE_KEY, JSON.stringify(baseline));
  } catch {
    // A browser that keeps nothing: the walk-through then begins anew after a reload.
  }
}

const storageOfTheTab = () => {
  try {
    return globalThis.sessionStorage ?? null;
  } catch {
    return null;
  }
};

export class ChorusSpeakerSetup extends LitElement {
  static properties = {
    // The adopted speakers (state.js, speakersOf), or null before a state has been read.
    speakers: { attribute: false },
    // The store's status: "connecting", "live", "lost" or "signed-out".
    status: { type: String },
    // The address of the speakers screen, where a new speaker is named.
    back: { type: String },
    // Where the ids it began with are kept across a reload; the tab's session storage.
    storage: { attribute: false },
    // The ids of the speakers adopted when the walk-through began, or null
    // before the first state.
    _baseline: { state: true },
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
    ol {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li,
    [data-done],
    [role="status"] {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    li p + p {
      margin-top: var(--control-pad-y);
    }
    [role="status"] {
      font-size: var(--body-size);
    }
    [data-away] {
      color: var(--warn);
    }
    [data-done] p:first-of-type {
      color: var(--ok);
      font-size: var(--body-size);
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      margin-top: var(--surface-gap);
    }
    button,
    a {
      display: inline-flex;
      box-sizing: border-box;
      align-items: center;
      justify-content: center;
      min-width: var(--control-basis);
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
      text-decoration: none;
    }
    button:focus-visible,
    a:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
  `;

  constructor() {
    super();
    this.speakers = null;
    this.status = "connecting";
    this.back = "#/";
    this.storage = storageOfTheTab();
    this._baseline = null;
  }

  // It begins at the first state it is shown: the speakers adopted then are
  // the ones that are not news. A page loaded again while its screen was open
  // goes on from the ids it began with.
  willUpdate() {
    if (!this.isConnected || this._baseline !== null || !Array.isArray(this.speakers)) return;
    this._baseline = heldBaseline(this.storage) ?? this.speakers.map((speaker) => speaker.id);
    holdBaseline(this.storage, this._baseline);
  }

  // Leaving the screen ends the walk-through: the next one begins anew.
  disconnectedCallback() {
    super.disconnectedCallback();
    holdBaseline(this.storage, null);
    this._baseline = null;
  }

  // Another speaker: begin again from the speakers adopted now.
  _onAgain() {
    if (!Array.isArray(this.speakers)) return;
    this._baseline = this.speakers.map((speaker) => speaker.id);
    holdBaseline(this.storage, this._baseline);
  }

  _status(arrived) {
    if (arrived.length > 0) return nothing;
    // "connecting" is the server not heard from yet, which the last line says.
    if (this.status === "lost" || this.status === "signed-out") {
      return html`<p role="status" data-away>
        ${this.status === "signed-out"
          ? "Signed out of the chorus server: sign in again to go on."
          : "This page cannot reach the chorus server now. That is expected while the phone is on the speaker's access point: it goes on by itself when the phone is back on the house's network."}
      </p>`;
    }
    if (this._baseline === null) return html`<p role="status" data-waiting>Reading this server's speakers.</p>`;
    return html`<p role="status" data-waiting>
      Waiting for a new speaker. This page goes on by itself when one is adopted.
    </p>`;
  }

  _done(arrived) {
    if (arrived.length === 0) return nothing;
    return html`
      <div data-done role="status">
        ${arrived.map(
          (speaker) => html`<p data-arrived=${speaker.id}>${speaker.name} (${speaker.id}) joined and was adopted.</p>`,
        )}
        <p>It has no name of its own and is in no room yet.</p>
        <div class="row">
          <a href=${this.back} data-route aria-label="Name the new speaker and give it a room">Name it and give it a room</a>
          <button type="button" aria-label="Set up another speaker" @click=${this._onAgain}>Set up another</button>
        </div>
      </div>
    `;
  }

  render() {
    const arrived = this._baseline === null ? [] : arrivals(this._baseline, this.speakers);
    return html`
      <h2>Set up a Wi-Fi speaker</h2>
      <p>
        A compact Wi-Fi speaker learns the house's network from a phone, on a page the speaker serves itself. This app
        says the steps and watches for the speaker; it never asks for the network's passphrase.
      </p>
      ${this._done(arrived)}
      <ol aria-label="Steps" ?data-complete=${arrived.length > 0}>
        ${SETUP_STEPS.map(
          (step, at) => html`
            <li data-step=${step.id}>
              <h3>${at + 1}. ${step.title}</h3>
              ${step.text.map((paragraph) => html`<p>${paragraph}</p>`)}
            </li>
          `,
        )}
      </ol>
      ${this._status(arrived)}
    `;
  }
}

customElements.define("chorus-speaker-setup", ChorusSpeakerSetup);
