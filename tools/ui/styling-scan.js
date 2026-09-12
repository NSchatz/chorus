// The styling rules, over the stylesheet SOURCES.
//
// This is a source-text check and it says so: what it grades is what the files
// say, which is exactly the half of the styling conventions that is about the
// files. Clause S1 sanctions the split - "a mechanical check over the
// stylesheet sources, which is a check of source text and so does not collide
// with F2" - and everything about what the page PAINTS is graded in a browser
// engine by tools/ui-render-run.sh instead. Nothing here reports on a rendered
// colour, a rendered length or a rendered face, and nothing here is allowed to
// stand in for one.
//
//   node tools/ui/styling-scan.js                 # this repository
//   node tools/ui/styling-scan.js --fixture DIR   # a committed tree, to be
//                                                 # shown going red
//
// Exit codes, distinct per failure mode:
//
//   0  every rule held
//   2  a rule was broken; every breakage is named with its file and line
//   3  a category examined nothing at all, so the scan has stopped looking
//   5  a file this scan needs is not there

const fs = require("fs");
const path = require("path");

const tokens = require("./tokens");
const { CLAIMS } = require("./claims");

const REPO_ROOT = path.resolve(__dirname, "..", "..");

/// Every rule this scan runs, by the name the clause record cites it as.
const RULES = {
  "tokens-parse": "the token file parses as the format these checks expect",
  "role-vocabulary":
    "all fifteen role tokens are declared with an explicit value in both themes",
  "three-tiers":
    "primitives, semantics and components are separate tiers, and a surface reaches only for the last two",
  "no-colour-literals":
    "no stylesheet source outside the token file carries a colour literal",
  "spacing-scale":
    "no stylesheet source outside the token file carries a length literal, and every length token is a whole multiple of 4px",
  "hand-authored-themes":
    "every theme's value for every role is a literal chosen for that theme, derived from nothing",
  "tokens-resolve": "every token a stylesheet reaches for is declared in both themes",
  "border-and-surface-separation":
    "surfaces are separated by a border token and a surface token, and nothing is raised",
  "one-accent-hue": "one non-neutral hue outside the state roles, and it is the accent",
  "clause-record":
    "the committed styling record gives every clause exactly one disposition",
  "decision-record":
    "the decision that put a measured ratio beside each value is on record, and no comment still denies it",
  "exemptions-stay-true":
    "every clause the record exempts is still a clause this surface does not reach",
};

// The properties whose lengths the 4px scale governs: spacing, padding,
// margin, gap and size, which is what convention clause S3 names. A property
// outside this list still may not carry a length literal - that is
// `no-colour-literals`' sibling rule below, and it has no exceptions - but its
// value is not held to the scale, because a hairline, a focus ring offset and
// a type size are not spacing and a 4px one would be a slab.
const SCALE_PROPERTIES = [
  "margin", "margin-top", "margin-right", "margin-bottom", "margin-left",
  "margin-block", "margin-inline",
  "padding", "padding-top", "padding-right", "padding-bottom", "padding-left",
  "padding-block", "padding-inline",
  "gap", "row-gap", "column-gap",
  "width", "height", "min-width", "min-height", "max-width", "max-height",
  "inline-size", "block-size",
  "top", "right", "bottom", "left", "inset",
  "flex-basis", "flex",
];

// `flex` is in the list above because its third component is a length, and out
// of this one because its first two are bare numbers.
const NO_BARE_NUMBER_PROPERTIES = SCALE_PROPERTIES.filter((p) => p !== "flex");

const LENGTH_UNITS = "px|rem|em|ch|ex|cap|ic|lh|rlh|vh|vw|vmin|vmax|vi|vb|pt|pc|in|cm|mm|q";
const LENGTH_LITERAL = new RegExp(
  `(^|[\\s,(/])(-?\\d*\\.?\\d+)(${LENGTH_UNITS})\\b`,
  "i"
);
const BARE_NUMBER = /(^|[\s,(/])(-?\d*\.?\d+)(?![\w%.])/;

const COLOUR_FUNCTION = /\b(rgb|rgba|hsl|hsla|hwb|lab|lch|oklab|oklch|color|color-mix|light-dark)\s*\(/i;
const HEX_LITERAL = /#[0-9a-fA-F]{3,8}\b/;

// Every CSS named colour, plus the two keywords that are colours by another
// name. A page that writes one of these is writing a colour, whatever it calls
// it.
const NAMED_COLOURS = `aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond
blue blueviolet brown burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson
cyan darkblue darkcyan darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta darkolivegreen
darkorange darkorchid darkred darksalmon darkseagreen darkslateblue darkslategray darkslategrey
darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite
forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew hotpink
indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral
lightcyan lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen
lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime limegreen linen magenta
maroon mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen mediumslateblue
mediumspringgreen mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin
navajowhite navy oldlace olive olivedrab orange orangered orchid palegoldenrod palegreen paleturquoise
palevioletred papayawhip peachpuff peru pink plum powderblue purple rebeccapurple red rosybrown
royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue slateblue slategray
slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke
yellow yellowgreen transparent currentcolor`
  .split(/\s+/)
  .filter(Boolean);

const NAMED_COLOUR = new RegExp(`(^|[\\s,(])(${NAMED_COLOURS.join("|")})(?=$|[\\s,)/;])`, "i");

// The forms that make one theme out of another. S6 refuses all of them: a
// derived palette misses contrast floors, and the hand fixes land anyway
// without the honesty of having been chosen.
const DERIVED_FORMS = [
  { pattern: /color-mix\s*\(/i, what: "a color-mix()" },
  { pattern: /light-dark\s*\(/i, what: "a light-dark()" },
  { pattern: /\b(rgb|hsl|hwb|lab|lch|oklab|oklch|color)\s*\(\s*from\b/i, what: "a relative-colour form" },
  { pattern: /\bfilter\s*:/i, what: "a filter" },
  { pattern: /\b(invert|saturate|hue-rotate|brightness)\s*\(/i, what: "a filter function" },
];

// A comment that still asserts the rule the decision record supersedes. A file
// carrying both the new annotations and the old argument against them is worse
// than either.
const SUPERSEDED_CLAIMS = [
  /no ratio[^*]{0,80}beside a value/i,
  /ratio[^*]{0,40}(is not|are not|never)[^*]{0,30}written beside/i,
  /there is no ratio written/i,
];

const MOTION = [
  { pattern: /^transition/, what: "a transition" },
  { pattern: /^animation/, what: "an animation" },
  { pattern: /^scroll-behavior/, what: "a smooth scroll" },
];

const RAISED = [
  { pattern: /^box-shadow/, what: "a box shadow" },
  { pattern: /^text-shadow/, what: "a text shadow" },
];

// --- what to scan ------------------------------------------------------------

const DEMONSTRATIONS = path.join(REPO_ROOT, "tools", "styling-demonstrations");
const BASE = path.join(DEMONSTRATIONS, "base");

/// A committed tree to scan, composed over the base tree beside it.
///
/// `base/` is a small tree that holds every rule. A demonstration commits ONLY
/// the file it breaks and inherits the rest, so what a reader has to compare is
/// one file against one file rather than a copy of everything against a copy of
/// everything, and so "this tree breaks exactly one thing" is visible instead of
/// asserted. The record and the decision record fall back to this repository's
/// own, which is what lets a demonstration break one of those in isolation too.
function fixtureConfig(dir) {
  const fixture = path.resolve(dir);
  const repo = repoConfig();
  const pick = (name) =>
    fs.existsSync(path.join(fixture, name))
      ? path.join(fixture, name)
      : path.join(BASE, name);
  const cssIn = (where) =>
    fs.existsSync(where)
      ? fs.readdirSync(where).filter((name) => name.endsWith(".css") && name !== "tokens.css")
      : [];
  const names = [...new Set([...cssIn(BASE), ...cssIn(fixture)])].sort();
  const ownRecord = path.join(fixture, "styling-record.md");
  const ownDecisions = path.join(fixture, "decisions");
  return {
    label: path.relative(REPO_ROOT, fixture) || fixture,
    tokenFile: pick("tokens.css"),
    sources: names.map(pick),
    record: fs.existsSync(ownRecord) ? ownRecord : repo.record,
    decisions: fs.existsSync(ownDecisions) ? ownDecisions : repo.decisions,
  };
}

function repoConfig() {
  const ui = path.join(REPO_ROOT, "crates", "server", "src", "ui");
  return {
    label: "this repository",
    tokenFile: path.join(ui, "tokens.css"),
    sources: fs.existsSync(ui)
      ? fs
          .readdirSync(ui)
          .filter((name) => name.endsWith(".css") && name !== "tokens.css")
          .map((name) => path.join(ui, name))
          .sort()
      : [],
    record: path.join(REPO_ROOT, "docs", "styling-conventions-record.md"),
    decisions: path.join(REPO_ROOT, "docs", "decisions"),
  };
}

// --- the report --------------------------------------------------------------

class Report {
  constructor() {
    this.failures = [];
    this.ran = new Set();
    this.empty = [];
  }
  runs(rule) {
    if (!RULES[rule]) {
      throw new Error(`"${rule}" is not a declared rule`);
    }
    this.ran.add(rule);
  }
  fail(rule, where, message) {
    this.runs(rule);
    this.failures.push({ rule, where, message });
  }
  stoppedLooking(what) {
    this.empty.push(what);
  }
  print() {
    for (const failure of this.failures) {
      console.log(`FAIL ${failure.rule} ${failure.where}: ${failure.message}`);
    }
    for (const what of this.empty) {
      console.log(`STOPPED LOOKING: ${what}`);
    }
    const broken = new Set(this.failures.map((f) => f.rule));
    for (const rule of [...this.ran].sort()) {
      if (!broken.has(rule)) {
        console.log(`pass ${rule}: ${RULES[rule]}`);
      }
    }
    console.log(`ran: ${[...this.ran].sort().join(",")}`);
  }
}

// --- the rules ---------------------------------------------------------------

function scan(config) {
  const report = new Report();

  if (!fs.existsSync(config.tokenFile)) {
    console.log(`FAIL there is no token file at ${config.tokenFile}`);
    return 5;
  }
  const parsed = tokens.parseTokens(
    fs.readFileSync(config.tokenFile, "utf8"),
    path.relative(REPO_ROOT, config.tokenFile)
  );

  report.runs("tokens-parse");
  for (const problem of parsed.problems) {
    report.fail("tokens-parse", parsed.path, problem);
  }
  if (parsed.problems.length) {
    // Every rule below reads the parse. Reporting them against a file that did
    // not parse would be reporting noise, and reporting the ones that happened
    // to pass would be reporting part of the check as passed, which AC-5
    // refuses by name.
    report.print();
    console.log(
      `\n${config.label}: the token file did not parse, so no part of this check is reported as passed`
    );
    return 2;
  }

  const tables = {};
  for (const theme of tokens.THEMES) {
    tables[theme] = tokens.tableFor(parsed, theme);
  }

  const sources = config.sources.map((file) => ({
    path: path.relative(REPO_ROOT, file),
    text: fs.readFileSync(file, "utf8"),
  }));
  for (const source of sources) {
    source.declarations = tokens.declarationsOf(source.text, source.path);
    source.comments = tokens.commentsOf(source.text);
  }

  if (sources.length === 0) {
    report.stoppedLooking("there is no stylesheet source beside the token file to check");
  }
  const declarationCount = sources.reduce((n, s) => n + s.declarations.length, 0);
  if (sources.length > 0 && declarationCount === 0) {
    report.stoppedLooking("no declaration was found in any stylesheet source");
  }

  roleVocabulary(report, parsed, tables);
  threeTiers(report, parsed, tables, sources);
  noColourLiterals(report, sources);
  spacingScale(report, tables, sources);
  handAuthoredThemes(report, parsed, tables);
  tokensResolve(report, parsed, tables, sources);
  separation(report, sources);
  oneAccentHue(report, tables);
  const record = clauseRecord(report, config);
  decisionRecord(report, config, sources);
  exemptionsStayTrue(report, record, sources);

  report.print();
  if (report.empty.length) {
    console.log(`\n${config.label}: a category examined nothing at all`);
    return 3;
  }
  if (report.failures.length) {
    console.log(`\n${config.label}: ${report.failures.length} styling rules did not hold`);
    return 2;
  }
  console.log(
    `\n${config.label}: ${sources.length} stylesheet source(s) and ${declarationCount} declarations hold every styling rule`
  );
  return 0;
}

/// S2, and AC-1: the fifteen names, in both themes, each with a value.
function roleVocabulary(report, parsed, tables) {
  report.runs("role-vocabulary");
  for (const theme of tokens.THEMES) {
    for (const role of tokens.ROLES) {
      const declared = parsed.declarations.find(
        (d) => d.name === role && d.tier === "semantic" && d.theme === theme
      );
      if (!declared) {
        report.fail(
          "role-vocabulary",
          `${parsed.path}`,
          `${role} is not declared in the ${theme} theme`
        );
        continue;
      }
      if (!declared.value) {
        report.fail(
          "role-vocabulary",
          `${parsed.path}:${declared.line}`,
          `${role} is declared in the ${theme} theme with no value`
        );
      }
    }
    for (const name of parsed.tiers.semantic[theme]) {
      if (!tokens.ROLES.includes(name) ) {
        report.fail(
          "role-vocabulary",
          `${parsed.path}`,
          `${name} is in the ${theme} semantic tier and is not one of the fifteen role names; a role the vocabulary does not have belongs in the component tier`
        );
      }
    }
  }
}

/// S2's tiers: a surface reaches for a semantic or a component token, a
/// component reaches for a role rather than for the palette, and only the
/// semantic tier names a primitive colour.
function threeTiers(report, parsed, tables, sources) {
  report.runs("three-tiers");
  const primitives = new Set(parsed.tiers.primitive);
  const isPrimitiveColour = (name) => {
    const declaration = tables.light.get(name);
    return Boolean(declaration && primitives.has(name) && tokens.isHex(declaration.value));
  };

  for (const source of sources) {
    for (const declaration of source.declarations) {
      for (const used of tokens.varsIn(declaration.value)) {
        if (primitives.has(used.name)) {
          report.fail(
            "three-tiers",
            `${declaration.path}:${declaration.line}`,
            `${declaration.property} reaches for the primitive ${used.name}; a surface names a semantic role or a component token, so that the value can be changed where it means something`
          );
        }
      }
    }
  }

  for (const declaration of parsed.declarations) {
    if (declaration.tier !== "component") {
      continue;
    }
    for (const used of tokens.varsIn(declaration.value)) {
      if (isPrimitiveColour(used.name)) {
        report.fail(
          "three-tiers",
          `${parsed.path}:${declaration.line}`,
          `the component token ${declaration.name} names the primitive colour ${used.name}; a component binds a ROLE, and the palette is reached through the semantic tier`
        );
      }
    }
  }
}

/// S1's first half, and AC-2.
function noColourLiterals(report, sources) {
  report.runs("no-colour-literals");
  for (const source of sources) {
    for (const declaration of source.declarations) {
      const where = `${declaration.path}:${declaration.line}`;
      const value = declaration.value;
      let found = null;
      if (HEX_LITERAL.test(value)) {
        found = `the hexadecimal colour ${HEX_LITERAL.exec(value)[0]}`;
      } else if (COLOUR_FUNCTION.test(value)) {
        found = `the colour function ${COLOUR_FUNCTION.exec(value)[1]}()`;
      } else if (NAMED_COLOUR.test(value)) {
        found = `the CSS named colour "${NAMED_COLOUR.exec(value)[2]}"`;
      }
      if (found) {
        report.fail(
          "no-colour-literals",
          where,
          `${declaration.property} carries ${found}; every colour resolves to a token declared in the token file`
        );
      }
    }
  }
}

/// S1's second half and S3, which is AC-3.
function spacingScale(report, tables, sources) {
  report.runs("spacing-scale");
  let lengthsChecked = 0;
  for (const source of sources) {
    for (const declaration of source.declarations) {
      const where = `${declaration.path}:${declaration.line}`;
      const property = declaration.property.toLowerCase();
      const value = declaration.value;

      if (LENGTH_LITERAL.test(value)) {
        const hit = LENGTH_LITERAL.exec(value);
        report.fail(
          "spacing-scale",
          where,
          `${declaration.property} carries the length ${hit[2]}${hit[3]} written inline; every length resolves to a token declared in the token file`
        );
      }
      if (NO_BARE_NUMBER_PROPERTIES.includes(property) && BARE_NUMBER.test(value)) {
        report.fail(
          "spacing-scale",
          where,
          `${declaration.property} carries the bare number ${BARE_NUMBER.exec(value)[2]}; zero is a length like any other and the scale declares it`
        );
      }
      if (!SCALE_PROPERTIES.includes(property)) {
        continue;
      }
      for (const used of tokens.varsIn(value)) {
        for (const theme of tokens.THEMES) {
          const resolved = tokens.resolve(tables[theme], used.name);
          if (resolved.error) {
            // tokens-resolve reports this; reporting it twice helps nobody.
            continue;
          }
          lengthsChecked += 1;
          const length = /^(-?\d+(?:\.\d+)?)px$/.exec(resolved.value.trim());
          if (!length) {
            report.fail(
              "spacing-scale",
              where,
              `${declaration.property} spends ${used.name}, which is "${resolved.value}" in the ${theme} theme; a length on the scale is written in whole pixels`
            );
            continue;
          }
          const pixelsOf = Number(length[1]);
          if (!Number.isInteger(pixelsOf) || pixelsOf % 4 !== 0) {
            report.fail(
              "spacing-scale",
              where,
              `${declaration.property} spends ${used.name}, which is ${pixelsOf}px in the ${theme} theme and is not a whole multiple of 4px`
            );
          }
        }
      }
    }
  }
  if (sources.length > 0 && lengthsChecked === 0) {
    report.stoppedLooking(
      "no length token was examined at all, so the 4px scale was not checked against anything"
    );
  }
}

/// S6, which is AC-4: each theme's value is a literal chosen for that theme.
function handAuthoredThemes(report, parsed, tables) {
  report.runs("hand-authored-themes");
  for (const declaration of parsed.declarations) {
    for (const form of DERIVED_FORMS) {
      if (form.pattern.test(declaration.value)) {
        report.fail(
          "hand-authored-themes",
          `${parsed.path}:${declaration.line}`,
          `${declaration.name} is ${form.what}; a theme derived by rule is refused, because the derived colours miss the floors and the hand fixes land anyway`
        );
      }
    }
  }
  for (const theme of tokens.THEMES) {
    for (const role of tokens.ROLES) {
      const declaration = tables[theme].get(role);
      if (!declaration || declaration.theme !== theme) {
        continue;
      }
      const alias = /^var\(\s*(--[a-zA-Z0-9-]+)\s*\)$/.exec(declaration.value);
      if (!alias) {
        if (tokens.isHex(declaration.value)) {
          continue;
        }
        report.fail(
          "hand-authored-themes",
          `${parsed.path}:${declaration.line}`,
          `${role} in the ${theme} theme is "${declaration.value}", which is neither a literal nor one named primitive`
        );
        continue;
      }
      if (!parsed.tiers.primitive.includes(alias[1])) {
        report.fail(
          "hand-authored-themes",
          `${parsed.path}:${declaration.line}`,
          `${role} in the ${theme} theme is ${alias[1]}, which is not a primitive; a role names one raw value chosen for that theme and never another role`
        );
        continue;
      }
      const resolved = tokens.resolve(tables[theme], role);
      if (resolved.error) {
        report.fail(
          "hand-authored-themes",
          `${parsed.path}:${declaration.line}`,
          resolved.error
        );
      }
    }
  }
}

/// AC-5: a token a stylesheet reaches for and no theme declares.
function tokensResolve(report, parsed, tables, sources) {
  report.runs("tokens-resolve");
  let checked = 0;
  const places = [
    ...sources.flatMap((source) =>
      source.declarations.map((d) => ({ ...d, where: `${d.path}:${d.line}` }))
    ),
    ...parsed.declarations.map((d) => ({
      property: d.name,
      value: d.value,
      where: `${parsed.path}:${d.line}`,
    })),
  ];
  for (const place of places) {
    for (const used of tokens.varsIn(place.value)) {
      checked += 1;
      if (used.hasFallback) {
        report.fail(
          "tokens-resolve",
          place.where,
          `${place.property} gives ${used.name} a fallback; a fallback is how an undefined token stops being visible, and this check would have nothing left to find`
        );
      }
      for (const theme of tokens.THEMES) {
        const resolved = tokens.resolve(tables[theme], used.name);
        if (resolved.error && resolved.kind !== "derived") {
          report.fail(
            "tokens-resolve",
            place.where,
            `${place.property} reaches for ${used.name}: ${resolved.error} (${theme} theme)`
          );
        }
      }
    }
  }
  if (sources.length > 0 && checked === 0) {
    report.stoppedLooking("no token reference was examined at all");
  }
}

/// S8: separation is a border and a surface, and nothing is raised.
function separation(report, sources) {
  report.runs("border-and-surface-separation");
  let borders = 0;
  let surfaces = 0;
  for (const source of sources) {
    for (const declaration of source.declarations) {
      const property = declaration.property.toLowerCase();
      for (const raised of RAISED) {
        if (raised.pattern.test(property)) {
          report.fail(
            "border-and-surface-separation",
            `${declaration.path}:${declaration.line}`,
            `${declaration.property} raises a surface with ${raised.what}; this page separates by border and surface, and the shadow scale S8 asks for is written down as absent in the styling record`
          );
        }
      }
      if (/^border(-(top|right|bottom|left))?$/.test(property) || /^border-color$/.test(property)) {
        borders += 1;
      }
      if (/^background(-color)?$/.test(property)) {
        surfaces += 1;
      }
    }
  }
  if (sources.length > 0 && (borders === 0 || surfaces === 0)) {
    report.stoppedLooking(
      `separation examined ${borders} border declarations and ${surfaces} surface declarations, so it graded nothing`
    );
  }
}

/// S10: one hue that is not a state, and it is the accent.
function oneAccentHue(report, tables) {
  report.runs("one-accent-hue");
  for (const theme of tokens.THEMES) {
    const hues = [];
    for (const role of tokens.ROLES) {
      if (tokens.STATE_ROLES.includes(role)) {
        continue;
      }
      const resolved = tokens.resolve(tables[theme], role);
      if (resolved.error || !tokens.isHex(resolved.value)) {
        continue;
      }
      const c = tokens.chroma(resolved.value);
      if (c > 0.14) {
        hues.push({ role, value: resolved.value, hue: tokens.hue(resolved.value), chroma: c });
      }
    }
    if (hues.length === 0) {
      report.fail(
        "one-accent-hue",
        theme,
        "no role outside the state tokens carries a hue at all, so this repository declares no accent"
      );
      continue;
    }
    const accent = hues.find((h) => h.role === "--accent");
    if (!accent) {
      report.fail(
        "one-accent-hue",
        theme,
        `the hues outside the state tokens are ${hues.map((h) => h.role).join(", ")} and --accent is not among them`
      );
      continue;
    }
    for (const other of hues) {
      const apart = Math.abs(((other.hue - accent.hue + 540) % 360) - 180);
      if (apart > 15) {
        report.fail(
          "one-accent-hue",
          theme,
          `${other.role} is ${other.value}, a second hue ${Math.round(apart)} degrees from the accent ${accent.value}; every colour outside the state tokens is neutral or the accent`
        );
      }
    }
  }
}

/// AC-14: the committed record, one disposition per clause.
function clauseRecord(report, config) {
  report.runs("clause-record");
  const clauses = Array.from({ length: 10 }, (_, i) => `S${i + 1}`);
  const rows = new Map();
  if (!fs.existsSync(config.record)) {
    report.fail("clause-record", config.record, "there is no styling record at this path");
    return rows;
  }
  const text = fs.readFileSync(config.record, "utf8");
  const where = path.relative(REPO_ROOT, config.record);
  for (const line of text.split("\n")) {
    const match = /^\s*\|\s*(S\d+)\s*\|([^|]*)\|([^|]*)\|([^|]*)\|\s*$/.exec(line);
    if (!match) {
      continue;
    }
    const clause = match[1];
    const row = {
      assertions: match[2].split(",").map((s) => s.trim()).filter(Boolean),
      exemption: match[3].trim(),
      review: match[4].trim(),
    };
    if (rows.has(clause)) {
      report.fail("clause-record", where, `${clause} appears in the record more than once`);
      continue;
    }
    if (!clauses.includes(clause)) {
      report.fail("clause-record", where, `${clause} is not a clause of the styling conventions`);
      continue;
    }
    rows.set(clause, row);
  }
  for (const clause of clauses) {
    const row = rows.get(clause);
    if (!row) {
      report.fail("clause-record", where, `${clause} is absent from the record`);
      continue;
    }
    const given = [
      row.assertions.length ? "an assertion" : null,
      row.exemption ? "an exemption" : null,
      row.review ? "the convention's own review disposition" : null,
    ].filter(Boolean);
    if (given.length === 0) {
      report.fail(
        "clause-record",
        where,
        `${clause} names neither an assertion, nor an exemption with its reason, nor the convention's own review disposition`
      );
      continue;
    }
    if (given.length > 1) {
      report.fail("clause-record", where, `${clause} carries ${given.join(" and ")}`);
      continue;
    }
    if (row.review && !/Graded:/.test(row.review)) {
      report.fail(
        "clause-record",
        where,
        `${clause} takes the review disposition without quoting the convention's own "*Graded:*" line, which is the only thing that can put a clause there`
      );
    }
    for (const assertion of row.assertions) {
      if (assertion.startsWith("source:")) {
        const rule = assertion.slice("source:".length);
        if (!RULES[rule]) {
          report.fail(
            "clause-record",
            where,
            `${clause} names "${assertion}", which is not a rule this scan runs`
          );
          continue;
        }
        if (!report.ran.has(rule)) {
          report.fail(
            "clause-record",
            where,
            `${clause} names the rule "${assertion}", which did not run in this invocation`
          );
        }
        continue;
      }
      if (!Object.prototype.hasOwnProperty.call(CLAIMS, assertion)) {
        report.fail(
          "clause-record",
          where,
          `${clause} names "${assertion}", which is neither a rule this scan runs nor a rendered claim tools/ui/claims.js declares`
        );
      }
    }
  }
  return rows;
}

/// AC-15: the decision that put a ratio beside each value, and no comment still
/// arguing the opposite.
function decisionRecord(report, config, sources) {
  report.runs("decision-record");
  const wanted = [
    {
      key: "Decision:",
      pattern: /ratio[\s\S]{0,80}beside/i,
      says: "that a measured contrast ratio is recorded beside each token value",
    },
    {
      key: "Supersedes:",
      pattern: /ratio/i,
      says: "which rule about writing a ratio in the stylesheet it supersedes",
    },
    {
      key: "Enforcement:",
      pattern: /reconcil/i,
      says: "that a stale annotation is caught by the reconciliation rather than by a reader",
    },
  ];
  const dir = config.decisions;
  const files = fs.existsSync(dir)
    ? fs.readdirSync(dir).filter((name) => name.endsWith(".md")).map((name) => path.join(dir, name))
    : [];
  const found = files.find((file) => {
    const text = fs.readFileSync(file, "utf8");
    return wanted.every((line) => new RegExp(`^${line.key}`, "m").test(text));
  });
  if (!found) {
    report.fail(
      "decision-record",
      path.relative(REPO_ROOT, dir),
      `no decision record here carries all of ${wanted.map((w) => `"${w.key}"`).join(", ")}, so the reversal these annotations depend on is not written down`
    );
  } else {
    const text = fs.readFileSync(found, "utf8");
    for (const line of wanted) {
      const said = new RegExp(`^${line.key}(.*)$`, "m").exec(text);
      if (!said || !line.pattern.test(said[1])) {
        report.fail(
          "decision-record",
          path.relative(REPO_ROOT, found),
          `the "${line.key}" line does not say ${line.says}`
        );
      }
    }
  }
  for (const source of sources) {
    for (const comment of source.comments) {
      for (const claim of SUPERSEDED_CLAIMS) {
        if (claim.test(comment.text)) {
          report.fail(
            "decision-record",
            `${source.path}:${comment.line}`,
            "this comment still asserts that no ratio is written beside a value, which the decision record supersedes; a file that argues both is worse than either"
          );
        }
      }
    }
  }
}

/// The other half of an exemption: it has to still be true. A clause exempted
/// because this surface does not reach it stops being exempt the moment the
/// surface reaches it, and that is a thing a machine can see.
function exemptionsStayTrue(report, record, sources) {
  report.runs("exemptions-stay-true");
  const exempt = (clause) => Boolean(record.get(clause) && record.get(clause).exemption);
  if (exempt("S9")) {
    for (const source of sources) {
      for (const declaration of source.declarations) {
        for (const motion of MOTION) {
          if (motion.pattern.test(declaration.property.toLowerCase())) {
            report.fail(
              "exemptions-stay-true",
              `${declaration.path}:${declaration.line}`,
              `${declaration.property} is ${motion.what}, and the record exempts S9 on the ground that this page animates nothing; assert the clause or drop the motion`
            );
          }
        }
      }
      if (/@keyframes/.test(source.text)) {
        report.fail(
          "exemptions-stay-true",
          source.path,
          "this stylesheet declares @keyframes, and the record exempts S9 on the ground that this page animates nothing"
        );
      }
    }
  }
  if (exempt("S4")) {
    for (const source of sources) {
      if (/\[data-density|--density/.test(source.text)) {
        report.fail(
          "exemptions-stay-true",
          source.path,
          "this stylesheet switches on a density, and the record exempts S4 on the ground that only the compact one is built"
        );
      }
    }
  }
}

// --- the entry point ---------------------------------------------------------

function main(argv) {
  const fixtureAt = argv.indexOf("--fixture");
  if (fixtureAt >= 0) {
    const dir = argv[fixtureAt + 1];
    if (!dir || !fs.existsSync(dir)) {
      console.log(`FAIL there is no fixture tree at ${dir}`);
      return 5;
    }
    return scan(fixtureConfig(dir));
  }
  return scan(repoConfig());
}

if (require.main === module) {
  process.exit(main(process.argv.slice(2)));
}

module.exports = { RULES, scan, repoConfig, fixtureConfig };
