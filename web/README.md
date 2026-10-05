# web

The chorus app: Lit 3 elements bundled by esbuild (proposal P5, Option C; the decision record
`docs/decisions/0000-the-web-app-stack.md` says what is pinned and why). Today it is the shell
that later screens are added to: one element, `chorus-app`, with the design tokens.

| Path | What |
|---|---|
| `src/` | `index.html`, `app.css` and `tokens.css` (the document and the tokens), `main.js` (the entry point), the elements and their pure logic |
| `test/` | the unit tests (`*.test.js`), `setup.js` (happy-dom's globals, loaded before Lit) and `label-query.js` (find an element by its label, through shadow roots) |
| `build.mjs` | the build: `src/` into `dist/`, deterministic |
| `dist/` | the build's output, committed: `chorus-server` embeds it and never runs node |
| `licences.txt`, `licences.mjs` | the licence of every locked package, and the check of the installed ones against it |

From the repository root, with the pinned node and pnpm of `mise.toml` (`mise install`):

```sh
make web-test     # node --test over happy-dom: no browser
make web-build    # rebuild dist/; commit it with the change to src/
```

Rules, each held by a check (`docs/conventions.md`, rules 13 and 14):

- Install scripts are off (`pnpm-workspace.yaml`, where pnpm 12 reads it; there is no
  `.npmrc`); every dependency is an exact version; the direct ones are lit, esbuild, happy-dom and `@happy-dom/global-registrator` and
  no other. A lockfile change brings the new packages' lines in `licences.txt`.
- A change to `src/` comes with the rebuilt `dist/`: the gate step `web-build` rebuilds and
  fails on a difference.
- An element's styles name tokens of `tokens.css`, never a literal colour or length, and
  nothing is inline: the server's Content-Security-Policy allows neither inline script nor
  inline style.
- A test finds a control by its label (`getByLabel`), which also proves it has one.
