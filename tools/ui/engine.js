// Two things only an engine can tell you.
//
// The first is that a page was PAINTED at all. Every other measurement in this
// directory would be just as happy on a page that rendered nothing, so this one
// asks the compositor for evidence: how many distinct colours came out, and
// whether the colour actually in the framebuffer where an element sits is the
// colour the engine computed for it. A page that was never drawn has one colour
// and no agreement.
//
// The second is what the BROWSER thought of the Content-Security-Policy it was
// sent. A policy is not a header to read back; it is a set of decisions the
// browser makes about loads it was asked to perform, and the only honest report
// of it is the browser's own violation events.

const pixels = require("./pixels");

function hexOf(css) {
  const numbers = String(css || "").match(/[\d.]+/g);
  if (!numbers || numbers.length < 3) {
    return null;
  }
  return pixels.hex([Number(numbers[0]), Number(numbers[1]), Number(numbers[2])]);
}

/// Evidence that a browser engine drew this page.
async function paintEvidence(page, selector = "body") {
  const shot = pixels.decodePng(await page.screenshot());
  const where = await page.evaluate((selector) => {
    const el = document.querySelector(selector) || document.body;
    const box = el.getBoundingClientRect();
    return {
      background: window.getComputedStyle(el).backgroundColor,
      rect: { x: box.x, y: box.y, width: box.width, height: box.height },
    };
  }, selector);
  const dominant = pixels.dominant(pixels.inside(shot, where.rect));
  const painted = dominant ? pixels.hex(dominant.colour) : null;
  return {
    colours: pixels.distinctColours(shot),
    computed: where.background,
    computedHex: hexOf(where.background),
    painted,
    agrees: painted !== null && painted === hexOf(where.background),
  };
}

/// The surface a region was painted on, straight out of the framebuffer, with
/// its relative luminance. This is what decides whether a theme is light or
/// dark: not which media query is in the file, but how bright the pixels are.
async function surfaceLuminance(page, selector) {
  const shot = pixels.decodePng(await page.screenshot());
  const rect = await page.evaluate((selector) => {
    const el = document.querySelector(selector);
    if (!el) {
      return null;
    }
    const box = el.getBoundingClientRect();
    return { x: box.x, y: box.y, width: box.width, height: box.height };
  }, selector);
  if (!rect) {
    return null;
  }
  const dominant = pixels.dominant(pixels.inside(shot, rect));
  if (!dominant) {
    return null;
  }
  return {
    selector,
    colour: pixels.hex(dominant.colour),
    luminance: pixels.luminance(dominant.colour),
  };
}

/// Watch what the browser makes of the policy it is sent. Must be installed
/// BEFORE the page is loaded, because a violation is an event and an event
/// nobody was listening for is a violation nobody can report.
async function watchPolicy(page) {
  const responses = [];
  page.on("response", (response) => {
    responses.push({
      url: response.url(),
      status: response.status(),
      policy: response.headers()["content-security-policy"] || null,
    });
  });
  const consoleErrors = [];
  page.on("console", (message) => {
    if (/content security policy/i.test(message.text())) {
      consoleErrors.push(message.text());
    }
  });
  await page.addInitScript(() => {
    window.__chorusPolicyViolations = [];
    document.addEventListener("securitypolicyviolation", (event) => {
      window.__chorusPolicyViolations.push({
        directive: event.effectiveDirective || event.violatedDirective,
        blocked: event.blockedURI,
        from: event.sourceFile || "",
      });
    });
  });
  return {
    responses,
    consoleErrors,
    violations: async () =>
      page.evaluate(() => window.__chorusPolicyViolations || []),
    policyFor: (pattern) => {
      const match = responses.find((r) => r.url.includes(pattern));
      return match ? match.policy : undefined;
    },
  };
}

module.exports = { hexOf, paintEvidence, surfaceLuminance, watchPolicy };
