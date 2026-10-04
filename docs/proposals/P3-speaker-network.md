# P3: Speaker network placement

- Decisions: K35
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- If deferred: The PR to the owner's homelab repo is drafted on a branch and not opened; the line reads PROPOSED
- Deferred at Checkpoint K (2026-09-30). Goal 7 drafted Option B' on the owner's homelab repo, branch `chorus-g7/speaker-network` (commit `0d03e7c`, no pull request); the body to open it with, including the owner action, is `docs/proposals/P3-speaker-network-homelab-pr.md`. Goal 7's house simulation compares the switched (B') and routed (B) paths (`docs/measurements/sim-house-8-rooms.md`, simulation, not timing evidence)
- Builds on: goal 7 (§11 item 6, "the PR to the owner's homelab repo per §0.1, or the drafted branch if P3 was deferred", done-when F; I9 places the speaker-network PR there); goal 14 (§18, adoption and Wi-Fi provisioning onto this network, K92); goal 24 (§28 item 2, the compact speaker's PoE class "against the switch's 250 W at 8 rooms"); the goal-4 deploy PR (§8, K34 host networking) is its prerequisite in the owner's homelab repo

## Question

Where do chorus speakers live on the house network, and what PR to the owner's homelab repo does that imply? K35:
"Research and propose: a goal weighs IoT VLAN + rule vs a dedicated audio VLAN vs the Servers VLAN
against sync (router hop), security and the staged OPNsense cutover, proposes one, and drafts the
PR to the owner's homelab repo without opening it until the owner decides. Not chosen as final: any of the three."

The owner decisions that bound it, quoted:

- K34: "Grant host networking ... chorus-server joins homelab's host-network exception list".
- K40: "the PWA and control API sit behind homelab's Traefik `lan-only` + household login; chorus
  adds no auth of its own (trusted LAN). Endpoints use the LAN protocol directly."
- K92: "any chorus speaker appearing on **the audio network** is adopted automatically ... speaker
  sessions stay encrypted, and pairing becomes trust-on-first-use (the key is pinned at adoption; a
  changed key is refused and surfaced in the app and HA)." (emphasis added)
- K90: "PoE+ for the compact speaker; mains for two-way, sub, rack amp and soundbar (with
  Ethernet), since the Omada switch is 802.3af/at only."
- K91: "a few compact speakers may run on Wi-Fi (the house's Wi-Fi 5 AP), held to the Wi-Fi
  tier's looser targets and never bonded into stereo pairs or theater sets; every other class is
  wired."
- K75: "Up to 8 rooms (about 20 speakers)". K28: PRs to the owner's homelab repo are opened, never merged, applied or
  deployed by a goal. I9: "the speaker-network PR (P3) in goal 7".

## Constraints that bind every option

- **Sync targets** (BRIEF §2.2): same room and stereo pair < 0.5 ms acceptable (< 0.2 ms
  aspirational); different rooms < 5 ms (< 1 ms). chorus encodes them as `wired_bound_us = 500` and
  `wireless_bound_us = 5000` (`config/transport.conf:40,44`). BRIEF §5.3: the NTP-style offset is
  exact only for symmetric paths, and "network jitter is microseconds on a quiet switched segment".
- **Measurement-backed timing claims** (BRIEF §3.1 rule 3): nothing below claims a number for a
  router hop; the options differ structurally (a hop or none), and goal 7 measures.
- **All chorus traffic is unicast** (speaker protocol over TCP 4010, `firmware/config/endpoint.conf:67`;
  control on the server; OTA pulled from the server; planning research §4.1). No IGMP, PIM or
  multicast routing is needed by any option; only discovery uses mDNS, and discovery is a
  convenience (endpoints keep a configured server address).
- **homelab rules** (K28, brief §0.1): the PR is titled `<type>(<scope>): <summary> (chorus, never
  merged by agents)`, branch `chorus-g<n>/<topic>`, carries the owner action in homelab's
  owner-action format, the rootless checks and a "Related open PRs" section; homelab text says
  "chorus speaker network", never "P3" (the owner's homelab repo's own network phase is called P3).
- **Identity** (K27): no address, subnet, VLAN tag, hostname or SSID from homelab appears here or
  in the PR body; this proposal names networks by role. The PR leaves the numbers (tag, subnet,
  server address in it) to the owner's action, as other programs' PRs to the owner's homelab repo do with their
  added-line address scan.
- CLAUDE.md rule 8: fitness for chorus's requirements only.

## Re-verification of the planning research

The planning research (`research-platform-network.md` §4, 2026-09-29) recommended "b, a dedicated
Audio VLAN, routed at first, with b' as the measured escape hatch", confidence medium, "the
router-hop cost is unmeasured and could make b' the day-one choice". Re-checked 2026-09-30 against
the owner's homelab repo, `origin/main` `d82e2ae` (the sibling clone, pulled; `docs/network.md`, the Ansible
host and firewall variables by key name, the open PRs) and the vendor pages:

| Claim | Re-check | Changed? |
|---|---|---|
| The network is one flat /24 on a UniFi USG today; the OPNsense design is staged | `docs/network.md` "Status: design, staged. The USG still routes the house"; staging steps 1-6, phase A (OPNsense replaces the USG), phase B (VLANs and the readdress) | No |
| Every house network rides one trunk from the switch to the firewall, so a routed speaker hairpins | `docs/network.md` "Topology": one firewall port carries "Mgmt untagged, [the other networks] tagged" to the switch; the server has one uplink port on the Servers network | No |
| IoT may reach the server only on MQTT | `docs/network.md` "Firewall policy": IoT to the server "1883, 8883" only; Servers to IoT "any"; "Anything not listed is blocked" | No |
| The mDNS repeater joins Trusted, IoT and Servers; "requires at least 2 interfaces, and no more than 5" | homelab: "OPNsense's mDNS repeater joins Trusted, IoT and Servers; never Guest, Cameras or Mgmt"; OPNsense docs: "mdns-repeater requires at least 2 interfaces, and no more than 5 interfaces to work", settings are only "Enabled" and "Interfaces" (no per-service filter) | No |
| The switch: 24 PoE+ ports, 802.3af/at, 250 W, 30 W per port | TP-Link ES228GP datasheet: "24× 802.3af/at-compliant PoE+", "250W PoE Budget, with up to 30W for each PoE port", and a footnote: "PoE budget calculations are based on laboratory testing. The actual PoE power budget is not guaranteed". Also: "802.1Q Tag VLAN, Max 32 VLAN Groups", IGMP v1/v2/v3 snooping, 802.1p/DSCP priority, LLDP; ports: "26× 10/100/1000Mbps RJ45 ports (24× 802.3af/at-compliant PoE+)", "2× Gigabit SFP slots" (so two RJ45 ports have no PoE) | Adds the VLAN-group limit, the budget caveat, QoS and the port count |
| The server has spare NICs | `docs/network.md` "The server's NICs": one port is the only uplink, one becomes the firewall admin link, two "stay unused"; a trunk to the server is "worth it only once cameras exist" | New detail: b' can use an untagged spare port instead of a trunk |
| Wi-Fi: one Wi-Fi 5 AP | `docs/network.md` "Switch and Wi-Fi": the same AP, SSIDs tagged into Trusted, IoT and Guest; the IoT SSID is "a new 2.4 GHz WPA2" one | New: the IoT SSID is 2.4 GHz only |
| The Servers network's DHCP pool | `docs/network.md` "Networks and addresses": the Servers pool holds 20 addresses (the other house networks 150) | New: too small for K75's about 20 speakers without a change |
| the owner's homelab repo's own open network PRs touch the same files | `gh pr list` on the owner's homelab repo: #227 (OPNsense plugins incl. the mDNS repeater; `ansible/inventory/host_vars/opnsense.yml`, `ansible/roles/opnsense/*`), #228 (DNS to Unbound; `host_vars/opnsense.yml`, the server's `host_vars` file, ...), #229 (the server readdress; the same two host_vars files, docs), #230 (`ansible/roles/firewall/defaults/main.yml`, `docs/network.md`, `docs/security.md`), #231 and #233 (`docs/network.md` and host_vars files; #231 also `docs/security.md`), all drafts | Adds #228-#233 to the overlap list |
| Router-hop cost | Still unmeasured (planning research §4.2: "ASSUMED tens of microseconds idle, more when the shared trunk is busy") | No; stays ASSUMED, goal 7 measures |

Two facts the planning research did not weigh, and which change the recommendation:

1. **K92 makes the speaker network a trust boundary.** A device that reaches the adoption port on
   "the audio network" is adopted by trust-on-first-use. An adopted impostor receives room audio,
   can stream "mic" audio into the voice path (I4 sends speaker mic audio to HA Assist) and can act
   in the controller role (K65: speaker buttons control a room or group). Placement therefore
   decides who can become a speaker: on IoT, any compromised gadget; on a dedicated network, only
   what the owner plugs into its ports or joins to its SSID.
2. **The ESP32-S3's radio is 2.4 GHz only** ("2.4 GHz Wi-Fi (802.11 b/g/n) with 40 MHz of bandwidth
   support", Espressif), so the K91 Wi-Fi tier on an S3 uses 2.4 GHz whichever SSID it joins; a
   5 GHz-capable radio depends on P1.

Adversarially verified 2026-09-30 (goal-1 verifier 3): 10 claims confirmed, 0 refuted, 4 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Options

All four options use the same switch, the same PoE budget and the same Wi-Fi AP; they differ in
which network the speaker ports and the speaker SSID sit on, and in whether traffic crosses the
firewall.

### Option A: IoT network plus a firewall rule

- What: speaker switch ports and Wi-Fi speakers go on the IoT network (its existing 2.4 GHz SSID).
  One new firewall pass rule: IoT, from a `chorus_speakers` source alias, to the server on a
  `chorus` port alias (the speaker protocol port, the OTA port, a future UDP port); the host
  firewall accepts the same ports from the IoT network (after phase B the host's LAN set excludes
  IoT). mDNS already crosses (IoT is in the repeater).
- Costs: money none (the switch is staged; K38's bench and the speakers are priced elsewhere).
  Effort: the smallest PR to the owner's homelab repo (two aliases, one rule, one host firewall line, docs). Maintenance:
  the source alias is built from DHCP reservations, so **every new speaker needs a reservation, a
  PR to the owner's homelab repo and an owner apply**, or the rule opens the chorus ports to the whole IoT network.
- Risks: K92's auto-adopt then trusts every IoT device (printers, thermostat, ESPHome devices,
  anything compromised there); speaker traffic hairpins through the firewall on the shared trunk; the
  2.4 GHz IoT SSID is the Wi-Fi tier's only path.
- Fit: poor. It either defeats auto-adoption (a PR per speaker) or makes the IoT network the
  adoption boundary.

### Option B: a dedicated audio network, routed through the firewall

- What: a new network for speakers only (a VLAN tag and subnet the owner picks from homelab's
  numbering). Rules: audio to the server on the `chorus` port alias; Servers to audio any
  (server-initiated control and OTA push); audio to the firewall for DHCP, DNS and NTP only; no audio
  to the internet or to any other network; no Trusted to audio (people control speakers through the
  server). The audio network joins the mDNS repeater (4 of its 5 interfaces) or stays out of it
  (discovery by configured address). A new SSID tagged to the audio network for Wi-Fi speakers.
  The host firewall accepts the audio network on chorus's ports only.
- Costs: money none. Effort: a medium PR to the owner's homelab repo (one network, aliases, three rules, the repeater
  line, docs, the switch and SSID steps as owner actions). Maintenance: none per speaker.
- Risks: every audio packet and every time-sync exchange crosses the firewall and the one trunk
  twice (in on the audio network, out on the Servers network), sharing its queue with the whole
  house's internet traffic and the server's other flows; the cost is unmeasured and falls on the
  same-room bound (0.5 ms). The AP's fourth SSID per band (Trusted, IoT, Guest, audio) meets a
  UniFi limit of four per band while wireless meshing is on (`LEAD`, a Ubiquiti help-page snippet).
- Fit: good on security (K92's boundary is exactly the speakers), uncertain on sync until measured.

### Option B': a dedicated audio network with a server leg (recommended)

- What: as B, plus one of the server's two unused 1 GbE ports cabled to a switch port untagged on
  the audio network, with a static address there and no gateway on it. Speakers and the server are
  then on one switched segment: the speaker protocol, time sync and OTA never cross the firewall or
  the house trunk. chorus-server (host networking, K34) binds its speaker listener and its OTA
  listener to that address only; the control API and the PWA stay on the server's main address
  behind Traefik (K40). The host firewall (nftables, policy drop) accepts, on that interface only,
  the chorus speaker ports and mDNS (UDP 5353, which chorus-server answers on that interface) and
  nothing else, which also keeps host-network services (Plex and the rest) unreachable from speakers.
  homelab's mDNS reflector relays only the interfaces it is given (`SERVER_ALLOW_INTERFACES`, "never
  every interface"), and the audio leg is not given to it, so admitting 5353 does not carry speaker
  discovery into Servers. The firewall still serves the audio network's DHCP, DNS and NTP and blocks
  audio to everything else. mDNS works natively on the segment; the audio network stays out of the
  repeater, so speaker discovery does not leak into Trusted or IoT. Speakers also keep a configured
  server address (the audio-leg address), so discovery stays a convenience.
- Costs: money one patch cable (**ASSUMED** on hand; not priced). Effort: B's PR minus the
  audio-to-server rule, plus the server port's configuration in the host Ansible variables and an
  interface-scoped rule in the host firewall role. One more switch port. Maintenance: none per
  speaker.
- Risks: speaker-to-server enforcement moves from the firewall to the host firewall (already
  policy drop; homelab CI checks the Ansible syntax and lint, not the nftables ruleset, so the PR's
  owner check carries the proof); a mis-bound chorus listener or a host-network container
  listening on all addresses is reachable from the segment unless the interface rule drops it (the
  PR carries a test line for it in the owner's check); the server gains a second address to keep
  straight through the readdress (phase B step 4).
- Fit: the best structural fit: the same-room bound is met on "a quiet switched segment" (BRIEF
  §5.3) with no hop to measure away, K92's boundary is the speaker network alone, and chorus's
  bandwidth (about 20 PCM streams at 1.5 Mb/s, about 31 Mb/s, **ASSUMED** 48 kHz 16-bit stereo) no
  longer shares the server's only uplink.

### Option C: the Servers network

- What: speaker ports and a speaker SSID on the Servers network, beside the server; no firewall
  rule; the host firewall already admits the Servers network.
- Costs: money none. Effort: small, but the Servers DHCP pool (20 addresses) must grow for K75's
  about 20 speakers, and a Wi-Fi SSID into the Servers network is new.
- Risks: the weakest security. OTA-updatable embedded devices get the Servers network's reach
  (Servers to the internet any, to IoT any, to the TVs and cameras) and every server port the host
  firewall opens to Servers, including chorus's unauthenticated control API (K40) unless chorus
  binds it elsewhere; auto-adoption (K92) trusts anything plugged into a Servers port.
- Fit: good sync (one segment), poor security; declined.

### The fallback (the "If deferred" line)

Goal 7 drafts the recommended PR on a pushed branch `chorus-g7/speaker-network` of the owner's homelab repo and does
not open it; the goal-7 line reads PROPOSED. Until the owner applies a placement, speakers sit
wherever the house network puts them (today the flat LAN; after phase B, whatever port the owner
patches), and the endpoint keeps its configured server address.

## Comparison

| Criterion | A: IoT + rule | B: audio, routed | B': audio + server leg | C: Servers |
|---|---|---|---|---|
| Sync path (speaker to server) | firewall hop, trunk hairpin | firewall hop, trunk hairpin | one switched segment | one switched segment |
| Shares the server's only uplink | yes | yes | no | yes |
| Who can be auto-adopted (K92) | any IoT device | only audio-network devices | only audio-network devices | any Servers-network device |
| Speaker reach if compromised | IoT's reach plus chorus ports | chorus ports only | chorus ports and mDNS only (host firewall) | Servers' reach, every open server port |
| Control API exposure (K40) | not from IoT | not from audio | not from audio (interface rule) | reachable unless bound elsewhere |
| mDNS | already repeated | repeater slot 4 of 5, or none | native on the segment (host accepts UDP 5353 on the audio leg), not repeated | native |
| Per-speaker homelab work | a reservation and PR each | none | none | none (pool grown once) |
| Wi-Fi tier (K91) | existing 2.4 GHz IoT SSID | new audio SSID (4th per band, LEAD) | new audio SSID (4th per band, LEAD) | new SSID into Servers |
| PoE (K90) | same for all: 250 W, 30 W per port | same | same, one more port used | same |
| Enforcement point | firewall | firewall | firewall (audio to elsewhere) and host firewall (to the server) | host firewall only |
| size of the PR to the owner's homelab repo | small | medium | medium | small |

PoE, the same for every option: eight Class-4 compact speakers at the 30 W per-port maximum take
240 W of the datasheet's 250 W (which TP-Link does not guarantee); the AP is also powered from the
switch (the UAP-AC-LR draws at most 6.5 W, Ubiquiti datasheet; its 802.3af class is not published).
If the switch allocates by class (**ASSUMED**), the AP may reserve 15.4 W and seven Class-4 speakers
is the ceiling (240 + 15.4 W exceeds 250 W); eight fit only if the switch budgets by measured draw
or the AP advertises Class 2 or lower. Goal 24 reads the switch's PoE page with the AP attached.
Nine do not fit either way. Two-ways, subs, the rack amp and the soundbar are mains-powered (K90).
The switch has 26 RJ45 ports (24 PoE+ and 2 without PoE) plus 2 SFP slots. With the trunk, the AP,
the server, its management port and (B') the server's audio leg on five ports, 21 RJ45 ports remain
(19 to 21 of them PoE+, since the unpowered links can use the two non-PoE RJ45 ports), plus two SFP
slots, for house devices and wired speakers; the room list (a goal-1 Needs item) decides whether per-room switches
carrying the audio network are needed.

## Recommendation

**Recommendation:** Option B', a dedicated audio network with the server on it through a spare port, because it makes the speaker network alone the K92 adoption boundary (as B does) while keeping the speaker protocol and time sync on one switched segment, off the firewall and off the server's only uplink.

Why: A is ruled out by K92 (auto-adoption would trust the IoT network, or every speaker would need
a PR), C by security (embedded, OTA-updated devices with the Servers network's reach beside an
unauthenticated control API). Between the two audio-network variants, B' removes the one
unmeasured risk to the same-room bound (a firewall hop on a trunk shared with the house's internet
traffic) instead of measuring it and then moving, and it costs about the same PR to the owner's homelab repo plus one
cable and one switch port. What the owner gives up: the server stops being single-homed, and
speaker-to-server filtering is done by the host firewall rather than OPNsense. If the owner prefers
a single-homed server, B is the second choice; goal 7's software measurement of a routed versus a
switched path (planning research §4.2) then says whether B is enough, and a B-to-B' move later is one
cable and one host PR.

### The PR to the owner's homelab repo this implies (drafted in goal 7, never merged by a goal)

- Branch `chorus-g7/speaker-network`; title `feat(network): chorus speaker network (chorus, never
  merged by agents)`. Named "chorus speaker network" throughout, never "P3". Based on homelab `main`
  as goal 7 finds it, after re-reading `docs/network.md`'s cutover state (brief §11 item 6); it
  applies only once phase B has created the VLANs, and after the goal-4 deploy PR (K34) has put
  chorus-server on the host network.
- Files and what each gains (the numbers are the owner's to fill in the owner action, so the PR
  adds no address):
  - `ansible/inventory/host_vars/opnsense.yml`: an `audio` entry in `opnsense_networks` (tag,
    subnet and pool from the owner action); `audio` in `opnsense_house_ifs` (DHCP, DNS and NTP from
    the firewall); no pass rule from audio to anywhere else, and no Trusted-to-audio rule (the
    default block covers both). Not added to the mDNS repeater's interfaces. (Option B instead adds
    a `chorus` entry to `opnsense_port_aliases` and an audio-to-`server` rule.)
  - the server's `ansible/inventory/host_vars/<server>.yml`: the server's audio-leg port and its static address
    (from the owner action) with no gateway; a firewall variable listing chorus's speaker ports for
    that interface.
  - The host network and firewall roles (`ansible/roles/base` or the role that already configures
    the firewall admin link's port, and `ansible/roles/firewall/templates/host.nft.j2` with its
    `defaults/main.yml`): bring the port up; accept chorus's speaker ports and UDP 5353 (mDNS) on
    that interface only, drop everything else there; the mDNS reflector's interface list is not
    given the audio leg.
  - `docs/network.md`: the audio network's row in "Networks and addresses"; its rows in "Firewall
    policy"; "Switch and Wi-Fi": the server's audio-leg port, the speaker ports' untagged network,
    the AP port's tagged list gaining the audio network, the speaker SSID (2.4 GHz for S3 speakers,
    per P1) tagged into it; "DHCP, NTP and discovery": the audio network is not repeated; "The
    server's NICs": the spare port's new role.
  - `docs/security.md`: why speakers are isolated (K92's trust-on-first-use adoption, K40's
    unauthenticated control plane) and that the host firewall is the speaker-to-server filter.
- Body: what changes; the owner action in the owner's homelab repo's owner-action format (pick the tag and subnet;
  patch the server's spare port; set the switch ports and the AP port in the switch's UI; add the
  SSID in the UniFi UI; `opnsense-apply.sh --check` then `--apply`; `host-apply.sh --check` then
  `--apply`; the check that a speaker port reaches only chorus's ports on the server and nothing
  else); the rootless checks pasted (yamllint, shellcheck where scripts change, gitleaks, CI's inline
  Python checks, the added-line address scan); "Related open PRs" with the per-hunk overlap against
  #227 (the repeater and `opnsense.yml`), #228 and #229 (`opnsense.yml`, the server's host_vars file, the
  readdress), #230 (`ansible/roles/firewall/defaults/main.yml`, `docs/network.md`, `docs/security.md`),
  #231 and #233 (`docs/network.md`, host_vars files; #231 also `docs/security.md`), and any other
  open PR goal 7 finds; a note that the owner or a session in the owner's homelab repo runs
  `make ci` and merges (K28).

## If the owner defers

Goal 7 drafts the PR above (option B') on the pushed branch `chorus-g7/speaker-network` and does
not open it; its done-when F is met by the branch, the goal-7 ledger line reads PROPOSED, and the
proposal is listed under "Proposals awaiting the owner" to the finale. Nothing else in chorus waits
on it: endpoints keep a configured server address, adoption (goal 14) and the sync work run on the
simulator and fixtures. The cost is that no placement is live when the owner's first speakers
arrive, and K92's auto-adoption then trusts whatever network the owner patches them into.

## Open inputs

- The owner's choice among A, B, B', C (Checkpoint K).
- The audio network's VLAN tag, subnet and the server's address on it: the owner's, in the owner
  action (never written by chorus).
- The room list with Cat6, PoE and speaker-wire runs (goal-1 Needs item): decides port count and
  whether per-room switches are needed.
- Router-hop cost on the real firewall: **ASSUMED** tens of microseconds idle (planning research);
  goal 7 measures a routed versus switched path in software; the real network is NEEDS-OWNER after
  the cutover.
- The AP's SSID limit (four per band with meshing on): **LEAD** from a search snippet of Ubiquiti's
  help page (the page refused a direct read); verify in the UniFi UI.
- The AP's PoE draw: at most 6.5 W (Ubiquiti's UniFi AC APs datasheet, UAP-AC-LR).
- The AP's 802.3af PoE class: not published; under class allocation it may reserve 15.4 W, which
  sets the ceiling at seven Class-4 speakers. Goal 24 reads it on the switch's PoE page.
- The Switch's PoE allocation mode (by class or by measured draw): **ASSUMED** by class for the
  ceiling above (seven speakers by class, eight only by measured draw or an AP at Class 2 or lower).
- A spare patch cable: **ASSUMED** on hand.

## Sources

- OPNsense documentation, "Multicast DNS" how-to, https://docs.opnsense.org/manual/how-tos/multicast-dns.html, read 2026-09-30
- TP-Link, ES228GP(UN) 1.0 datasheet, https://static.tp-link.com/upload/product-overview/2025/202512/20251209/ES228GP(UN)1.0_Datasheet.pdf, read 2026-09-30
- Espressif, ESP32-S3 product page, https://www.espressif.com/en/products/socs/esp32-s3, read 2026-09-30
- Ubiquiti, UniFi AC APs datasheet (UAP-AC-LR: "802.3af/A PoE", "Maximum Power Consumption 6.5W"), https://dl.ui.com/datasheets/unifi/UniFi_AC_APs_DS.pdf, read 2026-09-30
- Ubiquiti help centre, "Broadcasting Multiple WiFi SSIDs", https://help.ui.com/hc/en-us/articles/15320966415127-Broadcasting-Multiple-WiFi-SSIDs, search snippet only (403 on fetch), 2026-09-30, `LEAD`
- The owner's homelab repo's `docs/network.md` (Status, At a glance, Topology, Networks and addresses, Firewall policy, DHCP NTP and discovery, Switch and Wi-Fi, Configuration as code, Staging, Cutover phase B, MQTT plan, The server's NICs), `ansible/inventory/host_vars/opnsense.yml` (top-level key names), `ansible/roles/firewall/defaults/main.yml`, the server's `ansible/inventory/host_vars/<server>.yml` (firewall keys), `ansible/roles/firewall/templates/host.nft.j2` (`policy drop`), `.github/workflows/ci.yml` ("Ansible syntax and lint": `ansible-playbook --syntax-check` and `ansible-lint` only), `home-automation/mdns-reflector/docker-compose.yml` (`SERVER_ALLOW_INTERFACES`), `origin/main` `d82e2ae`, read 2026-09-30
- The owner's homelab repo's open PRs #191, #192, #208, #224, #225, #227-#231, #233 (`gh pr list`; files of #227, #228, #229 via `gh pr view`, of #230, #231, #233 via `gh pr list --json files`), read 2026-09-30
- chorus `BRIEF.md` §2.2, §5.3; `config/transport.conf:40,44`; `config/sync.conf:43`; `firmware/config/endpoint.conf:67` (baseline snapshot of `origin/main`), read 2026-09-30
- Goal-1 verifier 3 report, `/cache/tmp/chorus-g1/verify/verify-3.md` (P3 section), read 2026-09-30
- `research-platform-network.md` §0 item 8, §4, §5; `review-homelab.md` §0, §1, §2, §7; `review-cross-program-v1.md` M7 (chorus planning research), read 2026-09-30

## What was read

chorus: [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) (§0.1, §0.3, §0.7, §0.13, §1, §2, §3.4, §4.8,
§5, §11), the research files `research-platform-network.md`, `review-homelab.md`,
`review-cross-program-v1.md`, `research-ha-ma-sources.md` §4-§5 (MQTT and security facts only),
`/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`, `/cache/tmp/chorus-g1/verify/verify-3.md`, and the baseline
snapshot files above. homelab (`/cache/wt/chorus/sib/homelab`, pulled, read-only, nothing committed
or pushed): the files and PRs listed under Sources. Web: the five URLs under Sources, plus two web
searches (the ES228GP datasheet, the UniFi SSID limit). No GPL source and no reciprocal hardware
design file was opened. No address, subnet, VLAN tag, hostname or SSID from homelab is written here.
