// Keeping a wall tablet's screen on (K86: always on), with the Screen Wake
// Lock API.
//
// The browser gives a page a screen wake lock only while the page is visible
// and takes it back when the page is hidden (another tab, the screen switched
// off by hand, the tablet's own lock). It does not give it back when the page
// is shown again: the page asks again. So this asks when it starts, if the
// page is visible, and again each time the page becomes visible.
//
// It never throws and never rejects, and the app does not depend on it:
//   no API       navigator.wakeLock is there only in a secure context (HTTPS
//                or the loopback address) and only in a browser that has it.
//                Without it this does nothing, and says so (`supported`).
//   refused      the browser may refuse a request (a battery saver, a policy).
//                The screen then sleeps as the tablet is set to; the next
//                time the page becomes visible this asks again.

/**
 * @param {{ navigator?: object, document?: object }} [host] the browser's
 *   navigator and document; a test gives its own
 * @returns {{
 *   supported: boolean,
 *   held: () => boolean,
 *   settled: () => Promise<void>,
 *   stop: () => Promise<void>,
 * }} `supported`: whether there is a wake lock to ask for; `held()`: whether
 *   the screen is held on now; `settled()`: resolves when the request being
 *   made, if any, has been answered; `stop()`: let the screen sleep again and
 *   ask no more
 */
export function keepAwake({ navigator = globalThis.navigator, document = globalThis.document } = {}) {
  let wakeLock = null;
  try {
    wakeLock = navigator?.wakeLock ?? null;
  } catch {
    wakeLock = null;
  }
  if (!wakeLock || typeof wakeLock.request !== "function" || typeof document?.addEventListener !== "function") {
    return { supported: false, held: () => false, settled: async () => {}, stop: async () => {} };
  }

  let sentinel = null;
  let asking = null;
  let stopped = false;

  const release = async (lock) => {
    try {
      await lock.release();
    } catch {
      // Already released: there is nothing left to give back.
    }
  };

  const ask = () => {
    if (stopped || sentinel || asking || document.visibilityState !== "visible") return;
    asking = (async () => {
      try {
        const lock = await wakeLock.request("screen");
        if (stopped) {
          await release(lock);
          return;
        }
        sentinel = lock;
        // The browser took it back (the page was hidden): hold nothing, so
        // the next time the page is visible this asks again.
        lock.addEventListener?.("release", () => {
          if (sentinel === lock) sentinel = null;
        });
      } catch {
        // Refused. The app goes on without; see the header.
      } finally {
        asking = null;
      }
    })();
  };

  document.addEventListener("visibilitychange", ask);
  ask();

  return {
    supported: true,
    held: () => sentinel !== null && sentinel.released !== true,
    settled: async () => {
      while (asking) await asking;
    },
    stop: async () => {
      stopped = true;
      document.removeEventListener("visibilitychange", ask);
      while (asking) await asking;
      const lock = sentinel;
      sentinel = null;
      if (lock) await release(lock);
    },
  };
}
