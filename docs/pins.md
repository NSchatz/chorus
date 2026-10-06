# Pins: what is current and what is held back

Every pin chorus has, with the newest upstream release and, for each one behind, how far and
why. `docs/conventions.md` rule 14 says where each kind of pin lives and what checks it; this
file is the one place that says whether each pin is current. It is re-read whenever a pin
changes and before each release.

Read 2026-10-06 against main at `8c89353`, from each project's release metadata (the URLs are
under "Sources"). No GPL source was opened: for shellcheck, cppcheck and Espressif's QEMU only
release lists were read. "Age" is how long the newer upstream release has been out.

## Why a pin can be behind

An upgrade is its own change saying why (rule 14), and a Rust or ESP-IDF upgrade goes through a
proposal first (K51). This record moves no pin. A pin behind for the reason "not yet taken" has a
newer release that came out after it was pinned and that no change has taken yet; the follow-up
is the pin refresh named at the end.

## Toolchains

| Pin | Where | Pinned | Newest upstream | State |
|---|---|---|---|---|
| Rust | `rust-toolchain.toml` | 1.98.1 (2026-09-01) | 1.99.0 (2026-10-01) | behind 5 days: an upgrade needs a proposal (K51), and none is written yet |
| ESP-IDF | `firmware/config/endpoint.conf` | v6.1 at `fff9895` (2026-08-27) | v6.1 (newer dates are older lines: v6.0.3, v5.3.6, v5.2.8) | current (ADR 0042) |
| Espressif QEMU | `tools/qemu/pins.conf` | esp-develop-9.2.2-20260417 | the same | current; it must be the release ESP-IDF v6.1's `tools.json` names |
| micromamba | `tools/qemu/pins.conf` | 2.9.0-0 | 2.9.0-0 | current |
| Python (the Home Assistant harness) | `integrations/homeassistant/harness.pin` | 3.14.8 | 3.14.8 (2026-09-30) | current |

## The gate's tools (`mise.toml`, digests in `mise.lock`)

| Tool | Pinned | Newest upstream | State |
|---|---|---|---|
| gitleaks | 8.30.1 | 8.30.1 | current |
| cargo-deny | 0.20.2 | 0.20.2 | current |
| shellcheck | 0.11.0 | 0.11.0 | current |
| actionlint | 1.7.12 | 1.7.12 | current |
| yamllint | 1.38.0 | 1.38.0 | current |
| clang-format | 23.1.1 | 23.1.3 (2026-10-06; 23.1.2 2026-10-01) | behind 5 days: not yet taken; a formatter bump can reformat C, so it lands with any reformatting it causes |
| cppcheck (the PyPI wheel) | 1.5.1, cppcheck 2.17.1 | 1.5.2, cppcheck 2.20.0 (2026-10-06); upstream cppcheck 2.22.0 (2026-09-19) has no wheel | behind 0 days: `mise.toml`'s reason (no newer wheel installs rootless) held until today; 1.5.2 is the next step, and a new analyser can raise new findings |
| zig | 0.16.0 | 0.17.0 (2026-10-01) | behind 5 days: not yet taken; zig is the C compiler of the endpoint package's and the server image's cross builds, so a move is checked by both |
| cargo-zigbuild | 0.23.4 | 0.23.4 | current |
| cargo-nextest | 0.9.146 | 0.9.146 | current |
| prometheus (promtool) | 3.13.3 | 3.15.0 (2026-09-25); 3.13.4 on the pinned line (2026-10-02) | behind 11 days: not yet taken; promtool only lints a scrape and is not a gate step |
| uv | 0.12.22 | 0.12.23 (2026-10-03) | behind 3 days: not yet taken |
| node | 24.21.0 | 24.21.0 is the newest LTS; 26.10.0 (2026-09-21) is not LTS yet | current: the app builds on the LTS line (ADR 0181) |
| pnpm | 12.8.1 | 12.9.1 (2026-10-03) | behind 3 days: not yet taken |

mise itself is pinned in each workflow (`jdx/mise-action` with `version: 2026.7.5`); the newest
is v2026.10.3 (2026-10-05), so it is behind 1 day: not yet taken, and it moves with the
mise-action refresh below. It counts as one pin.

The image tools are pinned where they run (`tools/image.sh`, `tools/soloist-image.sh`): crane
0.22.1 and umoci 0.6.0, both current.

## Container bases

| Base | Where | Pinned | Upstream today | State |
|---|---|---|---|---|
| The server image (`make image`) | `tools/image.sh` | `gcr.io/distroless/static-debian12:nonroot` at `afa5c87` | the tag still resolves to `afa5c87`; a `static-debian13` line exists | current for its tag; staying on Debian 12 is not a recorded choice, and a move to 13 is its own change with the image test |
| `deploy/Dockerfile`, build stage | `deploy/Dockerfile` | `rust:1.98.1-slim-bookworm` at `ff52144` | the tag still resolves to `ff52144` | current: it follows the Rust pin (B-16) and moves with it |
| `deploy/Dockerfile`, runtime stage | `deploy/Dockerfile` | `debian:bookworm-slim` at `8820086` (resolved 2026-09-08) | the tag resolves to `7c7b2c9` (2026-10-06); Debian 13 is stable | behind 0 days (the pinned digest is 28 days old): the tag is rolling and the digest is re-resolved with the command in `deploy/README.md`; the released image is `make image`'s, not this file's |
| The `chorus-soloist` image | `tools/soloist-image.sh` | `debian:trixie-20260918-slim` at `a99cfc5` (resolved 2026-10-03) | `trixie-20261005-slim` (2026-10-06) | behind 0 days: the base date is the snapshot.debian.org timestamp of `deploy/soloist/debian-packages.pins`, so both move together |

Two package lists follow their parent pin and are not counted apart: the 60 Debian packages of
`deploy/soloist/debian-packages.pins` follow the Soloist base's snapshot date above, and the 56
conda-forge packages of `tools/qemu/libs.explicit.txt` follow the emulator's QEMU and micromamba
pins. The CI runner image (`runs-on: ubuntu-24.04`) is a named release line GitHub maintains, not a
pin: the toolchains the jobs use come from mise and rustup, not from the runner.

## CI actions (`.github/workflows/`)

| Action | Pinned | Newest upstream | State |
|---|---|---|---|
| actions/checkout | `11d5960` = v4.4.0 | v7.0.1 (2026-07-20) | three majors behind: no reason was recorded when it was pinned |
| jdx/mise-action | `9149ea8` = v5.0.0 | v5.1.1 (2026-10-04) | behind 2 days: not yet taken |
| Swatinem/rust-cache | `63fed3e`, labelled v2.9.2 | v2.9.2 | current, but `63fed3e` is the annotated tag object of v2.9.2, not its commit `6323deb`; rule 14 asks for the commit |
| actions/upload-artifact | `ea165f8` = v4.6.2 | v7.0.1 (2026-04-10) | three majors behind: no reason was recorded (ADR 0140 records the commit, not why v4) |
| actions/download-artifact | `d3f86a1` = v4.3.0 | v8.0.1 (2026-03-11) | four majors behind: no reason was recorded |

## Libraries

| Pin | Where | Pinned | Newest upstream | State |
|---|---|---|---|---|
| Crates | `Cargo.lock` | | `cargo update --dry-run` moves four, all patch releases: ctutils 0.4.2 to 0.4.3, lazy_static 1.5.0 to 1.5.1, libc 0.2.189 to 0.2.190, zeroize 1.9.0 to 1.9.1 | four patch releases behind: not yet taken |
| cc (opus-sys's build dependency) | `crates/opus-sys/Cargo.toml` | `=1.5.1` | 1.6.0 (2026-10-03) | behind 3 days: not yet taken (ADR 0044 cites 1.5.1, not a reason to hold it) |
| espressif/w5500 | `firmware/main/idf_component.yml` | 2.0.0 | 2.0.0 | current |
| espressif/network_provisioning | `firmware/main/idf_component.yml` | 1.2.5 | 1.3.1 (2026-10-01) | behind 5 days, held on purpose: 1.3.x swaps cJSON for a json_generator major published the same day and adds nothing the endpoint uses (the reason is in `idf_component.yml`) |
| libopus | `third_party/opus` | 1.6.1 | 1.6.1 | current |
| dr_flac | `third_party/dr_flac` | 0.13.3 | 0.13.3 is the newest tag; untagged seeking fixes on master | current; `third_party/README.md` takes 0.13.4 when it is tagged, and chorus never seeks |
| microWakeWord model | `third_party/wakeword` | `40ff33f` | `40ff33f` | current |
| lit, esbuild, happy-dom, @happy-dom/global-registrator, @playwright/test | `web/package.json` | 3.3.3, 0.28.2, 20.14.5, 20.14.5, 1.63.0 | the same | current |
| Home Assistant core (hassfest's tag) | `integrations/homeassistant/harness.pin` | 2026.9.3 | 2026.9.4 (2026-09-27); 2026.10 is in beta | behind 9 days, held on purpose: the harness moves only with the homelab's Home Assistant pin (`docs/home-assistant.md`) |
| pytest-homeassistant-custom-component | `integrations/homeassistant/harness.pin` | 0.13.366 | 0.13.369 (for 2026.10.0b2); 0.13.367 is 2026.9.4's | held with Home Assistant above; when it moves, the target is the release for the homelab's version, not the newest |
| The Home Assistant components' test imports (hassil, home-assistant-intents, gazetteer-matcher, pymicro-vad, pyspeex-noise, mutagen, ha-ffmpeg) and hassfest's (infrared-protocols) | `integrations/homeassistant/pyproject.toml` | the versions core 2026.9.3 pins | follow core | held with Home Assistant above: each is the version core 2026.9.3 pins, as `pyproject.toml` says |
| mypy | `integrations/homeassistant/pyproject.toml` | 2.3.1 | 2.4.0 (2026-10-01) | behind 5 days: not yet taken |
| ruff | `integrations/homeassistant/pyproject.toml` | 0.16.3 | 0.16.10; 0.16.4 came out 2026-08-20 | behind 47 days: not yet taken, and no reason was recorded; a linter bump can raise new findings, so it lands with their fixes |

## Totals and the follow-up

Of 48 pins, 26 are current, 4 are held on purpose with their reason (network_provisioning, Home
Assistant, its harness and the components' test imports), and 18 are behind with no hold: 12 by
releases under two weeks old (Rust, clang-format, cppcheck, zig, promtool, uv, pnpm, mise,
mise-action, mypy, the crates, cc), ruff by 47 days, 2 base digests that move with a dated tag
(the Dockerfile's runtime stage, the Soloist base), and the 3 GitHub actions several majors
behind. The rust-cache pin is current but names a tag object.

The follow-up is one pin refresh, a change of its own: the patch and tool releases above (ruff
and mypy outside the Home Assistant harness's hold), the
three actions to their current majors and rust-cache to its commit, each checked by the CI
steps that use it; and a proposal for Rust 1.99.

## Sources (read 2026-10-06)

- Rust: https://static.rust-lang.org/dist/channel-rust-stable.toml
- GitHub release and tag lists (`gh api repos/<owner>/<repo>/releases`, `/tags`): gitleaks,
  cargo-deny, shellcheck, actionlint, cargo-zigbuild, nextest, prometheus, uv, pnpm,
  go-containerregistry, umoci, actions/checkout, jdx/mise-action, Swatinem/rust-cache,
  actions/upload-artifact, actions/download-artifact, espressif/esp-idf, espressif/qemu,
  mamba-org/micromamba-releases, cppcheck-opensource/cppcheck, xiph/opus, mackron/dr_libs,
  esphome/micro-wake-word-models
- mise: `gh api repos/jdx/mise/releases/latest`
- PyPI: https://pypi.org/pypi/mypy/json, https://pypi.org/pypi/ruff/json,
  https://pypi.org/pypi/yamllint/json, https://pypi.org/pypi/clang-format/json,
  https://pypi.org/pypi/cppcheck/json,
  https://pypi.org/pypi/pytest-homeassistant-custom-component/json,
  https://pypi.org/pypi/homeassistant/json
- https://nodejs.org/dist/index.json, https://ziglang.org/download/index.json
- npm: https://registry.npmjs.org/ for lit, esbuild, happy-dom, @happy-dom/global-registrator,
  @playwright/test, pnpm
- https://components.espressif.com/api/components/espressif/w5500 and
  `.../network_provisioning`
- Docker Hub tags for `library/rust` and `library/debian`; `crane digest` for
  `gcr.io/distroless/static-debian12:nonroot`
- https://downloads.xiph.org/releases/opus/, https://crates.io/api/v1/crates/cc
- Crates: `cargo update --dry-run` (resolves only; the lockfile is unchanged)
