#!/usr/bin/env bash
# `make ha-hassfest`: Home Assistant's own hassfest over the chorus integration, from a core
# checkout at the pinned tag (integrations/homeassistant/harness.pin).
#
# hassfest validates quality_scale.yaml (and runs the quality-scale validators: config flow,
# runtime data, test before setup, unique config entry, discovery, reconfiguration flow,
# strict typing) only for an integration under homeassistant/components of a core checkout
# (script/hassfest/quality_scale.py, `if not integration.core: return`). So this runs it as
# core, proves that run graded the file, and runs it as custom:
#
#   1. as core: the integration is copied to homeassistant/components/chorus of a THROWAWAY
#      worktree of the pinned checkout, with exactly the changes being core demands and
#      nothing else: `version` and `issue_tracker` leave the manifest (the core schema has
#      neither), `documentation` becomes the core form, the domain joins `.strict-typing`,
#      and the vendored `brand/` directory is left out (core's brands live in another
#      repository). Everything else is the integration byte for byte.
#   2. the proof: the same copy with one rule removed from quality_scale.yaml must fail.
#   3. as custom: the tracked tree, unpatched, with hassfest's custom-integration schema.
#
# The checkout is a shallow clone of the tag, verified against the commit the pin records,
# kept OUTSIDE the repository; the network is needed only the first time. hassfest runs on
# the integration's own environment (`uv sync --locked --all-groups`: the harness brings
# Home Assistant itself, the `hassfest` group what hassfest imports beyond it).
#
# Environment:
#   CHORUS_HA_CORE          where the checkout is kept (default /cache/chorus-ha-core where
#                           /cache exists, else under $XDG_CACHE_HOME or ~/.cache)
#   UV_PROJECT_ENVIRONMENT  as tools/ha-test.sh
#
# A gate step of the full tier (`ha-hassfest`), about a quarter of a minute warm. Where uv or
# the checkout cannot be had it fails by name, under CI as anywhere else: a gate step that did
# not run is red, never a green SKIPPED
# (docs/decisions/0142-the-home-assistant-gate-steps-run-or-fail.md).

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
T0=$(date +%s.%N)
PROJECT="$ROOT/integrations/homeassistant"
PIN="$PROJECT/harness.pin"
pin() { sed -n "s/^$1 = \(.*\)$/\1/p" "$PIN"; }
TAG="$(pin core-tag)"
SHA="$(pin core-commit)"
[ -n "$TAG" ] && [ -n "$SHA" ] || { echo "FAIL: $PIN names no core-tag and core-commit"; exit 1; }

if ! command -v uv > /dev/null 2>&1 && command -v mise > /dev/null 2>&1; then
    eval "$(MISE_TRUSTED_CONFIG_PATHS="$ROOT" mise env -s bash 2> /dev/null)"
fi
command -v uv > /dev/null 2>&1 || {
    echo "FAIL: uv is not on PATH; hassfest runs on the integration's environment, which the pinned uv of mise.toml installs: install the pinned tools rootless with \`mise install\`"
    exit 1
}
if [ -d /cache ] && [ -w /cache ]; then
    CACHE=/cache
else
    CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/chorus"
fi
export UV_PROJECT_ENVIRONMENT="${UV_PROJECT_ENVIRONMENT:-$CACHE/venvs/chorus-ha}"
export PYTHONDONTWRITEBYTECODE=1
CORE="${CHORUS_HA_CORE:-$CACHE/chorus-ha-core}/$TAG"

if [ ! -d "$CORE/.git" ]; then
    echo "ha-hassfest: cloning home-assistant/core at $TAG into $CORE (once)"
    mkdir -p "$(dirname "$CORE")"
    git clone --quiet --depth 1 --branch "$TAG" https://github.com/home-assistant/core.git "$CORE" || {
        rm -rf "$CORE"
        echo "FAIL: could not clone home-assistant/core at $TAG; hassfest needs the checkout at $CORE (the network, once)"
        exit 1
    }
fi
have="$(git -C "$CORE" rev-parse HEAD)"
[ "$have" = "$SHA" ] || {
    echo "FAIL: $CORE is at $have, and the pin ($PIN) says tag $TAG is $SHA"
    exit 1
}
[ -z "$(git -C "$CORE" status --porcelain)" ] || {
    echo "FAIL: $CORE has local changes; it is a pristine checkout of $TAG, never edited"
    exit 1
}

(cd "$PROJECT" && uv sync --locked --all-groups --quiet)
VENV_BIN="$UV_PROJECT_ENVIRONMENT/bin"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/chorus-ha-hassfest.XXXXXX")"
cleanup() {
    git -C "$CORE" worktree remove --force "$WORK/core" > /dev/null 2>&1 || true
    git -C "$CORE" worktree prune > /dev/null 2>&1 || true
    rm -rf "$WORK"
}
trap cleanup EXIT
git -C "$CORE" worktree add --quiet --detach "$WORK/core" "$SHA"

# The tracked files of the integration, and nothing a run left beside them.
SRC="integrations/homeassistant/custom_components/chorus"
DEST="$WORK/core/homeassistant/components/chorus"
CUSTOM="$WORK/custom_components/chorus"
mkdir -p "$DEST" "$CUSTOM"
git ls-files -z -- "$SRC" | while IFS= read -r -d '' f; do
    rel="${f#"$SRC"/}"
    mkdir -p "$CUSTOM/$(dirname "$rel")"
    cp "$f" "$CUSTOM/$rel"
    case "$rel" in brand/*) continue ;; esac
    mkdir -p "$DEST/$(dirname "$rel")"
    cp "$f" "$DEST/$rel"
done

# Exactly what being core demands of the manifest and the checkout.
"$VENV_BIN/python" - "$DEST/manifest.json" << 'PY'
import json, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as f:
    manifest = json.load(f)
removed = [key for key in ("version", "issue_tracker") if manifest.pop(key, None) is not None]
manifest["documentation"] = f"https://www.home-assistant.io/integrations/{manifest['domain']}"
with open(path, "w", encoding="utf-8") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
print(f"ha-hassfest: core copy: removed {', '.join(removed)}; documentation -> {manifest['documentation']}")
PY
printf 'homeassistant.components.chorus.*\n' >> "$WORK/core/.strict-typing"
echo "ha-hassfest: core copy: homeassistant.components.chorus.* added to .strict-typing"

rc=0
cd "$WORK/core"
echo "ha-hassfest: 1/3 as a core integration (home-assistant/core $TAG, $SHA)"
PATH="$VENV_BIN:$PATH" python -m script.hassfest --action validate \
    --integration-path homeassistant/components/chorus || rc=1

# That the run above graded quality_scale.yaml and did not skip it: the same copy with one
# rule taken out of the file must fail, naming the quality scale.
echo "ha-hassfest: 2/3 the proof: the core copy with the strict-typing rule removed must fail"
sed -i '/^  strict-typing: done$/d' homeassistant/components/chorus/quality_scale.yaml
proof="$(PATH="$VENV_BIN:$PATH" python -m script.hassfest --action validate \
    --integration-path homeassistant/components/chorus 2>&1)" && {
    echo "FAIL: hassfest passed a quality_scale.yaml with a rule missing: it is not grading the file"
    rc=1
}
printf '%s\n' "$proof" | grep -F '[QUALITY_SCALE]' || {
    echo "FAIL: the failing run did not name the quality scale"
    printf '%s\n' "$proof" | tail -n 20
    rc=1
}
rm -rf homeassistant/components/chorus

echo "ha-hassfest: 3/3 as a custom integration (the tracked tree, unpatched)"
PATH="$VENV_BIN:$PATH" python -m script.hassfest --action validate \
    --integration-path "$CUSTOM" || rc=1

T1=$(date +%s.%N)
if [ "$rc" -ne 0 ]; then
    awk -v a="$T0" -v b="$T1" 'BEGIN{printf "ha-hassfest: FAIL, wall-clock %.1fs\n", b-a}'
    exit 1
fi
awk -v a="$T0" -v b="$T1" 'BEGIN{printf "ha-hassfest: PASS (core and custom), wall-clock %.1fs\n", b-a}'
