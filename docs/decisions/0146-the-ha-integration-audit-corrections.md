# 0146: the Home Assistant integration's `dependency-transparency` exemption rests on the manifest listing no requirement, not on the repository being private, and the endpoint audit covers every webhook added and every route put on a router directly

- Status: accepted, 2026-10-04; corrects two statements of 0138. The decisions of 0138 (the
  tier, the vendored client, no endpoint) are unchanged.
- Recorded by: the owner's agent harness, in the pull request that adds this record and
  `docs/audit/2026-10-ha-integration.md`.
- Implemented in: `integrations/homeassistant/custom_components/chorus/quality_scale.yaml`
  (one comment), `integrations/homeassistant/tests/endpoint_audit.py`,
  `tests/test_no_unauthenticated_endpoint.py`, `tests/test_media_player.py`,
  `tools/conventions/check-ha-integration.sh`

## Context

An adversarial check of the integration's three finish lines broke the integration on purpose,
49 ways, and ran the narrow tests against each (`docs/audit/2026-10-ha-integration.md`). It
found two statements of 0138 that were not true.

## What was read

All on 2026-10-04. No GPL source was opened.

- Home Assistant core (Apache-2.0) at tag 2026.9.3:
  https://raw.githubusercontent.com/home-assistant/core/2026.9.3/script/hassfest/quality_scale.py
  (`ALL_RULES`, compared with `integrations/homeassistant/quality-scale-rules.txt`: the same
  54 rules).
- `gh repo view --json visibility` for this repository: `PUBLIC`.
- chorus's own: 0138, 0136, 0142, `docs/home-assistant.md`, `docs/control-plane.md`, the
  integration and its tests, `tools/conventions/check-ha-integration.sh`.

## Decision

**The `dependency-transparency` exemption.** 0138 gave as its reason that "a private
repository cannot publish the client as an open PyPI package built by public CI". The
repository is public, so that reason is false. The exemption stands on the other half of
0138's reasoning, which is true and is the one the brief asks for: the manifest lists no
requirement, because a requirement is installed into the Home Assistant container at start
and again after every recreate, so the client is vendored and there is no dependency to
publish. `quality_scale.yaml` says that now. For the same reason 0138's "Not chosen" entry
for a PyPI client rests on its second clause alone.

**The endpoint audit.** 0138 says `tests/test_no_unauthenticated_endpoint.py` "would catch
the first one done wrong, at run time and statically". Two planted registrations passed both
halves: a webhook that is not local-only registered under another domain's name in a form the
static scan cannot read, and a route put on the aiohttp router directly after setup. The
run-time half now audits every webhook added since its snapshot, whatever domain it names,
and the static half and the grep backstop report a route put on a router directly. What still
escapes is a registration that is both hidden from the static scan and made after setup; the
test is for a mistake, and review is the control for code written to hide.

**What 0138 listed as ASSUMED is observed.** `make ha-live` passed against a server built
from the audited tree, and the tests' fake agreed with that server on 22 of 22 command steps
(the audit report has the sequences): `join` to a room in a saved group keeps the saved
group's id, a `take` without a source leaves what the fake says, and a 1.5 s clip is seen as
`Announcement`.

## Not chosen

- **Marking `dependency-transparency` as `done`.** The rule is about the dependencies an
  integration requires; with none, `exempt` with the true reason says more than `done`.
- **Publishing the client now that the repository is public.** It would put a requirement in
  the manifest, which is what the vendoring avoids.
- **Editing 0138 in place.** A record says what was decided when; the correction is this one.

## Consequences

- BRIEF.md does not state the false reason and is unchanged.
- `integrations/homeassistant/README.md` and `docs/conventions.md` each still call the
  repository private in one sentence; the audit report lists both as open.
