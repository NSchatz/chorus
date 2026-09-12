// Which face a run was painted in, decided by measuring what the engine paints.
//
// The question S5 asks - is this run set in a fixed-advance face? - cannot be
// answered from a stylesheet. `font-family` is a LIST of names, most of them
// generic, and which entry the engine actually uses depends on what is
// installed; `ui-monospace` and `system-ui` both resolve to whatever this
// machine calls its own. So the answer here is a measurement: two runs of the
// same number of characters are laid out in the face the engine computed for a
// painted element, and their widths are compared.
//
//   a fixed-advance face paints "00000000" and "11111111" at one width, and
//   "iiiiiiii" and "MMMMMMMM" at one width;
//   a proportional face paints "iiiiiiii" far narrower than "MMMMMMMM".
//
// The digits are the pair the criterion names. They are not enough on their own
// and are never used on their own: most proportional UI faces have tabular
// figures, so equal digit widths say nothing about the face. The narrow-and-wide
// pair is what tells the two apart, and both are required.
//
// The face measured is the one the ENGINE COMPUTED for the element that is
// really on the page - the used value after the cascade, inheritance, the media
// queries that matched and the user agent sheet have all had their say - and the
// widths come out of the engine's own layout. Nothing here opens a stylesheet.

const DIGITS = ["00000000", "11111111"];
const NARROW_WIDE = ["iiiiiiii", "MMMMMMMM"];

/// The font properties that decide how wide a string is painted. Copied one by
/// one rather than through the `font` shorthand, which computes to the empty
/// string as soon as a value it cannot express is in play.
const FONT_PROPERTIES = [
  "fontFamily",
  "fontSize",
  "fontWeight",
  "fontStyle",
  "fontStretch",
  "fontVariantNumeric",
  "fontFeatureSettings",
  "fontKerning",
  "letterSpacing",
  "wordSpacing",
  "textTransform",
];

/// Every element the selector matches, with the widths the engine paints each
/// probe string at in that element's own face.
async function faces(page, selector) {
  return page.evaluate(
    ({ selector, digits, narrowWide, properties }) => {
      const describe = (el) => {
        const bits = [el.tagName.toLowerCase()];
        for (const attribute of el.getAttributeNames()) {
          if (attribute.startsWith("data-")) {
            bits.push(`[${attribute}]`);
          }
        }
        if (el.className && typeof el.className === "string") {
          for (const name of el.className.split(/\s+/).filter(Boolean)) {
            bits.push(`.${name}`);
          }
        }
        return bits.join("");
      };
      const out = [];
      for (const el of document.querySelectorAll(selector)) {
        const style = window.getComputedStyle(el);
        const box = el.getBoundingClientRect();
        if (style.display === "none" || style.visibility === "hidden") {
          continue;
        }
        if (box.width < 1 || box.height < 1) {
          continue;
        }
        const probe = document.createElement("span");
        for (const property of properties) {
          probe.style[property] = style[property];
        }
        probe.style.position = "absolute";
        probe.style.left = "-99999px";
        probe.style.top = "0";
        probe.style.display = "inline-block";
        probe.style.whiteSpace = "pre";
        probe.style.padding = "0";
        probe.style.border = "0";
        probe.style.margin = "0";
        document.body.appendChild(probe);
        const widthOf = (text) => {
          probe.textContent = text;
          return probe.getBoundingClientRect().width;
        };
        const row = {
          what: describe(el),
          text: (el.innerText || el.value || "").trim().slice(0, 40),
          family: style.fontFamily,
          size: style.fontSize,
          digits: digits.map(widthOf),
          narrowWide: narrowWide.map(widthOf),
        };
        probe.remove();
        out.push(row);
      }
      return out;
    },
    { selector, digits: DIGITS, narrowWide: NARROW_WIDE, properties: FONT_PROPERTIES }
  );
}

/// A face is fixed-advance when every character takes the same width: the two
/// digit strings measure the same, and so do the narrow and the wide one.
function isFixedAdvance(row) {
  return (
    Math.abs(row.digits[0] - row.digits[1]) < 0.5 &&
    Math.abs(row.narrowWide[0] - row.narrowWide[1]) < 0.5 &&
    row.narrowWide[0] > 0
  );
}

/// Rows the criterion wants fixed-advance and the engine painted otherwise.
function notFixedAdvance(rows) {
  return rows.filter((row) => !isFixedAdvance(row)).map(summarise);
}

/// Rows the criterion wants in the UI face and the engine painted fixed-advance.
///
/// The test is the same measurement read the other way: a run whose narrow and
/// wide probes come out the same width is being set in a face where every
/// character takes the same room, which is the face this page keeps for figures.
function notProportional(rows) {
  return rows.filter((row) => isFixedAdvance(row)).map(summarise);
}

function summarise(row) {
  return {
    what: row.what,
    text: row.text,
    family: row.family,
    digits: row.digits.map((w) => Number(w.toFixed(2))),
    narrowWide: row.narrowWide.map((w) => Number(w.toFixed(2))),
  };
}

module.exports = { DIGITS, NARROW_WIDE, faces, isFixedAdvance, notFixedAdvance, notProportional, summarise };
