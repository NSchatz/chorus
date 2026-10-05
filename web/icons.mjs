// Draws the app's icons, src/icons/icon-<size>.png: `node icons.mjs`, by hand,
// when the drawing changes. The build copies the committed files and never
// runs this, so the bytes chorus-server embeds do not depend on the zlib of
// whichever node ran the build.
//
// The drawing is chorus's own and has no source but this file: a dot and two
// rings (one speaker, heard in every room) in the app's accent on its dark
// background, the two colours of src/tokens.css (--blue-300 on --neutral-950).
// The background is the whole square and the drawing stays inside the middle
// 60 percent, so the same file serves as a maskable icon: a launcher may cut
// any shape out of it and loses nothing.

import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { crc32, deflateSync } from "node:zlib";

const SIZES = [180, 192, 512];
const BACKGROUND = [0x0f, 0x14, 0x1a];
const INK = [0x7a, 0xb8, 0xf5];
// Radii as fractions of the icon's width: the dot, then each ring's inner
// and outer edge. The outermost edge is 0.3, the maskable safe zone's limit
// being 0.4.
const DOT = 0.075;
const RINGS = [
  [0.14, 0.185],
  [0.255, 0.3],
];
// Samples per pixel along each axis, for the edges.
const SAMPLES = 4;

const inked = (radius) => radius <= DOT || RINGS.some(([inner, outer]) => radius >= inner && radius <= outer);

function draw(size) {
  // One filter byte (none) and three bytes a pixel, per row.
  const rows = Buffer.alloc(size * (1 + size * 3));
  for (let y = 0; y < size; y += 1) {
    const row = y * (1 + size * 3);
    for (let x = 0; x < size; x += 1) {
      let hits = 0;
      for (let sy = 0; sy < SAMPLES; sy += 1) {
        for (let sx = 0; sx < SAMPLES; sx += 1) {
          const dx = (x + (sx + 0.5) / SAMPLES) / size - 0.5;
          const dy = (y + (sy + 0.5) / SAMPLES) / size - 0.5;
          if (inked(Math.hypot(dx, dy))) hits += 1;
        }
      }
      const cover = hits / (SAMPLES * SAMPLES);
      for (let channel = 0; channel < 3; channel += 1) {
        rows[row + 1 + x * 3 + channel] = Math.round(BACKGROUND[channel] + (INK[channel] - BACKGROUND[channel]) * cover);
      }
    }
  }
  return rows;
}

function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const sum = Buffer.alloc(4);
  sum.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, sum]);
}

function png(size) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8; // bits per channel
  header[9] = 2; // truecolour, no alpha: an icon with no transparent corner
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(draw(size), { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const out = path.join(path.dirname(fileURLToPath(import.meta.url)), "src/icons");
await mkdir(out, { recursive: true });
for (const size of SIZES) {
  const bytes = png(size);
  await writeFile(path.join(out, `icon-${size}.png`), bytes);
  console.log(`icons: src/icons/icon-${size}.png ${bytes.length} B`);
}
