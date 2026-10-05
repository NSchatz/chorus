// Which layout the app paints, read from the page's query string. A wall
// tablet opens the app with `?kiosk` (K86: always on, big touch targets, no
// browser chrome); every other place gets the ordinary layout. Pure: it takes
// the query string and touches nothing.

/** The two layouts. */
export const MODES = Object.freeze(["app", "kiosk"]);

/**
 * @param {string} search a location's query string, with or without its `?`
 * @returns {"app" | "kiosk"} `kiosk` when the query carries `kiosk`, bare or
 *   with any value but `0` or `false`; `app` otherwise
 */
export function displayMode(search) {
  const value = new URLSearchParams(search).get("kiosk");
  if (value === null || value === "0" || value === "false") return "app";
  return "kiosk";
}
