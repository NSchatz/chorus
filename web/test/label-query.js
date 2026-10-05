// A label query for tests: find an element by its accessible label, looking
// through shadow roots. Lit elements render into shadow roots, where
// `document.querySelector` and a testing library's `getByLabelText` do not
// look; and a test that finds a control by its label also proves the control
// has one.
//
// An element's label, in the order checked: `aria-labelledby` (the text of the
// elements it names, in the same root), `aria-label`, a `<label for>` in the
// same root, a `<label>` that wraps it (a control only: the label's other
// content is its text, not something it labels). This is the part of the
// accessible name computation the app's markup uses, not the whole of it.

// The elements a wrapping <label> labels (HTML's labelable elements).
const LABELABLE = new Set(["button", "input", "meter", "output", "progress", "select", "textarea"]);

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();

/** The element's label, or "" when it has none. */
export function labelOf(element) {
  const root = element.getRootNode();
  const ids = element.getAttribute("aria-labelledby");
  if (ids) {
    return ids
      .split(/\s+/)
      .map((id) => root.getElementById?.(id))
      .filter(Boolean)
      .map(text)
      .join(" ");
  }
  const aria = element.getAttribute("aria-label");
  if (aria) return aria.trim();
  if (element.id) {
    for (const label of root.querySelectorAll("label")) {
      if (label.getAttribute("for") === element.id) return text(label);
    }
  }
  const wrapping = LABELABLE.has(element.localName) ? element.closest("label") : null;
  return wrapping ? text(wrapping) : "";
}

// Every element under `root`, in document order, entering each shadow root
// before the element's light children.
function* elementsIn(root) {
  for (const element of root.children) {
    yield element;
    if (element.shadowRoot) yield* elementsIn(element.shadowRoot);
    yield* elementsIn(element);
  }
}

/** Every element under `root` (a document, an element or a shadow root) with exactly this label. */
export function queryAllByLabel(root, label) {
  const start = root.shadowRoot ?? root;
  return [...elementsIn(start)].filter(
    (element) => element.localName !== "label" && labelOf(element) === label,
  );
}

/** The one element with this label; throws when there is none or more than one. */
export function getByLabel(root, label) {
  const found = queryAllByLabel(root, label);
  if (found.length !== 1) {
    throw new Error(`${found.length} elements are labelled "${label}", expected exactly one`);
  }
  return found[0];
}
