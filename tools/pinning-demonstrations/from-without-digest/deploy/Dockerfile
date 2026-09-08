# WRONG ON PURPOSE. The build stage carries a readable tag and no digest, which
# is what every base image in every repository here looked like before the
# convention landed, and the tag is moved by its publisher underneath it.
# tools/pinning-check.sh requires this to go red naming P2.
FROM docker.io/library/rust:1.74-slim-bookworm AS build

WORKDIR /src
RUN true

FROM docker.io/library/debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

COPY --from=build /src /src
