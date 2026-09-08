// Contrast, measured off the framebuffer.
//
// The colour a text run is PAINTED IN comes from the engine's computed style,
// which is the used value after the cascade, inheritance, the media queries that
// matched and the UA sheet have all had their say. The colour it is painted ON
// comes from the screenshot: the commonest colour inside the run's own box,
// which is the surface actually behind it whatever put it there. Neither number
// is read from a stylesheet, and no stylesheet is opened by this file.
//
// The floors are WCAG 2.2 AA: 4.5:1 for text, 3:1 for text at 24px or at
// 18.66px bold, and 3:1 for the non-text things a person has to see in order to
// operate a control. `work/specs/.../sources/www.w3.org-TR-WCAG22` on the
// umbrella side is the text those numbers come from (1.4.3 Contrast (Minimum),
// 1.4.11 Non-text Contrast).

const pixels = require("./pixels");

const TEXT_FLOOR = 4.5;
const LARGE_TEXT_FLOOR = 3.0;
const NON_TEXT_FLOOR = 3.0;

const INTERACTIVE =
  'button, select, input, textarea, a[href], [role="button"], [role="switch"], [tabindex]:not([tabindex="-1"])';

/// Every run of text the page actually painted, with the colour it was painted
/// in and the box it occupies.
async function textRuns(page) {
  return page.evaluate(() => {
    const runs = [];
    const skip = new Set(["SCRIPT", "STYLE", "TITLE", "OPTION", "HEAD"]);
    const describe = (el) => {
      const bits = [el.tagName.toLowerCase()];
      for (const attribute of el.getAttributeNames()) {
        if (attribute.startsWith("data-")) {
          bits.push(`[${attribute}]`);
        }
      }
      return bits.join("");
    };
    const painted = (el) => {
      const style = window.getComputedStyle(el);
      if (
        style.display === "none" ||
        style.visibility === "hidden" ||
        Number(style.opacity) === 0
      ) {
        return null;
      }
      const box = el.getBoundingClientRect();
      if (box.width < 2 || box.height < 2) {
        return null;
      }
      if (
        box.bottom < 0 ||
        box.right < 0 ||
        box.top > window.innerHeight ||
        box.left > window.innerWidth
      ) {
        return null;
      }
      return { style, box };
    };
    const push = (el, text) => {
      const shown = painted(el);
      if (!shown) {
        return;
      }
      const weight = Number(shown.style.fontWeight) || 400;
      const size = parseFloat(shown.style.fontSize);
      runs.push({
        what: describe(el),
        text: text.trim().slice(0, 60),
        colour: shown.style.color,
        fontSize: size,
        fontWeight: weight,
        large: size >= 24 || (size >= 18.66 && weight >= 700),
        rect: {
          x: shown.box.x,
          y: shown.box.y,
          width: shown.box.width,
          height: shown.box.height,
        },
      });
    };

    document.querySelectorAll("*").forEach((el) => {
      if (skip.has(el.tagName)) {
        return;
      }
      // A select renders the selected option's text itself; an input renders
      // its value. Neither has a text node to find, and both are text a person
      // reads.
      if (el.tagName === "SELECT") {
        const option = el.selectedOptions && el.selectedOptions[0];
        if (option && option.textContent.trim()) {
          push(el, option.textContent);
        }
        return;
      }
      if (el.tagName === "INPUT") {
        if (el.type === "text" && el.value.trim()) {
          push(el, el.value);
        }
        return;
      }
      let own = "";
      for (const child of el.childNodes) {
        if (child.nodeType === 3) {
          own += child.nodeValue;
        }
      }
      if (own.trim()) {
        push(el, own);
      }
    });
    return runs;
  });
}

function parseColour(css) {
  const numbers = css.match(/[\d.]+/g);
  if (!numbers || numbers.length < 3) {
    return null;
  }
  return [Number(numbers[0]), Number(numbers[1]), Number(numbers[2])];
}

/// Grade every text run on the page against its floor.
///
/// Returns one row per run, so a caller can report the offenders by name rather
/// than saying that something, somewhere, was too pale.
async function textContrast(page) {
  const runs = await textRuns(page);
  const shot = pixels.decodePng(await page.screenshot());
  return runs.map((run) => {
    const ink = parseColour(run.colour);
    // The surface the run is painted on: the commonest colour inside its own
    // box. Glyphs never make up the majority of a text box, and this answers
    // the question the criterion asks - what is BEHIND it - without having to
    // guess which ancestor supplied it.
    const surface = pixels.dominant(pixels.inside(shot, run.rect));
    const floor = run.large ? LARGE_TEXT_FLOOR : TEXT_FLOOR;
    if (!ink || !surface) {
      return { ...run, ratio: 0, floor, surface: null, ok: false };
    }
    const ratio = pixels.contrast(ink, surface.colour);
    return {
      ...run,
      floor,
      ratio,
      ink: pixels.hex(ink),
      surface: pixels.hex(surface.colour),
      ok: ratio >= floor - 0.005,
    };
  });
}

function tooPale(rows) {
  return rows.filter((row) => !row.ok);
}

/// Where every interactive control is, and what kind it is.
async function controlBoxes(page) {
  return page.$$eval(INTERACTIVE, (nodes) =>
    nodes.map((node) => {
      const box = node.getBoundingClientRect();
      return {
        tag: node.tagName.toLowerCase(),
        type: node.getAttribute("type") || "",
        label: (node.getAttribute("aria-label") || node.textContent || "")
          .trim()
          .slice(0, 40),
        rect: { x: box.x, y: box.y, width: box.width, height: box.height },
      };
    })
  );
}

/// Grade the non-text things a person has to see to work a control: the
/// boundary that says where the control is, and, on a slider, the track it runs
/// along and the thumb that carries the value.
///
/// Every number comes out of the screenshot. A border declared in the stylesheet
/// and then painted transparent measures transparent here, which is the case the
/// slider in chorus.css is written around.
async function nonTextContrast(page) {
  const controls = await controlBoxes(page);
  await page.evaluate(() => {
    if (document.activeElement && document.activeElement.blur) {
      document.activeElement.blur();
    }
  });
  const shot = pixels.decodePng(await page.screenshot());
  const rows = [];
  for (const control of controls) {
    const rect = control.rect;
    if (control.tag === "a") {
      // A link has no boundary to measure, and drawing one round it would be a
      // box nobody wants. What identifies it is its ink, which the text check
      // above already holds to 4.5:1, and its underline, which linkMarks reads
      // out of the framebuffer.
      continue;
    }
    if (rect.width < 2 || rect.height < 2) {
      rows.push({ ...control, kind: "unpainted", ratio: 0, ok: false });
      continue;
    }
    const outside = pixels.dominant(pixels.frame(shot, rect, 2, 6));
    if (!outside) {
      rows.push({ ...control, kind: "no-surround", ratio: 0, ok: false });
      continue;
    }
    if (control.tag === "input" && control.type === "range") {
      // A slider's boundary is not its border box. What a person has to see is
      // the track, and then the thumb against the track.
      const middle = Math.round(rect.y + rect.height / 2);
      const line = pixels.alongRow(shot, middle, rect.x + 1, rect.x + rect.width - 1);
      const track = pixels.dominant(line);
      let thumb = null;
      let best = 0;
      for (const colour of line) {
        const ratio = pixels.contrast(colour, track.colour);
        if (ratio > best) {
          best = ratio;
          thumb = colour;
        }
      }
      const trackRatio = pixels.contrast(track.colour, outside.colour);
      rows.push({
        ...control,
        kind: "slider",
        surround: pixels.hex(outside.colour),
        track: pixels.hex(track.colour),
        thumb: thumb ? pixels.hex(thumb) : null,
        trackRatio,
        thumbRatio: best,
        ratio: Math.min(trackRatio, best),
        ok: trackRatio >= NON_TEXT_FLOOR - 0.005 && best >= NON_TEXT_FLOOR - 0.005,
      });
      continue;
    }
    // A boundary is one pixel wide, so a ring averaged over a perimeter loses
    // it to the surface on either side. What is measured instead is a short
    // probe crossing the edge at the middle of a side, where no corner radius
    // reaches: the most distinguishable thing painted anywhere along that probe,
    // against the surface just outside it. A control with a border reports the
    // border; a control with no border but a fill of its own reports the fill; a
    // control that is neither reports 1:1, which is what invisible looks like.
    const cx = Math.round(rect.x + rect.width / 2);
    const cy = Math.round(rect.y + rect.height / 2);
    const probes = [
      {
        where: "top",
        outside: pixels.at(shot, cx, rect.y - 4),
        line: pixels.alongColumn(shot, cx, rect.y - 2, rect.y + 4),
      },
      {
        where: "left",
        outside: pixels.at(shot, rect.x - 4, cy),
        line: pixels.alongRow(shot, cy, rect.x - 2, rect.x + 4),
      },
    ];
    let best = 0;
    let drawn = null;
    let over = null;
    let where = null;
    for (const probe of probes) {
      for (const colour of probe.line) {
        const ratio = pixels.contrast(colour, probe.outside);
        if (ratio > best) {
          best = ratio;
          drawn = colour;
          over = probe.outside;
          where = probe.where;
        }
      }
    }
    rows.push({
      ...control,
      kind: "boundary",
      surround: over ? pixels.hex(over) : pixels.hex(outside.colour),
      edge: drawn ? pixels.hex(drawn) : null,
      where,
      ratio: best,
      ok: best >= NON_TEXT_FLOOR - 0.005,
    });
  }
  return rows;
}

/// Whether a link carries a mark that is not its colour.
///
/// A link in a bar of other text is identified by its underline as much as by
/// its ink, and a page that dropped the underline would be telling a person who
/// cannot pick the hue out that there is nothing there. Measured from the
/// framebuffer: the longest unbroken run of the link's own colour inside its
/// box, against the box's width. An underline runs the width of the text; the
/// longest run inside a glyph is a crossbar.
async function linkMarks(page) {
  const links = await page.$$eval("a[href]", (nodes) =>
    nodes.map((node) => {
      const box = node.getBoundingClientRect();
      const style = window.getComputedStyle(node);
      return {
        label: (node.textContent || "").trim().slice(0, 40),
        colour: style.color,
        rect: { x: box.x, y: box.y, width: box.width, height: box.height },
      };
    })
  );
  const shot = pixels.decodePng(await page.screenshot());
  return links.map((link) => {
    const ink = parseColour(link.colour);
    const run = ink ? pixels.longestRunOf(shot, link.rect, ink) : 0;
    const coverage = run / Math.max(1, link.rect.width);
    return {
      ...link,
      ink: ink ? pixels.hex(ink) : null,
      run,
      coverage,
      ok: coverage >= 0.5,
    };
  });
}

function unmarkedLinks(rows) {
  return rows.filter((row) => !row.ok);
}

function indistinct(rows) {
  return rows.filter((row) => !row.ok);
}

/// What a control's focus indicator does to the pixels around it.
///
/// The control is screenshotted with nothing focused and then with it focused,
/// over a region a few pixels larger than the control so that an indicator drawn
/// OUTSIDE the border box is inside the comparison. A control whose indicator
/// was suppressed changes nothing and is reported by name.
async function focusIndicators(page, pad = 8) {
  const handles = await page.$$(INTERACTIVE);
  const rows = [];
  for (const handle of handles) {
    const label = await handle.evaluate((node) =>
      (node.getAttribute("aria-label") || node.textContent || node.tagName)
        .trim()
        .slice(0, 40)
    );
    const box = await handle.boundingBox();
    if (!box) {
      rows.push({ label, pixelsChanged: 0, ratio: 0, ok: false, why: "not painted" });
      continue;
    }
    const clip = {
      x: Math.max(0, box.x - pad),
      y: Math.max(0, box.y - pad),
      width: box.width + 2 * pad,
      height: box.height + 2 * pad,
    };
    await page.evaluate(() => {
      if (document.activeElement && document.activeElement.blur) {
        document.activeElement.blur();
      }
    });
    const before = pixels.decodePng(await page.screenshot({ clip }));
    await handle.evaluate((node) => node.focus({ preventScroll: true }));
    const after = pixels.decodePng(await page.screenshot({ clip }));
    await page.evaluate(() => {
      if (document.activeElement && document.activeElement.blur) {
        document.activeElement.blur();
      }
    });
    const report = pixels.difference(before, after);
    rows.push({
      label,
      ...report,
      ok: report.pixelsChanged >= 24 && report.ratio >= NON_TEXT_FLOOR - 0.005,
    });
  }
  return rows;
}

function unfocusable(rows) {
  return rows.filter((row) => !row.ok);
}

module.exports = {
  INTERACTIVE,
  TEXT_FLOOR,
  LARGE_TEXT_FLOOR,
  NON_TEXT_FLOOR,
  textRuns,
  textContrast,
  tooPale,
  controlBoxes,
  nonTextContrast,
  indistinct,
  linkMarks,
  unmarkedLinks,
  focusIndicators,
  unfocusable,
};
