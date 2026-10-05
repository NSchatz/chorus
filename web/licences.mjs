// Run by tools/web.sh after every install: the `license` field of each
// package the install put in node_modules is the one licences.txt records for
// that name and version. tools/conventions/check-web.sh holds licences.txt to
// the lockfile and to the allowlist without an install; this holds it to the
// packages themselves.

import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const recorded = new Map();
for (const line of (await readFile(path.join(root, "licences.txt"), "utf8")).split("\n")) {
  if (!line || line.startsWith("#")) continue;
  const [name, version, licence] = line.split(" ");
  recorded.set(`${name}@${version}`, licence);
}

// pnpm's virtual store: node_modules/.pnpm/<name>@<version>/node_modules/<name>/package.json
const store = path.join(root, "node_modules/.pnpm");
let installed = 0;
let wrong = 0;
for (const entry of await readdir(store, { withFileTypes: true })) {
  if (!entry.isDirectory() || entry.name === "node_modules") continue;
  const modules = path.join(store, entry.name, "node_modules");
  const names = [];
  for (const name of await readdir(modules)) {
    if (!name.startsWith("@")) names.push(name);
    else for (const sub of await readdir(path.join(modules, name))) names.push(`${name}/${sub}`);
  }
  for (const name of names) {
    const manifest = JSON.parse(await readFile(path.join(modules, name, "package.json"), "utf8"));
    // The directory holds the package itself and links to what it depends on.
    if (entry.name !== `${manifest.name.replace("/", "+")}@${manifest.version}`) continue;
    installed += 1;
    const key = `${manifest.name}@${manifest.version}`;
    if (recorded.get(key) !== manifest.license) {
      wrong += 1;
      console.log(`FAIL: ${key} is licensed ${manifest.license}; licences.txt records ${recorded.get(key) ?? "nothing"}`);
    }
  }
}
if (installed === 0) {
  console.log("FAIL: node_modules/.pnpm holds no package; run the install first");
  process.exit(1);
}
if (wrong > 0) process.exit(1);
console.log(`web: ${installed} installed packages, each with the licence licences.txt records`);
