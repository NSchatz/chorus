// The app's further screens and their addresses (the ADR of the navigation).
//
// The groups and the rooms are the app's home: the address the app is served
// at, with no fragment or with `#/`. Every other screen has an address of its
// own in the fragment, `#/` and a path:
//
//   /app/#/                      the groups and the rooms
//   /app/#/rooms/living/sound    the sound of the room "living"
//
// The fragment is the one part of the address that is the page's alone. The
// server serves one document at `/app/` and nothing under it but the app's
// files, so a path of its own would be the server's to answer; the query
// string is the kiosk's switch (mode.js); and the fragment is never sent, so
// the login in front of the server and the service worker see the same
// request for every screen.
//
// A screen registers itself here (`registerScreen`) and the shell
// (chorus-app.js) renders whichever one the address names. That is all a
// later screen does to be reachable: a module that registers
//
//   { id, path, title(params, view), render(params, context) }
//
//   id      the screen's name, unique
//   path    its address after `#/`: segments, a literal or `:name` for a
//           parameter ("rooms/:room/sound" for a room's screen, "alarms" for
//           a house-wide one)
//   title   the words the screen's region is labelled with
//   render  the screen, as a Lit template. `context` is what the shell holds:
//           { view, refusals, refusalFields, store }, the store's view, the
//           server's words (and the field it named) for the last refused
//           command of each subject, and the store itself, for a screen
//           that has to await the server's answers (room-correction.js)
//
// and a link to it anywhere under the shell, `<a href=${addressOf(id, params)}
// data-route>`: the shell opens it as an entry of the browser's history, so
// the browser's back button returns from it.

const screens = [];

const segmentsOf = (path) => String(path).split("/").filter(Boolean);

/** Register a screen. A second screen with an id or a path already taken is a mistake, and throws. */
export function registerScreen(screen) {
  const { id, path, title, render } = screen ?? {};
  if (typeof id !== "string" || !id || id === "home") throw new Error("a screen has an id, and it is not 'home'");
  if (typeof title !== "function" || typeof render !== "function") throw new Error(`the screen '${id}' has a title and a render`);
  const segments = segmentsOf(path);
  if (segments.length === 0) throw new Error(`the screen '${id}' has a path`);
  const shape = (parts) => parts.map((part) => (part.startsWith(":") ? ":" : part)).join("/");
  for (const other of screens) {
    if (other.id === id) throw new Error(`the screen '${id}' is registered twice`);
    if (shape(other.segments) === shape(segments)) throw new Error(`the screens '${other.id}' and '${id}' have the same path`);
  }
  screens.push({ id, segments, title, render });
}

/** The registered screen with this id, or null. */
export function screenOf(id) {
  return screens.find((screen) => screen.id === id) ?? null;
}

/** The address of the home: the groups and the rooms. */
export const HOME_ADDRESS = "#/";

/** The route of the home. */
export const HOME = Object.freeze({ screen: "home", params: Object.freeze({}), address: HOME_ADDRESS });

/**
 * The address of a registered screen, as a fragment a link can hold.
 *
 * @param {string} id the screen
 * @param {Record<string, string>} [params] a value for each parameter of its path
 * @returns {string} for example `#/rooms/living/sound`
 */
export function addressOf(id, params = {}) {
  const screen = screenOf(id);
  if (!screen) throw new Error(`there is no screen '${id}'`);
  const parts = screen.segments.map((segment) => {
    if (!segment.startsWith(":")) return segment;
    const value = params[segment.slice(1)];
    if (typeof value !== "string" || !value) throw new Error(`the screen '${id}' needs '${segment.slice(1)}'`);
    return encodeURIComponent(value);
  });
  return `#/${parts.join("/")}`;
}

/**
 * The route a fragment names. Pure. A fragment that names no registered
 * screen (none, `#/`, a mistyped one, one of a screen a later version adds)
 * is the home.
 *
 * @param {string} hash a location's fragment, with or without its `#`
 * @returns {{ screen: string, params: Record<string, string>, address: string }}
 */
export function routeOf(hash) {
  let parts;
  try {
    parts = segmentsOf(String(hash ?? "").replace(/^#/, "")).map((part) => decodeURIComponent(part));
  } catch {
    return HOME;
  }
  for (const screen of screens) {
    if (screen.segments.length !== parts.length) continue;
    const params = {};
    const matches = screen.segments.every((segment, at) => {
      if (!segment.startsWith(":")) return segment === parts[at];
      params[segment.slice(1)] = parts[at];
      return true;
    });
    if (matches) return { screen: screen.id, params, address: addressOf(screen.id, params) };
  }
  return HOME;
}

/**
 * The navigation over a browser's address and history.
 *
 *   route()         the route the address names now
 *   open(address)   go to a screen's address as a new entry of the history,
 *                   so the browser's back button returns from it
 *   back()          leave a further screen for the home: one step back in the
 *                   history when the entry below is the one this app opened
 *                   the screen from, else (the screen was the address the app
 *                   was loaded at, or was typed) the home in its place, so
 *                   the app never steps out of itself
 *   watch(onChange) `onChange` is called with the route now, at once, and at
 *                   every change: the app's own, the browser's back and
 *                   forward buttons, an address edited by hand. Returns the
 *                   function that stops it.
 *
 * An entry `open` makes carries a mark in its history state, which the
 * browser keeps with the entry through back, forward and a reload: that mark
 * is how `back` knows what is below.
 *
 * @param {{ location?: Location, history?: History, addEventListener?: Function, removeEventListener?: Function }} [host]
 */
export function createNavigation(host = globalThis) {
  const watchers = new Set();
  const route = () => routeOf(host.location?.hash ?? "");
  const tell = () => {
    const now = route();
    for (const watcher of [...watchers]) watcher(now);
  };
  return {
    route,
    open(address) {
      const to = routeOf(address);
      if (to.address === route().address) return;
      host.history.pushState({ chorus: true }, "", to.address);
      tell();
    },
    back() {
      if (route().screen === "home") return;
      if (host.history.state?.chorus === true) {
        // The browser says when it has stepped back (popstate), and the
        // watchers are told then.
        host.history.back();
        return;
      }
      host.history.replaceState(null, "", HOME_ADDRESS);
      tell();
    },
    watch(onChange) {
      const watcher = (now) => onChange(now);
      if (watchers.size === 0) {
        host.addEventListener?.("popstate", tell);
        host.addEventListener?.("hashchange", tell);
      }
      watchers.add(watcher);
      watcher(route());
      return () => {
        watchers.delete(watcher);
        if (watchers.size === 0) {
          host.removeEventListener?.("popstate", tell);
          host.removeEventListener?.("hashchange", tell);
        }
      };
    },
  };
}
