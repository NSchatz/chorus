// The accessibility tree, read out of the engine that computed it.
//
// Not the markup. An accessible name is the product of a whole algorithm -
// aria-label, aria-labelledby, a wrapping label, the content, the title, in that
// order, with hidden subtrees removed - and only the engine has run it. So this
// asks Chromium for the tree it computed, through the Chrome DevTools Protocol,
// and grades that. A page that has all the right attributes and still computes
// an empty name is caught here and could not be caught by reading the HTML.
//
// The same tree carries the pressed state of a toggle, which is what a control
// exposing "muted" as a STATE rather than only as a word looks like from the
// outside.

/// Roles a person can operate. A node with one of these and no name is a control
/// that announces nothing.
const OPERABLE = new Set([
  "button",
  "checkbox",
  "combobox",
  "link",
  "listbox",
  "menuitem",
  "radio",
  "slider",
  "spinbutton",
  "switch",
  "textbox",
]);

/// Every node of the accessibility tree, flattened, with its role, its computed
/// name and its state properties.
async function tree(page) {
  const cdp = await page.context().newCDPSession(page);
  try {
    await cdp.send("Accessibility.enable");
    const { nodes } = await cdp.send("Accessibility.getFullAXTree");
    return nodes.map((node) => {
      const properties = {};
      for (const property of node.properties || []) {
        properties[property.name] =
          property.value && property.value.value !== undefined
            ? property.value.value
            : null;
      }
      return {
        role: node.role ? node.role.value : "",
        name: node.name ? String(node.name.value || "") : "",
        description: node.description ? String(node.description.value || "") : "",
        ignored: Boolean(node.ignored),
        properties,
      };
    });
  } finally {
    await cdp.detach();
  }
}

/// The operable nodes of the tree, which is the set every other check here is
/// about.
function controls(nodes) {
  return nodes.filter((node) => !node.ignored && OPERABLE.has(node.role));
}

/// Controls the engine computed no name for.
function unnamed(nodes) {
  return controls(nodes).filter((node) => node.name.trim().length === 0);
}

/// Controls whose name does not say which zone they belong to.
///
/// `zoneNames` is what the page RENDERED as each zone's name, read back from the
/// page, so this compares the tree against the surface and not against a
/// fixture's idea of either.
function withoutTheirZone(nodes, zoneNames) {
  return controls(nodes).filter((node) => {
    const name = node.name.toLowerCase();
    return !zoneNames.some((zone) => name.includes(zone.toLowerCase()));
  });
}

/// Every node that exposes a pressed state, and what it is.
function pressed(nodes) {
  return controls(nodes)
    .filter((node) => node.properties.pressed !== undefined)
    .map((node) => ({ name: node.name, pressed: node.properties.pressed }));
}

module.exports = { OPERABLE, tree, controls, unnamed, withoutTheirZone, pressed };
