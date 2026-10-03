# Third-party code in the chorus-server image

chorus itself is MIT OR Apache-2.0. The `chorus-server` binary in this image also contains
the code below, statically linked. `docs/decoders.md` and
`docs/decisions/0000-the-server-decoders.md` in the chorus repository say why each is there.

## libopus 1.6.1 (BSD-3-Clause)

The Opus decoder. Its licence text, which a binary distribution must reproduce, is the file
`libopus-COPYING` beside this one (the `COPYING` file of the libopus 1.6.1 release,
<https://downloads.xiph.org/releases/opus/opus-1.6.1.tar.gz>).

## Symphonia (MPL-2.0)

The demuxers and decoders for MP3, FLAC, Ogg Vorbis, ALAC in MP4 and WAV: the `symphonia`
crates, used unmodified as published on crates.io. The Mozilla Public License 2.0 is at
<https://www.mozilla.org/en-US/MPL/2.0/>. The source of each crate, in the exact version this
binary was built from, is at `https://crates.io/crates/<name>/<version>` (the download
`https://static.crates.io/crates/<name>/<name>-<version>.crate`), and every chorus release
attaches those `.crate` files beside this image. The crates and versions in this build:

