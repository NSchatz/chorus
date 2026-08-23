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

FROM docker.io/library/rust:1.74-slim-bookworm AS build

WORKDIR /src
COPY Cargo.toml ./
COPY crates ./crates
COPY fixtures ./fixtures
COPY audio-path.conf ./

# No external dependencies, so no network is needed past the base image.
RUN cargo build --release --workspace --bins

FROM docker.io/library/debian:bookworm-slim

# libasound2 is needed only by the client. The server neither opens nor links
# an audio device, and this image is the server's.
RUN useradd --system --create-home --uid 10001 chorus

COPY --from=build /src/target/release/chorus-server /usr/local/bin/chorus-server
COPY --from=build /src/target/release/chorus-rt-spin /usr/local/bin/chorus-rt-spin
COPY config/verification.conf /etc/chorus/verification.conf

USER chorus
EXPOSE 4010

# No default that hides the contract: the entrypoint takes the priority and the
# CPU-time bound from the run command, and refuses if the host granted neither.
ENTRYPOINT ["/usr/local/bin/chorus-server"]
CMD ["--listen", "0.0.0.0:4010", "--source", "tone", "--serve-forever"]
