# The ESP32-S3 endpoint, as an ESP-IDF project.
#
# `tools/firmware-image.sh` (make firmware-image) drives this, and refuses by
# name before it gets here when the toolchain is absent or is not the version
# firmware/config/endpoint.conf declares.
#
# Everything under firmware/src is shared with the host build and is graded
# there, on a machine with no ESP32-S3. What is here and nowhere else is
# firmware/main, which binds those units to ESP-IDF's own drivers. That binding
# is NOT host-gradable and this repository does not claim it: the two criteria
# that grade it, AC-1 and AC-3, are the operator ones, and
# tools/endpoint-rig-run.sh refuses by name until somebody has the hardware.

cmake_minimum_required(VERSION 3.16)

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(chorus-endpoint)
