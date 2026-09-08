// The pixels the engine actually painted.
//
// Everything in this file starts from a screenshot taken by the browser that
// drew the page, so what it reports is what came out of the compositor and not
// what any stylesheet declared. That distinction is the whole point: a declared
// colour can lose the cascade, sit under a media query that did not match, be
// composited over something else, or be painted by a UA rule nobody wrote. Only
// the framebuffer knows.
//
// The PNG decoder is here rather than pulled in as a dependency because it is
// eighty lines against a format Chromium emits in exactly one shape (8 bits per
// channel, no interlace), and a decoder that reads that shape is smaller than
// the argument for adding a package to a check that is supposed to have no
// moving parts.
//
// The viewport is rendered at deviceScaleFactor 1 (playwright.config.js), so one
// pixel here is one CSS pixel there.

const zlib = require("zlib");

const SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

/// Decode a PNG into straight RGBA bytes.
function decodePng(buffer) {
  if (!buffer.subarray(0, 8).equals(SIGNATURE)) {
    throw new Error("that is not a PNG");
  }
  let at = 8;
  let header = null;
  const compressed = [];
  while (at + 8 <= buffer.length) {
    const length = buffer.readUInt32BE(at);
    const type = buffer.toString("ascii", at + 4, at + 8);
    const data = buffer.subarray(at + 8, at + 8 + length);
    if (type === "IHDR") {
      header = {
        width: data.readUInt32BE(0),
        height: data.readUInt32BE(4),
        depth: data[8],
        colour: data[9],
        interlace: data[12],
      };
    } else if (type === "IDAT") {
      compressed.push(data);
    } else if (type === "IEND") {
      break;
    }
    at += 12 + length;
  }
  if (!header) {
    throw new Error("the PNG has no header chunk");
  }
  if (header.depth !== 8 || header.interlace !== 0) {
    throw new Error(
      `this decoder reads 8-bit non-interlaced PNG; got depth ${header.depth} interlace ${header.interlace}`
    );
  }
  const channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[header.colour];
  if (!channels) {
    throw new Error(`unsupported PNG colour type ${header.colour}`);
  }

  const raw = zlib.inflateSync(Buffer.concat(compressed));
  const { width, height } = header;
  const stride = width * channels;
  const out = new Uint8Array(width * height * 4);
  let previous = Buffer.alloc(stride);
  let cursor = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = raw[cursor];
    cursor += 1;
    const line = Buffer.from(raw.subarray(cursor, cursor + stride));
    cursor += stride;
    unfilter(filter, line, previous, channels);
    for (let x = 0; x < width; x += 1) {
      const from = x * channels;
      const to = (y * width + x) * 4;
      if (channels >= 3) {
        out[to] = line[from];
        out[to + 1] = line[from + 1];
        out[to + 2] = line[from + 2];
        out[to + 3] = channels === 4 ? line[from + 3] : 255;
      } else {
        out[to] = line[from];
        out[to + 1] = line[from];
        out[to + 2] = line[from];
        out[to + 3] = channels === 2 ? line[from + 1] : 255;
      }
    }
    previous = line;
  }
  return { width, height, data: out };
}

function unfilter(filter, line, previous, bpp) {
  const stride = line.length;
  switch (filter) {
    case 0:
      return;
    case 1:
      for (let i = bpp; i < stride; i += 1) {
        line[i] = (line[i] + line[i - bpp]) & 0xff;
      }
      return;
    case 2:
      for (let i = 0; i < stride; i += 1) {
        line[i] = (line[i] + previous[i]) & 0xff;
      }
      return;
    case 3:
      for (let i = 0; i < stride; i += 1) {
        const left = i >= bpp ? line[i - bpp] : 0;
        line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xff;
      }
      return;
    case 4:
      for (let i = 0; i < stride; i += 1) {
        const a = i >= bpp ? line[i - bpp] : 0;
        const b = previous[i];
        const c = i >= bpp ? previous[i - bpp] : 0;
        line[i] = (line[i] + paeth(a, b, c)) & 0xff;
      }
      return;
    default:
      throw new Error(`unknown PNG filter ${filter}`);
  }
}

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) {
    return a;
  }
  return pb <= pc ? b : c;
}

/// The colour at one pixel, as [r, g, b].
function at(image, x, y) {
  const px = Math.max(0, Math.min(image.width - 1, Math.round(x)));
  const py = Math.max(0, Math.min(image.height - 1, Math.round(y)));
  const i = (py * image.width + px) * 4;
  return [image.data[i], image.data[i + 1], image.data[i + 2]];
}

function key(colour) {
  return (colour[0] << 16) | (colour[1] << 8) | colour[2];
}

function fromKey(k) {
  return [(k >> 16) & 0xff, (k >> 8) & 0xff, k & 0xff];
}

/// The colour that appears most often among a list of pixels.
///
/// Used instead of "the pixel in the middle" everywhere a surface is wanted: a
/// glyph, an antialiased edge or a focus ring can be under any one point, and
/// none of them is ever the majority of a box.
function dominant(colours) {
  const counts = new Map();
  for (const colour of colours) {
    const k = key(colour);
    counts.set(k, (counts.get(k) || 0) + 1);
  }
  let best = null;
  let bestCount = -1;
  for (const [k, count] of counts) {
    if (count > bestCount) {
      best = k;
      bestCount = count;
    }
  }
  return best === null ? null : { colour: fromKey(best), count: bestCount, of: colours.length };
}

/// Every pixel inside a rectangle, clipped to the image.
function inside(image, rect) {
  const out = [];
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(image.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(image.height, Math.ceil(rect.y + rect.height));
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      out.push(at(image, x, y));
    }
  }
  return out;
}

/// The pixels in a frame around a rectangle: everything between `rect` grown by
/// `from` and grown by `to`. A frame rather than a line, so a rounded corner or
/// a one-pixel misalignment cannot decide the answer.
function frame(image, rect, from, to) {
  const outer = {
    x: rect.x - to,
    y: rect.y - to,
    width: rect.width + 2 * to,
    height: rect.height + 2 * to,
  };
  const innerX0 = rect.x - from;
  const innerY0 = rect.y - from;
  const innerX1 = rect.x + rect.width + from;
  const innerY1 = rect.y + rect.height + from;
  const out = [];
  const x0 = Math.max(0, Math.floor(outer.x));
  const y0 = Math.max(0, Math.floor(outer.y));
  const x1 = Math.min(image.width, Math.ceil(outer.x + outer.width));
  const y1 = Math.min(image.height, Math.ceil(outer.y + outer.height));
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      if (x >= innerX0 && x < innerX1 && y >= innerY0 && y < innerY1) {
        continue;
      }
      out.push(at(image, x, y));
    }
  }
  return out;
}

/// The pixels along a horizontal line.
function alongRow(image, y, x0, x1) {
  const out = [];
  for (let x = Math.max(0, Math.floor(x0)); x < Math.min(image.width, Math.ceil(x1)); x += 1) {
    out.push(at(image, x, y));
  }
  return out;
}

/// The pixels down a vertical line.
function alongColumn(image, x, y0, y1) {
  const out = [];
  for (let y = Math.max(0, Math.floor(y0)); y < Math.min(image.height, Math.ceil(y1)); y += 1) {
    out.push(at(image, x, y));
  }
  return out;
}

function near(a, b, tolerance) {
  return (
    Math.abs(a[0] - b[0]) <= tolerance &&
    Math.abs(a[1] - b[1]) <= tolerance &&
    Math.abs(a[2] - b[2]) <= tolerance
  );
}

/// The longest unbroken horizontal run of one colour inside a rectangle.
///
/// What it is for: telling an underline from the glyphs above it. An underline
/// is one run the width of the text; the longest run inside a letter is a
/// crossbar a few pixels across. Nothing about the markup can fake this, because
/// what is counted is pixels that came out of the compositor.
function longestRunOf(image, rect, colour, tolerance = 48) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(image.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(image.height, Math.ceil(rect.y + rect.height));
  let best = 0;
  for (let y = y0; y < y1; y += 1) {
    let run = 0;
    for (let x = x0; x < x1; x += 1) {
      if (near(at(image, x, y), colour, tolerance)) {
        run += 1;
        if (run > best) {
          best = run;
        }
      } else {
        run = 0;
      }
    }
  }
  return best;
}

function channel(value) {
  const v = value / 255;
  return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
}

/// WCAG 2.2 relative luminance.
function luminance(colour) {
  return (
    0.2126 * channel(colour[0]) +
    0.7152 * channel(colour[1]) +
    0.0722 * channel(colour[2])
  );
}

/// WCAG 2.2 contrast ratio, 1 to 21.
function contrast(a, b) {
  const la = luminance(a);
  const lb = luminance(b);
  const light = Math.max(la, lb);
  const dark = Math.min(la, lb);
  return (light + 0.05) / (dark + 0.05);
}

function hex(colour) {
  return (
    "#" +
    colour
      .map((c) => c.toString(16).padStart(2, "0"))
      .join("")
  );
}

/// How many distinct colours a rendering contains. One means nothing was
/// painted but a flat fill, which is what a page that never rendered looks like.
function distinctColours(image) {
  const seen = new Set();
  for (let i = 0; i < image.data.length; i += 4) {
    seen.add((image.data[i] << 16) | (image.data[i + 1] << 8) | image.data[i + 2]);
    if (seen.size > 4096) {
      break;
    }
  }
  return seen.size;
}

/// Where two renderings of the same region differ, and by how much.
///
/// The report is deliberately in two parts: how many pixels moved at all, and
/// the contrast between the colour that arrived and the colour it replaced.
/// A focus indicator has to clear both - one that changed nothing is invisible,
/// and one that changed a lot by a hair of luminance is invisible too.
function difference(before, after, tolerance = 8) {
  if (before.width !== after.width || before.height !== after.height) {
    throw new Error("two renderings of different sizes cannot be compared");
  }
  const changedNew = [];
  const changedOld = [];
  for (let i = 0; i < before.data.length; i += 4) {
    const dr = Math.abs(before.data[i] - after.data[i]);
    const dg = Math.abs(before.data[i + 1] - after.data[i + 1]);
    const db = Math.abs(before.data[i + 2] - after.data[i + 2]);
    if (dr > tolerance || dg > tolerance || db > tolerance) {
      changedOld.push([before.data[i], before.data[i + 1], before.data[i + 2]]);
      changedNew.push([after.data[i], after.data[i + 1], after.data[i + 2]]);
    }
  }
  if (changedNew.length === 0) {
    return { pixelsChanged: 0, ratio: 0, drawn: null, over: null };
  }
  const drawn = dominant(changedNew).colour;
  // The colour the indicator was drawn ON: the commonest colour that was there
  // before, among exactly the pixels the indicator now covers.
  const over = dominant(changedOld).colour;
  return {
    pixelsChanged: changedNew.length,
    ratio: contrast(drawn, over),
    drawn: hex(drawn),
    over: hex(over),
  };
}

module.exports = {
  decodePng,
  at,
  dominant,
  inside,
  frame,
  alongRow,
  alongColumn,
  longestRunOf,
  luminance,
  contrast,
  hex,
  distinctColours,
  difference,
};
