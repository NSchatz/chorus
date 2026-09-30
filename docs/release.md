# Releases

A chorus release is a `v<x.y.z>` tag on a squash-merge commit of `main` plus a GitHub
release on NSchatz/chorus with its artifacts attached (K41). Versions are SemVer `0.x`
until the program's finale says otherwise. Nothing is pushed to a registry by the
program: that is the owner's step (below).

## What a release carries

| Artifact | What it is | Built by |
|---|---|---|
| `chorus-server-v<ver>-x86_64-unknown-linux-musl` | the static server binary | `cargo build --release --locked --target x86_64-unknown-linux-musl` |
| `chorus-server-v<ver>-oci.tar` | the server as an OCI image layout tarball on a digest-pinned distroless base, tested unpacked (`--help`, `GET /api/state`) | `tools/image.sh` |
| `chorus-endpoint-esp32s3-v<ver>.bin` | the ESP32-S3 endpoint application image | `tools/firmware-image.sh` (ESP-IDF at the pin in `firmware/config/endpoint.conf`, then the eFuse and image guard) |
| `chorus-endpoint-esp32s3-v<ver>.tar.gz` | the bootloader, partition table, application and `flasher_args.json` (offsets and flash flags) | the same build |
| `SHA256SUMS` | sha256 of every artifact | `tools/release.sh` |

Later releases add the Linux endpoint package (goal 10) and more firmware targets as the
program builds them.

## Cutting one

1. `main` is green: the last `make gate` on `main`'s tree passed (a PR's gate, run on the
   branch up to date with `main`, counts).
2. The workspace version in `Cargo.toml` is the release's version (a PR changes it first
   if needed).
3. From a clean checkout of that commit, under the heavy lock:

   ```
   git switch --detach origin/main
   timeout 3600 flock -o -w 1800 /cache/locks/chorus-heavy.lock make release VERSION=<ver>
   ```

   `tools/release.sh` refuses by name on a dirty tree, a commit not on `origin/main`, a
   version that differs from `Cargo.toml`, or an ESP-IDF that is not the pinned one. It
   writes the artifacts, `SHA256SUMS` and the notes body (`NOTES.md`) to `dist/v<ver>/`.
4. Tag that commit and publish:

   ```
   git tag -a v<ver> -m "chorus v<ver>" <sha> && git push origin v<ver>
   gh release create v<ver> --verify-tag --title "chorus v<ver>" \
       --notes-file dist/v<ver>/NOTES.md dist/v<ver>/chorus-* dist/v<ver>/SHA256SUMS
   ```

5. `gh release view v<ver>` lists the assets; the goal's ledger records the tag's SHA.

## Source of MPL-licensed dependencies (P9)

MPL-2.0 requires that the source of the MPL-covered files be available to whoever
receives a binary that contains them. chorus's rule: every release whose binaries
contain an MPL-2.0 dependency (Symphonia, from goal 16, by its ADR) says in its notes,
per such crate, its name, version and the exact source location (the crate's
`https://crates.io/crates/<name>/<version>` download, whose checksum is in `Cargo.lock`),
and attaches the crate's source tarball as `<name>-<version>.crate` beside the binaries,
so the release is complete on its own. `tools/release.sh` counts the external crates in
`Cargo.lock` and says so in the notes; v0.1.0 has none, so it carries no MPL source.

## Pushing the image to a registry (the owner's step)

The program never pushes (no registry credential exists here and none is created, K4,
K41). With the release's OCI tarball on a machine where the owner is logged in to the
registry, `crane` (the tool `tools/image.sh` pins, `aqua:google/go-containerregistry`
0.22.1) pushes the layout unchanged, so the manifest digest in the release notes is the
digest the registry serves ("If the PATH is a directory, it will be read as an OCI image
layout", crane push reference,
https://github.com/google/go-containerregistry/blob/main/cmd/crane/doc/crane_push.md,
read 2026-09-30):

```
mkdir chorus-oci && tar -xf chorus-server-v<ver>-oci.tar -C chorus-oci
mise exec aqua:google/go-containerregistry@0.22.1 -- crane push chorus-oci ghcr.io/nschatz/chorus-server:<ver>
mise exec aqua:google/go-containerregistry@0.22.1 -- crane digest ghcr.io/nschatz/chorus-server:<ver>
```

The last line must print the manifest digest from the release notes; the homelab stack
pins `image:` to that digest.
