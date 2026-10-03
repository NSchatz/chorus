# Third-party software in the chorus-soloist image

chorus itself is MIT OR Apache-2.0. `/usr/local/bin/chorus-soloistd` is chorus's own program:
a static binary built from the chorus workspace, with no third-party code linked into it. It
links none of the libraries below; it starts PipeWire and WirePlumber as separate programs.

**This image contains no Spotify software.** Spotify Soloist is proprietary; chorus does not
ship it. Whoever runs this image mounts their own Soloist binary at `/opt/soloist`.

Everything else in the image is Debian 13 (trixie):

- the base image `docker.io/library/debian:trixie-20260918-slim`, unmodified. Each of its
  packages carries its licence at `/usr/share/doc/<package>/copyright`, and its package list
  is `/var/lib/dpkg/status`;
- the Debian binary packages in the table below, each unpacked unmodified from the `.deb`
  Debian published (the sha256 of each is in `deploy/soloist/debian-packages.pins` in the
  chorus repository). Each carries its licence at `/usr/share/doc/<package>/copyright`, as
  Debian ships it, and its control file is `/var/lib/dpkg/status.d/<package>`. The only
  generated file is `/etc/ld.so.cache`, the loader's index of the libraries, which Debian's
  own `ldconfig` wrote.

The licences those copyright files state include the MIT (Expat) licence (PipeWire, apart
from the parts Debian marks LGPL-2.1-or-later, and WirePlumber), the GNU LGPL 2.1 or later
(GLib, libpulse, libsndfile, ALSA's library and others), the GNU GPL with the GCC Runtime
Library Exception (the GCC runtime libraries) and BSD-style licences. The copyright file of
each package is the authority, not this summary.

## Source

The complete corresponding source of every package below is its Debian source package, at
the same archive snapshot the binary came from: in the directory named in the last column,
the files `<source package>_<source version>.dsc` and the archives that `.dsc` names (an
epoch such as `2:` is not part of a file name). The base image's packages are in the same
snapshot, `https://snapshot.debian.org/archive/debian/20260918T000000Z/`, under
`pool/main/`. snapshot.debian.org is the Debian project's permanent archive of every
package it has published.

The libraries are separate shared objects in `/usr/lib/x86_64-linux-gnu`, used through
their published interfaces by the programs Debian built against them: replacing one with a
modified build is replacing that file.

## The packages
