// The house's autoplay rules: for each input, where it plays when its signal
// arrives (a room or a saved group) and whether the rule is in force
// (docs/control-plane.md: the `autoplay` command and the state's `autoplay`).
//
// It is one of the app's further screens and registers itself with the
// navigation (routes.js) at `#/autoplay`; the home links to it.
//
// It lists every input the state names: the ones offered now (`inputs`), in
// the server's order, and then any that has a rule and is not offered now (a
// speaker that is switched off keeps its rule). Each has a switch and a
// target picker, and both show the server's rule and nothing else.
//
// The catalog's command creates or replaces an input's whole rule, so each
// change sends one `autoplay` for that input: the field the control changes,
// and the rest of the rule as the server last said it. An input with no rule
// has no target, so its switch waits for one: choosing a target makes the
// rule, switched off, and the switch then turns it on. No control changes
// what another shows.
//
// A rule's TV options (`stop_on_standby`, `low_latency`) are not shown on
// this screen. A rule that has one switched off keeps it through a change
// made here (api.js, `autoplayCommand`).
//
// A room's theater screen (theater.js) shows the same element for the room's
// TV inputs alone, with `tv` set: each input then has the two options as
// switches of their own, and, given the room as `home`, an input with no rule
// can be switched on at once, playing in that room. Every change is still one
// `autoplay` for the input, carrying its whole rule.

import { LitElement, css, html, nothing } from "lit";

import { autoplayCommand } from "./api.js";
import { registerScreen } from "./routes.js";
import { autoplayOf } from "./state.js";

/** The subject an input's autoplay commands are sent under: its refusals are shown on its row. */
export const autoplaySubject = (input) => `autoplay:${input}`;

/**
 * The rows of the screen: one for each input the state names.
 *
 * @param {{ state: object | null, inputs: { id: string, label: string }[] }} view the store's view
 * @returns {{ input: string, label: string, offered: boolean, rule: object | null }[]}
 */
export function autoplayRows(view) {
  const rules = autoplayOf(view.state);
  const ruleOf = (input) => rules.find((rule) => rule.input === input) ?? null;
  const offered = (view.inputs ?? []).map((input) => ({
    input: input.id,
    label: input.label,
    offered: true,
    rule: ruleOf(input.id),
  }));
  const listed = new Set(offered.map((row) => row.input));
  const labels = new Map(
    (Array.isArray(view.state?.input_labels) ? view.state.input_labels : [])
      .filter((label) => label && typeof label.input === "string" && typeof label.name === "string" && label.name)
      .map((label) => [label.input, label.name]),
  );
  return [
    ...offered,
    ...rules
      .filter((rule) => !listed.has(rule.input))
      .map((rule) => ({ input: rule.input, label: labels.get(rule.input) ?? rule.input, offered: false, rule })),
  ];
}

// What a TV input is, for a person, by the kind the state says (`input_kinds`).
const KIND_NAMES = { optical: "Optical", hdmi_arc: "HDMI ARC" };

// A rule's two TV options, in the catalog's order, by the name state.js reads
// each under. Both are on unless the rule says otherwise.
const TV_OPTIONS = [
  { field: "stopOnStandby", name: "Stop on standby", says: "The TV going to standby stops it at once, with no hold" },
  { field: "lowLatency", name: "Low latency", says: "Played in low-latency mode when it plays in one wired room; off keeps it on the ordinary path" },
]; // prettier-ignore

export class ChorusAutoplay extends LitElement {
  static properties = {
    // The rows (autoplayRows), or null before a state has been read.
    rows: { attribute: false },
    // Where an input can play: the rooms and the saved groups, [{ id, name }] each.
    rooms: { attribute: false },
    groups: { attribute: false },
    // The server's words for the last refused command, by subject.
    refusals: { attribute: false },
    // The TV form (a room's theater screen): the inputs are a room's TV
    // inputs, and each shows its rule's two TV options. Reflected, for the
    // styles: the element is then a part of that screen, not a panel.
    tv: { type: Boolean, reflect: true },
    // The room an input with no rule plays in when it is switched on, { id,
    // name }, or null: then a rule starts with a target chosen from the list.
    home: { attribute: false },
  };

  static styles = css`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    :host([tv]) {
      padding: var(--reset-margin);
      border: none;
      background: none;
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
    button,
    select {
      box-sizing: border-box;
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    select {
      flex: 1 1 var(--control-basis);
      min-width: var(--shrink-min);
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    select:focus-visible,
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
    this.rows = null;
    this.rooms = [];
    this.groups = [];
    this.refusals = {};
    this.tv = false;
    this.home = null;
  }

  _row(input) {
    return (this.rows ?? []).find((row) => row.input === input) ?? null;
  }

  // One `autoplay` for the input: its rule as the server holds it, with the
  // one change.
  _ask(row, target, enabled, options = row.rule ?? {}) {
    this.dispatchEvent(
      new CustomEvent("chorus-command", {
        detail: {
          subject: autoplaySubject(row.input),
          body: autoplayCommand(row.input, target, enabled, options),
        },
        bubbles: true,
        composed: true,
      }),
    );
  }

  _onSwitch(event) {
    const row = this._row(event.currentTarget.dataset.input);
    if (!row) return;
    if (row.rule) this._ask(row, row.rule.target, !row.rule.enabled);
    // No rule yet: switching it on makes one that plays in the home room.
    else if (this.home) this._ask(row, this.home.id, true);
  }

  // One of a rule's two TV options: the opposite of what the server holds
  // (each is on unless the rule says otherwise), with the rest of the rule as
  // it is. On an input with no rule it makes one for the home room, switched
  // off, that carries the option.
  _onOption(event) {
    const row = this._row(event.currentTarget.dataset.input);
    const field = event.currentTarget.dataset.option;
    const target = row?.rule?.target ?? this.home?.id;
    if (!row || !target) return;
    const held = { stopOnStandby: row.rule?.stopOnStandby ?? true, lowLatency: row.rule?.lowLatency ?? true };
    this._ask(row, target, row.rule?.enabled ?? false, { ...held, [field]: !held[field] });
  }

  // Ask for the target, and put the list back on what the server holds: it
  // follows when the state that resulted comes back, and stays if the server
  // refuses.
  _onTarget(event) {
    const row = this._row(event.target.dataset.input);
    const value = event.target.value;
    const held = row?.rule?.target ?? "";
    event.target.value = held;
    if (!row || !value || value === held) return;
    this._ask(row, value, row.rule?.enabled ?? false);
  }

  // The list takes the server's target whenever the screen is painted: an
  // option's `selected` is only where a list starts.
  updated() {
    for (const list of this.renderRoot.querySelectorAll("select[data-input]")) {
      const held = this._row(list.dataset.input)?.rule?.target ?? "";
      if (list.value !== held) list.value = held;
    }
  }

  _targets(rule) {
    const rooms = this.rooms ?? [];
    const groups = this.groups ?? [];
    const known = !rule || [...rooms, ...groups].some((place) => place.id === rule.target);
    const option = (place) =>
      html`<option value=${place.id} ?selected=${rule?.target === place.id}>${place.name}</option>`;
    return html`
      ${rule ? nothing : html`<option value="" selected>Nowhere yet</option>`}
      ${known ? nothing : html`<option value=${rule.target} selected>${rule.target} (not on this server now)</option>`}
      ${rooms.length === 0 ? nothing : html`<optgroup label="Rooms">${rooms.map(option)}</optgroup>`}
      ${groups.length === 0 ? nothing : html`<optgroup label="Saved groups">${groups.map(option)}</optgroup>`}
    `;
  }

  _input(row) {
    const { input, label, offered, rule } = row;
    const refusal = this.refusals?.[autoplaySubject(input)] ?? "";
    const startsHere = !rule && Boolean(this.home);
    const says = rule
      ? rule.enabled ? "On" : "Off"
      : startsHere ? `Off: switching it on plays it in ${this.home.name}` : "Choose where it plays, then switch it on"; // prettier-ignore
    return html`
      <li data-input=${input}>
        <h3>${label}</h3>
        ${label === input ? nothing : html`<p data-id>${input}</p>`}
        ${row.kind ? html`<p data-kind>${KIND_NAMES[row.kind] ?? row.kind}</p>` : nothing}
        ${offered ? nothing : html`<p data-absent>Not offered now: its speaker is not connected.</p>`}
        <div class="row">
          <button
            type="button"
            data-input=${input}
            aria-label="Autoplay for ${label}"
            aria-pressed=${rule?.enabled ? "true" : "false"}
            ?disabled=${!rule && !startsHere}
            @click=${this._onSwitch}
          >
            Autoplay
          </button>
          <span data-value="enabled">${says}</span>
        </div>
        <div class="row">
          <label for="target-${input}">Plays in</label>
          <select id="target-${input}" data-input=${input} aria-label="Autoplay target for ${label}" @change=${this._onTarget}>
            ${this._targets(rule)}
          </select>
        </div>
        ${this.tv ? TV_OPTIONS.map((option) => this._option(row, option, !rule && !startsHere)) : nothing}
        <p role="alert">${refusal ? `Refused: ${refusal}` : nothing}</p>
      </li>
    `;
  }

  _option({ input, label, rule }, { field, name, says }, disabled) {
    const on = rule?.[field] ?? true;
    return html`
      <div class="row">
        <button
          type="button"
          data-input=${input}
          data-option=${field}
          aria-label="${name} for ${label}"
          aria-pressed=${on ? "true" : "false"}
          ?disabled=${disabled}
          @click=${this._onOption}
        >
          ${name}
        </button>
        <span data-value=${field}>${on ? "On" : "Off"}</span>
        <p>${says}</p>
      </div>
    `;
  }

  render() {
    if (this.rows === null) return html`<p role="status" data-missing>Reading this server's inputs.</p>`;
    const none = this.tv
      ? "This room has no TV input now."
      : "This server offers no input now, and has no autoplay rule.";
    return html`
      ${this.tv ? html`<h3>TV autoplay</h3>` : html`<h2>Autoplay</h2>`}
      <p>
        ${this.tv
          ? "A TV input with a rule that is on plays when the TV's signal arrives."
          : "An input with a rule that is on plays in its room or its group when its signal arrives."}
      </p>
      ${this.rows.length === 0
        ? html`<p role="status" data-none>${none}</p>`
        : html`<ul aria-label=${this.tv ? "TV inputs" : "Inputs"}>
            ${this.rows.map((row) => this._input(row))}
          </ul>`}
    `;
  }
}

customElements.define("chorus-autoplay", ChorusAutoplay);

/** The id this screen is registered under (routes.js): `addressOf(AUTOPLAY_SCREEN)` is its address. */
export const AUTOPLAY_SCREEN = "autoplay";

const places = (list) => list.map(({ id, name }) => ({ id, name }));

registerScreen({
  id: AUTOPLAY_SCREEN,
  path: "autoplay",
  title: () => "Autoplay",
  render: (_params, { view, refusals }) => html`
    <chorus-autoplay
      .rows=${view.state === null ? null : autoplayRows(view)}
      .rooms=${places(view.rooms)}
      .groups=${places((view.groups ?? []).filter((group) => group.kind === "saved"))}
      .refusals=${refusals}
    ></chorus-autoplay>
  `,
});
