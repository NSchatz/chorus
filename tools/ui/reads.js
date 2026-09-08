// Reading what the page SHOWS.
//
// Every function here goes through the live document: `innerText`, which is the
// text the engine laid out rather than the markup it was built from,
// `getBoundingClientRect`, which is where the box ended up, and the scroll
// geometry the engine computed. Nothing here opens a file, and there is no
// assertion built on any of it that a text search of the HTML, the CSS or the
// JavaScript could satisfy.
//
// It is shared by ui.spec.js, which points it at the page a real chorus-server
// served, and by mutation.spec.js, which points the SAME functions at pages
// written to break exactly one property, so that every measurement here is one
// that has been seen going red.

const SELECTOR =
  'button, select, input, textarea, a[href], [role="button"], [role="switch"], [tabindex]:not([tabindex="-1"])';

// A short description of an element that names it in a failure without being a
// selector any assertion depends on. It is written out inside each evaluate
// body rather than passed in as source, because a page under a
// Content-Security-Policy without 'unsafe-eval' cannot eval anything, and a
// grader that needed the policy loosened to run would be grading a page nobody
// ships.

// --- the three states -------------------------------------------------------

/// Which of the three states the view is in, counted rather than asserted, so
/// that "two at once" is a number and not a judgement.
async function viewStates(page) {
  return page.evaluate(() => ({
    loading: document.querySelectorAll("[data-loading]").length,
    empty: document.querySelectorAll("[data-empty]").length,
    error: document.querySelectorAll("[data-error]").length,
    zones: document.querySelectorAll("[data-zone]").length,
    bodyText: document.body.innerText.trim().length,
  }));
}

function statesShowing(counts) {
  return ["loading", "empty", "error"].filter((state) => counts[state] > 0);
}

// --- what a zone card says --------------------------------------------------

/// Every zone card, as rendered text. This is the measuring code the "shows
/// every zone" claim is graded with and the one its demonstration goes red on.
async function renderedZones(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll("[data-zone]")).map((card) => {
      const text = (selector) => {
        const el = card.querySelector(selector);
        return el ? el.innerText.trim() : null;
      };
      const mute = card.querySelector("[data-mute]");
      return {
        id: card.getAttribute("data-zone"),
        name: text("[data-zone-name]"),
        volume: text("[data-volume]"),
        mute: text("[data-mute-state]"),
        endpoints: text("[data-endpoints]"),
        freshness: text("[data-freshness]"),
        meta: text("[data-zone-meta]"),
        refusal: text("[data-refusal]"),
        pressed: mute ? mute.getAttribute("aria-pressed") : null,
        renameValue: (card.querySelector("[data-rename]") || {}).value || null,
        sliderValue: (card.querySelector("[data-volume-slider]") || {}).value || null,
        groupValue: (card.querySelector("[data-group-select]") || {}).value || null,
      };
    })
  );
}

/// Where what the page shows and what it was supposed to show disagree.
function zoneMismatches(zones, expected) {
  const problems = [];
  for (const [id, wanted] of Object.entries(expected)) {
    const zone = zones.find((z) => z.id === id);
    if (!zone) {
      problems.push({ zone: id, field: "card", shown: null, wanted: "a card" });
      continue;
    }
    for (const [field, value] of Object.entries(wanted)) {
      if (zone[field] !== value) {
        problems.push({ zone: id, field, shown: zone[field], wanted: value });
      }
    }
  }
  return problems;
}

// --- figures ----------------------------------------------------------------

/// An aggregate that says only how many, with no word about what it counted.
function isBareCount(text) {
  return /^\s*\d+\s+endpoints?\b/i.test(String(text || "").trim());
}

/// An aggregate that says which rows it counted.
function statesItsSet(text) {
  return /\b\d+\s+of\s+\d+\b/.test(String(text || ""));
}

/// The renderings a figure that could not be read must never have. A blank, a
/// zero, a dash and the three words a language leaks when nobody looked all read
/// to a person as a measurement that was taken.
const NOT_A_WORD = [
  /^$/,
  /^0%?$/,
  /^[-–—]+$/,
  /nan/i,
  /undefined/i,
  /null/i,
  /\[object/i,
];

function readsAsAMeasurement(text) {
  const trimmed = String(text === null || text === undefined ? "" : text).trim();
  return NOT_A_WORD.some((pattern) => pattern.test(trimmed));
}

/// Figures that claim a value where the state carried none.
function dishonestFigures(figures) {
  return Object.entries(figures)
    .filter(([, text]) => readsAsAMeasurement(text))
    .map(([what, text]) => ({ what, text }));
}

// --- state marks, and whether colour is doing the work -----------------------

/// Every mark on the page that says which state something is in, as the text a
/// person reads and whether it was painted at all.
async function stateMarks(page) {
  return page.evaluate(() => {
    const marks = {};
    const add = (id, el) => {
      if (!el) {
        marks[id] = { text: null, painted: false };
        return;
      }
      const box = el.getBoundingClientRect();
      marks[id] = {
        text: el.innerText.trim(),
        painted: box.width > 0 && box.height > 0,
      };
    };
    add("connection", document.querySelector("[data-connection]"));
    const each = (attribute, prefix) => {
      document.querySelectorAll(`[${attribute}]`).forEach((el) => {
        add(`${prefix}:${el.getAttribute(attribute)}`, el);
      });
    };
    each("data-freshness", "freshness");
    each("data-mute-state", "mute");
    each("data-volume", "volume");
    each("data-endpoints", "endpoints");
    return marks;
  });
}

/// Marks whose rendered text is the same in both states, so the only thing that
/// could be telling them apart is how they are coloured.
///
/// This is the measurement the colour-removed claim is graded with. It is taken
/// twice against a forced-colours rendering, once per state, and a mark that
/// reads identically in both is a mark a person who cannot see the hue cannot
/// read.
/// Freshness marks that still read as current. This is the measurement the
/// severed-feed claim turns on: a page whose feed has dropped and whose figures
/// still say `live` is the silent freeze the clause forbids.
function stillReadingAsCurrent(marks) {
  return Object.entries(marks)
    .filter(([id]) => id.startsWith("freshness:"))
    .filter(([, mark]) => mark.text !== "last known")
    .map(([id, mark]) => ({ mark: id, text: mark.text }));
}

function colourOnly(first, second, ids) {
  const problems = [];
  for (const id of ids) {
    const a = first[id];
    const b = second[id];
    if (!a || !b) {
      problems.push({ mark: id, why: "the mark was not on the page in both states" });
      continue;
    }
    if (!a.painted || !b.painted) {
      problems.push({ mark: id, why: "the mark was not painted in both states" });
      continue;
    }
    if (a.text === b.text) {
      problems.push({ mark: id, why: `both states read "${a.text}"` });
    }
  }
  return problems;
}

// --- labels, regions and the document a region links to ----------------------

/// Every run of text on the surface, with how many words it is.
///
/// Words are whitespace-separated tokens carrying at least one letter or digit,
/// so the separators a meta line uses are not counted against it.
async function surfaceText(page) {
  return page.evaluate(() => {
    const describe = (el) => {
      const bits = [el.tagName.toLowerCase()];
      for (const attribute of el.getAttributeNames()) {
        if (attribute.startsWith("data-")) {
          bits.push("[" + attribute + "]");
        }
      }
      return bits.join("");
    };
    const skip = new Set(["SCRIPT", "STYLE", "TITLE", "OPTION", "HEAD"]);
    const runs = [];
    document.querySelectorAll("*").forEach((el) => {
      if (skip.has(el.tagName)) {
        return;
      }
      let own = "";
      for (const child of el.childNodes) {
        if (child.nodeType === 3) {
          own += child.nodeValue;
        }
      }
      own = own.trim();
      if (!own) {
        return;
      }
      const box = el.getBoundingClientRect();
      if (box.width < 1 || box.height < 1) {
        return;
      }
      const words = own.split(/\s+/).filter((word) => /[a-z0-9]/i.test(word));
      runs.push({ what: describe(el), text: own.slice(0, 90), words: words.length });
    });
    return runs;
  });
}

function tooManyWords(runs, most) {
  return runs.filter((run) => run.words > most);
}

/// Every region the page marks out, and the links inside it.
async function regions(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll("[data-region]")).map((el) => {
      const box = el.getBoundingClientRect();
      return {
        region: el.getAttribute("data-region"),
        painted: box.width > 0 && box.height > 0,
        links: Array.from(el.querySelectorAll("a[href]")).map((a) => a.href),
      };
    })
  );
}

function regionsWithoutOneLink(rows) {
  return rows.filter((row) => row.painted && row.links.length !== 1);
}

/// Follow a link IN THE ENGINE and report what came back. A link to an
/// explanation that answers 404, or answers with nothing, is a dead link
/// whatever the markup says.
async function follow(page, href) {
  const response = await page.goto(href);
  const body = await page.evaluate(() => document.body.innerText.trim());
  return {
    href,
    status: response ? response.status() : 0,
    length: body.length,
    ok: Boolean(response) && response.status() === 200 && body.length > 200,
  };
}

// --- the keyboard -----------------------------------------------------------

/// Tab through the page and report where focus went, in order.
///
/// The index is the control's position in the document's own order, so a page
/// whose tab order does not match its reading order shows up as a sequence that
/// does not increase.
async function tabOrder(page, limit = 40) {
  await page.evaluate(() => {
    if (document.activeElement && document.activeElement.blur) {
      document.activeElement.blur();
    }
  });
  const visited = [];
  for (let i = 0; i < limit; i += 1) {
    await page.keyboard.press("Tab");
    const where = await page.evaluate((selector) => {
      const el = document.activeElement;
      if (!el || el === document.body || el === document.documentElement) {
        return null;
      }
      const all = Array.from(document.querySelectorAll(selector));
      return {
        index: all.indexOf(el),
        tag: el.tagName.toLowerCase(),
        label: (el.getAttribute("aria-label") || el.textContent || "").trim().slice(0, 40),
      };
    }, SELECTOR);
    if (!where || where.index < 0) {
      break;
    }
    if (visited.some((seen) => seen.index === where.index)) {
      break;
    }
    visited.push(where);
  }
  return visited;
}

/// Controls the keyboard never reached.
async function unreachableByKeyboard(page, order) {
  const total = await page.evaluate(
    (selector) => document.querySelectorAll(selector).length,
    SELECTOR
  );
  const reached = new Set(order.map((step) => step.index));
  const missed = [];
  for (let i = 0; i < total; i += 1) {
    if (!reached.has(i)) {
      missed.push(i);
    }
  }
  return missed;
}

function outOfReadingOrder(order) {
  const out = [];
  for (let i = 1; i < order.length; i += 1) {
    if (order[i].index < order[i - 1].index) {
      out.push({ after: order[i - 1], then: order[i] });
    }
  }
  return out;
}

// --- the narrow viewport ----------------------------------------------------

/// The page's own scroll geometry, and every control's box, at whatever
/// viewport the caller set.
async function reflow(page, minimum = 24) {
  return page.evaluate(
    ({ selector, minimum }) => {
      const describe = (el) => {
        const bits = [el.tagName.toLowerCase()];
        for (const attribute of el.getAttributeNames()) {
          if (attribute.startsWith("data-")) {
            bits.push("[" + attribute + "]");
          }
        }
        return bits.join("");
      };
      const doc = document.scrollingElement || document.documentElement;
      const controls = Array.from(document.querySelectorAll(selector)).map((el) => {
        const box = el.getBoundingClientRect();
        return {
          what: describe(el),
          label: (el.getAttribute("aria-label") || el.textContent || "").trim().slice(0, 40),
          left: box.left,
          right: box.right,
          width: box.width,
          height: box.height,
        };
      });
      return {
        viewport: window.innerWidth,
        scrollWidth: doc.scrollWidth,
        clientWidth: doc.clientWidth,
        bodyScrollWidth: document.body.scrollWidth,
        bodyClientWidth: document.body.clientWidth,
        outside: controls.filter(
          (c) => c.left < -0.5 || c.right > window.innerWidth + 0.5
        ),
        small: controls.filter((c) => c.width < minimum || c.height < minimum),
      };
    },
    { selector: SELECTOR, minimum }
  );
}

/// Whether an element that is too wide for the viewport scrolls INSIDE ITSELF
/// rather than dragging the page body sideways. Driven, not read: the element is
/// asked to scroll and then asked where it is.
async function scrollsInsideItself(page, selector) {
  return page.evaluate((selector) => {
    const el = document.querySelector(selector);
    if (!el) {
      return { found: false };
    }
    const doc = document.scrollingElement || document.documentElement;
    const wider = el.scrollWidth > el.clientWidth + 1;
    el.scrollLeft = 0;
    el.scrollLeft = 9999;
    const moved = el.scrollLeft;
    el.scrollLeft = 0;
    return {
      found: true,
      wider,
      moved,
      scrolls: wider ? moved > 0 : true,
      pageScrollWidth: doc.scrollWidth,
      pageClientWidth: doc.clientWidth,
    };
  }, selector);
}

// --- the refusal ------------------------------------------------------------

async function refusalsShown(page) {
  return page.evaluate(() => {
    const out = {};
    document.querySelectorAll("[data-refusal]").forEach((el) => {
      const box = el.getBoundingClientRect();
      out[el.getAttribute("data-refusal")] = {
        text: el.innerText.trim(),
        painted: box.width > 0 && box.height > 0,
      };
    });
    return out;
  });
}

function refusalSwallowed(refusals, zone) {
  const shown = refusals[zone];
  if (!shown) {
    return { swallowed: true, why: "there is no refusal region on that card" };
  }
  if (!shown.painted || shown.text.length === 0) {
    return { swallowed: true, why: "the refusal region is empty" };
  }
  return { swallowed: false, text: shown.text };
}

// --- whether the page under it ever went away -------------------------------
//
// A reload is invisible to the navigation timing buffer, which holds one entry
// for whatever document is current and starts again with the next one. So the
// document is MARKED instead: a value put on `window` and the time origin it was
// created with. Both survive any amount of re-rendering and neither survives a
// new document, which is exactly the difference the claim is about.

async function markDocument(page) {
  return page.evaluate(() => {
    window.__chorusDocumentMark = Math.random().toString(36).slice(2);
    return {
      mark: window.__chorusDocumentMark,
      origin: performance.timeOrigin,
    };
  });
}

async function documentSurvived(page, marked) {
  const now = await page.evaluate(() => ({
    mark: window.__chorusDocumentMark || null,
    origin: performance.timeOrigin,
  }));
  return {
    sameDocument: now.mark === marked.mark && now.origin === marked.origin,
    mark: now.mark,
    origin: now.origin,
    wanted: marked,
  };
}

module.exports = {
  SELECTOR,
  viewStates,
  statesShowing,
  renderedZones,
  zoneMismatches,
  isBareCount,
  statesItsSet,
  readsAsAMeasurement,
  dishonestFigures,
  stateMarks,
  stillReadingAsCurrent,
  colourOnly,
  surfaceText,
  tooManyWords,
  regions,
  regionsWithoutOneLink,
  follow,
  tabOrder,
  unreachableByKeyboard,
  outOfReadingOrder,
  reflow,
  scrollsInsideItself,
  refusalsShown,
  refusalSwallowed,
  markDocument,
  documentSurvived,
};
