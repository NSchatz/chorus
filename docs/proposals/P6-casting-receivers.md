# P6: Casting protocols beyond UPnP AV, and where receivers run

- Decisions: K57, K58
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- If deferred: UPnP AV only
- Builds on: goal 4 (§8, BRIEF §2.3's streaming line per R7), goal 16 (§20 item 3, UPnP AV renderers per room, saved group and live group), goal 17 (§21 item 2, "Other protocols per P6"; line C), goal 18 (§22, sources and media players in HA)

## Question

Which cast or push protocols, beyond the UPnP AV renderer every target already gets, pass the
strict bar and are worth building, and for each protocol does the receiver run inside
chorus-server or as a sidecar process? The brief's §5 cell: "Casting protocols beyond UPnP AV and
where receivers run; never librespot, a reverse-engineered AirPlay 2 receiver or Cast (K60)".

The owner decisions that bound it:

- K57: "**UPnP/DLNA renderer, plus anything legal (research)**: every room and group is a
  UPnP/DLNA MediaRenderer; goal 1 researches every other cast/push protocol and proposes the legal
  ones ... Google Cast (needs Google device certificates) and Tidal/Qobuz Connect (partner-only)
  are infeasible."
- K58: "**Research and propose**: goal 1 compares sidecar processes (pinned unmodified receiver
  binaries per room/group feeding PCM) vs receivers inside chorus-server, per protocol (licences,
  ToS, BRIEF §2.3, sync and latency, metadata and control, maintenance)."
- K59: every room, saved group and live group is a target in each supported protocol.
- K60: "**Strict: open only**: no receiver that violates a service's terms or reverse-engineers a
  DRM or authentication handshake (so no librespot Spotify Connect, no reverse-engineered AirPlay
  2 receiver, no Cast)."
- K64: inputs only (no radio directory, library, Plex/Jellyfin browsing or podcasts).
- K54 and K55: chorus groups rooms, and groups hold only chorus speakers.

Spotify through Soloist is P7's question; this proposal only places it in the K58 table.

## Constraints that bind every option

- **K60, excluded by name and never proposed:** librespot, any reverse-engineered AirPlay 2
  receiver, Google Cast. They appear below only as excluded.
- **Clean-room (K33):** GPL projects (gmrender-resurrect, upmpdcli, MPD, Snapcast, squeezelite,
  shairport-sync, the Lyrion server) are read as docs only. Permissive code (pupnp BSD-3, ohNet
  MIT, ohPipeline MIT, ohSongcast BSD-2) may be read and cited.
- **Licence (K26, K95):** chorus stays MIT OR Apache-2.0; another licence enters only by ADR.
- **BRIEF §2.3** "No DRM streaming integrations in this codebase", reworded by R7 as "official
  receivers run as separate processes, no DRM code in chorus". DLNA's DTCP-IP link protection is
  DRM and is never implemented.
- **BRIEF §3.2:** build what is small and instructive, vendor what is large and undifferentiated,
  and log the call.
- **Security (§4.8):** "No arbitrary URL fetch except the input paths the decisions name (UPnP
  renders, ...)"; every volume path is clamped by K81's limits (I10).
- **Naming:** "UPnP AV media renderer", never "DLNA Certified" (goal 16).
- **Rule 8:** fitness for chorus's requirements only.

## Re-verification of the planning research

The planning research (`research-casting-decoders.md` §1, §2 and §6; `verify-ha-casting.md`
claims 9 and 10) was re-checked on 2026-09-30 against primary sources:

| Planning claim | Re-checked | Change |
|---|---|---|
| MediaRenderer:1 requires RenderingControl and ConnectionManager; AVTransport is needed for HTTP pull (inference) | Confirmed: "Each implementation of the MediaRenderer requires a Rendering Control and ConnectionManager service" and "If an implementation ... supports any of these transfer protocols, then it must implement the AVTransport service" (MediaRenderer:1 template 1.01) | None |
| SSDP details (max-age, repeats) were `ASSUMED` | Now read in UDA 1.1 (15 Oct 2008): advertisement durations "close to the minimum of 1800 seconds"; discovery sets sent "more than once" and "SHOULD NOT be sent more than three times"; the device "MUST re-send its advertisements periodically prior to expiration"; `BOOTID.UPNP.ORG` "MUST be increased each time a device (re)joins the network" | `ASSUMED` removed |
| (not in the planning research) | UDA 2.0 (17 Apr 2020): "The subscription request containing a delivery URL not on the same network segment as the fully qualified event subscription URL shall not be accepted." The next sentence reads this for private networks as the RFC 1918 ranges: "For private networks this means that the delivery URL provided will adhere to the following IP ranges:" (10/8, 172.16/12, 192.168/16). This is the fix for CallStranger, CVE-2020-12695 (NVD: the spec before 2020-04-17 "does not forbid the acceptance of a subscription request with a delivery URL on a different network segment") | New design input: eventing across the homelab's future VLANs (below) |
| OCF hosts the specs free, with "NOTHING CONTAINED IN THESE DOCUMENTS SHALL BE DEEMED AS GRANTING YOU ANY KIND OF LICENSE" | Confirmed on the OCF page and in UDA 2.0's front matter; no royalty programme found | None |
| DLNA dissolved in 2017; SpireSpark runs certification | Confirmed (dlna.org). New: the DLNA Guidelines (June 2016, ten parts including Link Protection and DRM Interoperability) are obtained by contacting SpireSpark, not downloaded | chorus implements from the public UPnP specs only |
| OpenHome: ohNet MIT, ohPipeline MIT, ohSongcast BSD-2; service list `ASSUMED` (docs.openhome.org did not resolve) | Licence files confirmed ("licensed under the MIT license"; "2-clause (Simplified) BSD license"). docs.openhome.org does not resolve and wiki.openhome.org times out. The service list is now confirmed from ohNet's MIT service descriptions: Info1, Playlist1, PlaylistManager1, Product1, Radio1, Receiver1, Sender1, Time1, Volume1; the archived wiki says the Product service's sources may be Playlist, Radio, Receiver, UpnpAv, NetAux, Analog, Digital or Hdmi. The maintained set is ohPipeline's `OpenHome/Av/ServiceXml/OpenHome` (MIT, pushed 2026-09-25): Product2 to Product4, Volume2 to Volume4, Info1, Time1, Playlist1, Radio1 and Radio2, Receiver1, Sender1 and Sender2, Transport1, Pins1 and others; ohNet (last pushed 2025-10-29) holds only the version-1 set | `ASSUMED` removed; Option B is written from ohPipeline's set |
| Rust crates: rupnp 3.0.0, ssdp-client 2.1.0 (MIT/Apache-2.0), dlna-dmr 0.1.3 (MIT), cotton-ssdp 0.1.0 (CC0) | Confirmed unchanged on crates.io; quick-xml 0.42.0 (MIT, 2026-08-22) added | None |
| pupnp BSD-3; gmrender-resurrect GPL-2.0 | Confirmed (GitHub licence API; pupnp pushed 2026-09-30, active) | None |
| AirPlay receivers rely on a key extracted by reverse engineering | Confirmed: "On April 8, 2011, James Laird reverse-engineered and released the private key used by the Apple AirPort Express to decrypt incoming audio streams" (Wikipedia, AirPlay) | None |
| Roon Ready partner-only (`ASSUMED`) | Now primary: Roon Ready is for "audio player hardware products that have both implemented RAAT, and have been certified by Roon Labs"; certification means "We sit down with a device, typically for a period of a few weeks". Roon Bridge "runs on x64 and armv7hf and armv8" and "Roon depends on ALSA" | `ASSUMED` removed for Roon Ready |
| Tidal Connect and Qobuz Connect partner-only (`ASSUMED`) | Qobuz Connect: an integrator shipped it "as soon as the service specification was final" and runs "the Qobuz self-test process for our partners to ensure successful certification" (StreamUnlimited, 26 Jun 2025). TIDAL: tidal.com/connect offers "Become a Device Partner" (search result only); the page, TIDAL's support article and the Wayback copy all returned 403 and developer.tidal.com has no Connect content, so the partner-only status is unverifiable | Qobuz confirmed; TIDAL unverifiable (403s) and moot: K57 already records TIDAL Connect as infeasible |
| BubbleUPnP plays to "UPnP/DLNA/OpenHome" renderers and streams Qobuz and TIDAL | Confirmed on its page | None |
| Snapcast's protocol is a server-to-client protocol | Confirmed (doc only): TCP 1704 by default, Hello, Server Settings, Codec Header, Wire Chunk, Time | None |

Adversarially verified 2026-09-30 (goal-1 verifier 2): 14 claims confirmed, 1 refuted, 1 partly right, 1 unverifiable; corrections applied; the recommendation stands.

## The protocols, judged under K60

| Protocol | Open without partner status or reverse engineering? | Verdict |
|---|---|---|
| UPnP AV MediaRenderer (UDA, AVTransport, RenderingControl, ConnectionManager) | Yes: public OCF specs, no royalty programme found | **Built for every target (K57, goal 16)** |
| OpenHome (Product, Volume, Info, Time, Playlist; also Radio, Receiver, Sender) | Yes: Linn's open standard; service descriptions published MIT in ohPipeline (current) and ohNet (version 1) | **Proposed** (Option B), except Radio (a station directory, K64) |
| OpenHome Songcast receiver | Yes: Linn's protocol, BSD-2 reference (ohSongcast) | Optional (Option C) |
| DLNA extensions (the DLNA fields in `protocolInfo`, DLNA HTTP headers) | Guidelines by request from SpireSpark only; DTCP-IP is DRM | Tolerate what control points send; never claim certification; no DTCP-IP |
| Spotify Connect via Soloist | Yes (official software, K66) | P7 |
| Roon Ready (RAAT in firmware) | No: certification by Roon Labs for manufacturers | Excluded |
| Roon Bridge (Roon's own endpoint software) | Yes as a program (Roon's EULA forbids reverse engineering, so it is run unmodified); only useful with a Roon subscription the owner does not have (no Roon in the homelab survey) | Not now (Option D names it) |
| Qobuz Connect, TIDAL Connect | No: partner specification and certification | Excluded (K57); both services reach chorus through UPnP control points such as BubbleUPnP |
| AirPlay 1 (RAOP) receiver | No: open receivers work because the AirPort Express private key was extracted by reverse engineering | **Excluded (K60)** |
| AirPlay 2 receiver, Google Cast, librespot | No | **Excluded by name (K60)** |
| Snapcast protocol | A server-to-client sync protocol, not a cast protocol; GPL project, docs only | No: chorus has its own protocol and chorus-only groups (K55) |
| Squeezebox SlimProto | Documented (Lyrion docs: "The server listens on TCP port 3483"); the server is GPL (`ASSUMED`, GitHub reports NOASSERTION) | No: needs a Lyrion server the owner does not run, which would own queues and grouping (K54) |
| MPRIS | A local D-Bus control interface (spec v2.2), not a network transport | Not applicable |
| MPD protocol | A control protocol around a music database (K64 excludes one) | No |
| Matter media clusters, DIAL | Control or app-launch, not audio transport (`ASSUMED`, not fetched) | Not applicable |
| Bluetooth A2DP | Declined (K53) | No |

## Where receivers run (K58), per protocol

| Protocol | Where | Licences and ToS | BRIEF §2.3 | Sync and latency | Metadata and control | Maintenance |
|---|---|---|---|---|---|---|
| UPnP AV renderer | **Inside chorus-server** | chorus's own code from public specs | No DRM | Decoded straight into the target's timeline; one clock domain, stamped with the monotonic clock | DIDL-Lite in, LastChange events out, in-process | chorus's code and tests; control-point interop is the ongoing cost |
| OpenHome services | **Inside chorus-server**, on the same devices | Same; MIT service descriptions | No DRM | Same | Renderer-held queue, Info/Time events, in-process | Same stack, more services |
| Songcast receiver (if approved) | Inside chorus-server | Open protocol, BSD-2 reference | No DRM | Same | Minimal | Small |
| Spotify via Soloist | **Sidecar** (P7) | Proprietary, no redistribution; must run unmodified | DRM stays in Spotify's process (R7) | PCM through a FIFO, stamped on arrival; an extra pipe buffer, absorbed by chorus's jitter buffer | Loopback WebSocket relay | 90-day binary refresh by the owner |
| Roon Bridge (if ever) | Sidecar | Roon EULA; run unmodified | DRM, if any, stays in Roon's process | As Soloist; its ALSA output would need a PipeWire ALSA bridge inside the container (`ASSUMED`) | Roon controls it; chorus sees little | Vendor updates |

The rule that falls out: **open protocols chorus can implement from a spec run inside
chorus-server; proprietary receivers run as unmodified sidecars feeding PCM.** Inside, the
renderer sees room and group changes directly (K59's live groups appear and vanish, K78 events
the displaced rooms) and adds no process or pipe; a sidecar is the only lawful way to use a
binary chorus may not ship or modify.

## Options

Every option includes the UPnP AV renderer that K57 already decided. The implementation choice
for it is shared by all options and is in the next subsection.

### Option A: UPnP AV only (the "If deferred" cell)

- What: goal 16's renderer per room, saved group and live group; no other protocol; goal 17's
  item 2 builds nothing and line C quotes P6.
- Costs: none beyond goal 16.
- Risks: with plain AVTransport the control point owns the queue and feeds the next URI, so if
  the phone app is killed or sleeps, playback stops after the current and prefetched track
  (planning inference from AVTransport's SetNextAVTransportURI model, re-read: "allows a device to
  'prefetch' the data to be played next"). No control point can switch a room to a line-in or the
  TV.
- Fit: meets K57's floor.

### Option B: UPnP AV plus OpenHome services (recommended)

- What: on each target's device, add OpenHome Product (sources: Playlist, UpnpAv, and each of the
  target's line-ins and the TV path as Analog, Digital or Hdmi sources), Volume, Info, Time and
  Playlist. The Playlist is a queue of URIs the control point pushes, held by chorus, so the phone
  can sleep; it is not a library or directory, so K64 holds (inference, as planning said). Radio is
  not implemented (a station list is a directory, K64). Written in Rust inside chorus-server from
  the MIT service descriptions in ohPipeline's `OpenHome/Av/ServiceXml/OpenHome` (Product up to
  version 4, Volume up to version 4, Info1, Time1, Playlist1, and Transport1, which ohNet's older
  copies lack); goal 17's interop tests with BubbleUPnP and a scripted control point settle which
  versions to announce (reading ohNet and ohPipeline source is allowed). A suggestion for goal 17:
  the Product service's `NetAux` source ("3rd party, non OpenHome controllable, network protocols
  such as AirPlay", archived wiki) could list each target's Soloist instance, so OpenHome control
  points can show and select Spotify.
- Costs: goal 17 item 2, about half a goal-day beyond plan (`ASSUMED`): five SOAP services on the
  existing stack, Playlist's id-array semantics, and tests against a scripted control point and
  BubbleUPnP's OpenHome mode.
- Risks: control points differ in OpenHome dialects and service versions; docs.openhome.org is
  offline, so semantics come from ohPipeline's MIT service XMLs (and ohNet's older ones), the
  archived wiki and ohPipeline's code (all permissive).
- Fit: K57 (legal, open), K59 (same targets), K65 (Info and Time give metadata; Product exposes
  inputs to control points), K64 (no Radio).

### Option C: B plus an OpenHome Songcast receiver

- What: a Songcast receiver per target (the Receiver service), so a PC or phone Songcast sender
  can play its own audio into a room or group.
- Costs: another protocol in chorus-server; Songcast is documented mainly by ohSongcast's BSD-2
  code (`ASSUMED`), so it is a code study.
- Risks: current sender apps and their platforms are `ASSUMED`, not checked. Songcast carries its
  own multiroom timing, which chorus ignores (it only receives).
- Fit: legal and open; answers a need the owner has not stated.

### Option D: B plus a Roon Bridge sidecar

- What: Roon's own endpoint software, unmodified, per target, feeding PCM like Soloist.
- Costs: a Roon subscription (not priced here; the owner has none), an ALSA-to-PipeWire bridge in
  the sidecar, the network questions P7 found for Soloist.
- Risks: Roon groups zones itself, against K54; Roon Ready certification is not available to
  chorus.
- Fit: legal, but only if the owner adopts Roon. Not proposed now.

### The UPnP AV implementation (shared by every option)

| Choice | Licence | For | Against |
|---|---|---|---|
| U1: write it in Rust inside chorus-server, from UDA 1.1/2.0 and the AV specs; quick-xml (MIT) for XML; rupnp and ssdp-client (MIT/Apache-2.0) and crab-dlna (MIT OR Apache-2.0, planning R21) as test control points | MIT OR Apache-2.0 | Tied directly to rooms and groups (16 root devices, live groups announcing and leaving, K78 events); chorus already hand-writes DNS-SD and HTTP + SSE; small and instructive (BRIEF §3.2); CallStranger and URL-fetch rules are chorus's own | Size `ASSUMED` at 2k to 4k lines; interop quirks found by testing |
| U2: depend on `dlna-dmr` 0.1.3 | MIT | A renderer skeleton | One release line, young, no reverse dependencies (planning); built on axum and tokio, a large new tree for chorus-server |
| U3: wrap pupnp (libupnp) | BSD-3-Clause | Mature, maintained UDA stack | A C library and its threading and callbacks in the server; the image's C-toolchain question (goal 16 item 2); the device model must still be mapped to chorus's targets |
| U4: ohNet (OpenHome's stack) | MIT | UPnP plus OpenHome in one | C++ and large; last pushed 2025-10-29 |
| Excluded | GPL or LGPL | gmrender-resurrect, upmpdcli (docs only); GUPnP and Rygel, Platinum (licences `ASSUMED` LGPL and GPL-or-commercial) | Licence (K26, K33) |

U1 is proposed (as planned). Two security rules come with it:

- **Eventing (CallStranger):** UDA 2.0 does not accept a delivery URL off the subscription URL's
  network segment, and for private networks reads that as the RFC 1918 ranges (10/8, 172.16/12,
  192.168/16). chorus is stricter: it accepts delivery URLs only inside a configured list of
  household subnets (default: the server's own). After the homelab's VLAN cutover the list adds
  Trusted's subnet, which still conforms to UDA 2.0's private-network reading. An ADR records the
  list as a stricter choice; it is not a departure from UDA 2.0.
- **Fetching (§4.8):** `SetAVTransportURI` makes chorus fetch a URL it is given. Only `http` and
  `https`, no redirects to loopback, link-local or the server's own services, bounded size and
  time.

## Network consequences (homelab facts, the owner's homelab repo at `d82e2ae`)

- chorus-server runs on the host network (K34), so SSDP multicast works. The host firewall
  (`ansible/roles/firewall`) accepts UDP 1900 and 5353 from the LAN (`firewall_lan_udp`), but a
  host-network service's TCP port must be listed: `firewall_lan_tcp: [1180, 32469]` today holds
  only Plex's. The renderer's HTTP port (description, SOAP, GENA) needs adding there, an owner
  step through the PR to the owner's homelab repo and `host-apply`.
- After the cutover, `docs/network.md`'s policy allows Trusted to reach the server only on listed
  ports (Plex's DLNA server has its own row, "32469, plus relayed SSDP") and lists no Servers to
  Trusted rule. A renderer needs Trusted to the server on its HTTP port, and the server to Trusted
  for GENA callbacks and for media a phone serves itself. OPNsense's SSDP relay already joins
  Trusted and Servers. These rules go in the goal-17 receivers PR (§21 item 5) and are coordinated
  with P3's "chorus speaker network" PR.

## Comparison

| Criterion | A: UPnP AV only | B: + OpenHome | C: + Songcast | D: + Roon Bridge |
|---|---|---|---|---|
| Passes K60 | Yes | Yes | Yes | Yes (unmodified) |
| Phone can sleep during playback | No | Yes (renderer-held queue) | Yes | Roon's own |
| Control points can pick a line-in or the TV | No | Yes (Product sources) | Yes | No |
| Extra effort | None | About half a goal-day (`ASSUMED`) | More, code study | Sidecar plus subscription |
| New licence or ToS surface | None | None (MIT descriptions) | None (BSD-2 reference) | Roon EULA, a subscription |
| Owner input needed | None | None | Whether PC audio to a room is wanted | Adopting Roon |

## Recommendation

**Recommendation:** Option B, the UPnP AV media renderer written in Rust inside chorus-server plus OpenHome Product, Volume, Info, Time and Playlist services on the same devices, with Soloist (P7) as the only sidecar, because OpenHome is the one other open protocol that adds real value (a renderer-held queue and input selection) at small cost.

Every other candidate is either partner-only (Roon Ready, Qobuz Connect, TIDAL Connect),
dependent on reverse engineering (AirPlay 1 and 2, Cast, librespot), not a cast protocol
(Snapcast, SlimProto, MPRIS, MPD), or useful only with a service the owner does not use (Roon
Bridge). B costs about half a goal-day in goal 17 and no new licence, service terms or homelab
deviation beyond the renderer's own port rules. The owner gives up a Songcast receiver (C) and a
Roon endpoint (D) for now; either can be added later on the same device stack or sidecar pattern
without rework. Receivers for open protocols run inside chorus-server; proprietary ones run as
unmodified sidecars (K58).

## If the owner defers

Goal 16 builds the UPnP AV renderer per target as planned (U1 applies either way), and goal 17's
item 2 builds nothing (line C quotes this proposal). Cost: the phone must stay awake to advance a
queue, and control points cannot switch a room to a line-in or the TV; the chorus app and HA still
can. Adding OpenHome later is additive on the same devices.

## Open inputs

- **The control points the household uses** (BubbleUPnP, Linn Kazoo, Windows, others): shapes
  interop testing; `ASSUMED` BubbleUPnP on Android. No Needs item exists; one could be added if
  the owner wants specific apps tested.
- **Whether PC or phone audio to a room is wanted** (Option C) and **whether the owner adopts
  Roon** (Option D): owner inputs at Checkpoint K.
- **Household subnets for GENA callbacks** after the cutover: from the homelab network design;
  the goal-17 PR to the owner's homelab repo names them without addresses in chorus's files.
- **Renderer HTTP port:** a chorus choice made in goal 16; the homelab firewall rule follows it.
- **Size of U1** (2k to 4k lines) and **Option B's effort**: `ASSUMED`.
- **TIDAL Connect's partner terms:** unverifiable (every TIDAL page tried returned 403; only a
  search result says "Become a Device Partner"), and moot: K57 already records TIDAL Connect as
  infeasible.
- **Songcast protocol documentation and sender apps:** `ASSUMED`.

## Sources

- UPnP Device Architecture 1.1 (15 Oct 2008), https://upnp.org/specs/arch/UPnP-arch-DeviceArchitecture-v1.1.pdf, read 2026-09-30
- UPnP Device Architecture 2.0 (17 Apr 2020), https://openconnectivity.org/upnp-specs/UPnP-arch-DeviceArchitecture-v2.0-20200417.pdf, read 2026-09-30
- MediaRenderer:1 Device Template 1.01, https://upnp.org/specs/av/UPnP-av-MediaRenderer-v1-Device.pdf, read 2026-09-30
- OCF UPnP specifications page, https://openconnectivity.org/developer/specifications/upnp-resources/upnp/, read 2026-09-30
- NVD, CVE-2020-12695 (CallStranger), https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2020-12695, read 2026-09-30
- DLNA (SpireSpark), https://www.dlna.org/ and https://www.dlna.org/guidelines, read 2026-09-30
- OpenHome licence files: https://raw.githubusercontent.com/openhome/ohNet/master/License.txt, https://raw.githubusercontent.com/openhome/ohPipeline/master/License.txt, https://raw.githubusercontent.com/openhome/ohSongcast/master/License.txt, read 2026-09-30
- ohNet service descriptions (tree listing and Playlist1.xml), https://github.com/openhome/ohNet/tree/master/OpenHome/Net/Service/Upnp/OpenHome, read 2026-09-30
- ohPipeline service descriptions (tree listing of `OpenHome/Av/ServiceXml/OpenHome`), https://api.github.com/repos/openhome/ohPipeline/git/trees/master?recursive=1, read 2026-09-30; ohPipeline repository metadata (pushed 2026-09-25), https://api.github.com/repos/openhome/ohPipeline, read 2026-09-30
- ohNet repository metadata (pushed 2025-10-29), https://api.github.com/repos/openhome/ohNet, read 2026-09-30
- OpenHome GitHub organisation, https://github.com/openhome, read 2026-09-30
- OpenHome wiki, Product service (archived 29 Oct 2020), https://web.archive.org/web/20201029223320/http://wiki.openhome.org/wiki/Av:Developer:ProductService, read 2026-09-30 (docs.openhome.org does not resolve and wiki.openhome.org times out)
- crates.io API: rupnp, ssdp-client, dlna-dmr, cotton-ssdp, quick-xml, upnp-rs, https://crates.io/api/v1/crates/<name>, read 2026-09-30
- GitHub licence API: pupnp/pupnp, hzeller/gmrender-resurrect, openhome/*, LMS-Community/slimserver, https://api.github.com/repos/<owner>/<repo>, read 2026-09-30
- Wikipedia, AirPlay, https://en.wikipedia.org/wiki/AirPlay, read 2026-09-30
- Roon Ready, https://help.roonlabs.com/portal/en/kb/articles/roon-ready, read 2026-09-30
- Roon partner programs, https://help.roonlabs.com/portal/en/kb/articles/roon-partner-programs, read 2026-09-30
- Installing Roon on Linux, https://help.roonlabs.com/portal/en/kb/articles/linux-install, read 2026-09-30
- Roon Terms and Conditions, https://roon.app/en/termsandconditions, read 2026-09-30
- StreamUnlimited, Qobuz Connect via StreamSDK (26 Jun 2025), https://www.streamunlimited.com/qobuz-connect-now-available-via-streamsdk/, read 2026-09-30
- TIDAL Connect, https://tidal.com/connect (search result only; direct fetch returned 403), 2026-09-30; also tried https://support.tidal.com/hc/en-us/articles/360004565898-Tidal-Connect (403) and https://developer.tidal.com/ (no Connect content), 2026-09-30 (goal-1 verifier 2)
- BubbleUPnP, https://bubblesoftapps.com/bubbleupnp/, read 2026-09-30
- Snapcast binary protocol (doc only), https://raw.githubusercontent.com/badaix/snapcast/develop/doc/binary_protocol.md, read 2026-09-30
- Lyrion, SlimProto protocol (doc), https://lyrion.org/reference/slimproto-protocol/, read 2026-09-30
- MPRIS D-Bus Interface Specification v2.2, https://specifications.freedesktop.org/mpris-spec/latest/, read 2026-09-30
- The owner's homelab repo at `d82e2ae`: `ansible/roles/firewall/templates/host.nft.j2`, `ansible/roles/firewall/defaults/main.yml`, `docs/network.md`, `docs/security.md`, read 2026-09-30

## What was read

- Rules and format: `/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`,
  `/cache/tmp/chorus-g1/prompt-PC.md`.
- The brief [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md): §0.1, §0.8, §0.9, §1 (K26, K33, K34,
  K53 to K66, K75 to K80, K95, R1 to R17, I1 to I20), §2, §3.2 to §3.4, §4.8, §5, §20, §21, §22,
  §34, §35.
- Planning research: `research-casting-decoders.md`, `verify-ha-casting.md`, `review-homelab.md`.
- `BRIEF.md` §2, §3 and §5.8 (baseline worktree).
- The owner's homelab repo (read-only clone): the files listed in Sources, plus `CLAUDE.md`,
  `.github/workflows/ci.yml` and `home-automation/mdns-reflector/docker-compose.yml`.
- Every URL in Sources; five web searches (Roon Ready, Roon Bridge, Qobuz Connect, TIDAL Connect,
  OpenHome services).
- No GPL source file was opened (K33): for gmrender-resurrect and the Lyrion server only licence
  metadata was read, and for Snapcast and Lyrion only documentation pages; upmpdcli and MPD were
  not opened this session (their facts come from the planning research's docs reading). Permissive material read:
  ohNet, ohPipeline and ohSongcast licence files and ohNet's `Playlist1.xml` service description.
- Corrections pass (2026-09-30): the goal-1 verifier 2 report
  (`/cache/tmp/chorus-g1/verify/verify-2.md`) and the ohPipeline tree listing and repository
  metadata (GitHub API, MIT project).
