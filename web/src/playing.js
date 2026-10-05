// What a room alone or a group plays: what is playing (the now-playing
// record: artwork, title, artist, album, and whether it is playing, paused or
// buffering), what it plays (its source, in words), and the inputs the server
// offers, one button each, with the one it plays now marked.
//
// Everything shown is the server's (state.js): the record is a fact about
// now that the server sets, and a group with none shows its source and no
// made-up title. The picker lists the state's `inputs` and nothing else:
// which inputs there are is the server's to say. Choosing one asks, with a
// `chorus-command` event, for a `take` that names the group and the input;
// the mark moves when the state that resulted comes back, and a refusal is
// shown by the card this sits in.
//
// The app controls rooms, groups, inputs and sound, not content (K64): there
// is no transport, queue or browsing here.
//
// The artwork is an <img> on this server's own artwork route, never on the
// record's address (somebody else's, which the page's Content-Security-Policy
// would not load). The route answers with no image for many reasons (the
// origin is gone, it sent something that is no image, the server is busy), so
// an image that fails to load gives way to the placeholder a record without
// artwork has. A new cover is a new address (api.js: artworkUrl) and a new
// <img>, so one failure does not hide the next track's cover.

import { LitElement, css, html, nothing } from "lit";
import { keyed } from "lit/directives/keyed.js";

import { takeSourceCommand } from "./api.js";

const STATE_WORDS = { playing: "Playing", paused: "Paused", buffering: "Buffering" };

// A source as the catalog spells it, in words. An offered input is called by
// its label; a line-in that is not offered now by its own id.
export function sourceText(source, inputs = []) {
  if (!source) return "Unavailable";
  const offered = inputs.find((input) => input.source === source);
  if (offered) return offered.label;
  if (source === "stream") return "The server's stream";
  if (source === "none") return "Nothing";
  const [kind, ...rest] = source.split(":");
  const named = rest.join(":");
  if (kind === "line-in" && named) return `Input ${named}`;
  if (kind === "player" && named) return `Network player ${named}`;
  if (kind === "chime" && named) return `Chime ${named}`;
  if (kind === "soloist" && named) return "Spotify";
  return source;
}

export class ChorusPlaying extends LitElement {
  static properties = {
    // The group this is about: its id (a room alone is the group named for
    // it) and the name a label calls it by.
    target: { type: String },
    name: { type: String },
    // The group's source and now-playing record, as state.js reads them.
    source: { attribute: false },
    nowPlaying: { attribute: false },
    // The inputs the server offers, as state.js reads them: [{ id, source, label }].
    inputs: { attribute: false },
    // Whether the inputs can be chosen here. A card says no where naming its
    // group in a `take` would also move rooms.
    pick: { type: Boolean },
    // The artwork address that failed to load, or null.
    _failed: { state: true },
  };

  static styles = css`
    :host {
      display: block;
    }
    .now {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    img,
    .placeholder {
      flex: none;
      width: var(--artwork-size);
      height: var(--artwork-size);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--artwork-radius);
      background: var(--bg);
      object-fit: cover;
    }
    .placeholder {
      display: flex;
      align-items: center;
      justify-content: center;
      box-sizing: border-box;
      color: var(--muted);
      font-size: var(--heading-size);
    }
    .words {
      flex: 1;
      min-width: var(--shrink-min);
    }
    p,
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
      color: var(--muted);
      font-size: var(--meta-size);
    }
    p[data-title] {
      color: var(--fg);
      font-size: var(--body-size);
    }
    ul {
      display: flex;
      flex-wrap: wrap;
      gap: var(--surface-gap);
    }
    .row {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
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
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
  `;

  constructor() {
    super();
    this.target = "";
    this.name = "";
    this.source = null;
    this.nowPlaying = null;
    this.inputs = [];
    this.pick = false;
    this._failed = null;
  }

  _onArtworkError(event) {
    this._failed = event.target.getAttribute("src");
  }

  // Ask for the input. The one it plays already is not asked for again.
  _onInput(input) {
    if (input.source === this.source) return;
    this.dispatchEvent(
      new CustomEvent("chorus-command", {
        detail: { subject: this.target, body: takeSourceCommand(this.target, input.source) },
        bubbles: true,
        composed: true,
      }),
    );
  }

  _artwork(record) {
    const placeholder = html`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;
    if (!record.artwork || record.artwork === this._failed) return placeholder;
    return keyed(
      record.artwork,
      html`<img
        data-artwork="image"
        src=${record.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`,
    );
  }

  render() {
    const record = this.nowPlaying;
    const inputs = this.inputs ?? [];
    return html`
      ${record
        ? html`<div class="now" data-now-playing=${record.state ?? "unknown"}>
            ${this._artwork(record)}
            <div class="words">
              <p data-title>${record.title ?? "Unknown title"}</p>
              ${record.artist ? html`<p data-artist>${record.artist}</p>` : nothing}
              ${record.album ? html`<p data-album>${record.album}</p>` : nothing}
              <p data-state>${STATE_WORDS[record.state] ?? "Unavailable"}</p>
            </div>
          </div>`
        : nothing}
      <p class="row" data-source=${this.source ?? ""}>Source: ${sourceText(this.source, inputs)}</p>
      ${this.pick && inputs.length > 0
        ? html`<ul aria-label="Inputs for ${this.name}">
            ${inputs.map(
              (input) =>
                html`<li data-input=${input.id}>
                  <button
                    type="button"
                    aria-label="Play ${input.label} in ${this.name}"
                    aria-pressed=${input.source === this.source ? "true" : "false"}
                    @click=${() => this._onInput(input)}
                  >
                    ${input.label}
                  </button>
                </li>`,
            )}
          </ul>`
        : nothing}
    `;
  }
}

customElements.define("chorus-playing", ChorusPlaying);
