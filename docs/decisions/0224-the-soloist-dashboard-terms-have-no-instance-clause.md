# 0224: the Spotify Soloist terms shown in the developer dashboard have no clause on instances, servers or commercial use; the pool of 16 slots stands

- Status: decided by the owner, 2026-10-06 (harness task 18, goals issue #94)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `docs/proposals/P7-spotify-soloist.md` ("Terms and limits as found", "Open
  inputs")

## Context

chorus runs one official Soloist receiver per room, saved group and live group (K66), from a pool
of 16 slots (P7, accepted with the pool by the owner on 2026-10-05). The public Soloist docs, the
Developer Terms v10, the Developer Policy and the consumer Terms of Use state no limit on instances
per API key, account or household. The Soloist Terms and Conditions shown in the developer
dashboard before a key is generated are behind a login no agent uses (K4), so P7 left them as an
open input for the owner.

## Decision

Asked whether the dashboard terms carry a clause on the number of Soloist instances or devices
(per key, account, host or household), on running Soloist in a container or on a server, or on
personal versus commercial use, the owner answered: no such clause.

1. P7 records the answer; its instance-limit row no longer waits on the dashboard.
2. The pool of 16 slots stands as accepted on 2026-10-05; nothing in the code or the images
   changes.
3. Still open in P7, and not answered by this: the owner's reading of the Developer Terms'
   "private personal use ... on Approved Devices" grant and the Developer Policy's clauses
   (webcasting, visual media) as they bear on chorus's controller. Reported, never a reason to
   drop Soloist (K66).
