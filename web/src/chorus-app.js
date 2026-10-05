// The app shell: the one element index.html holds. It paints the header and a
// labelled main region, and every screen a later change adds is rendered
// inside that region. It holds no server state yet; the shell's one property
// is the layout.

import { LitElement, css, html } from "lit";

import { MODES } from "./mode.js";

export class ChorusApp extends LitElement {
  static properties = {
    // "app" or "kiosk" (mode.js). Reflected, so the styles below and a test
    // can read it off the element.
    mode: { type: String, reflect: true },
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
  }

  willUpdate() {
    if (!MODES.includes(this.mode)) this.mode = "app";
  }

  render() {
    return html`
      <header>
        <h1>chorus</h1>
      </header>
      <main aria-label="Rooms">
        <slot></slot>
      </main>
    `;
  }
}

customElements.define("chorus-app", ChorusApp);
