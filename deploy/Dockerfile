# The chorus server, containerized.
#
# What matters here is not the image, it is what the RUN command grants it.
# `sched(7)`: the RLIMIT_RTPRIO resource limit "defines a ceiling on an
# unprivileged thread's static priority for the SCHED_RR and SCHED_FIFO
# policies", and Docker hands that ceiling in per container with
# `--ulimit rtprio=<n>`. Nothing in a Dockerfile can grant it; only the run
# can. deploy/run-server.sh is therefore the other half of this file, and
# neither is useful alone.
#
# The server refuses to start when the ceiling is zero or when locking memory
# is denied, unless configuration explicitly says otherwise, so a container
# started without those limits fails loudly rather than quietly running as an
# ordinary process.

# Both bases carry a tag AND the digest the tag resolved to. The tag stays
# readable; the digest is what actually resolves, and it is what makes two
# builds of this file the same build. deploy/README.md records when each digest
# was resolved and the exact command that re-resolves it.
# The build base is the toolchain rust-toolchain.toml pins (B-16); 1.74 could not
# build the workspace.
FROM docker.io/library/rust:1.98.1-slim-bookworm@sha256:ff521445a372125ed4f76e1453a1f8098f2d05332d1601d30db1c1f62757e730 AS build

WORKDIR /src
COPY Cargo.toml ./
COPY Cargo.lock ./
COPY crates ./crates
COPY fixtures ./fixtures
COPY audio-path.conf ./
# The server compiles the control page's documentation into itself
# (crates/server/src/control.rs includes docs/control-page.md), so the build
# context carries that one document (B-14).
COPY docs/control-page.md ./docs/control-page.md

# No external dependencies, so no network is needed past the base image.
# --locked is not belt and braces: it makes a resolution that would change the
# committed lock a build failure rather than a silent difference between this
# image and the workspace it was built from.
RUN cargo build --locked --release --workspace --bins

FROM docker.io/library/debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

# libasound2 is needed only by the client. The server neither opens nor links
# an audio device, and this image is the server's.
RUN useradd --system --create-home --uid 10001 chorus

COPY --from=build /src/target/release/chorus-server /usr/local/bin/chorus-server
COPY --from=build /src/target/release/chorus-rt-spin /usr/local/bin/chorus-rt-spin
COPY config/verification.conf /etc/chorus/verification.conf

RUN mkdir -p /var/lib/chorus && chown chorus /var/lib/chorus
VOLUME /var/lib/chorus

USER chorus
EXPOSE 4010 4020

# No default that hides the contract: the entrypoint takes the priority and the
# CPU-time bound from the run command, and refuses if the host granted neither.
ENTRYPOINT ["/usr/local/bin/chorus-server"]
# The deployable shape (B-15): audio, the control plane and persisted zones.
CMD ["--listen", "0.0.0.0:4010", "--control-listen", "0.0.0.0:4020", "--state-file", "/var/lib/chorus/zones.state", "--source", "tone", "--serve-forever"]
