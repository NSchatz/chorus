// The token file, read back.
//
// One parser, used by two checks that must not be allowed to disagree:
// tools/styling-check.sh reads the SOURCE with it, and tools/ui/ui.spec.js
// reads the recorded contrast ratios with it before comparing them to what a
// browser engine measured off the painted pixels. Two parsers of one format is
// two chances to read it differently.
//
// Nothing here decides anything about what was PAINTED. What it knows is what
// the file says: which names are declared, in which tier, for which theme, what
// each one resolves to, and what ratio is recorded beside it. Every claim about
// a rendered colour is settled in the engine, by tools/ui/contrast.js.
//
// THE FORMAT IS A CONTRACT, and a file that does not hold to it is a parse
// failure named by line rather than a silently smaller check:
//
//   /* tier: primitive */              raw values, no meaning attached
//   /* tier: semantic, theme: light */ a primitive assigned a role
//   /* tier: semantic, theme: dark */  the same roles, chosen again
//   /* tier: component */              a role bound to one component
//   --name: value;                     a declaration, inside the block above it
//   /* contrast: --a on --b = N.NN, floor F */
//                                      the measured ratio for that pair, in the
//                                      theme of the block it sits in

const pixels = require("./pixels");

const THEMES = ["light", "dark"];

/// The fifteen semantic role names every repository in the umbrella declares.
/// From `.sdd/conventions/styling.md` clause S2.
const ROLES = [
  "--bg",
  "--panel",
  "--line",
  "--fg",
  "--muted",
  "--accent",
  "--ok",
  "--warn",
  "--bad",
  "--border",
  "--mark",
  "--focus",
  "--disabled",
  "--selected",
  "--link",
];

/// The roles that paint a state rather than a surface or its ink. S10 lets each
/// of these carry its own hue; everything else is neutral or the one accent.
const STATE_ROLES = ["--ok", "--warn", "--bad", "--focus", "--disabled", "--selected"];

const TIER_LINE = /^\s*\/\*\s*tier:\s*(primitive|semantic|component)\s*(?:,\s*theme:\s*(light|dark)\s*)?\*\/\s*$/;
const DECLARATION_LINE = /^\s*(--[a-zA-Z0-9-]+)\s*:\s*([^;]+);\s*$/;
const CONTRAST_LINE =
  /^\s*\/\*\s*contrast:\s*(--[a-zA-Z0-9-]+)\s+on\s+(--[a-zA-Z0-9-]+)\s*=\s*(\d+(?:\.\d+)?)\s*,\s*floor\s*(\d+(?:\.\d+)?)\s*\*\/\s*$/;
const LOOKS_LIKE_CONTRAST = /^\s*\/\*\s*contrast:/;

/// Parse a token file. Returns what it found and every reason it could not be
/// read, so a caller reports all of them rather than the first.
function parseTokens(text, path) {
  const problems = [];
  const declarations = [];
  const contrast = [];
  const tiers = { primitive: [], component: [], semantic: { light: [], dark: [] } };
  let tier = null;
  let theme = null;
  let seenTier = { primitive: false, component: false, "semantic:light": false, "semantic:dark": false };
  let depth = 0;

  const lines = text.split("\n");
  lines.forEach((line, index) => {
    const at = `${path}:${index + 1}`;
    const marker = TIER_LINE.exec(line);
    if (marker) {
      tier = marker[1];
      theme = marker[2] || null;
      if (tier === "semantic" && !theme) {
        problems.push(`${at}: a semantic block must name its theme, as "tier: semantic, theme: light"`);
      }
      seenTier[tier === "semantic" ? `semantic:${theme}` : tier] = true;
      return;
    }
    depth += (line.match(/{/g) || []).length;
    depth -= (line.match(/}/g) || []).length;

    const declaration = DECLARATION_LINE.exec(line);
    if (declaration) {
      const [, name, value] = declaration;
      if (!tier) {
        problems.push(`${at}: ${name} is declared before any "tier:" marker, so nothing says which tier it is in`);
        return;
      }
      const already = declarations.find(
        (d) => d.name === name && d.tier === tier && d.theme === theme
      );
      if (already) {
        problems.push(`${at}: ${name} is declared twice in the same block (also at line ${already.line})`);
        return;
      }
      const entry = { name, value: value.trim(), tier, theme, line: index + 1 };
      declarations.push(entry);
      if (tier === "semantic") {
        tiers.semantic[theme].push(name);
      } else {
        tiers[tier].push(name);
      }
      return;
    }

    const ratio = CONTRAST_LINE.exec(line);
    if (ratio) {
      if (tier !== "semantic") {
        problems.push(
          `${at}: a contrast annotation belongs beside the token it annotates, inside a semantic block; this one is in the ${tier || "no"} tier`
        );
        return;
      }
      contrast.push({
        ink: ratio[1],
        on: ratio[2],
        ratio: Number(ratio[3]),
        floor: Number(ratio[4]),
        theme,
        line: index + 1,
      });
      return;
    }
    if (LOOKS_LIKE_CONTRAST.test(line)) {
      problems.push(
        `${at}: this reads as a contrast annotation and does not parse as one; the form is "/* contrast: --a on --b = N.NN, floor F */"`
      );
    }
  });

  for (const [name, seen] of Object.entries(seenTier)) {
    if (!seen) {
      problems.push(`${path}: there is no "tier: ${name.replace(":", ", theme: ")}" block`);
    }
  }
  if (depth !== 0) {
    problems.push(`${path}: the braces do not balance (${depth} left open), so no block boundary can be trusted`);
  }

  return { path, text, problems, declarations, contrast, tiers, themes: THEMES };
}

/// What a theme sees: every primitive and component token, plus that theme's
/// semantics and not the other theme's. This is the table the engine's own
/// cascade produces, built from the file rather than from the engine, which is
/// why the two are compared rather than one being trusted.
function tableFor(parsed, theme) {
  const table = new Map();
  for (const declaration of parsed.declarations) {
    if (declaration.tier === "semantic" && declaration.theme !== theme) {
      continue;
    }
    table.set(declaration.name, declaration);
  }
  return table;
}

const ALIAS = /^var\(\s*(--[a-zA-Z0-9-]+)\s*\)$/;
const ANY_VAR = /var\(/;

/// Resolve a name to the literal it stands for in one theme, following aliases.
///
/// A value that MIXES var() with anything else - a calc(), a color-mix(), a
/// relative-colour form - is refused rather than resolved: the point of the
/// token file is that a theme's value is a value somebody chose, and a value
/// computed from another one is not that.
function resolve(table, name, seen = []) {
  if (seen.includes(name)) {
    return {
      kind: "cycle",
      error: `${name} resolves through a cycle: ${[...seen, name].join(" -> ")}`,
    };
  }
  const declaration = table.get(name);
  if (!declaration) {
    return { kind: "undeclared", error: `${name} is not declared in this theme` };
  }
  const alias = ALIAS.exec(declaration.value);
  if (alias) {
    const next = resolve(table, alias[1], [...seen, name]);
    return next.error ? next : { ...next, chain: [name, ...(next.chain || [])] };
  }
  if (ANY_VAR.test(declaration.value)) {
    // Declared, and not a value anybody chose. Which rule that breaks is
    // `hand-authored-themes`' business and not `tokens-resolve`': the token is
    // there, and reporting it under both would leave a demonstration unable to
    // break exactly one thing.
    return {
      kind: "derived",
      error: `${name} is computed from other tokens rather than chosen: ${declaration.value}`,
      declaration,
    };
  }
  return { value: declaration.value, declaration, chain: [name] };
}

// --- colours -----------------------------------------------------------------

const HEX = /^#([0-9a-fA-F]{6})$/;

function isHex(value) {
  return HEX.test(String(value || "").trim());
}

function toRgb(value) {
  const match = HEX.exec(String(value).trim());
  if (!match) {
    return null;
  }
  const n = parseInt(match[1], 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

function ratio(a, b) {
  return pixels.contrast(toRgb(a), toRgb(b));
}

/// How far a colour is from grey, 0 to 1. Used instead of HSL saturation
/// because saturation explodes at the light and dark ends: #eef1f4 is a grey
/// anybody would call grey and its saturation is 0.21.
function chroma(value) {
  const rgb = toRgb(value);
  if (!rgb) {
    return null;
  }
  return (Math.max(...rgb) - Math.min(...rgb)) / 255;
}

/// The hue angle in degrees, or null for a colour with no hue to speak of.
function hue(value) {
  const rgb = toRgb(value);
  if (!rgb) {
    return null;
  }
  const [r, g, b] = rgb.map((c) => c / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const span = max - min;
  if (span === 0) {
    return null;
  }
  let angle;
  if (max === r) {
    angle = ((g - b) / span) % 6;
  } else if (max === g) {
    angle = (b - r) / span + 2;
  } else {
    angle = (r - g) / span + 4;
  }
  angle *= 60;
  return angle < 0 ? angle + 360 : angle;
}

// --- reading an ordinary stylesheet ------------------------------------------

/// Every declaration in a stylesheet, with the selector it sits under and the
/// line it is on.
///
/// A tiny state machine rather than a regular expression, because a selector
/// like `input[type="range"]::-webkit-slider-thumb` has colons in it and a
/// regular expression that looks for `word: value` finds one there.
function declarationsOf(css, path) {
  const out = [];
  const stripped = blankComments(css);
  let buffer = "";
  let line = 1;
  const stack = [];
  const flush = () => {
    const text = buffer.trim();
    buffer = "";
    if (!text) {
      return;
    }
    const colon = text.indexOf(":");
    if (colon < 0) {
      return;
    }
    out.push({
      property: text.slice(0, colon).trim(),
      value: text.slice(colon + 1).trim(),
      selector: stack[stack.length - 1] || "",
      line: line - (text.match(/\n/g) || []).length,
      path,
    });
  };
  for (const character of stripped) {
    if (character === "\n") {
      line += 1;
    }
    if (character === "{") {
      stack.push(buffer.trim().replace(/\s+/g, " "));
      buffer = "";
      continue;
    }
    if (character === "}") {
      flush();
      stack.pop();
      continue;
    }
    if (character === ";") {
      flush();
      continue;
    }
    buffer += character;
  }
  return out;
}

/// The same text with every comment replaced by spaces, so line and column
/// numbers still line up with the file a person is reading.
function blankComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, (comment) =>
    comment.replace(/[^\n]/g, " ")
  );
}

/// Every comment in a stylesheet, with the line it starts on.
function commentsOf(css) {
  const out = [];
  const pattern = /\/\*[\s\S]*?\*\//g;
  let match;
  while ((match = pattern.exec(css)) !== null) {
    out.push({
      text: match[0],
      line: css.slice(0, match.index).split("\n").length,
    });
  }
  return out;
}

/// Every token a value reaches for.
function varsIn(value) {
  const out = [];
  const pattern = /var\(\s*(--[a-zA-Z0-9-]+)\s*([,)])/g;
  let match;
  while ((match = pattern.exec(value)) !== null) {
    out.push({ name: match[1], hasFallback: match[2] === "," });
  }
  return out;
}

module.exports = {
  THEMES,
  ROLES,
  STATE_ROLES,
  parseTokens,
  tableFor,
  resolve,
  isHex,
  toRgb,
  ratio,
  chroma,
  hue,
  declarationsOf,
  blankComments,
  commentsOf,
  varsIn,
};
