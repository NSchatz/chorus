# P7: Spotify through Soloist

- Decisions: K63, K66
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- Outcome: approved at Checkpoint K, Option C (`docs/decisions/0130-the-soloist-receiver-supervisor.md` to `0132-the-soloist-receivers-in-the-server.md`) (recorded 2026-10-06)
- If deferred: One instance per room only, on bridge networking
- Builds on: goal 4 (§8, BRIEF §2.3's streaming line per R7), goal 11 (§15 item 3, alarms; Soloist sources join in goal 17), goal 17 (§21, Soloist sidecars, alarm sources, P11 and the homelab receivers PR), goal 18 (§22, sources and group volume in HA), goal 27 (§31, no Soloist file in any release)

## Question

How do Spotify (and phone apps) reach chorus under the strict legal bar, given that the owner has
already chosen Spotify Soloist? The brief's §5 cell: "Spotify through Soloist: its terms and
instance limits as found (reported, never a reason to drop Soloist, K66), the sidecar and sink
design, and its network (bridge with the existing mDNS reflector, or a new host-network exception
the owner must grant)".

The owner decisions that bound it:

- K63: "Research and propose ... how Spotify and phone apps reach chorus" under K60.
- K66: "**Yes, it's official**: Soloist passes K60's strict bar ... Plan: one Soloist instance per
  room, saved group and live group (K59), each feeding a chorus audio sink; the owner creates the
  API key and downloads builds (they expire after 90 days; no redistribution), a recurring Needs
  Owner item; K63's proposal still checks Spotify's terms and any instance limits and reports
  them. Not chosen: only if terms are clear; no."
- K59: every room, saved group and live group is a cast target in each supported protocol.
- K65: metadata and artwork flow source to server to app, HA and endpoints; any controller
  controls its room or group.
- K77: "Sonos-style: the group slider scales every room relatively"; K78: "Take the room".
- K80: an alarm may start "a Spotify playlist started on the room's Soloist instance through
  Soloist's local API (if that API allows starting playback; research checks)".
- K34: host networking was granted to chorus-server only; a second exception is the owner's call.

## Constraints that bind every option

- **K60 strict bar and K66.** Soloist is Spotify's own software, run unmodified. No wrapper that
  interposes on its process (for example a substitute `libpulse`, which one integrator describes
  in Soloist issue #2, LEAD) is used: chorus consumes audio only through the documented PipeWire
  or PulseAudio output and controls it only through the documented WebSocket API.
- **Terms are reported, never a reason to drop Soloist (K66).** Where a clause bears on a chorus
  feature, this proposal changes the feature's design or flags it for the owner; it never removes
  Soloist.
- **K4 and §0.6.** Creating the API key, downloading builds and reading the dashboard terms are
  owner steps. chorus never downloads, ships or redistributes a Soloist file: the binary is
  mounted from a host volume (goal 17 line A, goal 27 line B check the image and release lists).
- **BRIEF §2.3 ("No DRM streaming integrations in this codebase"), as R7 rewords it:** "official
  receivers run as separate processes, no DRM code in chorus". Soloist does its own DRM; chorus
  receives PCM.
- **BRIEF §3.1 rule 4 (monotonic clocks).** Soloist's `position.timestamp_ms` is "Server
  wall-clock time as Unix epoch milliseconds" (WebSocket reference): it may drive the app's
  progress bar, never the audio path.
- **Security (§4.8).** The WebSocket API "has no built-in client authentication, authorization,
  TLS, Origin validation, CSRF protection" (WebSocket reference): it is bound to loopback inside a
  namespace nothing else shares. The API key is a secret: never in the repo, a log or a PR body
  (K27).
- **homelab rules (cited from the owner's homelab repo at `d82e2ae`, read 2026-09-30).** The baseline is
  "never `privileged` or the host network" (`CLAUDE.md`, Security baseline); `docs/security.md`
  says "bridge networks, not the host's" and keeps "the one list of deviations", which
  `make ci` enforces "name for name" (`.github/workflows/ci.yml`, `EXCEPT = {... "network_mode
  host": {"mdns-reflector", "plex"} ...}`). Today only plex and the mDNS reflector have host
  networking. Every service needs `cap_drop: [ALL]`, a justified minimal `cap_add`,
  `no-new-privileges`, `mem_limit`, `pids_limit`, `cpus`, a healthcheck, json-file logs and an
  image pinned by digest. chorus opens PRs to the owner's homelab repo and never merges them (K28, §0.1).
- **Rule 8.** Every choice below is argued on fitness for chorus's requirements alone.

## Re-verification of the planning research

Every Soloist page, the Developer Terms and the Music Assistant page were fetched again on
2026-09-30 (Sources). Result per planning claim (`research-casting-decoders.md` §3,
`verify-ha-casting.md` claim 8):

| Planning claim | Re-checked 2026-09-30 | Change |
|---|---|---|
| Official headless Connect client for Linux arm64, arm32, x86_64; PipeWire or PulseAudio output | Confirmed (overview, getting started, command line). The arm64 build is "primarily tested on Raspberry Pi 3 Model A+ running the Debian Trixie-based Raspberry Pi OS (glibc 2.41)" | None |
| Flags `-n`, `-k`, `-D`, `-C`, `-z`, `-d/--pipewire-device`, `-i`, `-s`, `-p`, `-w`; exit 10 = expired | Confirmed | New: exit 1 covers "another Spotify Soloist process already using the data directory", so every instance needs its own data directory; "Spotify Soloist does not currently read a configuration file" |
| WebSocket API: `play` with optional `uri`, events with metadata, cover URLs, `position_sync`, `activate`/`deactivate`, `queue_changed` capped at 10 | Confirmed | New: `get_queue` with `limit` 0 returns "all available entries"; `command_result` "does not guarantee that playback state has already changed"; timestamps are wall clock (above) |
| Premium needed to create the key; the key is personal | Confirmed | New wording: "Each Spotify Soloist user must generate their own key from Spotify for Developers" |
| Builds expire 90 days after their build date; no redistribution; `soloist --version` prints the build timestamp | Confirmed | New: Soloist "logs the remaining lifetime at startup, logs warnings as expiry approaches" (overview) |
| The Soloist terms are behind the dashboard login, NOT FETCHED | **Changed in part.** The getting-started step says "Review the Spotify Terms and Conditions of Use and accept them if prompted", and that link is the public consumer agreement (spotify.com/legal/end-user-agreement); the Soloist landing page says "By using Spotify Soloist, you accept the Spotify Terms and Conditions of Use" with the same link | Read and reported below. Whether the dashboard shows any Soloist-specific clause is still unseen, so the Needs item stays, narrowed to that question |
| Developer Terms v10 (15 May 2025): "private personal use", no reverse engineering, "may not use more than one Security Code for each SDA" | Confirmed, still Version 10 | New: the licence grant also says "on Approved Devices", and the Developer Policy (read this time) has clauses on alarms, voice, overlapping audio, content analysis and synchronising with visual media (below) |
| No instance limit per key, account or household in any public page | Confirmed: none in the docs, blog, README, Developer Terms, Developer Policy, Terms of Use or User Guidelines | None |
| Music Assistant: Spotify's terms "do not clearly allow" its use of Soloist | Confirmed verbatim; MA also downloads Soloist "on your behalf ... and updated automatically" | chorus does neither (K4) |
| Design: one `chorus-soloist` container on **host network**, one Soloist process per target | **Changed.** Soloist issue #1 (LEAD, 2026-08-18, soloist 1.3.7.292): on a host "where avahi-daemon owns UDP 5353", under `network_mode: host`, Soloist "cannot bind 5353, so it never advertises `_spotify-connect._tcp`, and nothing in the log says so"; "the identical setup in its own network namespace (docker macvlan, 5353 free)" works. The homelab's mDNS reflector is avahi on the host network. Issue #4 (LEAD): the Connect port is ephemeral per start | The network design changes (Options) |
| Resource cost per instance: no published figures | Still none. Issue #8 (LEAD): `--cache-size 200` grew to 431 MB after hours | Cache is budgeted on disk, not trusted to the flag |

Adversarially verified 2026-09-30 (goal-1 verifier 2): 15 claims confirmed, 0 refuted, 2 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Terms and limits as found (reported, never a reason to drop Soloist)

Not legal advice. "Inference" marks a reading, not a fact.

| Source and clause | What it says | What it means for chorus |
|---|---|---|
| Soloist landing page | "By using Spotify Soloist, you accept the Spotify Terms and Conditions of Use" | The consumer agreement governs Soloist use |
| Terms of Use (last updated 4 Sep 2026) | "limited, non-exclusive, revocable permission to make personal, non-commercial use of the Spotify Service" | A household system fits |
| User Guidelines | Not permitted: "copying, reproducing, redistributing, 'ripping,' recording, transferring, performing ... or any other use which is not expressly permitted", and "transferring copies of cached Content from an authorized Device to any other Device" | chorus carries PCM live to its own speakers, never writes it to disk, never touches Soloist's cache. Spotify's own pages describe this use: "Connect Spotify Soloist to a local amplifier, DAC, or Bluetooth speaker" (landing page) and "a home server feeding a multi-room setup" (launch blog). Inference: chorus's use is the use Spotify describes |
| Developer Terms v10 | "Spotify Developer Application" or "SDA" is "any application, website or service that accesses the Spotify Service or Spotify Content through ... the Spotify Platform"; the Platform is the tools and documentation on the Developer Website | Inference: chorus's controller, which drives Soloist through the documented WebSocket API, may count as an SDA. Then the Developer Terms and Policy below apply to it |
| Developer Terms v10, Security Codes | "You must use a separate Security Code for each SDA. You may not use more than one Security Code for each SDA" | Ambiguous for N instances sharing one API key. Nothing public caps instances; the dashboard clause is the owner's Needs item |
| Developer Terms v10, licence | SDAs only "(i) for private personal use; and (ii) on Approved Devices; and (iii) in accordance with the Developer Policy", where "'Approved Devices' means only desktop computers, laptops, netbook PCs, tablets, mobile, and such other devices that we approve in writing from time to time" | Inference: bears only if chorus's controller is an SDA; a home server driving speakers is not on the list, while Spotify's own Soloist pages describe server and multi-room use ("a home server feeding a multi-room setup", launch blog). Reported to the owner, never a reason to drop Soloist (K66); the Needs item asks the owner's reading |
| Developer Policy | "Display metadata and cover art when streaming ... there shall be no playback of Spotify Content without showing relevant cover art and metadata in your SDA" | chorus meets it by design (K65): the app and HA show the item, artists and cover |
| Developer Policy | "Do not create ringtone or alert tone functionality or alarm functionality in an SDA, unless you receive Spotify's written approval" | Bears on K80's Spotify alarm source. Reported to the owner; design response below |
| Developer Policy | "Do not create a voice-enabled SDA that enables a user to control Spotify with their voice, or any kind of voice assistant that provides voice-control functionality" | Bears on K71 as a whole if chorus's controller is an SDA (row above): the second limb reaches any voice assistant in the SDA, not only voice commands to a Soloist source. Flagged to P8 and goal 20 as an owner question; at minimum, no voice intent targets a Soloist source |
| Developer Policy | "Do not permit any device or system to segue, mix, re-mix, or overlap any Spotify Content with any other audio content (including other Spotify Content)" | Announcements pause a Soloist source instead of ducking over it; a room never mixes a Soloist source with another source. Two Soloist instances are never mixed or crossfaded either: on take-the-room (K78) the displaced instance is paused before the group instance's audio reaches the room, and source switches to or from a Soloist source cut or fade out then in, never overlap |
| Developer Policy | "Do not create any product or service which includes any non-interactive internet webcasting service. For example, you can't create an application which plays content from a single source to several simultaneous listeners" | Bears on group instances (K59) read literally. Inference: the clause is about internet webcasting; a group plays inside one home, the use the launch blog names ("a home server feeding a multi-room setup"). Reported |
| Developer Policy | "Do not create any product or service which is integrated with streams or content from another service" | Inference: chorus never combines a Spotify stream with another service's stream; a room plays one source at a time (K78). Reported |
| Developer Policy | "Do not analyze the Spotify Content ... for any purpose, including ... creating new or derived listenership metrics"; and, beside it, "Do not synchronize any sound recordings with any visual media, including any advertising, film, television program, slideshow, video, or similar content" | Both bear on K65's visualizer computed from the audio. The launch blog itself suggests "an LED matrix visualizer", and the synchronize clause's examples are video-like media, so Spotify does not appear to read it as covering LEDs (inference). Both reported to the owner, never a reason to drop Soloist (K66); the visualizer keeps nothing |
| Developer Policy | "Streaming ... shall only be made available to subscribers to the Premium Spotify Service" | Soloist's blog: "once it's running both Free and Premium users can connect". Soloist, not chorus, does the streaming; reported |
| Instance limits | None found in any public page; the owner reports the dashboard's Soloist terms have no such clause (2026-10-06, `docs/decisions/0224-the-soloist-dashboard-terms-have-no-instance-clause.md`) | None: the pool of 16 stands as accepted |

## The design common to every option

- **Slots.** A pool of identical Soloist slots. Each slot is one container with its own network
  namespace, a headless PipeWire and WirePlumber (both MIT, planning R33), one Soloist process and
  one `chorus-soloistd` supervisor (Rust, MIT OR Apache-2.0). A slot hosts any target: the device
  name and data directory are chosen when chorus assigns it. Data directory per target key,
  `/var/lib/chorus/soloist/<target-key>`, where the key is the UPnP renderer's (room id, saved
  group id, or the sorted member-room set of a live group), so a re-formed live group keeps its
  Connect identity ("Use the same data directory across restarts to keep the same device identity
  and stored Spotify Connect session", getting started). Default pool 16 slots (8 rooms, K75, plus
  saved and live groups; `ASSUMED` counts), configurable; P11 in goal 17 measured the limit (K76) and
  the owner accepted it, the pool of 16 included, on 2026-10-05.
- **Why slots and not a container per target:** live groups come and go in seconds, and chorus
  has no Docker socket (and should not get one); a fixed pool avoids both.
- **Audio sink.** Per slot, a PipeWire pipe-tunnel sink: `tunnel.mode = sink` ("Samples played on
  the sink will be written to the pipe"), `pipe.filename` on a volume shared with chorus-server,
  `audio.rate = 44100`, 2 channels, a 32-bit format (MA reports "44.1 kHz/32-bit with Soloist",
  LEAD) instead of the module's default "16 bits, stereo, 48KHz", and Soloist started with
  `--pipewire-device <node name>`. chorus-server reads the FIFO as an ordinary FIFO input (BRIEF
  §2.1), stamps arrival with the monotonic clock and resamples into the target's timeline.
  Because the sink's `tunnel.may-pause` is "by default false" for sink mode (PipeWire docs), chorus
  drains every slot's FIFO continuously, discarding when the target is idle, so a full pipe never
  stalls Soloist (inference; goal 17 tests it on a fake).
  - Alternatives: ALSA loopback is out, because Soloist has no ALSA output ("plays audio through
    PipeWire or PulseAudio"; issue #2 asks for one, LEAD) and it would need a host kernel module and
    `/dev/snd` passthrough. A native PipeWire client inside chorus-server (libpipewire is MIT,
    ASSUMED for the Rust binding) is the fallback if FIFO pacing misbehaves.
- **Control and metadata (K65).** Soloist runs `--ws 127.0.0.1:0` inside its slot's namespace;
  the supervisor reads `ws.port` from the data directory and relays events and commands over a
  Unix socket on the shared volume, so chorus-server needs no network path to a slot. Metadata:
  `track_changed`, `playback_state`, `context_changed`, cover URLs ("small, default, large, and
  xlarge"), `position_sync`. The app shows metadata late by the target's playout delay so it
  matches what is heard (inference). Commands from the app, HA, chorusctl and controller buttons:
  `play`, `pause`, `skip_next`, `skip_prev`, `seek`, `set_volume`, `activate`, `deactivate`.
- **Discovery and pairing.** Soloist advertises itself; pairing is selecting the device once in
  the Spotify app (authentication page). Device name = the target's display name ("Kitchen",
  "Downstairs", "Kitchen + Den").
- **Group volume (K77).** One number per target across the Spotify app, the chorus app and HA.
  Which gain stage Soloist's volume drives is not documented (`--initial-volume`: "If omitted,
  Spotify Soloist uses the audio system's current/default volume"; MA offers "Player volume only
  (default, the audio always arrives untouched)", LEAD), so goal 17 builds both mappings behind one
  interface: if the audio can arrive untouched, chorus applies the volume once in its DSP;
  otherwise Soloist's volume acts as the target's group gain and chorus keeps per-room relative
  trims. K81 limits clamp after either (I10).
- **Take the room (K78).** When a group's instance reports `playing`, chorus moves the member rooms
  into the group, and each displaced room's own instance gets `pause` then `deactivate` so the
  Spotify app shows the truth. The displaced instance is paused before the group instance's audio
  reaches the room, so two Soloist instances are never mixed or crossfaded (Policy "overlap"
  clause, "including other Spotify Content"). Selecting a room's instance while its room plays in a group takes
  the room out of the group (the same rule, the other way).
- **Live groups (K59).** A slot is assigned when the group forms and freed after a grace period
  (60 s, `ASSUMED`) once the group is dissolved and idle. If the pool is exhausted, the newest live
  group has no Spotify device and the app says so.
- **Alarms (K80).** The API allows it: `play` takes "a playable Spotify URI such as a track,
  album, playlist, or episode URI", and all control commands "require a logged-in Spotify Connect
  session". At alarm time chorus checks `auth_state` (`logged_in`) and the build's remaining life,
  sends `play` with the URI, and falls back to K80's chime on any failure. It plays in whichever
  household account last paired that room's instance (the stored session). Because of the
  Developer Policy's alarm clause, the Spotify alarm source is built and tested but ships
  **switched off**; the owner turns it on after reading the dashboard clause (same Needs item).
- **Announcements (K71, the ducking mixer).** A room whose source is a Soloist instance is paused
  through the API for the announcement and resumed after it, instead of ducked (Policy "overlap"
  clause). Every other source ducks as designed. Source switches to or from a Soloist source cut,
  or fade out then in, and never overlap.
- **Build expiry.** The supervisor reads the build timestamp from `soloist --version`, raises
  "Soloist build expires in N days" at T-14 days in the app and HA, and treats exit 10 as
  "expired" (no restart loop). Updating is the owner replacing the one mounted binary and running
  `chorusctl soloist restart`: a Needs Owner item every 90 days (K66).
- **API key.** Kept in a root-owned 0400 file under the stack's `/opt/docker/config/` directory
  (homelab's secrets pattern), mounted read-only. Soloist accepts it only as `--api-key`, so it is
  visible in `/proc/<pid>/cmdline` (issue #12, LEAD): the slot container has no other user or
  process, and chorus redacts it from every log line.
- **Cache.** `--cache-size` set per slot (256 MB, `ASSUMED`) on a per-slot volume with a disk
  alert, since issue #8 (LEAD) reports the limit is not respected.
- **Image.** `chorus-soloist`: Debian 13 base (glibc to match Soloist's tested platform,
  inference), PipeWire, WirePlumber and `chorus-soloistd`. It holds no Soloist file; goal 17 line A
  lists its contents.
- **Resource cost per instance: `ASSUMED`** (nothing published; no binary or key exists here).
  Soloist 50 to 100 MB RSS and 1% to 3% of one Broadwell core while playing, near zero idle;
  PipeWire plus WirePlumber 20 to 40 MB per slot; so 16 slots about 1.1 to 2.2 GB. Per-slot
  `mem_limit: 192m`, `cpus: 0.25`, `pids_limit: 64` (`ASSUMED`) until P11 measures with the
  owner's key.

## Options

The network is what separates the options; the slot design above is the same code in each (a
slot runs in whatever network its compose file gives it).

Facts every option faces:

- Spotify Connect discovery: "The hardware device must use mDNS/DNS-SD ... to advertise its IP
  address, the port of its HTTP service", service type `_spotify-connect._tcp`, and "The client
  logs in to the device using the ZeroConf API via HTTP" (Spotify's ZeroConf guide for commercial
  hardware; that Soloist follows it is inferred from issue #1's `_spotify-connect._tcp` record).
  So the phone must reach the advertised address and port directly.
- Soloist runs its own responder on UDP 5353 and advertises nothing if it cannot bind it (issue
  #1, LEAD). Its Connect port is ephemeral per start (issue #4, LEAD).
- The homelab's `mdns-reflector` is avahi 0.9 with `network_mode: host`, reflecting between the
  LAN interface and the `homeassistant` bridge only, with publishing disabled and D-Bus off
  (`home-automation/mdns-reflector/docker-compose.yml`). chorus-server on the host network (K34)
  also answers on 5353.
- After the planned VLAN cutover, OPNsense's mDNS repeater joins the Trusted, IoT and Servers
  networks, and the firewall allows Trusted to reach the server only on listed ports
  (`docs/network.md`); today the LAN is one flat subnet.

### Option A: rooms only, bridge network (the "If deferred" cell)

- What: one slot per room (8), on a Docker bridge network; the reflector's interface list gains
  the chorus bridge.
- Costs: no new homelab deviation; one PR to the owner's homelab repo (the stack plus the reflector's
  `MDNS_INTERFACES`). Effort as for C minus the pool logic.
- Risks: a container on a bridge advertises its bridge address, which phones on the LAN cannot
  route to, and the ephemeral Connect port cannot be published ahead of time (issue #4, LEAD). Inference,
  high confidence: **the instances would not be castable from the Spotify app.** The reflector
  relays records unchanged; it does not make the address reachable.
- Fit: fails K59 (no saved or live group targets) and, by the above, K66's purpose.

### Option B: every target, one container, host network (the planning design)

- What: one `chorus-soloist` container with `network_mode: host`, one Soloist process per target.
- Costs: a **new host-network exception the owner must grant** beyond K34: the PR to the owner's homelab repo adds
  `chorus-soloist` to `docs/security.md`'s "Baseline exceptions" table and to CI's
  `EXCEPT["network_mode host"]` set, with the justification, as K34 did for chorus-server; after the
  cutover, a host-firewall rule (the Ansible `firewall` role) for the ephemeral Connect ports.
- Risks: on this host the avahi reflector already owns UDP 5353 on the host network, which is
  issue #1's failing setup exactly (LEAD); and several Soloist processes in one namespace would contend
  for 5353 the same way (inference from #1). **Not workable today.** It becomes workable only if
  Spotify adds avahi registration (issue #1's request) and a host avahi publishes for it.
- Fit: meets K59 on paper; fails in practice on the homelab as it is.

### Option C: every target, one network namespace per slot on macvlan (recommended)

- What: the slot pool as one compose service with `deploy.replicas` (16; that compose's IPAM hands
  each replica an address from the range is `ASSUMED`), attached to a macvlan
  network on the server's LAN interface (ipvlan L2 if the switch or AP limits MAC addresses per
  port, `ASSUMED` not). Each slot gets its own LAN address from a reserved range, so 5353 is free
  in every namespace, as in issue #1's working setup. chorus-server reaches slots only through the
  shared volume (FIFOs and Unix sockets), which also sidesteps macvlan's host-to-child isolation
  (general macvlan behaviour, `ASSUMED`).
- Costs: about 16 LAN addresses outside the DHCP pool (an owner step today; a Servers-network
  range after the cutover, parameterised like `LAN_IP`); a PR to the owner's homelab repo for the stack; after the
  cutover, one OPNsense rule (Trusted to the slot range, TCP, any port, because the Connect port is
  ephemeral); the mDNS repeater carries the records, and the host-network reflector is not
  involved. Effort: goal 17 (the pool, the supervisor, fakes), about one goal-day, as planned.
- Risks: macvlan is **not** host networking, so CI's `EXCEPT` set does not cover or catch it, but
  the baseline says "bridge networks, not the host's" and the owner's homelab repo's `CLAUDE.md` says "Docker bridge
  networks": the owner must approve it as a new kind of deviation, and the PR adds a row to "Baseline
  exceptions" (inference that homelab would want one; its session may add a CI check). 16 replicas
  of PipeWire cost memory (`ASSUMED` above).
- Fit: meets K59, K66, K77, K78 and K80; robust whichever way the 5353 LEAD turns out (a private
  namespace works either way).

### Option D (not proposed): one container that makes a namespace per instance

- What: the supervisor creates a network namespace and a macvlan interface per instance itself.
- Why not: it needs `NET_ADMIN` and `SYS_ADMIN` inside the container, against homelab's
  "minimal `cap_add`" rule and its "Root never comes through Docker" contract, for no gain over C.

## Comparison

| Criterion | A: rooms, bridge | B: all, host net | C: all, macvlan slots |
|---|---|---|---|
| Castable from the Spotify app | No (inference: unroutable address, ephemeral port) | No on this host (5353 held by avahi, LEAD) | Yes (issue #1's working setup, LEAD) |
| K59 targets | Rooms only | All | All |
| New homelab deviation | None | Host-network exception | macvlan network (new kind) |
| Homelab work | Stack PR, reflector interface | Stack PR, CI list, host firewall | Stack PR, address range, OPNsense rule after cutover |
| Talks to chorus-server via | Shared volume | Shared volume | Shared volume |
| Depends on Spotify changing Soloist | No | Yes (avahi support) | No |
| Memory (`ASSUMED`) | about 0.6 to 1.1 GB | about 0.8 to 1.6 GB (one PipeWire) | about 1.1 to 2.2 GB |

## Recommendation

**Recommendation:** Option C, a pool of Soloist slots each in its own network namespace on a macvlan network, because Soloist's own mDNS responder cannot share UDP 5353 with the homelab's avahi reflector (LEAD) and a bridge address is unreachable from phones (inference), so A and B would not appear in the Spotify app.

Every room, saved group and live group gets a Spotify Connect device (K59, K66), fed through a
PipeWire pipe-tunnel FIFO into chorus's existing FIFO input, with metadata, control, alarms and
take-the-room over each instance's loopback WebSocket. It costs one goal-17 build (as planned),
about 16 LAN addresses, a PR to the owner's homelab repo introducing macvlan (a deviation the owner approves), and an
OPNsense rule after the cutover. The owner gives up the "no new deviation" simplicity of A, and
takes a 90-day binary refresh as a standing chore. Three Soloist uses are shipped conservatively
because of the Developer Policy (the Spotify alarm source off until the owner enables it,
announcements pause rather than duck and two instances never overlap, voice control flagged to P8
and goal 20); the owner may relax each at Checkpoint K. The Developer Terms' "Approved Devices"
clause and the Policy's visual-media clause are reported for the owner's reading, never a reason
to drop Soloist (K66).

## If the owner defers

Goal 17 builds one slot per room on a bridge network. The slot code is the same as C's, so moving
to C later is a homelab compose change plus the pool logic, not a rewrite. Cost: by the evidence
above, the rooms' Spotify devices would probably not appear in the Spotify app on the homelab as
it is, and no saved or live group is a Spotify target (K59 unmet for Spotify; "take the room" for
Spotify reduces to "move this room to a group" in the chorus app).

## Open inputs

- **Soloist dashboard terms: answered 2026-10-06.** The owner reports the dashboard's
  Soloist terms have no clause on instances per key, account, host or household, on servers or
  containers, or on personal versus commercial use
  (`docs/decisions/0224-the-soloist-dashboard-terms-have-no-instance-clause.md`). Still open:
  whether the owner reads the Developer Terms and Policy as applying to chorus's controller,
  including the Terms' "for private personal use; and ... on Approved Devices" grant (desktops, laptops,
  netbooks, tablets, mobile, or devices approved in writing; a home server is not listed) and the
  Policy's "Do not synchronize any sound recordings with any visual media" clause (K65's
  visualizer). Reported, never a reason to drop Soloist (K66).
- **Spotify voice clause** ("Do not create a voice-enabled SDA that enables a user to control
  Spotify with their voice, or any kind of voice assistant that provides voice-control
  functionality"): the owner's call at Checkpoint K, cross-referenced in P8's open inputs and
  goal 20. At minimum, no voice intent targets a Soloist source.
- **Spotify alarm source switch** (K80 against the Developer Policy's alarm clause): the owner's
  call at Checkpoint K or after the Needs item.
- **The 5353 and ephemeral-port behaviour** (LEADs #1 and #4): proposed Needs item for goal 17,
  once the owner has a key and a build: run two instances in one namespace and one beside the
  reflector, record `ss -ulpn` and whether each appears in the Spotify app.
- **Resource cost per instance** (RSS, CPU, PipeWire): `ASSUMED`; measured for P11 in goal 17 with
  the owner's key.
- **Volume gain stage** and **Soloist's output sample format** (32-bit float per LEADs): `ASSUMED`;
  measured in goal 17.
- **LAN address range for the slots** and whether the switch or AP limits MACs per port: owner
  input through the PR to the owner's homelab repo.
- **Saved-group count** (sizes the pool): `ASSUMED` 2 to 4; the room-list Needs item (§5 item 5).
- **Live-group grace period** (60 s) and **cache size** (256 MB): `ASSUMED` defaults.

## Sources

- Spotify Soloist landing page, https://developer.spotify.com/documentation/soloist, read 2026-09-30
- Getting Started with Spotify Soloist, https://developer.spotify.com/documentation/soloist/tutorials/getting-started, read 2026-09-30
- Soloist command line reference, https://developer.spotify.com/documentation/soloist/reference/command-line, read 2026-09-30
- Soloist WebSocket API reference, https://developer.spotify.com/documentation/soloist/reference/websocket-api, read 2026-09-30
- soloist ctl reference, https://developer.spotify.com/documentation/soloist/reference/soloist-ctl, read 2026-09-30
- Soloist authentication, https://developer.spotify.com/documentation/soloist/concepts/authentication, read 2026-09-30
- Soloist overview, https://developer.spotify.com/documentation/soloist/concepts/overview, read 2026-09-30
- Soloist features, https://developer.spotify.com/documentation/soloist/features, read 2026-09-30
- Soloist downloads and updates, https://developer.spotify.com/documentation/soloist/reference/downloads-and-updates, read 2026-09-30
- Introducing Spotify Soloist (blog, 13 Aug 2026), https://developer.spotify.com/blog/2026-08-13-introducing-spotify-soloist, read 2026-09-30
- Spotify Developer Terms, Version 10 (effective 15 May 2025), https://developer.spotify.com/terms, read 2026-09-30 (licence grant and "Approved Devices" definition re-read 2026-09-30)
- Spotify Developer Policy (effective 15 May 2025), https://developer.spotify.com/policy, read 2026-09-30 (§III voice, overlap and visual-media clauses re-read 2026-09-30)
- Spotify Terms of Use (last updated 4 Sep 2026), https://www.spotify.com/us/legal/end-user-agreement/, read 2026-09-30
- Spotify User Guidelines, https://www.spotify.com/us/legal/user-guidelines/, read 2026-09-30
- Spotify Connect ZeroConf API guide (commercial hardware), https://developer.spotify.com/documentation/commercial-hardware/implementation/guides/zeroconf, read 2026-09-30
- spotify/soloist GitHub README and issues #1, #2, #4, #5, #6, #8, #12 (LEADs), https://github.com/spotify/soloist/issues, read 2026-09-30
- Music Assistant, Spotify Connect plugin, https://www.music-assistant.io/plugins/spotify-connect/, read 2026-09-30
- PipeWire 1.6.9 docs, Unix Pipe Tunnel, https://docs.pipewire.org/page_module_pipe_tunnel.html, read 2026-09-30
- The owner's homelab repo at `d82e2ae`: `CLAUDE.md`, `docs/security.md`, `.github/workflows/ci.yml` (Compose security baseline step), `home-automation/mdns-reflector/docker-compose.yml`, `docs/network.md` (firewall policy, mDNS and SSDP), read 2026-09-30

## What was read

- Rules and format: `/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`,
  `/cache/tmp/chorus-g1/prompt-PC.md`.
- The brief [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md): §0.1, §0.6, §0.8, §0.9, §1 (K4, K26,
  K27, K28, K33, K34, K53, K56 to K66, K71, K75 to K81, K95, R1 to R17, I1 to I20), §2, §3.2 to
  §3.4, §4.8, §5, §8, §15, §20 to §22, §31, §34, §35 (B-L8, B-L20, X-M8).
- Planning research: `research-casting-decoders.md`, `verify-ha-casting.md`, `review-homelab.md`.
- `BRIEF.md` §2 and §3.1 (baseline worktree); chorus's own `crates/discovery/src/net.rs` (chorus's own
  code).
- The owner's homelab repo (read-only clone at `d82e2ae`): the files listed in Sources.
- Every URL in Sources. Soloist is proprietary; no Soloist binary was downloaded or run.
- Corrections pass (2026-09-30): the goal-1 verifier 2 report
  (`/cache/tmp/chorus-g1/verify/verify-2.md`); the Developer Policy and Developer Terms re-fetched
  to confirm the full voice and overlap sentences, the visual-media clause, the licence grant and
  the "Approved Devices" definition.
- No GPL source file was opened (K33). A third-party `libpulse` shim named in a Soloist issue
  comment was not opened.
