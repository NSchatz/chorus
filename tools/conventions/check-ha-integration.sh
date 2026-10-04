#!/usr/bin/env bash
# Rule "The Home Assistant integration" (goal 18; brief section 4.8): cheap, with no virtual
# environment. What it holds, each a way the integration could quietly stop being what
# docs/home-assistant.md says it is:
#   - the manifest lists no runtime requirement (Home Assistant would `uv pip install` one
#     into its container at every recreate) and the project declares no dependency;
#   - the vendored client imports nothing from Home Assistant and ships py.typed;
#   - strings.json and translations/en.json are the same bytes;
#   - the test harness in pyproject.toml and uv.lock is the recorded pin (harness.pin): its
#     version, its wheel's sha256, the Home Assistant it requires, and the Python; every
#     locked package comes from PyPI with a sha256;
#   - quality_scale.yaml lists exactly the rule names of the pinned tier list
#     (quality-scale-rules.txt), each `done` or `exempt` with a comment;
#   - no view, webhook, static path or injected script (the grep-level backstop of
#     tests/test_no_unauthenticated_endpoint.py, which is the real check and runs in the
#     gate step `ha-integration`). The first authenticated view or local-only webhook comes
#     with a change to this pattern and to that test's expectations.
# It proves its endpoint pattern first, on one line of each kind that must match.
. "$(dirname "$0")/lib.sh"
rule="The Home Assistant integration"
rc=0
bad() { fail "$rule" "$1"; rc=1; }
P=integrations/homeassistant
C="$P/custom_components/chorus"
pin() { sed -n "s/^$1 = \(.*\)$/\1/p" "$P/harness.pin"; }

[ -f "$C/manifest.json" ] || { bad "$C/manifest.json is missing"; exit 1; }

# --- no runtime requirement ---------------------------------------------------------
[ "$(jq -c '.requirements' "$C/manifest.json")" = '[]' ] ||
    bad "$C/manifest.json: \"requirements\" is not []; the client is vendored in _aiochorus so Home Assistant installs nothing"
for k in domain:chorus iot_class:local_push config_flow:true integration_type:hub; do
    [ "$(jq -r ".${k%%:*}" "$C/manifest.json")" = "${k#*:}" ] || bad "$C/manifest.json: ${k%%:*} is not ${k#*:}"
done
[ "$(jq -c '.zeroconf' "$C/manifest.json")" = '["_chorus-ctl._tcp.local."]' ] ||
    bad "$C/manifest.json: zeroconf is not [\"_chorus-ctl._tcp.local.\"]"
command grep -q -x 'dependencies = \[\]' "$P/pyproject.toml" ||
    bad "$P/pyproject.toml declares a runtime dependency; the integration ships none"
command grep -q -x 'license = "MIT OR Apache-2.0"' "$P/pyproject.toml" ||
    bad "$P/pyproject.toml does not carry the project licence (MIT OR Apache-2.0)"

# --- the vendored client stands alone -------------------------------------------------
[ -f "$C/_aiochorus/py.typed" ] || bad "$C/_aiochorus/py.typed is missing (PEP 561)"
if git grep -n -E '^[[:space:]]*(from|import)[[:space:]]+homeassistant' -- "$C/_aiochorus"; then
    bad "the vendored client imports Home Assistant; it takes an injected aiohttp session and nothing else"
fi

# --- translations -----------------------------------------------------------------
cmp -s "$C/strings.json" "$C/translations/en.json" ||
    bad "$C/strings.json and translations/en.json differ; copy strings.json over en.json"

# --- the harness pin ----------------------------------------------------------------
harness="$(pin harness)"
hv="$(pin harness-version)"
sha="$(pin harness-wheel-sha256)"
ha="$(pin homeassistant)"
py="$(pin python)"
tag="$(pin core-tag)"
commit="$(pin core-commit)"
[[ "$hv" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && "$ha" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && "$py" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
    bad "$P/harness.pin: harness-version, homeassistant and python are exact x.y.z versions"
[[ "$sha" =~ ^[0-9a-f]{64}$ && "$commit" =~ ^[0-9a-f]{40}$ ]] ||
    bad "$P/harness.pin: the wheel's sha256 and the core tag's commit are full digests"
[ "$tag" = "$ha" ] || bad "$P/harness.pin: core-tag ($tag) is not the pinned Home Assistant ($ha)"
command grep -q -F "\"$harness==$hv\"" "$P/pyproject.toml" ||
    bad "$P/pyproject.toml does not pin $harness==$hv (harness.pin)"
command grep -q -x "requires-python = \"==$py\"" "$P/pyproject.toml" ||
    bad "$P/pyproject.toml: requires-python is not ==$py (harness.pin)"
[ "$(cat "$P/.python-version" 2> /dev/null)" = "$py" ] || bad "$P/.python-version is not $py (harness.pin)"
locked() { awk -v n="$1" '$0 == "name = \"" n "\"" { getline; gsub(/version = |"/, ""); print; exit }' "$P/uv.lock"; }
[ "$(locked "$harness")" = "$hv" ] || bad "$P/uv.lock locks $harness $(locked "$harness"), not $hv (harness.pin)"
[ "$(locked homeassistant)" = "$ha" ] || bad "$P/uv.lock locks homeassistant $(locked homeassistant), not $ha (harness.pin)"
command grep -q -F "hash = \"sha256:$sha\"" "$P/uv.lock" ||
    bad "$P/uv.lock does not hold the harness wheel's recorded sha256 (harness.pin)"
while IFS= read -r l; do
    [[ "$l" == *'dev = ['* || "$l" == *'hassfest = ['* || "$l" == ']' || "$l" =~ ^[[:space:]]*\"[A-Za-z0-9_.-]+==[0-9][0-9A-Za-z.]*\",$ ]] ||
        bad "$P/pyproject.toml: a development dependency is not pinned exactly: $l"
done < <(sed -n '/^\[dependency-groups\]/,/^\[/p' "$P/pyproject.toml" | command grep -v -E '^(\[|#|$)')
# Every locked file is a PyPI file with its sha256: no git source, no URL without a hash.
if command grep -n -E 'source = \{ (git|url|path) ' "$P/uv.lock"; then
    bad "$P/uv.lock has a package that is not from the PyPI registry"
fi
if command grep -E '\{ url = ' "$P/uv.lock" | command grep -v -q 'hash = "sha256:[0-9a-f]\{64\}"'; then
    bad "$P/uv.lock has a file without a sha256"
fi
files="$(command grep -c -E '\{ url = ' "$P/uv.lock")"

# --- the quality scale ---------------------------------------------------------------
want="$(command grep -v '^#' "$P/quality-scale-rules.txt" | awk '{print $2}' | sort)"
have="$(sed -n 's/^  \([a-z][a-z-]*\):.*/\1/p' "$C/quality_scale.yaml" | sort)"
[ -n "$want" ] || bad "$P/quality-scale-rules.txt lists no rule"
[ "$want" = "$have" ] || {
    diff <(printf '%s\n' "$want") <(printf '%s\n' "$have") | sed 's/^/  /'
    bad "$C/quality_scale.yaml does not list exactly the rules of $P/quality-scale-rules.txt (< pinned, > the file)"
}
command grep -q -F "$tag" "$P/quality-scale-rules.txt" || bad "$P/quality-scale-rules.txt does not cite the pinned tag $tag"
verdicts="$(awk '
    /^  [a-z][a-z-]*:/ {
        if (rule != "" && status == "exempt" && !comment) print "exempt-without-comment " rule
        rule = $1; sub(/:$/, "", rule); status = $2; comment = 0
        if (status == "") next
        if (status != "done") print "bad-status " rule
        else done++
        next
    }
    /^    status:/ { status = $2; if (status == "exempt") exempt++; else if (status == "done") done++; else print "bad-status " rule }
    /^    comment: ./ { comment = 1 }
    END {
        if (rule != "" && status == "exempt" && !comment) print "exempt-without-comment " rule
        print "counts " done + 0 " " exempt + 0
    }' "$C/quality_scale.yaml")"
if printf '%s\n' "$verdicts" | command grep -v '^counts '; then
    bad "$C/quality_scale.yaml: every rule is \`done\` or \`exempt\` with a comment"
fi

# --- no endpoint (the backstop) ---------------------------------------------------------
pattern='HomeAssistantView|register_view|register_static_path|async_register_static_paths|StaticPathConfig|add_extra_js_url|components[. ]+(import[[:space:]]+)?webhook|requires_auth|register_redirect'
for probe in 'class ArtView(HomeAssistantView):' 'hass.http.register_view(ArtView)' \
    '    requires_auth = False' 'hass.http.register_static_path("/x", "/y")' \
    'await hass.http.async_register_static_paths([StaticPathConfig("/x", "/y", True)])' \
    'add_extra_js_url(hass, "/x.js")' 'from homeassistant.components import webhook' \
    'from homeassistant.components.webhook import async_register' 'hass.http.register_redirect("/a", "/b")'; do
    printf '%s\n' "$probe" | command grep -q -E "$pattern" ||
        bad "the endpoint pattern no longer matches '$probe' (the check is broken, not the tree)"
done
if git grep -n -E "$pattern" -- "$C" ':!*.json' ':!*.yaml'; then
    bad "the integration registers an HTTP view, a webhook, a static path or a script; it has none (brief 4.8). A view requires auth and a webhook is local-only, and each comes with a change to this check and to tests/test_no_unauthenticated_endpoint.py"
fi

[ "$rc" = 0 ] && echo "home assistant integration: no requirement, client stands alone, translations equal, harness $harness $hv (Home Assistant $ha, Python $py) with $files hashed files locked, $(printf '%s\n' "$want" | wc -l) quality-scale rules ($(printf '%s\n' "$verdicts" | sed -n 's/^counts \([0-9]*\) \([0-9]*\)$/\1 done, \2 exempt/p')), no endpoint"
exit "$rc"
