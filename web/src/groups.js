// The groups region: every saved group, active or not (K59), then every live
// group, one card each (group-card.js), in the order state.js gives them.
//
// Each card is a drop target of the drag gesture (drag.js): a room dropped on
// it joins that group. While a room that is in a group is being dragged, one
// more target is offered: dropping the room there takes it out to play alone.

import { LitElement, css, html, nothing } from "lit";
import { repeat } from "lit/directives/repeat.js";

import "./group-card.js";

export class ChorusGroups extends LitElement {
  static properties = {
    // The groups as state.js reads them, or null before the first state.
    groups: { attribute: false },
    // Refusal words by group id.
    refusals: { attribute: false },
    // The room being dragged, { id, name, grouped }, or null.
    moving: { attribute: false },
    // The destination under the pointer during a drag (grouping.js), or null.
    over: { attribute: false },
  };

  static styles = css`
    :host {
      display: block;
    }
    ul {
      display: flex;
      flex-direction: column;
      gap: var(--surface-gap);
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    [data-drop="alone"] {
      display: flex;
      align-items: center;
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) dashed var(--border);
      border-radius: var(--surface-radius);
    }
    [hidden] {
      display: none;
    }
    [data-over] {
      outline: var(--focus-ring-width) solid var(--accent);
      outline-offset: var(--focus-ring-offset);
    }
  `;

  constructor() {
    super();
    this.groups = null;
    this.refusals = {};
    this.moving = null;
    this.over = null;
  }

  render() {
    const groups = this.groups ?? [];
    const over = this.over;
    return html`
      ${this.groups !== null && groups.length === 0
        ? html`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`
        : nothing}
      <ul>
        ${repeat(
          groups,
          (group) => group.id,
          (group) =>
            html`<li
              data-group=${group.id}
              data-drop="group"
              data-drop-id=${group.id}
              ?data-over=${over?.kind === "group" && over.id === group.id}
            >
              <chorus-group-card .group=${group} .refusal=${this.refusals[group.id] ?? ""}></chorus-group-card>
            </li>`,
        )}
      </ul>
      <p data-drop="alone" ?hidden=${!this.moving?.grouped} ?data-over=${over?.kind === "alone"}>
        ${this.moving ? `Drop here to play ${this.moving.name} alone.` : nothing}
      </p>
    `;
  }
}

customElements.define("chorus-groups", ChorusGroups);
