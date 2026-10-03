#!/usr/bin/env python3
"""`make soloist-lists`: what chorus ships, listed, and held to "no Soloist file".

Spotify Soloist is proprietary: chorus never ships it, in an image, a release or
the repository (docs/soloist.md; docs/conventions.md rule 24). This prints

  (a) the files chorus adds to the chorus-server image (the staged tree `make image` left),
  (b) the chorus-soloist image: what chorus adds, and the Debian packages
      (`--full` prints every package with its sha256 instead of the short form),
  (c) the release artifact names (`tools/release.sh --list`),
  (d) the tracked files whose path names soloist, by kind,

and FAILS, naming the path, when

  1. a path in an image or a release names soloist and is not one of chorus's own
     (the lists ALLOWED_* below), or /opt/soloist is anything but an empty directory;
  2. the fake Soloist is in an image or a release: by name (`fake-soloist`,
     `soloist-fake`, `soloist_fake`) or by content (a file carrying the fake's
     `FAKE_SOLOIST_` settings);
  3. a regular file chorus adds to an image is executable (a mode bit or an ELF
     header) and is not, byte for byte, a `[[bin]]` of this workspace as built
     for the image;
  4. a tracked file naming soloist is of no known kind, or is a binary or an archive.

It first proves itself on scratch trees (a planted `soloist` binary, the fake under
another name, a foreign executable, a Soloist archive in a release, a file under
/opt/soloist), each of which must fail by name. It builds nothing: it reads the
trees `make image` and `make soloist-image` left under target/image and refuses by
name when one is absent.
"""

import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "x86_64-unknown-linux-musl"
SOLOIST = re.compile("soloist", re.I)
FAKE_NAME = re.compile(r"fake[-_]?soloist|soloist[-_]?fake", re.I)
FAKE_MARK = b"FAKE_SOLOIST_"

# chorus's own names in the chorus-soloist image: the supervisor and the mount points.
ALLOWED_IMAGE = {
    "usr/local/bin/chorus-soloistd": "file",
    "opt/soloist": "dir",              # the owner's read-only mount: empty in the image
    "run/chorus/soloist": "dir",       # the receiver directory
    "run/chorus-soloist": "dir",       # the runtime directory (a tmpfs)
    "var/lib/chorus-soloist": "dir",   # state
    "var/cache/chorus-soloist": "dir",  # cache
}
# chorus's own names in a release.
ALLOWED_RELEASE = re.compile(r"^chorus-soloist-v[0-9]+\.[0-9]+\.[0-9]+(-oci\.tar|-NOTICES\.md)$")
# The kinds of tracked file that may name soloist, first match wins.
TRACKED = [
    ("source", re.compile(r"^crates/soloist(d|-fake)?/.*\.(rs|toml)$")),
    ("fixtures", re.compile(r"^fixtures/soloist/[a-z0-9-]+\.(json|line|txt)$")),
    ("docs", re.compile(r"^docs/(soloist\.md|decisions/[0-9]{4}-[a-z0-9-]+\.md|proposals/P7-spotify-soloist\.md)$")),
    ("config", re.compile(r"^deploy/soloist/(compose\.yaml|debian-packages\.pins|THIRD-PARTY-NOTICES\.md)$")),
    ("tools", re.compile(r"^tools/soloist-(image\.sh|image-test\.py|lists\.py)$")),
]
BINARY_SUFFIX = re.compile(r"\.(tar|tgz|gz|xz|zst|zip|deb|rpm|bin|so|AppImage)$", re.I)


def walk(tree):
    """Every path under tree, relative, with its lstat."""
    for base, dirs, files in os.walk(tree):
        for name in dirs + files:
            path = os.path.join(base, name)
            yield os.path.relpath(path, tree), os.lstat(path)


def head(path, size=1 << 16):
    with open(path, "rb") as f:
        return f.read(size)


def carries(path, mark):
    try:
        with open(path, "rb") as f:
            return mark in f.read()
    except PermissionError:
        # An unpacked rootfs keeps the modes of files such as /etc/shadow.
        return False


def check_names(label, tree, allowed, problems):
    """Rules 1 and 2 over a tree: paths naming soloist, and the fake by name or content."""
    for rel, st in walk(tree):
        path = os.path.join(tree, rel)
        if FAKE_NAME.search(rel):
            problems.append("%s: /%s is the fake Soloist, which no image or release may carry" % (label, rel))
        elif SOLOIST.search(rel):
            kind = allowed.get(rel)
            if kind is None:
                problems.append("%s: /%s names soloist and is not one of chorus's own paths" % (label, rel))
            elif (kind == "dir") != stat.S_ISDIR(st.st_mode) or (kind == "file" and not stat.S_ISREG(st.st_mode)):
                problems.append("%s: /%s is not the %s chorus stages there" % (label, rel, kind))
        if stat.S_ISREG(st.st_mode) and carries(path, FAKE_MARK):
            problems.append("%s: /%s carries the fake Soloist's settings (%s)" % (label, rel, FAKE_MARK.decode()))


def check_executables(label, tree, built, bins, problems):
    """Rule 3 over a tree chorus adds: every executable is a workspace bin as built."""
    for rel, st in walk(tree):
        if not stat.S_ISREG(st.st_mode):
            continue
        path = os.path.join(tree, rel)
        if not (st.st_mode & 0o111 or head(path, 4) == b"\x7fELF"):
            continue
        name = os.path.basename(rel)
        ours = os.path.join(built, name)
        if name not in bins:
            problems.append("%s: /%s is executable and is not a [[bin]] of this workspace" % (label, rel))
        elif not os.path.isfile(ours) or head(ours, 1 << 30) != head(path, 1 << 30):
            problems.append("%s: /%s is not the %s this workspace built (%s)" % (label, rel, name, ours))


def check_release(names, problems):
    for name in names:
        if FAKE_NAME.search(name):
            problems.append("release: %s is the fake Soloist, which no release may carry" % name)
        elif SOLOIST.search(name) and not ALLOWED_RELEASE.match(name):
            problems.append("release: %s names soloist and is not one of chorus's own artifacts" % name)


def classify(tracked, read, problems):
    counts = {}
    for path in tracked:
        kind = next((k for k, pattern in TRACKED if pattern.match(path)), None)
        if kind is None:
            problems.append("tracked: %s names soloist and is of no known kind (source, fixtures, docs, config, tools)" % path)
            continue
        data = read(path)
        if BINARY_SUFFIX.search(path) or data[:4] == b"\x7fELF" or b"\0" in data:
            problems.append("tracked: %s is a binary or an archive" % path)
            continue
        counts[kind] = counts.get(kind, 0) + 1
    return counts


def self_test():
    """Each planted fault must be named; the clean tree must pass."""
    scratch = tempfile.mkdtemp(prefix="chorus-soloist-lists.")
    try:
        built = os.path.join(scratch, "built")
        os.makedirs(built)
        good = b"\x7fELF chorus-soloistd as built"
        with open(os.path.join(built, "chorus-soloistd"), "wb") as f:
            f.write(good)
        bins = {"chorus-soloistd", "chorus-server"}

        def tree(name, files, dirs=("opt/soloist",)):
            top = os.path.join(scratch, name)
            for d in dirs:
                os.makedirs(os.path.join(top, d))
            for rel, (data, mode) in files.items():
                path = os.path.join(top, rel)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "wb") as f:
                    f.write(data)
                os.chmod(path, mode)
            return top

        def run(top):
            problems = []
            check_names("fixture", top, ALLOWED_IMAGE, problems)
            check_executables("fixture", top, built, bins, problems)
            return problems

        clean = {"usr/local/bin/chorus-soloistd": (good, 0o755), "etc/passwd": (b"root:x:0:0\n", 0o644)}
        cases = [
            ("a planted soloist binary", dict(clean, **{"usr/local/bin/soloist": (b"\x7fELF spotify", 0o755)}),
             "/usr/local/bin/soloist names soloist"),
            ("a file under /opt/soloist", dict(clean, **{"opt/soloist/soloist": (b"\x7fELF spotify", 0o755)}),
             "/opt/soloist/soloist names soloist"),
            ("the fake by name", dict(clean, **{"usr/local/bin/chorus-fake-soloist": (b"\x7fELF", 0o755)}),
             "/usr/local/bin/chorus-fake-soloist is the fake Soloist"),
            ("the fake's library", dict(clean, **{"usr/lib/libchorus_soloist_fake.rlib": (b"!<arch>", 0o644)}),
             "/usr/lib/libchorus_soloist_fake.rlib is the fake Soloist"),
            ("the fake under another name", dict(clean, **{"usr/local/bin/chorus-server": (b"\x7fELF FAKE_SOLOIST_CONTROL", 0o755)}),
             "/usr/local/bin/chorus-server carries the fake Soloist's settings"),
            ("a foreign executable", dict(clean, **{"usr/local/bin/helper": (b"#!/bin/sh\n", 0o755)}),
             "/usr/local/bin/helper is executable and is not a [[bin]]"),
            ("a foreign ELF without the mode bit", dict(clean, **{"usr/share/blob": (b"\x7fELF x", 0o644)}),
             "/usr/share/blob is executable and is not a [[bin]]"),
            ("another build of our own bin", {"usr/local/bin/chorus-soloistd": (b"\x7fELF other", 0o755)},
             "/usr/local/bin/chorus-soloistd is not the chorus-soloistd this workspace built"),
        ]
        if run(tree("clean", clean)):
            return "the clean fixture tree fails: %s" % run(os.path.join(scratch, "clean"))
        for i, (what, files, want) in enumerate(cases):
            found = run(tree("case%d" % i, files))
            if not any(want in line for line in found):
                return "%s was not refused by name (wanted %r, got %s)" % (what, want, found)
        problems = []
        check_release(["chorus-soloist-v1.2.3-oci.tar", "chorus-soloist-v1.2.3-NOTICES.md", "chorus-server-v1.2.3-oci.tar"], problems)
        if problems:
            return "chorus's own release names fail: %s" % problems
        for name in ("soloist-1.3.8-linux-x86_64.tar.gz", "chorus-fake-soloist", "chorus-soloist-v1.2.3-bin.tar"):
            problems = []
            check_release([name], problems)
            if not problems:
                return "the release name %s was not refused" % name
        problems = []
        blobs = {"crates/soloistd/src/main.rs": b"fn main() {}\n", "third_party/soloist/soloist": b"\x7fELF", "fixtures/soloist/build.tar": b"x"}
        classify(sorted(blobs), blobs.get, problems)
        if len(problems) != 2:
            return "a tracked Soloist binary or archive was not refused: %s" % problems
        return None
    finally:
        shutil.rmtree(scratch)


def out(cmd):
    return subprocess.run(cmd, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def listing(tree):
    files, dirs = [], []
    for rel, st in sorted(walk(tree)):
        (dirs if stat.S_ISDIR(st.st_mode) else files).append("/" + rel)
    # A directory is listed only when chorus put nothing under it: a mount point.
    return files, [d for d in dirs if not any(p.startswith(d + "/") for p in files + dirs)]


def wrap(items, width=96, indent="    "):
    line = indent
    for item in items:
        if len(line) + len(item) + 1 > width and line.strip():
            print(line.rstrip())
            line = indent
        line += item + " "
    if line.strip():
        print(line.rstrip())


def main():
    full = "--full" in sys.argv[1:]
    why = self_test()
    if why:
        print("soloist-lists: FAIL: the check does not prove itself: " + why)
        return 1
    print("soloist-lists: self-test: 8 planted trees, 3 release names and 2 tracked files each refused by name")

    td = os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target"))
    built = os.path.join(td, TARGET, "release")
    trees = {
        "server": os.path.join(td, "image/work/stage"),
        "server-rootfs": os.path.join(td, "image/work/test/bundle/rootfs"),
        "soloist": os.path.join(td, "image/soloist-work/stage-chorus"),
        "soloist-debian": os.path.join(td, "image/soloist-work/stage-debian"),
        "soloist-rootfs": os.path.join(td, "image/soloist-work/test/bundle/rootfs"),
    }
    for name, path in trees.items():
        if not os.path.isdir(path):
            target = "make image" if name.startswith("server") else "make soloist-image"
            print("soloist-lists: REFUSED: %s is absent; run `%s` first (this check builds nothing)" % (path, target))
            return 2
    meta = json.loads(out(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"]))
    bins = {t["name"] for p in meta["packages"] for t in p["targets"] if "bin" in t["kind"]}
    problems = []

    files, mounts = listing(trees["server"])
    print("(a) chorus-server image: %d files and %d directories added by chorus (tools/image.sh)" % (len(files), len(mounts)))
    wrap(files + [m + "/" for m in mounts])
    check_names("chorus-server image", trees["server"], {}, problems)
    check_names("chorus-server image (unpacked)", trees["server-rootfs"], {}, problems)
    check_executables("chorus-server image", trees["server"], built, bins, problems)

    files, mounts = listing(trees["soloist"])
    total = sum(1 for _ in walk(trees["soloist-rootfs"]))
    print("(b) chorus-soloist image: %d paths in all; %d files and %d mount points added by chorus (tools/soloist-image.sh)"
          % (total, len(files), len(mounts)))
    wrap(files + [m + "/" for m in mounts])
    pins = [line.split() for line in open(os.path.join(ROOT, "deploy/soloist/debian-packages.pins"))
            if line.strip() and not line.startswith("#")]
    pins_sha = out(["sha256sum", "deploy/soloist/debian-packages.pins"]).split()[0]
    print("    Debian packages: %d, on debian:trixie-slim; name=version below, each sha256 in" % len(pins))
    print("    deploy/soloist/debian-packages.pins (sha256 %s)" % pins_sha)
    if full:
        for p in pins:
            print("    %s %s %s" % (p[0], p[1], p[3]))
    else:
        wrap(["%s=%s" % (p[0], p[1]) for p in pins])
    for p in pins:
        if SOLOIST.search(p[0]) or SOLOIST.search(p[5]):
            problems.append("chorus-soloist image: the Debian package %s names soloist" % p[0])
    check_names("chorus-soloist image", trees["soloist"], ALLOWED_IMAGE, problems)
    check_names("chorus-soloist image (Debian packages)", trees["soloist-debian"], {}, problems)
    check_names("chorus-soloist image (unpacked)", trees["soloist-rootfs"], ALLOWED_IMAGE, problems)
    check_executables("chorus-soloist image", trees["soloist"], built, bins, problems)
    if os.listdir(os.path.join(trees["soloist-rootfs"], "opt/soloist")):
        problems.append("chorus-soloist image: /opt/soloist is not empty")

    names = out(["bash", "tools/release.sh", "--list"]).split()
    print("(c) release artifacts: %d (tools/release.sh --list; none built here)" % len(names))
    crates = [n for n in names if n.endswith(".crate")]
    wrap([n for n in names if not n.endswith(".crate")] + ["and %d MPL-2.0 source crates (%s ... %s)" % (len(crates), crates[0], crates[-1])])
    check_release(names, problems)

    tracked = [p for p in out(["git", "ls-files"]).split("\n") if p and SOLOIST.search(p)]
    counts = classify(tracked, lambda p: head(os.path.join(ROOT, p)) if os.path.isfile(os.path.join(ROOT, p)) else b"", problems)
    print("(d) tracked files naming soloist: %d, all chorus's own: %s"
          % (len(tracked), ", ".join("%d %s" % (counts[k], k) for k, _ in TRACKED if k in counts)))
    print("    no binary and no archive among them; the fake Soloist is source only (crates/soloist-fake, an example of crates/soloistd)")

    if problems:
        for line in problems:
            print("soloist-lists: FAIL: " + line)
        print("soloist-lists: FAIL (%d): docs/conventions.md rule 24: chorus ships no Soloist file and no fake" % len(problems))
        return 1
    print("soloist-lists: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
