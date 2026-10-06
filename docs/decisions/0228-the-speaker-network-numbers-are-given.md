# 0228: the owner gave the speaker network's VLAN tag, subnet, pool and server address; a homelab task fills them in on the drafted branch, opens it and merges it

- Status: decided by the owner, 2026-10-06 (harness task 28, goals issue #525)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `docs/proposals/P3-speaker-network.md` ("Open inputs"); the work itself is
  homelab harness task 207

## Context

P3 was accepted as option B' on 2026-10-05: a network of its own for chorus speakers, with the
server on it through a spare port. The homelab branch `chorus-g7/speaker-network` (commit
`0d03e7c`) carries the change with four values left null and marked for the owner: the VLAN
tag, the subnet, the DHCP pool and the server's address on the network. Its playbooks refuse to
run while any is null. On 2026-10-06 the branch still had all four null and no pull request.

## Decision

Asked for the numbers, with one set following homelab's existing numbering recommended, the
owner chose the recommended set.

1. The values are written only in the owner's homelab repo, never in chorus (K27): homelab task
   207 carries them, fills them in on the branch, opens it with the body in
   `docs/proposals/P3-speaker-network-homelab-pr.md`, runs `make ci` and merges it.
2. Applying it (OPNsense, the server, the switch port and the one cable, the UniFi network and
   SSID) stays the owner's action (K28), filed for the owner by that task after the merge.
3. Until it is applied, K92's auto-adoption still trusts whatever network the speakers are
   patched into, as P3 says.

## Not chosen

- **Another free tag and /24.** Offered; the owner took the set that follows the existing
  numbering.
- **Not yet.** Offered; it would have left the branch parked with its asserts refusing to run.

## What was read

- goals task 28 and goals issue #525, read 2026-10-06.
- `docs/proposals/P3-speaker-network.md` and `docs/proposals/P3-speaker-network-homelab-pr.md`
  at `origin/main` 06e9d46, read 2026-10-06.
- The owner's homelab repo, branch `chorus-g7/speaker-network` at `0d03e7c`: the opnsense and
  server host_vars files (the four placeholders and the existing networks), read 2026-10-06.
