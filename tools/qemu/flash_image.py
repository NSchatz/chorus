#!/usr/bin/env python3
"""Write an emulator's flash file from an ESP-IDF build directory.

usage: flash_image.py <build directory> <output file> <flash size in MB>

The emulator takes its flash as one file of exactly the flash's size. This
writes that file: every byte 0xFF, as erased flash reads, with each binary the
build's `flash_args` lists copied in at the offset `flash_args` gives it. It
writes one local file and nothing else. It talks to no serial port and no
device, runs no other program, and changes nothing in the build directory.

Why not ESP-IDF's own merge tool: that tool is the flashing program, and this
repository holds every script that names it to the owner-at-bench guard
(docs/conventions.md rule 20, tools/conventions/check-flash-tools-refuse.sh).
A file for an emulator needs none of what it does beyond the copy below, so the
copy is written out here and the guard's rule stays simple: nothing but the one
guarded tool names a flashing program.

What the merge tool would also do is rewrite the flash size in the bootloader's
image header. This does not rewrite it: it checks it. The build wrote the
header from the same configuration the caller read the size from, and a file
whose size disagrees with its own header is refused rather than patched. The
header is ESP-IDF's esp_image_header_t (components/bootloader_support/include/
esp_app_format.h:77-87 in the pinned v6.1, read 2026-10-02): byte 0 is the
magic 0xE9 and the high four bits of byte 3 are the flash size, 0 for 1 MB and
doubling with each step (:66-70).
"""

import os
import sys

HEADER_MAGIC = 0xE9
# esp_image_flash_size_t, the sizes the emulator's esp32s3 machine accepts.
SIZE_CODE_MB = {1: 2, 2: 4, 3: 8, 4: 16}


def fail(message):
    sys.stderr.write("flash_image: " + message + "\n")
    sys.exit(1)


def parts_of(build):
    """The (offset, path) pairs of the build's flash_args, in file order."""
    path = os.path.join(build, "flash_args")
    try:
        with open(path, "r", encoding="utf-8") as handle:
            lines = [line.strip() for line in handle if line.strip()]
    except OSError as error:
        fail("%s could not be read: %s" % (path, error))
    parts = []
    for line in lines:
        if line.startswith("--"):
            # The options line belongs to a flashing program; nothing here needs it.
            continue
        fields = line.split()
        if len(fields) != 2 or not fields[0].lower().startswith("0x"):
            fail("%s: '%s' is not '<offset> <file>'" % (path, line))
        try:
            offset = int(fields[0], 16)
        except ValueError:
            fail("%s: '%s' is not a hexadecimal offset" % (path, fields[0]))
        parts.append((offset, os.path.join(build, fields[1]), fields[1]))
    if not parts:
        fail("%s lists no binary" % path)
    return sorted(parts)


def main(argv):
    if len(argv) != 4:
        fail("usage: flash_image.py <build directory> <output file> <flash size in MB>")
    build, output = argv[1], argv[2]
    try:
        megabytes = int(argv[3])
    except ValueError:
        fail("'%s' is not a flash size in MB" % argv[3])
    if megabytes not in SIZE_CODE_MB.values():
        fail("a %d MB flash is not one the emulator takes (2, 4, 8 or 16 MB)" % megabytes)
    size = megabytes * 1024 * 1024

    image = bytearray(b"\xff" * size)
    end_of_last = 0
    placed = []
    for offset, path, name in parts_of(build):
        try:
            with open(path, "rb") as handle:
                data = handle.read()
        except OSError as error:
            fail("%s could not be read: %s" % (path, error))
        if not data:
            fail("%s is empty" % path)
        if offset < end_of_last:
            fail("%s at 0x%x overlaps the binary before it" % (name, offset))
        if offset + len(data) > size:
            fail("%s at 0x%x (%d bytes) does not fit a %d MB flash" % (name, offset, len(data), megabytes))
        image[offset : offset + len(data)] = data
        end_of_last = offset + len(data)
        placed.append((offset, len(data), name))

    first_offset, _, first_name = placed[0]
    if image[first_offset] != HEADER_MAGIC:
        fail("%s does not start with the image magic 0xE9" % first_name)
    code = image[first_offset + 3] >> 4
    if SIZE_CODE_MB.get(code) != megabytes:
        fail(
            "%s was built for a %s MB flash (header size code %d) and the file asked for is %d MB"
            % (first_name, SIZE_CODE_MB.get(code, "?"), code, megabytes)
        )

    temporary = output + ".writing"
    with open(temporary, "wb") as handle:
        handle.write(image)
    os.replace(temporary, output)
    for offset, length, name in placed:
        print("flash_image: 0x%06x %8d bytes %s" % (offset, length, name))
    print("flash_image: %s, %d MB, %d binaries, the rest erased (0xFF)" % (output, megabytes, len(placed)))


if __name__ == "__main__":
    main(sys.argv)
