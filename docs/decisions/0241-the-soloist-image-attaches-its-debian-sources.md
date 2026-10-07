# 0241: the Soloist image's releases attach the Debian source packages of every package in it, fetched from snapshot.debian.org and verified against the .dsc files, so LGPL-2.1 and GPL-2 source is offered from the same place as the image

- Status: decided by the owner, 2026-10-07 (harness task 316, filed by task 263, the 2026-09
  program report); supersedes the open point of ADR 0131 item 9 ("the owner either accepts the
  pointer ... or attaches the 47 source packages")
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: harness task 321: `deploy/soloist/debian-sources.pins` (the 47 `.dsc` files,
  each with its sha256 and size), `tools/soloist-sources.sh` (`make soloist-sources`: fetch,
  verify, tar), `tools/release.sh` (step 2c, the asset `chorus-soloist-v<ver>-debian-sources.tar`
  and the notes), `deploy/soloist/THIRD-PARTY-NOTICES.md` and `tools/soloist-image.sh` (the
  notices name the asset), `tools/conventions/check-pins.sh`, `tools/soloist-lists.py`, and
  `.github/workflows/release.yml` (the `sources-for` input that attached v0.2.0's); measured
  by the first run (CI, v0.2.0's, 2026-10-07): 154 files, 47 of them `.dsc`, 246,460,634
  bytes (item 4's "more than 100 MB"; gcc-14 about 97 MB, libmysofa about 81 MB), the tar
  246,599,680 bytes with its `SHA256SUMS`, sha256
  `7aecf04017281a101c9cffd2e2733a33ad8cc6ada84df95ca4262383cf7cebf5`, attached to v0.2.0

## Context

ADR 0131 item 9 left one point open because nothing was distributed: the chorus-soloist image
carries unmodified Debian packages, some under LGPL-2.1 or GPL-2, and its notices point at each
Debian source package on snapshot.debian.org. GPL-3 section 6(d) accepts a pointer to a third
party's server; LGPL-2.1 section 4 and GPL-2 section 3 ask for the source to be offered "from
the same place". Since then the repository went public, v0.2.0 was released with
`chorus-soloist-v0.2.0-oci.tar` attached, and CI pushed `ghcr.io/nschatz/chorus-soloist:0.2.0`,
which an anonymous pull reads. The image is distributed now.

## What was read

All on 2026-10-07.

- ADR 0131 item 9, `deploy/soloist/debian-packages.pins` (47 distinct source packages at
  `20260918T000000Z`), `docs/release.md`, the v0.2.0 release's asset list.
- The licence texts as ADR 0131 cites them (`/usr/share/common-licenses` in the base image):
  LGPL-2.1 section 4, GPL-2 section 3, GPL-3 section 6(d).
- The `.dsc` files of gcc-14 14.2.0-19, GLib and PipeWire at the pins' snapshot, for their
  listed file sizes (metadata only; no source file was fetched or opened).

## Options the owner was given

1. Attach the source packages to the release beside the image (recommended).
2. Accept the snapshot.debian.org pointer as written.
3. Stop publishing the image: the ghcr package private, the tarball dropped from releases.

## Decision

1. **Option 1.** Every release that attaches the Soloist image also attaches the Debian source
   packages of every package in it: for each of the 47 source packages that
   `deploy/soloist/debian-packages.pins` names, its `.dsc` and every file the `.dsc` lists, from
   snapshot.debian.org at the pins file's timestamp. The pointer stays in the notices as well;
   it is no longer the only offer.
2. **Verified, not trusted.** Each `.dsc` is held to a sha256 committed in the repository, and
   each file it lists is held to the sha256 the `.dsc` gives; a mismatch or a missing file
   fails the release. No source file is opened or read by anyone (clean-room, BRIEF 3.1):
   the step fetches, hashes and attaches.
3. **v0.2.0 too.** The image is already out at v0.2.0, so its source is attached to the
   v0.2.0 release as well, not only to the next one. The ghcr image's notices name the release
   that carries its source.
4. **What it costs.** A larger release: the 47 source packages come to more than 100 MB, most
   of it gcc-14's (about 97 MB, there for the GCC runtime; GLib's is about 6.4 MB and
   PipeWire's about 1.9 MB), and a fetch from snapshot.debian.org on each release build, cached
   by sha256 like the binary packages.
