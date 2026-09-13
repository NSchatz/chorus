// The recorded ratio, against the measured one.
//
// `crates/server/src/ui/tokens.css` writes a measured contrast ratio beside each
// token pair that has to clear a floor, in both themes, because a change that
// drops below a floor is then visible in the diff. The objection to doing that -
// the one the old stylesheet header made, and the one
// docs/decisions/0021-a-measured-ratio-is-recorded-beside-the-value.md answers -
// is that a number a person typed is a number nobody re-derives. This file is
// the answer: on every run, every recorded number is put back beside the pixels.
//
// THE MAPPING, which is the part worth being explicit about. The contrast
// measurement knows nothing of tokens: it reports a ratio between two COLOURS,
// one from the engine's computed style and one from the framebuffer. So the
// mapping from a measured row to the token pair that painted it is by colour
// value, against the table THE ENGINE resolved the tokens to in the theme that
// is in force - `getComputedStyle(document.documentElement)`, after the media
// query matched, not the file's own text. A row whose colour is not a value any
// role resolves to maps to no token pair and is reported by name; that is how a
// reconciliation that quietly covered half of what was measured is caught.
//
// Three refusals, and the third is the one AC-8 turns on:
//
//   a measured row that maps to no token pair;
//   a mapped pair with no ratio recorded for that theme;
//   a recorded ratio more than 0.05 from the measured one.
//
// And one more that does not need the page to have painted the pair at all:
// every recorded ratio is also checked against the contrast between the two
// values the engine resolved those tokens to, so an annotation for a pair this
// run did not happen to paint cannot go stale either.

const pixels = require("./pixels");

const TOLERANCE = 0.05;

/// The value the ENGINE resolved each token to, in the theme now in force.
async function resolvedTokens(page, names) {
  const raw = await page.evaluate((wanted) => {
    const style = window.getComputedStyle(document.documentElement);
    const out = {};
    for (const name of wanted) {
      out[name] = style.getPropertyValue(name).trim();
    }
    return out;
  }, names);
  const table = new Map();
  for (const [name, value] of Object.entries(raw)) {
    const colour = normalise(value);
    if (colour) {
      table.set(name, colour);
    }
  }
  return table;
}

/// #abc, #AABBCC and rgb(1, 2, 3) all say one colour; this is how they are all
/// said the same way, so two of them can be compared.
function normalise(value) {
  const text = String(value || "").trim().toLowerCase();
  let match = /^#([0-9a-f]{3})$/.exec(text);
  if (match) {
    return `#${match[1].split("").map((c) => c + c).join("")}`;
  }
  match = /^#([0-9a-f]{6})$/.exec(text);
  if (match) {
    return `#${match[1]}`;
  }
  match = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/.exec(text);
  if (match) {
    return pixels.hex([Number(match[1]), Number(match[2]), Number(match[3])]);
  }
  return null;
}

function key(a, b) {
  return [a, b].sort().join(" with ");
}

/// Which roles resolve to this colour in this theme. More than one is ordinary:
/// --accent, --link and --selected are the same hue on purpose.
function namesFor(table, colour) {
  const names = [];
  for (const [name, value] of table) {
    if (value === colour) {
      names.push(name);
    }
  }
  return names.sort();
}

/// Every row the contrast measurement produced, as a pair of colours and the
/// ratio between them. One shape, so nothing downstream has to know whether a
/// row came from a text run, a control boundary or a slider.
function rowsFrom(textRows, markRows) {
  const rows = [];
  for (const run of textRows) {
    rows.push({
      what: `${run.what} "${run.text}"`,
      kind: "text",
      colours: [normalise(run.ink), normalise(run.surface)],
      measured: run.ratio,
      floor: run.floor,
    });
  }
  for (const mark of markRows) {
    if (mark.kind === "slider") {
      rows.push({
        what: `the track of ${mark.label}`,
        kind: "slider track",
        colours: [normalise(mark.track), normalise(mark.surround)],
        measured: mark.trackRatio,
        floor: 3,
      });
      rows.push({
        what: `the thumb of ${mark.label}`,
        kind: "slider thumb",
        colours: [normalise(mark.thumb), normalise(mark.track)],
        measured: mark.thumbRatio,
        floor: 3,
      });
      continue;
    }
    rows.push({
      what: `the boundary of ${mark.label}`,
      kind: mark.kind,
      colours: [normalise(mark.edge), normalise(mark.surround)],
      measured: mark.ratio,
      floor: 3,
    });
  }
  return rows;
}

/// Put the recorded ratios beside the measured ones.
///
/// `ledger` is what the token file records for this theme, `resolved` is what
/// the engine resolved each role to, and `rows` is what the contrast measurement
/// produced. Everything it returns is a problem; an empty list is the pass.
function reconcile({ ledger, resolved, rows, theme }) {
  const problems = [];
  const recorded = new Map();

  for (const entry of ledger) {
    const ink = resolved.get(entry.ink);
    const on = resolved.get(entry.on);
    if (!ink || !on) {
      problems.push({
        why: "recorded-token-unresolved",
        message: `the record says "${entry.ink} on ${entry.on}" in the ${theme} theme and the engine resolves ${!ink ? entry.ink : entry.on} to nothing`,
      });
      continue;
    }
    const between = pixels.contrast(rgb(ink), rgb(on));
    if (Math.abs(between - entry.ratio) > TOLERANCE) {
      problems.push({
        why: "recorded-ratio-is-not-the-ratio-of-those-values",
        message: `${entry.ink} on ${entry.on} in the ${theme} theme is recorded as ${entry.ratio.toFixed(2)}, and the engine resolves those two tokens to ${ink} and ${on}, which are ${between.toFixed(2)} apart`,
      });
    }
    recorded.set(key(ink, on), { entry, ratio: between });
  }

  let mapped = 0;
  const used = new Set();
  for (const row of rows) {
    const [a, b] = row.colours;
    if (!a || !b) {
      problems.push({
        why: "unmapped-row",
        message: `${row.what} was measured at ${Number(row.measured || 0).toFixed(2)} in the ${theme} theme and one of the two colours it was measured between was not reported at all`,
      });
      continue;
    }
    const namesA = namesFor(resolved, a);
    const namesB = namesFor(resolved, b);
    if (namesA.length === 0 || namesB.length === 0) {
      problems.push({
        why: "unmapped-row",
        message: `${row.what} is painted ${a} on ${b} in the ${theme} theme and ${namesA.length === 0 ? a : b} is not a value any role resolves to, so this row maps to no token pair`,
      });
      continue;
    }
    const found = recorded.get(key(a, b));
    if (!found) {
      problems.push({
        why: "no-recorded-ratio",
        message: `${row.what} maps to ${namesA.join("/")} on ${namesB.join("/")} in the ${theme} theme, and no ratio is recorded for that pair`,
      });
      continue;
    }
    mapped += 1;
    used.add(`${found.entry.ink} on ${found.entry.on}`);
    if (Math.abs(found.entry.ratio - row.measured) > TOLERANCE) {
      problems.push({
        why: "stale-annotation",
        message: `${found.entry.ink} on ${found.entry.on} in the ${theme} theme is recorded as ${found.entry.ratio.toFixed(2)} and the engine measured ${row.measured.toFixed(2)} from the painted pixels of ${row.what}`,
      });
    }
    if (row.floor && row.measured + 0.005 < row.floor) {
      problems.push({
        why: "below-the-floor",
        message: `${row.what} measured ${row.measured.toFixed(2)} in the ${theme} theme against a floor of ${row.floor}`,
      });
    }
  }

  return { problems, mapped, used: [...used].sort(), rows: rows.length, recorded: recorded.size };
}

function rgb(hex) {
  return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
}

module.exports = { TOLERANCE, resolvedTokens, normalise, namesFor, rowsFrom, reconcile };
