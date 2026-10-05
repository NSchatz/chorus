// `make web-build`: src/ into dist/, the output chorus-server embeds.
//
//   dist/index.html                  src/index.html with the two asset names filled in
//   dist/assets/main-<hash>.js       the app and the Lit runtime, bundled and minified
//   dist/assets/app-<hash>.css       app.css and tokens.css, bundled and minified
//   dist/manifest.webmanifest        src/manifest.webmanifest, as written
//   dist/icons/icon-<size>.png       src/icons/, as committed (icons.mjs draws them)
//   dist/sw.js                       the service worker: sw.js and worker.js, bundled
//                                    and minified, with the files of the shell and the
//                                    build's version filled in
//
// The service worker keeps its own name, at the root of dist/: its scope is
// the directory it is served from, and the server never answers it as
// immutable. Its version is a digest of every other file of dist/, so a build
// that changes any of them is a new worker with a cache of its own, and one
// that changes none is the same bytes.
//
// dist/ is committed, and the gate rebuilds it and fails on a difference, so
// the build must give the same bytes every time: esbuild is one pinned binary,
// an asset's name is the hash of its content, nothing here reads a clock, the
// environment or the directory's path, and dist/ is emptied first so a renamed
// asset leaves no stale file. Lit's licence notices stay in the bundle
// (esbuild's legal comments, at the end of the file): BSD-3-Clause asks that
// redistributions keep them.

import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { build } from "esbuild";

const root = path.dirname(fileURLToPath(import.meta.url));
const dist = path.join(root, "dist");

await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });

const result = await build({
  absWorkingDir: root,
  entryPoints: ["src/main.js", "src/app.css"],
  outdir: "dist",
  entryNames: "assets/[name]-[hash]",
  bundle: true,
  minify: true,
  format: "esm",
  platform: "browser",
  target: "es2022",
  legalComments: "eof",
  charset: "utf8",
  metafile: true,
  logLevel: "warning",
});

// The output file of each entry point, as index.html names it: relative, so
// the page works wherever the server mounts it.
const asset = new Map();
for (const [file, meta] of Object.entries(result.metafile.outputs)) {
  if (meta.entryPoint) {
    asset.set(path.basename(meta.entryPoint), path.posix.relative("dist", file));
  }
}

const template = await readFile(path.join(root, "src/index.html"), "utf8");
const page = template.replace(/\{\{([a-z.]+)\}\}/g, (_, name) => {
  const file = asset.get(name);
  if (!file) throw new Error(`src/index.html names {{${name}}}, which is not an entry point`);
  return file;
});
await writeFile(path.join(dist, "index.html"), page);

// The files copied as they are.
const copied = ["manifest.webmanifest"];
for (const name of (await readdir(path.join(root, "src/icons"))).sort()) {
  if (name.endsWith(".png")) copied.push(`icons/${name}`);
}
for (const file of copied) {
  await mkdir(path.dirname(path.join(dist, file)), { recursive: true });
  await copyFile(path.join(root, "src", file), path.join(dist, file));
}

// The service worker, last: it names every file above. The shell is all of
// them, the document as "" (the app's directory itself, which is what a
// navigation asks for).
const files = ["index.html", ...[...asset.values()].sort(), ...copied];
const digest = createHash("sha256");
for (const file of files) {
  digest.update(`${file}\n`);
  digest.update(await readFile(path.join(dist, file)));
}
await build({
  absWorkingDir: root,
  entryPoints: ["src/sw.js"],
  outfile: "dist/sw.js",
  bundle: true,
  minify: true,
  format: "iife",
  platform: "browser",
  target: "es2022",
  legalComments: "eof",
  charset: "utf8",
  define: {
    __CHORUS_BUILD__: JSON.stringify(digest.digest("hex").slice(0, 16)),
    __CHORUS_SHELL__: JSON.stringify(files.map((file) => (file === "index.html" ? "" : file))),
  },
  logLevel: "warning",
});

for (const file of [...files, "sw.js"]) {
  const bytes = (await readFile(path.join(dist, file))).length;
  console.log(`web-build: dist/${file} ${bytes} B`);
}
