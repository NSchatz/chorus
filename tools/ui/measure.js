// Measuring what a page ACTUALLY PAINTED.
//
// The one rule this file exists to enforce: every number here comes from
// getBoundingClientRect() on a live element in a live engine, and none of it
// comes from reading a stylesheet. A declared `min-height` that never wins the
// cascade, a rule under a media query that did not match, a control inside a
// `display: none` ancestor and a value in a stylesheet nobody linked all
// measure differently from what they declare, and only an engine can tell them
// apart.
//
// It is shared by the real check and by the mutation demonstration beside it,
// so the thing shown going red is the same code that grades the page.

/// What counts as an interactive control: anything a person can operate.
const INTERACTIVE =
  'button, select, input, textarea, a[href], [role="button"], [role="switch"], [tabindex]:not([tabindex="-1"])';

/// Every interactive control on the page, with the box it was painted at.
async function measureControls(page) {
  return page.$$eval(INTERACTIVE, (nodes) =>
    nodes.map((node) => {
      const box = node.getBoundingClientRect();
      const style = window.getComputedStyle(node);
      return {
        // Enough to name the offender in a failure without being a selector
        // this check depends on.
        tag: node.tagName.toLowerCase(),
        type: node.getAttribute("type") || "",
        label: (node.getAttribute("aria-label") || node.textContent || "")
          .trim()
          .slice(0, 40),
        width: box.width,
        height: box.height,
        // Reported so that a control which measured zero because it was not
        // displayed is distinguishable from one that was painted too small.
        display: style.display,
        visibility: style.visibility,
      };
    })
  );
}

/// Every control painted smaller than `minimum` in either direction.
function tooSmall(controls, minimum) {
  return controls.filter((c) => c.width < minimum || c.height < minimum);
}

module.exports = { INTERACTIVE, measureControls, tooSmall };
