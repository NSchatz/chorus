// The committed identity, and the C2 blocklist, over the control page's SOURCES.
//
// Two halves of one convention. The umbrella's
// `.sdd/conventions/interface-craft.md` C1 asks this repository to commit a
// design record naming its display face, text face, accent, radius signature and
// shadow signature, each with one sentence saying why that value; C2 names the
// values a surface is not built from and allows one a repo names in that record
// with its reason. `docs/interface-craft-record.md` is the record. This is the
// check that holds it to the token file and holds the sources to the list.
//
//   node tools/ui/interface-craft-scan.js                 # this repository
//   node tools/ui/interface-craft-scan.js --fixture DIR   # a committed tree,
//                                                         # to be shown going red
//   node tools/ui/interface-craft-scan.js --help
//
// WHAT THIS IS AND IS NOT. It is a check of SOURCE TEXT, which is what C2 asks
// for in as many words: "a mechanical check over the stylesheet and template
// sources against this list". Nothing here reports on a rendered colour, a
// rendered length or a rendered face. What the page PAINTS is `make verify-ui`,
// in a real browser engine, and C3 to C8 belong there and not here.
//
// COMMENTS ARE NOT THE SURFACE. Every source is read with its comments blanked
// out, line numbers preserved. C2 is about what a surface DECLARES and what it
// SHIPS, and a comment declares nothing and ships nothing; blanking them is also
// what keeps a prose paragraph about a font stack from being parsed as one.
//
// Exit codes, distinct per failure mode. When more than one holds, the first
// listed below wins, so the most actionable one is the one a reader sees:
//
//   0   the record holds, and every identity source was swept against every
//       entry of the C2 blocklist with no hit the record does not name
//   7   the design record is absent
//   8   the design record is there and cannot be read
//   9   the design record does not parse in the shape this check expects
//   5   the record names an exception and gives it no reason, so the exception
//       is not granted and every default it was covering is reported
//   4   a category has stopped matching: the identity sources resolved to an
//       empty set, or a blocklist entry was tested against no source at all
//   10  the record's five identity entries are wrong: one is missing, named
//       twice, carries no reason, names a token the token file does not declare,
//       or disagrees with the value that file declares
//   2   an identity source carries a C2 blocklist entry the record does not name
//       as an exception
//   6   the record names an exception for an entry no identity source carries,
//       so a permission that has stopped being needed is still standing

const fs = require("fs");
const path = require("path");

const tokens = require("./tokens");

const REPO_ROOT = path.resolve(__dirname, "..", "..");
const DEMOS = path.join(REPO_ROOT, "tools", "interface-craft-demonstrations");
const BASE = path.join(DEMOS, "base");

const EXIT = {
  OK: 0,
  BLOCKLIST_HIT: 2,
  STOPPED_MATCHING: 4,
  EXCEPTION_WITHOUT_REASON: 5,
  EXCEPTION_NOTHING_NEEDS: 6,
  RECORD_ABSENT: 7,
  RECORD_UNREADABLE: 8,
  RECORD_UNPARSEABLE: 9,
  IDENTITY_WRONG: 10,
};

/// The order a code is chosen in when more than one failure holds.
const PRECEDENCE = [
  EXIT.RECORD_ABSENT,
  EXIT.RECORD_UNREADABLE,
  EXIT.RECORD_UNPARSEABLE,
  EXIT.EXCEPTION_WITHOUT_REASON,
  EXIT.STOPPED_MATCHING,
  EXIT.IDENTITY_WRONG,
  EXIT.BLOCKLIST_HIT,
  EXIT.EXCEPTION_NOTHING_NEEDS,
];

/// The five names C1 asks for, verbatim.
const IDENTITY_ENTRIES = [
  "display face",
  "text face",
  "accent",
  "radius signature",
  "shadow signature",
];

const RECORD_NAME = "interface-craft-record.md";
const SOURCE_SUFFIXES = [".css", ".html", ".js"];

// --- the blocklist -----------------------------------------------------------
//
// C2, verbatim: "The declared faces are not Inter, Roboto, Helvetica, Arial,
// Space Grotesk or a bare system stack alone; the accent is not Tailwind
// `indigo-500`/`blue-600` or an untouched shadcn `zinc`/`slate` ramp; no surface
// ships a purple-to-blue gradient, a gradient `bg-clip-text` heading,
// glassmorphism, or a uniform `rounded-2xl` + `shadow-lg` pairing."
//
// One entry per distinct thing that can be matched, rather than per clause of
// that sentence. A grouped entry would report less than it knows - "the accent
// is not Tailwind indigo-500/blue-600" tells a reader neither which identifier
// was found nor whether the exception standing over it is still needed for both
// - and a tree carrying one half of a grouped pairing would pass.

/// A face name, as a family name is written in a stack. Case-insensitive because
/// CSS family names are, and bounded so that `pointer` is not `Inter` and
/// `translate` is not `slate`. A leading `-` is deliberately NOT a boundary on
/// the left, so a custom property named `--blue-600` is the identifier it names.
function word(text) {
  return new RegExp(`(?<![A-Za-z0-9_])${text}(?![A-Za-z0-9_])`, "gi");
}

const GENERIC_FAMILIES = [
  "sans-serif", "serif", "monospace", "cursive", "fantasy", "system-ui",
  "ui-sans-serif", "ui-serif", "ui-monospace", "ui-rounded", "math", "emoji",
  "fangsong",
];

/// The families an operating system supplies. A stack built only from these and
/// the generic keywords above is C2's "a bare system stack alone": nothing in it
/// is a face the repository chose.
const SYSTEM_FAMILIES = [
  "-apple-system", "blinkmacsystemfont", "segoe ui", "segoe ui variable",
  "segoe ui symbol", "segoe ui emoji", "roboto", "helvetica neue", "helvetica",
  "arial", "apple color emoji", "sfmono-regular", "sf mono", "menlo", "monaco",
  "consolas", "liberation mono", "liberation sans", "courier new", "courier",
  "dejavu sans", "dejavu sans mono", "cantarell", "ubuntu", "oxygen",
  "oxygen-sans", "fira sans", "droid sans", "noto sans", "noto color emoji",
  "lucida grande", "tahoma", "verdana", "geneva", "apple sd gothic neo",
  "malgun gothic", "hiragino sans", "hiragino kaku gothic pro",
];

const CSS_WIDE_KEYWORDS = ["inherit", "initial", "unset", "revert", "revert-layer"];

/// Where a font stack can be written. A custom property is included because this
/// repository declares both of its faces as one, and `font` is included because
/// the shorthand carries a family in its last position.
const FONT_PROPERTY = /^(font|font-family|--[A-Za-z0-9-]*(?:font|face)[A-Za-z0-9-]*)$/i;

/// A gradient stop between these two hue angles either side is C2's
/// purple-to-blue gradient. The bands are named here rather than buried in the
/// rule so that a colour this repository actually ships can be checked against
/// them: `#0a5387` reads 205 degrees and `#7ab8f5` reads 210, both squarely
/// blue, and nothing here paints a purple at all.
const BLUE_BAND = [195, 255];
const PURPLE_BAND = [255, 330];

/// The named colours that land in one of those two bands. A gradient written
/// with a keyword is written with a colour, and a rule that only read hexadecimal
/// would report "no purple-to-blue gradient" over `linear-gradient(purple, blue)`.
const NAMED_HEX = {
  purple: "#800080", rebeccapurple: "#663399", blueviolet: "#8a2be2",
  darkviolet: "#9400d3", darkorchid: "#9932cc", mediumpurple: "#9370db",
  violet: "#ee82ee", magenta: "#ff00ff", fuchsia: "#ff00ff", orchid: "#da70d6",
  indigo: "#4b0082", mediumslateblue: "#7b68ee", slateblue: "#6a5acd",
  darkslateblue: "#483d8b", blue: "#0000ff", mediumblue: "#0000cd",
  darkblue: "#00008b", navy: "#000080", royalblue: "#4169e1",
  dodgerblue: "#1e90ff", cornflowerblue: "#6495ed", steelblue: "#4682b4",
  deepskyblue: "#00bfff", skyblue: "#87ceeb", lightskyblue: "#87cefa",
  midnightblue: "#191970",
};

const BLOCKLIST = [
  { id: "Inter", kind: "face name", pattern: word("Inter") },
  { id: "Roboto", kind: "face name", pattern: word("Roboto") },
  { id: "Helvetica", kind: "face name", pattern: word("Helvetica") },
  { id: "Arial", kind: "face name", pattern: word("Arial") },
  { id: "Space Grotesk", kind: "face name", pattern: word("Space\\s+Grotesk") },
  { id: "a bare system stack alone", kind: "description", find: bareSystemStacks },
  { id: "indigo-500", kind: "identifier", pattern: word("indigo-500") },
  { id: "blue-600", kind: "identifier", pattern: word("blue-600") },
  { id: "zinc", kind: "identifier", pattern: word("zinc") },
  { id: "slate", kind: "identifier", pattern: word("slate") },
  { id: "a purple-to-blue gradient", kind: "description", find: purpleToBlueGradients },
  { id: "bg-clip-text", kind: "identifier", pattern: word("bg-clip-text") },
  { id: "glassmorphism", kind: "description", find: glassmorphism },
  { id: "rounded-2xl", kind: "identifier", pattern: word("rounded-2xl") },
  { id: "shadow-lg", kind: "identifier", pattern: word("shadow-lg") },
];

const BLOCKLIST_IDS = BLOCKLIST.map((entry) => entry.id);

// --- what gets read ----------------------------------------------------------

/// This repository's identity sources, RESOLVED from the directory rather than
/// written down. `crates/server/src/ui/` IS the surface - `index.html` links
/// `/tokens.css`, `/chorus.css` and `/chorus.js` and nothing else - and a check
/// that hard-coded those four names would go on reporting a complete sweep after
/// a fifth file landed beside them.
function repoConfig() {
  const ui = path.join(REPO_ROOT, "crates", "server", "src", "ui");
  const record = path.join(REPO_ROOT, "docs", RECORD_NAME);
  return {
    label: "this repository",
    sourceDir: ui,
    sources: listSources(ui),
    record,
    recordAt: record,
  };
}

function listSources(dir) {
  if (!fs.existsSync(dir)) {
    return [];
  }
  return fs
    .readdirSync(dir)
    .filter((name) => SOURCE_SUFFIXES.some((suffix) => name.endsWith(suffix)))
    .sort()
    .map((name) => path.join(dir, name));
}

/// A committed tree to scan, composed over the base tree beside it.
///
/// `base/` holds every rule and carries none of the blocklist. A demonstration
/// commits ONLY the file it breaks and inherits the rest, so what a reader
/// compares is one file against one file.
///
/// Two escapes exist, and both are needed by a criterion rather than by
/// convenience. A file named `<name>.removed` under the fixture's `ui/` takes
/// `<name>` out of the composed source set, which is the only way to commit a
/// tree whose identity sources resolve to an EMPTY set while base still has
/// four. A file named `interface-craft-record.md.removed` in the fixture root
/// does the same for the record, which is the only way to commit a tree whose
/// record is genuinely absent.
function fixtureConfig(dir) {
  const fixture = path.resolve(dir);
  const fixtureUi = path.join(fixture, "ui");
  const baseUi = path.join(BASE, "ui");

  const names = [
    ...new Set([
      ...listNames(baseUi),
      ...listNames(fixtureUi),
    ]),
  ]
    .filter((name) => !fs.existsSync(path.join(fixtureUi, `${name}.removed`)))
    .sort();

  const removed = fs.existsSync(path.join(fixture, `${RECORD_NAME}.removed`));

  return {
    label: path.relative(REPO_ROOT, fixture) || fixture,
    sourceDir: fixtureUi,
    sources: names.map((name) => pick(fixtureUi, baseUi, name)),
    record: removed ? null : pick(fixture, BASE, RECORD_NAME),
    recordAt: path.join(fixture, RECORD_NAME),
  };
}

function listNames(dir) {
  if (!fs.existsSync(dir)) {
    return [];
  }
  return fs
    .readdirSync(dir)
    .filter((name) => SOURCE_SUFFIXES.some((suffix) => name.endsWith(suffix)));
}

function pick(own, fallback, name) {
  const mine = path.join(own, name);
  return fs.existsSync(mine) ? mine : path.join(fallback, name);
}

function label(file) {
  return path.relative(REPO_ROOT, file) || file;
}

// --- reading a source --------------------------------------------------------

/// A source file with its comments blanked out and its line offsets precomputed.
/// `text` is `null` when the path is there and cannot be read, which is a state
/// this check reports rather than skipping: a sweep that silently passed over a
/// file it could not open would report a complete sweep it did not do.
function readSource(file) {
  const source = { file, label: label(file), text: null, problem: null };
  let raw;
  try {
    raw = fs.readFileSync(file, "utf8");
  } catch (error) {
    source.problem = error.code === "EISDIR" ? "it is a directory" : String(error.message);
    return source;
  }
  if (file.endsWith(".css")) {
    source.text = tokens.blankComments(raw);
  } else if (file.endsWith(".js")) {
    source.text = blankJsComments(raw);
  } else {
    source.text = blankHtmlComments(raw);
  }
  source.raw = raw;
  return source;
}

/// Block comments, and a line comment that begins its own line. A `//` in the
/// middle of a line is left alone on purpose: the scheme separator of a URL in
/// an attribute is written that way, and blanking from there to the end of the
/// line would hide the rest of it from this sweep.
function blankJsComments(text) {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, blankOut)
    .replace(/^[ \t]*\/\/[^\n]*/gm, blankOut);
}

function blankHtmlComments(text) {
  return text.replace(/<!--[\s\S]*?-->/g, blankOut);
}

function blankOut(run) {
  return run.replace(/[^\n]/g, " ");
}

function lineAt(text, index) {
  let line = 1;
  for (let at = 0; at < index; at += 1) {
    if (text[at] === "\n") {
      line += 1;
    }
  }
  return line;
}

/// Every match of a pattern in a source, with the line it is on.
function textHits(source, pattern) {
  const out = [];
  const re = new RegExp(pattern.source, pattern.flags);
  let match;
  while ((match = re.exec(source.text)) !== null) {
    out.push({
      file: source.label,
      line: lineAt(source.text, match.index),
      detail: `"${match[0].replace(/\s+/g, " ")}" is written here`,
    });
    if (match[0] === "") {
      re.lastIndex += 1;
    }
  }
  return out;
}

// --- the three entries that are a description rather than a string -----------

/// Every font stack a source declares, resolved far enough to classify.
///
/// A declaration whose value is nothing but `var()` references is not a stack:
/// the families live in the token it reaches for, and that token is a
/// declaration of its own in the same sweep.
function fontStacks(source) {
  const out = [];
  const push = (value, line) => {
    const families = splitFamilies(value);
    if (families.length) {
      out.push({ value, families, line });
    }
  };
  if (source.file.endsWith(".css")) {
    for (const declaration of tokens.declarationsOf(source.raw, source.label)) {
      if (FONT_PROPERTY.test(declaration.property)) {
        push(declaration.value, declaration.line);
      }
    }
    return out;
  }
  const pattern = /font(?:-family)?\s*:\s*([^;"'`}<]+)/gi;
  let match;
  while ((match = pattern.exec(source.text)) !== null) {
    push(match[1], lineAt(source.text, match.index));
  }
  return out;
}

function splitFamilies(value) {
  const withoutVars = String(value).replace(/var\(\s*--[A-Za-z0-9-]+\s*(?:,[^()]*)?\)/g, "");
  return withoutVars
    .split(",")
    .map((family) => family.trim().replace(/^["']|["']$/g, "").trim())
    .filter(Boolean)
    .filter((family) => !CSS_WIDE_KEYWORDS.includes(family.toLowerCase()));
}

function isSystemFamily(family) {
  const name = family.toLowerCase().replace(/\s+/g, " ");
  return GENERIC_FAMILIES.includes(name) || SYSTEM_FAMILIES.includes(name);
}

/// C2's "a bare system stack alone": a declared stack in which no family is one
/// the repository chose. A stack led by a repo-chosen face is not this, however
/// many system faces stand behind it as fallbacks.
function bareSystemStacks(source) {
  return fontStacks(source)
    .filter((stack) => stack.families.every(isSystemFamily))
    .map((stack) => ({
      file: source.label,
      line: stack.line,
      detail: `every family in "${stack.value.trim()}" is a system face or a generic keyword`,
    }));
}

/// C2's purple-to-blue gradient: one gradient function carrying a stop in the
/// purple band and a stop in the blue band. Token references are resolved
/// through the token file, because a gradient written in role names is still a
/// gradient.
function purpleToBlueGradients(source, context) {
  const out = [];
  const pattern = /\b(?:repeating-)?(?:linear|radial|conic)-gradient\s*\(/gi;
  let match;
  while ((match = pattern.exec(source.text)) !== null) {
    const open = match.index + match[0].length - 1;
    const body = balanced(source.text, open);
    if (body === null) {
      continue;
    }
    const hues = stopHues(body, context);
    const purple = hues.filter((h) => inBand(h, PURPLE_BAND));
    const blue = hues.filter((h) => inBand(h, BLUE_BAND));
    if (purple.length && blue.length) {
      out.push({
        file: source.label,
        line: lineAt(source.text, match.index),
        detail:
          `a stop at ${Math.round(purple[0])} degrees is purple and a stop at ` +
          `${Math.round(blue[0])} degrees is blue`,
      });
    }
  }
  return out;
}

/// The text between an opening parenthesis and the one that closes it.
function balanced(text, open) {
  let depth = 0;
  for (let at = open; at < text.length; at += 1) {
    if (text[at] === "(") {
      depth += 1;
    } else if (text[at] === ")") {
      depth -= 1;
      if (depth === 0) {
        return text.slice(open + 1, at);
      }
    }
  }
  return null;
}

function stopHues(body, context) {
  const out = [];
  const add = (value) => {
    const angle = tokens.hue(value);
    if (angle !== null) {
      out.push(angle);
    }
  };
  for (const hex of body.match(/#[0-9a-fA-F]{6}\b/g) || []) {
    add(hex);
  }
  for (const [name, hex] of Object.entries(NAMED_HEX)) {
    if (word(name).test(body)) {
      add(hex);
    }
  }
  for (const reference of tokens.varsIn(body)) {
    const resolved = resolveToken(reference.name, context, []);
    if (resolved) {
      add(resolved);
    }
  }
  return out;
}

/// A token's value, following `var()` chains through the token file until a
/// literal is reached.
function resolveToken(name, context, seen) {
  if (seen.includes(name)) {
    return null;
  }
  const values = context.declarations.get(name);
  if (!values || !values.length) {
    return null;
  }
  for (const value of values) {
    if (tokens.isHex(value)) {
      return value;
    }
    const next = tokens.varsIn(value);
    if (next.length) {
      const resolved = resolveToken(next[0].name, context, [...seen, name]);
      if (resolved) {
        return resolved;
      }
    }
  }
  return null;
}

function inBand(angle, [low, high]) {
  return angle >= low && angle < high;
}

/// C2's glassmorphism. The `backdrop-filter` property IS the effect: it exists
/// to blur what is behind a translucent surface, and nothing else uses it. The
/// second shape is the same effect built by hand, a blurred filter on a rule
/// whose background is translucent, and it is matched per rule because a blur
/// somewhere in a file and a translucent panel somewhere else are two unrelated
/// declarations.
function glassmorphism(source) {
  const out = [];
  for (const hit of textHits(source, /-?(?:webkit-)?backdrop-filter\s*:/gi)) {
    out.push({ ...hit, detail: "a backdrop-filter blurs what is behind the surface" });
  }
  if (!source.file.endsWith(".css")) {
    return out;
  }
  const rules = new Map();
  for (const declaration of tokens.declarationsOf(source.raw, source.label)) {
    const key = declaration.selector;
    if (!rules.has(key)) {
      rules.set(key, []);
    }
    rules.get(key).push(declaration);
  }
  for (const declarations of rules.values()) {
    const blurred = declarations.find(
      (d) => /^-?(?:webkit-)?filter$/i.test(d.property) && /\bblur\s*\(/i.test(d.value)
    );
    const translucent = declarations.find(
      (d) => /background/i.test(d.property) && isTranslucent(d.value)
    );
    if (blurred && translucent) {
      out.push({
        file: source.label,
        line: blurred.line,
        detail: "a blurred filter over a translucent background is glassmorphism built by hand",
      });
    }
  }
  return out;
}

function isTranslucent(value) {
  if (/\b(?:rgba|hsla)\s*\(/i.test(value)) {
    return true;
  }
  if (/\b(?:rgb|hsl|hwb|lab|lch|oklab|oklch|color)\s*\([^)]*\//.test(value)) {
    return true;
  }
  return /#[0-9a-fA-F]{8}\b|#[0-9a-fA-F]{4}\b/.test(value);
}

// --- the design record -------------------------------------------------------

const IDENTITY_HEADING = "## The identity";
const EXCEPTIONS_HEADING = "## The C2 exceptions";
const IDENTITY_COLUMNS = ["entry", "declares", "why"];
const EXCEPTIONS_COLUMNS = ["blocklist entry", "why"];
const PAIR = /^`(--[A-Za-z0-9-]+)` = `([^`]*)`/;
const ABSENT = /^absent; kept absent by `([^`]+)` rule `([^`]+)`$/;
const BACKTICKED = /^`([^`]+)`$/;

/// What the record is, before anything is asked of what it says.
function loadRecord(file) {
  if (file === null || !fs.existsSync(file)) {
    return { state: "absent", file };
  }
  let text;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch (error) {
    const why = error.code === "EISDIR" ? "it is a directory" : String(error.message);
    return { state: "unreadable", file, why };
  }
  const parsed = parseRecord(text);
  if (parsed.problems.length) {
    return { state: "unparseable", file, problems: parsed.problems };
  }
  return { state: "ok", file, identity: parsed.identity, exceptions: parsed.exceptions };
}

function parseRecord(text) {
  const problems = [];
  const lines = text.split("\n");

  const identityRows = tableUnder(lines, IDENTITY_HEADING, IDENTITY_COLUMNS, problems);
  const exceptionRows = tableUnder(lines, EXCEPTIONS_HEADING, EXCEPTIONS_COLUMNS, problems);
  if (problems.length) {
    return { problems };
  }

  const identity = [];
  for (const row of identityRows) {
    const [entry, declares, why] = row.cells;
    const parsedCell = parseDeclares(declares);
    if (!parsedCell) {
      problems.push(
        `line ${row.line}: the declares cell for "${entry}" is neither a "; "-separated list of ` +
          '`--token` = `value` pairs nor an "absent; kept absent by `<path>` rule `<name>`" line'
      );
      continue;
    }
    identity.push({ entry, why, line: row.line, ...parsedCell });
  }

  const exceptions = [];
  for (const row of exceptionRows) {
    const [entry, why] = row.cells;
    const name = BACKTICKED.exec(entry);
    if (!name) {
      problems.push(
        `line ${row.line}: the blocklist entry "${entry}" is not written in backticks, so nothing ` +
          "says where the name starts and stops"
      );
      continue;
    }
    exceptions.push({ entry: name[1], why, line: row.line });
  }

  return { problems, identity, exceptions };
}

/// The first table under a heading, with its columns checked. Every way this can
/// go wrong is a record that does not parse in the shape this check expects, and
/// each one says which way it was.
function tableUnder(lines, heading, columns, problems) {
  const start = lines.findIndex((line) => line.trim() === heading);
  if (start < 0) {
    problems.push(`there is no "${heading}" heading`);
    return [];
  }
  let at = start + 1;
  while (at < lines.length && !lines[at].trim().startsWith("|")) {
    if (lines[at].trim().startsWith("## ")) {
      problems.push(`there is no table under "${heading}"`);
      return [];
    }
    at += 1;
  }
  if (at >= lines.length) {
    problems.push(`there is no table under "${heading}"`);
    return [];
  }
  const header = cellsOf(lines[at]);
  if (header.length !== columns.length || header.some((cell, i) => cell !== columns[i])) {
    problems.push(
      `line ${at + 1}: the table under "${heading}" has columns [${header.join(", ")}] ` +
        `and this check reads [${columns.join(", ")}]`
    );
    return [];
  }
  at += 1;
  if (at >= lines.length || !/^\|[\s:|-]+\|$/.test(lines[at].trim())) {
    problems.push(`line ${at + 1}: the table under "${heading}" has no separator row under its header`);
    return [];
  }
  at += 1;
  const rows = [];
  while (at < lines.length && lines[at].trim().startsWith("|")) {
    const cells = cellsOf(lines[at]);
    if (cells.length !== columns.length) {
      problems.push(
        `line ${at + 1}: this row of the table under "${heading}" has ${cells.length} cells ` +
          `and the header has ${columns.length}`
      );
      return [];
    }
    rows.push({ cells, line: at + 1 });
    at += 1;
  }
  return rows;
}

function cellsOf(line) {
  const trimmed = line.trim();
  return trimmed
    .slice(1, trimmed.endsWith("|") ? -1 : undefined)
    .split("|")
    .map((cell) => cell.trim());
}

function parseDeclares(cell) {
  const absent = ABSENT.exec(cell);
  if (absent) {
    return { absent: true, keptBy: { file: absent[1], rule: absent[2] }, pairs: [] };
  }
  const pairs = [];
  let rest = cell;
  for (;;) {
    const match = PAIR.exec(rest);
    if (!match) {
      return null;
    }
    pairs.push({ name: match[1], value: match[2] });
    rest = rest.slice(match[0].length);
    if (rest === "") {
      return { absent: false, pairs };
    }
    if (!rest.startsWith("; ")) {
      return null;
    }
    rest = rest.slice(2);
  }
}

/// One sentence. Empty is what AC-8 and C1 are about; the length floor is what
/// keeps "yes." from being one.
function isReason(text) {
  const reason = String(text || "").trim();
  return reason.length >= 20 && reason.endsWith(".");
}

// --- the declarations the record is held to ----------------------------------

/// Every `--token: value;` declaration in the token file, in the order they are
/// written. A token declared once per theme has one entry per theme, because the
/// record is held to agree with EVERY declaration of a name and not with the
/// first one.
function declarationsIn(source) {
  const out = new Map();
  if (!source || source.text === null) {
    return out;
  }
  const pattern = /(--[A-Za-z0-9-]+)\s*:\s*([^;{}]+);/g;
  let match;
  while ((match = pattern.exec(source.text)) !== null) {
    const name = match[1];
    if (!out.has(name)) {
      out.set(name, []);
    }
    out.get(name).push(normalize(match[2]));
  }
  return out;
}

function normalize(value) {
  return String(value).trim().replace(/\s+/g, " ");
}

// --- the report --------------------------------------------------------------

class Report {
  constructor() {
    this.findings = [];
  }
  fail(code, finding, subject, where, message) {
    this.findings.push({ code, finding, subject, where, message });
  }
  has(code) {
    return this.findings.some((finding) => finding.code === code);
  }
  print() {
    for (const finding of this.findings) {
      const subject = finding.subject === null ? "" : `[${finding.subject}] `;
      const where = finding.where ? `${finding.where}: ` : "";
      console.log(`FAIL ${finding.finding} ${subject}${where}${finding.message}`);
    }
  }
  code() {
    for (const code of PRECEDENCE) {
      if (this.has(code)) {
        return code;
      }
    }
    return EXIT.OK;
  }
}

// --- the scan ----------------------------------------------------------------

function scan(config) {
  const report = new Report();

  // 1. The record, as a file, before anything is asked of what it says.
  const record = loadRecord(config.record);
  if (record.state === "absent") {
    console.log(
      `FAIL record-absent [${label(config.recordAt)}]: ` +
        "the design record is absent; interface-craft C1 asks for one to be committed"
    );
    console.log("read: ");
    console.log("tested: ");
    console.log("sources-read: 0");
    console.log("entries-tested: 0");
    console.log("least-tested-against: 0");
    return EXIT.RECORD_ABSENT;
  }
  if (record.state === "unreadable") {
    console.log(
      `FAIL record-unreadable [${label(record.file)}]: the design record is there and cannot ` +
        `be read (${record.why})`
    );
    console.log("read: ");
    console.log("tested: ");
    console.log("sources-read: 0");
    console.log("entries-tested: 0");
    console.log("least-tested-against: 0");
    return EXIT.RECORD_UNREADABLE;
  }
  if (record.state === "unparseable") {
    for (const problem of record.problems) {
      console.log(
        `FAIL record-unparseable [${label(record.file)}]: the design record does not parse in ` +
          `the shape this check expects: ${problem}`
      );
    }
    console.log("read: ");
    console.log("tested: ");
    console.log("sources-read: 0");
    console.log("entries-tested: 0");
    console.log("least-tested-against: 0");
    return EXIT.RECORD_UNPARSEABLE;
  }

  // 2. What was actually resolved, and what could actually be read.
  const sources = config.sources.map(readSource);
  const readable = sources.filter((source) => source.text !== null);
  for (const source of sources) {
    if (source.text === null) {
      report.fail(
        EXIT.STOPPED_MATCHING,
        "stopped-matching",
        null,
        source.label,
        `this identity source could not be read (${source.problem}), so no blocklist entry was tested against it`
      );
    }
  }

  const tokenSource = readable.find((source) => source.file.endsWith("tokens.css"));
  const context = { declarations: declarationsIn(tokenSource) };

  // 3. The exceptions the record grants, and the ones it fails to.
  const granted = new Set();
  const seen = new Set();
  for (const exception of record.exceptions) {
    if (!BLOCKLIST_IDS.includes(exception.entry)) {
      report.fail(
        EXIT.EXCEPTION_NOTHING_NEEDS,
        "exception-nothing-needs",
        exception.entry,
        `${label(record.file)}:${exception.line}`,
        "the record excepts this, and it is not an entry of the C2 blocklist at all"
      );
      continue;
    }
    if (seen.has(exception.entry)) {
      report.fail(
        EXIT.EXCEPTION_NOTHING_NEEDS,
        "exception-nothing-needs",
        exception.entry,
        `${label(record.file)}:${exception.line}`,
        "the record excepts this twice, and the second permission is one nothing needs"
      );
      continue;
    }
    seen.add(exception.entry);
    if (!isReason(exception.why)) {
      report.fail(
        EXIT.EXCEPTION_WITHOUT_REASON,
        "exception-without-reason",
        exception.entry,
        `${label(record.file)}:${exception.line}`,
        "the record excepts this and gives no reason, so it is not excepted and every place it is carried is reported below"
      );
      continue;
    }
    granted.add(exception.entry);
  }

  // 4. The sweep: every entry against every source that could be read.
  if (config.sources.length === 0) {
    report.fail(
      EXIT.STOPPED_MATCHING,
      "stopped-matching",
      null,
      config.label,
      `the identity sources resolved to an EMPTY set under ${label(config.sourceDir)}, so this check read no file at all`
    );
  }
  const tested = [];
  for (const entry of BLOCKLIST) {
    const hits = [];
    for (const source of readable) {
      const found = entry.find
        ? entry.find(source, context)
        : textHits(source, entry.pattern);
      hits.push(...found);
    }
    tested.push({ entry, hits, against: readable.length });
    if (readable.length === 0) {
      report.fail(
        EXIT.STOPPED_MATCHING,
        "stopped-matching",
        entry.id,
        config.label,
        "this blocklist entry was tested against no source at all, which is a category that has stopped matching rather than a compliant surface"
      );
      continue;
    }
    if (granted.has(entry.id)) {
      if (hits.length === 0) {
        report.fail(
          EXIT.EXCEPTION_NOTHING_NEEDS,
          "exception-nothing-needs",
          entry.id,
          label(record.file),
          "the record excepts this and no identity source carries it, so the permission is withdrawn rather than left standing"
        );
      }
      continue;
    }
    for (const hit of hits) {
      report.fail(
        EXIT.BLOCKLIST_HIT,
        "blocklist-hit",
        entry.id,
        `${hit.file}:${hit.line}`,
        `${hit.detail}, and the design record does not name this entry as an exception`
      );
    }
  }

  // 5. The five identity entries, and their agreement with the token file.
  if (readable.length) {
    gradeIdentity(record, context, tokenSource, report);
  }

  // 6. What was measured, printed rather than inferred from a zero exit.
  report.print();
  printWhatWasRead(config, sources, readable, tested, granted);
  return report.code();
}

function gradeIdentity(record, context, tokenSource, report) {
  const where = label(record.file);
  const counted = new Map();
  for (const declared of record.identity) {
    counted.set(declared.entry, (counted.get(declared.entry) || 0) + 1);
  }
  for (const name of IDENTITY_ENTRIES) {
    const count = counted.get(name) || 0;
    if (count !== 1) {
      report.fail(
        EXIT.IDENTITY_WRONG,
        "identity",
        name,
        where,
        count === 0
          ? "interface-craft C1 asks for this entry and the record declares it nowhere"
          : `the record declares this entry ${count} times and C1 asks for exactly one`
      );
    }
  }
  for (const declared of record.identity) {
    if (!IDENTITY_ENTRIES.includes(declared.entry)) {
      report.fail(
        EXIT.IDENTITY_WRONG,
        "identity",
        declared.entry,
        `${where}:${declared.line}`,
        `this is not one of the five entries C1 names (${IDENTITY_ENTRIES.join(", ")})`
      );
      continue;
    }
    if (!isReason(declared.why)) {
      report.fail(
        EXIT.IDENTITY_WRONG,
        "identity",
        declared.entry,
        `${where}:${declared.line}`,
        "C1 asks for one sentence saying why that value, and this entry carries none"
      );
    }
    if (declared.absent) {
      gradeAbsence(declared, where, report);
      continue;
    }
    if (!tokenSource) {
      report.fail(
        EXIT.IDENTITY_WRONG,
        "identity",
        declared.entry,
        `${where}:${declared.line}`,
        "this entry names tokens and there is no tokens.css among the identity sources to agree with"
      );
      continue;
    }
    const wanted = new Map();
    for (const pair of declared.pairs) {
      if (!wanted.has(pair.name)) {
        wanted.set(pair.name, []);
      }
      wanted.get(pair.name).push(normalize(pair.value));
    }
    for (const [name, values] of wanted) {
      const found = context.declarations.get(name);
      if (!found) {
        report.fail(
          EXIT.IDENTITY_WRONG,
          "identity",
          declared.entry,
          `${where}:${declared.line}`,
          `this entry names ${name} and ${tokenSource.label} declares no such token`
        );
        continue;
      }
      const mine = [...values].sort();
      const theirs = [...found].sort();
      if (mine.length !== theirs.length || mine.some((value, i) => value !== theirs[i])) {
        report.fail(
          EXIT.IDENTITY_WRONG,
          "identity",
          declared.entry,
          `${where}:${declared.line}`,
          `this entry declares ${name} as ${mine.map((v) => `"${v}"`).join(", ")} and ` +
            `${tokenSource.label} declares it as ${theirs.map((v) => `"${v}"`).join(", ")}`
        );
      }
    }
  }
}

/// An entry that declares its value absent has to name the committed check that
/// keeps it absent, so the absence rests on something that runs rather than on
/// nobody having added one.
function gradeAbsence(declared, where, report) {
  const kept = path.join(REPO_ROOT, declared.keptBy.file);
  if (!fs.existsSync(kept)) {
    report.fail(
      EXIT.IDENTITY_WRONG,
      "identity",
      declared.entry,
      `${where}:${declared.line}`,
      `this entry is declared absent and kept absent by ${declared.keptBy.file}, and there is no such file`
    );
    return;
  }
  let text;
  try {
    text = fs.readFileSync(kept, "utf8");
  } catch (error) {
    report.fail(
      EXIT.IDENTITY_WRONG,
      "identity",
      declared.entry,
      `${where}:${declared.line}`,
      `this entry names ${declared.keptBy.file} as the check that keeps it absent, and that file cannot be read`
    );
    return;
  }
  if (!text.includes(declared.keptBy.rule)) {
    report.fail(
      EXIT.IDENTITY_WRONG,
      "identity",
      declared.entry,
      `${where}:${declared.line}`,
      `this entry names the rule "${declared.keptBy.rule}" in ${declared.keptBy.file}, and that file carries no such rule`
    );
  }
}

/// C2's check is decided by a zero exit and AC-4 refuses to let it be: what was
/// resolved and what was tested are PRINTED, so a reader sees the sweep rather
/// than inferring it. The two machine-readable lines are what
/// tools/interface-craft-check.sh asserts against its own committed copy of the
/// blocklist and its own listing of the source directory.
function printWhatWasRead(config, sources, readable, tested, granted) {
  console.log("");
  console.log(`--- ${config.label}: the identity sources, resolved from ${label(config.sourceDir)}`);
  for (const source of sources) {
    console.log(`    ${source.label}${source.text === null ? "  (UNREADABLE)" : ""}`);
  }
  console.log(`read: ${readable.map((source) => source.label).join(",")}`);
  console.log("");
  console.log(`--- ${config.label}: the C2 blocklist, every entry against every source read`);
  for (const row of tested) {
    const disposition = granted.has(row.entry.id)
      ? "excepted by the record"
      : row.hits.length
        ? "NOT excepted"
        : "not carried";
    console.log(
      `    ${row.entry.id.padEnd(28)} ${String(row.against).padStart(2)} source(s), ` +
        `${String(row.hits.length).padStart(2)} hit(s), ${disposition}`
    );
  }
  console.log(`tested: ${tested.map((row) => row.entry.id).join(",")}`);
  console.log(`sources-read: ${readable.length}`);
  console.log(`entries-tested: ${tested.length}`);
  console.log(
    `least-tested-against: ${tested.length ? Math.min(...tested.map((row) => row.against)) : 0}`
  );
}

// --- the surface -------------------------------------------------------------

const HELP = `usage: node tools/ui/interface-craft-scan.js [--fixture DIR]

The control page's committed identity, and the C2 blocklist, over the sources
under crates/server/src/ui. Reads docs/interface-craft-record.md.

  --fixture DIR   scan a committed tree under tools/interface-craft-demonstrations
                  instead of this repository, composed over that directory's base
  --help          this

exit codes:
  0   the record holds and no source carries a blocklist entry the record does
      not except
  2   an identity source carries a C2 blocklist entry the record does not name
      as an exception
  4   a category has stopped matching: the identity sources resolved to an empty
      set, or a blocklist entry was tested against no source at all
  5   the record names an exception and gives it no reason
  6   the record names an exception for an entry no identity source carries
  7   the design record is absent
  8   the design record is there and cannot be read
  9   the design record does not parse in the shape this check expects
  10  the record's five identity entries are wrong: missing, named twice, with
      no reason, or disagreeing with the token file

When more than one holds the first of 7, 8, 9, 5, 4, 10, 2, 6 wins.

example:
  node tools/ui/interface-craft-scan.js`;

function main(argv) {
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log(HELP);
    return EXIT.OK;
  }
  const fixtureAt = argv.indexOf("--fixture");
  if (fixtureAt >= 0) {
    const dir = argv[fixtureAt + 1];
    if (!dir || !fs.existsSync(path.resolve(dir))) {
      console.log(`FAIL there is no fixture tree at ${dir}`);
      return EXIT.STOPPED_MATCHING;
    }
    return scan(fixtureConfig(dir));
  }
  return scan(repoConfig());
}

if (require.main === module) {
  process.exit(main(process.argv.slice(2)));
}

module.exports = { BLOCKLIST_IDS, IDENTITY_ENTRIES, EXIT, scan, repoConfig, fixtureConfig };
