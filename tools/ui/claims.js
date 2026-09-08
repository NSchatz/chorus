// The claims this check makes, and the ledger of what actually ran.
//
// A claim is one thing the rendered page is asserted to do. Every claim is
// declared here by name, every assertion that proves one records it, and every
// demonstration that shows the SAME measuring code going red records it too. The
// runner then refuses on three things it could not otherwise see:
//
//   a claim that stopped running, because a deleted test is invisible from
//   inside a green test run;
//   a claim with no demonstration, because a check that cannot fail is not
//   evidence;
//   a demonstration recorded from the file that holds the assertions, because
//   `demonstrates()` beside the real page counts without demonstrating
//   anything;
//   a clause of the frontend conventions whose committed record names an
//   assertion that did not run in this invocation.
//
// The ledger is a file rather than a return value because the two spec files run
// in one worker and the runner is a shell script; a line appended per assertion
// is the smallest thing both can read. Each line carries the file it was written
// from, which is what makes the third refusal above possible.

const fs = require("fs");
const path = require("path");

/// Every rendered claim, by name, with the acceptance criterion it answers.
const CLAIMS = {
  "zones-shown":
    "every configured zone is shown by its name, volume, mute state and endpoint figure",
  "live-update":
    "another subscriber's change appears with no reload of the page",
  "target-size":
    "every interactive control is painted at 24 by 24 CSS pixels or more",
  "contrast-in-both-themes":
    "every text run and every non-text mark clears its contrast floor, in both themes, measured from painted pixels",
  "keyboard-operation":
    "every control is reachable by Tab in reading order and operable to the same effect a pointer has",
  "focus-visible":
    "the control with focus is painted differently, and the indicator clears 3:1, in both themes",
  "accessible-names":
    "every control has an accessible name saying what it does and which zone, and mute is a pressed state",
  "not-colour-alone":
    "every state the page distinguishes survives a rendering with the colour taken out",
  "aggregate-states-its-set":
    "the endpoint figure says which rows it counted and how many it left out",
  "unreadable-figure":
    "a figure that cannot be read renders as unavailable in words while the rest of the view draws",
  "stale-not-current":
    "a severed feed reads as lost within ten seconds and every figure as last known, and a restored one clears it",
  "paused-feed-visible":
    "a feed that has stopped delivering over a connection that is still established stops reading as live, and reads as live again when it resumes",
  "three-states":
    "loading, empty and error are distinct, actionable, and never two at once",
  "loading-never-unresolved":
    "a state request that never answers resolves the loading notice into the actionable error notice, inside a bounded time and with no interaction",
  "short-labels-and-doc-link":
    "labels stay to a few words and each region links once to a document that resolves",
  "reflow-360":
    "at 360 by 640 the body does not scroll sideways, controls stay inside it and stay big enough",
  "themes-follow-preference":
    "a light preference renders a light page and a dark one renders a dark page",
  "csp-clean":
    "the server sends a Content-Security-Policy, the browser reports no violation and everything still works",
  "refusal-shown":
    "a refused command is rendered where it was issued and the control keeps the state the server holds",
  "rendered-by-a-real-engine":
    "every claim above was measured on a page a browser engine painted",
};

const LEDGER =
  process.env.CHORUS_UI_LEDGER ||
  path.join(__dirname, ".playwright", "claims.ledger");

/// The file that called `proves` or `demonstrates`, as a bare name.
///
/// The ledger records it because a line reading only `demo three-states` cannot
/// say WHERE the demonstration was: `proves` and `demonstrates` are both
/// exported to both spec files, so a `demonstrates()` call sitting in
/// `ui.spec.js` beside the real page would satisfy the counter while
/// demonstrating nothing. With the file on the line, check-claims.js can refuse
/// it. Read off V8's structured stack rather than passed in by the caller,
/// because a value the caller supplies is one the caller can get wrong.
function callerFile() {
  const before = Error.prepareStackTrace;
  Error.prepareStackTrace = (_, frames) => frames;
  const frames = new Error().stack;
  Error.prepareStackTrace = before;
  if (!Array.isArray(frames)) {
    return "unknown";
  }
  for (const frame of frames) {
    const file = typeof frame.getFileName === "function" ? frame.getFileName() : null;
    if (file && path.resolve(file) !== __filename) {
      return path.basename(file);
    }
  }
  return "unknown";
}

function append(kind, id) {
  if (!Object.prototype.hasOwnProperty.call(CLAIMS, id)) {
    throw new Error(
      `"${id}" is not a declared claim; add it to tools/ui/claims.js or fix the name`
    );
  }
  fs.mkdirSync(path.dirname(LEDGER), { recursive: true });
  fs.appendFileSync(LEDGER, `${kind} ${id} ${callerFile()}\n`);
}

/// Called at the END of an assertion that proved a claim on the real page. At
/// the end, so that a failed assertion never records one.
function proves(id) {
  append("claim", id);
}

/// Called at the end of a demonstration: the same measuring code, pointed at a
/// page that breaks exactly this claim, shown reporting the break.
function demonstrates(id) {
  append("demo", id);
}

/// Where each kind of line is allowed to have been written. An assertion is
/// graded against the page a real server served and belongs in ui.spec.js; a
/// demonstration is the same measurement pointed at a page built to break one
/// claim and belongs in mutation.spec.js. Recording one from the other file is
/// the mistake this mapping exists to make visible.
const RECORDED_IN = { claim: "ui.spec.js", demo: "mutation.spec.js" };

/// Read the ledger back.
function readLedger(file = LEDGER) {
  if (!fs.existsSync(file)) {
    return { claims: [], demos: [], misfiled: [] };
  }
  const claims = new Set();
  const demos = new Set();
  const misfiled = [];
  for (const line of fs.readFileSync(file, "utf8").split("\n")) {
    const [kind, id, from] = line.trim().split(/\s+/);
    if (kind !== "claim" && kind !== "demo") {
      continue;
    }
    if (!id) {
      continue;
    }
    if (from !== RECORDED_IN[kind]) {
      misfiled.push({ kind, id, from: from || "nowhere" });
      continue;
    }
    if (kind === "claim") {
      claims.add(id);
    } else {
      demos.add(id);
    }
  }
  return { claims: [...claims].sort(), demos: [...demos].sort(), misfiled };
}

module.exports = { CLAIMS, LEDGER, RECORDED_IN, proves, demonstrates, readLedger };
