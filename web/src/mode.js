// Which mode the app runs in: the ordinary app, or the kiosk of a wall tablet
// (K86: always on, big touch targets, no browser chrome).
//
// The switch is the page's query string. A wall tablet is opened once with
// `?kiosk` and is a kiosk from then on: the choice is kept in this browser's
// storage, so it survives a reload, a restart of the browser and the installed
// app's start address, which carries no query. `?kiosk=0` (or `=false`)
// switches it off again, and that is kept too.
//
//   /app/?kiosk      kiosk, and remembered
//   /app/?kiosk=0    the ordinary app, and the memory cleared
//   /app/            whatever was last chosen here; the ordinary app if nothing was
//
// Storage is the browser's to refuse (a private window, a policy): the query
// string then still decides the page it is on, and nothing is remembered.

/** The two modes. */
export const MODES = Object.freeze(["app", "kiosk"]);

/** The storage key the kiosk choice is kept under, and the value it has. */
export const KIOSK_KEY = "chorus.kiosk";
const KIOSK_ON = "1";

/**
 * What a query string says about the kiosk. Pure.
 *
 * @param {string} search a location's query string, with or without its `?`
 * @returns {"kiosk" | "app" | null} `kiosk` when the query carries `kiosk`,
 *   bare or with any value but `0` or `false`; `app` when it carries one of
 *   those two; `null` when it does not name the kiosk at all
 */
export function kioskSwitch(search) {
  const value = new URLSearchParams(search).get("kiosk");
  if (value === null) return null;
  return value === "0" || value === "false" ? "app" : "kiosk";
}

/**
 * The mode a query string alone asks for. Pure: it takes the query string
 * and touches nothing.
 *
 * @param {string} search a location's query string, with or without its `?`
 * @returns {"app" | "kiosk"}
 */
export function displayMode(search) {
  return kioskSwitch(search) ?? "app";
}

/**
 * The mode the page runs in: what its query string says, kept; or, when the
 * query string says nothing, what was kept before.
 *
 * @param {string} search a location's query string
 * @param {Storage | null | undefined} storage where the choice is kept
 *   (localStorage); it may be absent and it may throw
 * @returns {"app" | "kiosk"}
 */
export function resolveMode(search, storage) {
  const asked = kioskSwitch(search);
  try {
    if (asked === "kiosk") storage?.setItem(KIOSK_KEY, KIOSK_ON);
    else if (asked === "app") storage?.removeItem(KIOSK_KEY);
    else return storage?.getItem(KIOSK_KEY) === KIOSK_ON ? "kiosk" : "app";
  } catch {
    // Storage refused: the query string still decides this page.
  }
  return asked ?? "app";
}

/**
 * The browser's storage, or null where reading the property itself throws.
 *
 * @param {object} [host]
 * @returns {Storage | null}
 */
export function storageOf(host = globalThis) {
  try {
    return host.localStorage ?? null;
  } catch {
    return null;
  }
}
