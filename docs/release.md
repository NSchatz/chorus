# Releases

A chorus release is a `v<x.y.z>` tag on a squash-merge commit of `main` plus a GitHub
release on NSchatz/chorus with its artifacts attached (K41). Versions are SemVer `0.x`
until the program's finale says otherwise. The images are pushed to
ghcr.io by CI, `.github/workflows/publish-images.yml` (below; decided by the owner 2026-10-06).

## What a release carries

| Artifact | What it is | Built by |
|---|---|---|
| `chorus-server-v<ver>-x86_64-unknown-linux-musl` | the static server binary (C, for libopus, compiled by the pinned zig through `tools/zig-musl-cc.sh`) | `cargo build --release --locked --target x86_64-unknown-linux-musl`, as `tools/image.sh` runs it |
| `chorus-server-v<ver>-oci.tar` | the server as an OCI image layout tarball on a digest-pinned distroless base, tested unpacked (`--help`, `GET /api/state`) | `tools/image.sh` |
| `chorus-soloist-v<ver>-oci.tar` | one Spotify Soloist receiver as an OCI image layout tarball: PipeWire, WirePlumber and `chorus-soloistd` on a digest-pinned `debian:trixie-slim`, the Debian packages pinned by sha256; tested unpacked. **It holds no Soloist file** (`docs/soloist.md`) | `tools/soloist-image.sh` |
| `chorus-soloist-v<ver>-NOTICES.md` | that image's third-party notices: each Debian package, its version, and the Debian source package that is its source (the file the image carries at `/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md`) | `tools/soloist-image.sh` |
| `chorus-endpoint-esp32s3-v<ver>.bin` | the ESP32-S3 endpoint application image | `tools/firmware-image.sh` (ESP-IDF at the pin in `firmware/config/endpoint.conf`, then the eFuse and image guard) |
| `chorus-endpoint-esp32s3-v<ver>.tar.gz` | the bootloader, partition table, application and `flasher_args.json` (offsets and flash flags) | the same build |
| `chorus-endpoint_<ver>_arm64.deb`, `chorus-endpoint_<ver>_amd64.deb` | the Linux endpoint package: `chorus-client`, its systemd unit, `/etc/chorus/client.conf`, real-time limits and `chorus-verify-host`, for glibc 2.36 and later (goal 10; `docs/linux-endpoint.md`) | `tools/endpoint-package.sh` (cross-built rootless with zig and cargo-zigbuild; readelf, dpkg-deb and `systemd-analyze verify` checks) |
| `<crate>-<ver>.crate` | the source of each MPL-2.0 crate a shipped binary links (the twelve Symphonia crates in `chorus-server`, four of them also in `chorus-client`), checked against `Cargo.lock`'s checksum | `tools/release.sh` (below) |
| `SHA256SUMS` | sha256 of every artifact | `tools/release.sh` |

Later releases add more firmware targets as the program builds them.

`bash tools/release.sh --list` prints exactly these names for the workspace's version and
builds nothing; a release refuses to finish unless its directory holds exactly that list, and
`make soloist-lists` (a gate step) reads it and fails if any name is a Soloist file or the fake
Soloist (conventions rule 24).

## Cutting one

CI cuts it (ADR 0235): `.github/workflows/release.yml` runs `make release` on a GitHub runner
with the pinned toolchains, as `nightly.yml` provisions them, and creates the release. Nothing
is built on a workstation or the agent host.

1. `main` is green: the last nightly `make gate` on `main` passed
   (`.github/workflows/nightly.yml`), and no pull request merged since has a red
   `make gate-changed`; the nightly workflow can also be run by hand on `main`'s tip.
2. The workspace version in `Cargo.toml` is the release's version (a PR changes it first
   if needed).
3. Optionally, a dry run of `main` first (its tip, so run it before anything else merges): it builds the release with `Cargo.toml`'s
   version and uploads `dist/v<ver>/` as the workflow artifact `chorus-v<ver>`, publishing
   nothing.

   ```
   gh workflow run release.yml --ref main -f dry-run=true
   ```

4. Tag the commit and push the tag:

   ```
   git tag -a v<ver> -m "chorus v<ver>" <sha> && git push origin v<ver>
   ```

   The tag push runs `make release VERSION=<ver>` on the tagged commit. `tools/release.sh`
   refuses by name on a dirty tree, a commit not on `origin/main`, a version that differs from
   `Cargo.toml`, or an ESP-IDF that is not the pinned one, and holds `dist/v<ver>/` to
   `tools/release.sh --list`. A second job, the only one with `contents: write` and one that
   runs no build script, then runs `gh release create v<ver> --verify-tag` with
   `dist/v<ver>/NOTES.md` as the notes and every other file in that directory attached, and
   fails unless the release lists exactly those assets. The same tag push has
   `publish-images.yml` push the images (below).
5. If the release job failed after the tag was pushed, fix forward on `main` only when the
   tagged commit itself is wrong (a new version and tag); otherwise run it again on the tag:
   `gh workflow run release.yml --ref v<ver> -f dry-run=false`. If the failure was in
   `gh release create` itself, delete the release or draft it left behind first
   (`gh release delete v<ver>`, which keeps the tag).
6. `gh release view v<ver>` lists the assets; the table below records the tag's commit.

## Releases so far

| Tag | Commit |
|---|---|
| v0.1.0 | `c233198c370ce528323db619ac25b6930fc249f9`, the squash-merge of #38 |
| v0.2.0 | the squash-merge of the pull request that set the workspace version to 0.2.0 (#239) |

## Source of MPL-licensed dependencies (P9)

MPL-2.0 requires that the source of the MPL-covered files be available to whoever
receives a binary that contains them. chorus's rule: every release whose binaries
contain an MPL-2.0 dependency (Symphonia, from goal 16, by its ADR) says in its notes,
per such crate, its name, version and the exact source location (the crate's
`https://crates.io/crates/<name>/<version>` download, whose checksum is in `Cargo.lock`),
and attaches the crate's source tarball as `<name>-<version>.crate` beside the binaries,
so the release is complete on its own. `tools/release.sh` counts the external crates in
`Cargo.lock` and says so in the notes. v0.1.0 carried no MPL source. From the first release
with the Linux endpoint packages (goal 10), `chorus-client` links Symphonia's FLAC crates
(ADR 0044), so `tools/release.sh` finds every MPL-2.0 crate in the shipped binaries with
`cargo tree`, copies each `.crate` from the local registry cache (or fetches it from
`https://static.crates.io/crates/<name>/<name>-<ver>.crate`), refuses unless its sha256 is
the checksum `Cargo.lock` pins, and lists each in the notes.

Since goal 16 the server links Symphonia too (MP3, FLAC, Vorbis, ALAC and WAV decoding;
`docs/decoders.md`, ADR 0122): twelve MPL-2.0 crates in all, which the same `cargo tree` query
finds, so the release attaches twelve `.crate` files. The image says where that source is
itself: `/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md` names each crate and version with its
crates.io location, and `/usr/share/doc/chorus/libopus-COPYING` reproduces libopus's
BSD-3-Clause licence, which a binary distribution must carry; `make image` checks both.

The endpoint packages also say it on the device: `/usr/share/doc/chorus-endpoint/copyright`
lists every crate linked into the packaged binaries with its licence and reproduces the
libopus, MIT and Apache-2.0 texts.

## The Debian packages in the Soloist receiver image

The `chorus-soloist` image is chorus's own static `chorus-soloistd` beside unmodified Debian
binary packages (PipeWire, WirePlumber and what they depend on), some under the LGPL. chorus
links none of them. The image carries each package's Debian copyright file and a notices file
naming every package, its version and its Debian source package at the snapshot.debian.org
timestamp the binary came from; the release attaches that notices file as
`chorus-soloist-v<ver>-NOTICES.md`. The source packages themselves are not attached: Debian's
snapshot archive is where they are. `docs/decisions/0131-the-chorus-soloist-image.md` says why
that is the position and what the owner would add before publishing the image to others.

## Pushing the images to a registry

CI pushes them (decided by the owner 2026-10-06, ADR 0222).
`.github/workflows/publish-images.yml` builds `make image` and `make soloist-image` from a commit
on `main`, unpacks each tarball to its OCI layout and pushes it unchanged with `crane` (the tool
`tools/image.sh` pins, `aqua:google/go-containerregistry` 0.22.1), so the manifest digest the
build prints is the digest the registry serves ("If the PATH is a directory, it will be read as
an OCI image layout", crane push reference,
https://github.com/google/go-containerregistry/blob/main/cmd/crane/doc/crane_push.md, read
2026-09-30). It authenticates with the job's own `GITHUB_TOKEN` (`packages: write`); no personal
registry credential exists here and none is created (K4, K41).

- A pushed release tag `v<ver>` publishes `ghcr.io/nschatz/chorus-server:<ver>` and
  `ghcr.io/nschatz/chorus-soloist:<ver>`.
- By hand, for a commit that has no release (a homelab pin such as `g17-a835a5f`):

  ```
  gh workflow run publish-images.yml -f ref=<sha> -f tag=<tag> \
      -f server-digest=<pinned digest> -f soloist-digest=<pinned digest>
  ```

  With the digests given, the run fails before pushing anything if a build differs from them.
  The job summary lists each pushed image as `<repo>:<tag>@<digest>`; the homelab stacks pin
  `image:` to that digest.

The registry packages are the owner's: a package is private when first pushed, so the host
either logs in (`docker login ghcr.io` with a read-only packages token) or the owner makes the
package public once in its GitHub settings.
