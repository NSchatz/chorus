// Which of the two layouts the app paints, from the width of the viewport.
//
//   phone     one column: the groups, then the rooms, and the navigation in a
//             bar fixed to the bottom edge, where a thumb reaches it
//   desktop   two columns: the groups, with what each plays, beside the rooms,
//             and the navigation in the header
//
// There is one breakpoint, DESKTOP_MIN_EM, and it is written here and nowhere
// else. A media query cannot read a custom property, so the breakpoint cannot
// be a token of tokens.css; and a literal length in an element's styles is
// what the app's rules refuse. So the element asks this module, which asks
// the browser with matchMedia, and the styles select on the attribute the
// element reflects (`layout="phone"`, `layout="desktop"`).
//
// The breakpoint is in em, which in a media query is the browser's initial
// font size (16 CSS pixels unless a person has changed it): 48em is 768 CSS
// pixels by default, and it moves with the text size a person asks for, so
// the two columns are never narrower in characters than they were designed.
//
// There is no television layout (K86).

/** The two layouts, the narrow one first. */
export const LAYOUTS = Object.freeze(["phone", "desktop"]);

/** The least viewport width, in em, that is painted as the desktop layout. */
export const DESKTOP_MIN_EM = 48;

/** The media query that is true of a viewport the desktop layout is for. */
export const DESKTOP_QUERY = `(min-width: ${DESKTOP_MIN_EM}em)`;

/**
 * @param {number} width a viewport's width, in em of the initial font size
 * @returns {"phone" | "desktop"} the layout that width is painted in
 */
export function layoutFor(width) {
  return width >= DESKTOP_MIN_EM ? "desktop" : "phone";
}

/**
 * Follow the viewport: `onChange` is called with the layout now, at once,
 * and again each time the viewport crosses the breakpoint.
 *
 * Where there is no matchMedia the layout is the phone's and stays so: one
 * column fits every width.
 *
 * @param {(layout: "phone" | "desktop") => void} onChange
 * @param {{ matchMedia?: (query: string) => MediaQueryList }} [host]
 * @returns {() => void} stop following
 */
export function watchLayout(onChange, host = globalThis) {
  if (typeof host?.matchMedia !== "function") {
    onChange("phone");
    return () => {};
  }
  const query = host.matchMedia(DESKTOP_QUERY);
  const tell = () => onChange(query.matches ? "desktop" : "phone");
  query.addEventListener("change", tell);
  tell();
  return () => query.removeEventListener("change", tell);
}
