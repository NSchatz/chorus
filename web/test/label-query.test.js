// The label query itself, on markup with each way of labelling and a shadow root.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import { getByLabel, labelOf, queryAllByLabel } from "./label-query.js";

afterEach(() => {
  document.body.replaceChildren();
});

function mount(markup) {
  const host = document.createElement("div");
  host.innerHTML = markup;
  document.body.append(host);
  return host;
}

test("each way of labelling is read", () => {
  const host = mount(`
    <button aria-label="Mute"></button>
    <label for="vol">Volume</label><input id="vol" type="range">
    <label>Night <span>mode</span> <input type="checkbox"></label>
    <h2 id="t">Den</h2><section aria-labelledby="t"></section>
    <input id="bare">
  `);
  assert.equal(getByLabel(host, "Mute").localName, "button");
  assert.equal(getByLabel(host, "Volume").id, "vol");
  assert.equal(getByLabel(host, "Night mode").type, "checkbox");
  assert.equal(getByLabel(host, "Den").localName, "section");
  assert.equal(labelOf(host.querySelector("#bare")), "");
});

test("a control inside a shadow root is found from the document", () => {
  const host = mount("<div id='outer'></div>");
  const shadow = host.querySelector("#outer").attachShadow({ mode: "open" });
  shadow.innerHTML = "<label for='in'>Input</label><select id='in'></select>";
  assert.equal(getByLabel(document, "Input").localName, "select");
  assert.equal(document.querySelector("select"), null);
});

test("none and more than one are both errors, and queryAllByLabel returns them all", () => {
  const host = mount("<button aria-label='Leave'></button><button aria-label='Leave'></button>");
  assert.equal(queryAllByLabel(host, "Leave").length, 2);
  assert.throws(() => getByLabel(host, "Leave"), /2 elements are labelled "Leave"/);
  assert.throws(() => getByLabel(host, "Join"), /0 elements are labelled "Join"/);
});
