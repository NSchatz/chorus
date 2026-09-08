# CORRECT ON PURPOSE, and that is what makes this tree the demonstration. Its
# one FROM carries a tag and a digest, so the dockerfile-from category is clean;
# every other category the scanner examines matches nothing at all here.
#
# A scanner that reported this tree green would be reporting on four patterns
# that stopped matching, which is indistinguishable from a compliant tree unless
# it says so. tools/pinning-check.sh requires this to go red naming the
# categories that examined nothing.
FROM docker.io/library/debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

RUN true
