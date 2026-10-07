# 0000: the Soloist image's releases attach the Debian source packages of every package in it, fetched from snapshot.debian.org and verified against the .dsc files, so LGPL-2.1 and GPL-2 source is offered from the same place as the image

- Status: decided by the owner, 2026-10-07 (harness task 316, filed by task 263, the 2026-09
  program report); supersedes the open point of ADR 0131 item 9 ("the owner either accepts the
  pointer ... or attaches the 47 source packages")
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: not yet; harness task 321 builds it (item 3 below)

## Context

ADR 0131 item 9 left one point open because nothing was distributed: the chorus-soloist image
carries unmodified Debian packages, some under LGPL-2.1 or GPL-2, and its notices point at each
Debian source package on snapshot.debian.org. GPL-3 section 6(d) accepts a pointer to a third
party's server; LGPL-2.1 section 4 and GPL-2 section 3 ask for the source to be offered "from
the same place". Since then the repository went public, v0.2.0 was released with
`chorus-soloist-v0.2.0-oci.tar` attached, and CI pushed `ghcr.io/nschatz/chorus-soloist:0.2.0`,
which an anonymous pull reads. The image is distributed now.

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
4. **What it costs.** A larger release (the 47 source packages, tens of MB; PipeWire's and
   GLib's are the largest) and a fetch from snapshot.debian.org on each release build, cached
   by sha256 like the binary packages.
