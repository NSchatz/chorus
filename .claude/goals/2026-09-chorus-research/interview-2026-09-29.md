# Interview log, chorus program (2026-09-29)

Arguments: "Chrous needs to be a first class project"

## Round 1 (2026-09-29 ~19:35 UTC)
Q1 First class (multi): Own /goal program / Leave the umbrella / Real product at home / Public OSS project -> **Own /goal program, Real product at home, Public OSS project**
Q2 Autonomy: One checkpoint after g1 / Checkpoint at hardware / Review after every goal -> **One checkpoint after g1**
Q3 Size: As big as needed / About 4 goals / About 8 goals -> **As big as needed**

## Round 2 (2026-09-29 ~19:45 UTC)
(Standing contract shown in chat before the question.)
Q1 Contract: Inherit unchanged / Inherit with changes / Walk through it -> **Inherit unchanged**
Q2 Other programs: Independent, beside them / Uses shopkit/devices/3d / Beside them, read-only -> **Uses shopkit/devices/3d**

## Round 3 (2026-09-29 ~19:50 UTC)
Q1 the Needs list: Shared list, ## chorus / Own NEEDS-NOAH.md (+ pointer) / Own list, no pointer -> **Shared list, ## chorus**
Q2 Turnaround: Days, in batches / Same day, often / Weeks; hardware later -> **Weeks; hardware later**

## Instruction between rounds (2026-09-29 ~19:55 UTC)
the owner: "Make the chorus repo private"
Done: `gh repo edit NSchatz/chorus --visibility private --accept-visibility-change-consequences` -> exit 0; `gh repo view` -> {"isPrivate":true,"visibility":"PRIVATE"}. Before: 0 stars, 0 forks, 0 watchers, no Pages.
Follow-ups opened: K1 "Public OSS project" now conflicts (public later, or dropped?); CI: Actions on this account's private repos never start (shopkit NEEDS-NOAH "## GitHub"), so chorus CI likely stops.

## Round 4 (2026-09-29 ~20:00 UTC)
Q1 OSS vs private (K1 conflict): Private now, public later / Private, OSS-quality / Private; drop OSS -> **Private, OSS-quality**

## Instruction between rounds (2026-09-29 ~20:05 UTC)
the owner: "Extensively search the web as needed"

## Round 5 (2026-09-29 ~20:15 UTC)
Q1 Umbrella (S0141 drain risk): the Needs list, super untouched / PR to super, you merge / PR to super, self-merge / Leave super as is -> Other: **"Forget super entirely. It is no longer used to develop applications"**
Q2 Acceptance source: Copy into chorus / BRIEF §8 + the brief / Research and propose -> Other: **"Forget about super"**
Q3 Audit (framed on super's spec history): Goal 1 audits, goal 2 fixes / Audit per phase / No audit -> Other: **"Forget super"** (premise rejected; the audit question is re-asked on chorus's own facts)

## Instruction between rounds (2026-09-29 ~20:20 UTC)
the owner: "Think of the following as end goal of chorus. Sonos parity. Software, hardware, home assistant integration, all of the above. FIRST CLASS."

## Round 6 (2026-09-29 ~20:30 UTC)
(Web-checked before asking: Sendspin = Open Home Foundation protocol, MA-native since 2.7, CSL 1.0 spec, Apache-2.0 SDKs, "technical preview" per music-assistant.io/player-support/sendspin, read 2026-09-29.)
Q1 Sources: Music Assistant feeds it / Built-in receivers / Both / Research and propose -> **Research and propose**
Q2 HA/MA path: Research and propose (Rec.) / Server bridges Sendspin / Native HA integration / Endpoints speak Sendspin -> **Research and propose (Rec.)**
Q3 App: App-grade PWA / Native mobile app / HA/MA are the app -> **App-grade PWA**
Q4 Theater: PCM + research DD / PCM only (BRIEF) / Research and propose / No theater path -> **Research and propose**

## Round 7 (2026-09-29 ~20:40 UTC)
Q1 Convention gates (multi): Pinning (P1-P8) / Comment density / Styling + frontend (S, F) / Wireless expectations -> Other: **"Recreate your own conventions completely forgetting about supers conventions"**
Q2 Gate: make gate + fmt/clippy / make gate, no fmt/clippy / make gate, drop ci.yml -> **make gate + fmt/clippy**
Q3 Audit: Goal 1 audits (Rec.) / Audit per phase / No audit -> **Goal 1 audits (Rec.)**
Q4 Owned HW (multi): ESP32-S3 boards / Amp or DAC modules / Pi-class Linux boards / Measurement kit -> **ESP32-S3 boards** (only; no notes on model)

## Round 8 (2026-09-29 ~20:50 UTC)
Q1 Lineup (multi): Compact smart speaker / Active two-way / Subwoofer / Streaming amp -> **all four**
Q2 HW home: devices, rows + serve / chorus/hardware + rows / Rows only, never PRs -> **devices, rows + serve**
Q3 Deploy: homelab PR, you merge / Packet only / Not in this program -> Other: **"I dont use proxmox. Review the homelab repo. You can open homelab PRs"** (premise correction: BRIEF §1 Proxmox/R730xd is stale; homelab survey launched)
Q4 Wired: Wired first (Rec.) / Both, per zone / Research and propose -> **Research and propose**

## Round 9 (2026-09-29 ~21:00 UTC)
Q1 Licence: MIT OR Apache-2.0 (Rec.) / GPL-3.0 / AGPL-3.0 / Research and propose -> **MIT OR Apache-2.0 (Rec.)**
Q2 Identity: Scrub files + scan / Scan new only / No identity rule -> **Scrub files + scan**
Q3 homelab merge: Open only, you merge / Self-merge, not apply / Merge and deploy -> **Self-merge, not apply**
Q4 Agents: 3 agents, 2 workers / 2 agents, 2 workers / Up to 5 agents -> **3 agents, 2 workers**

## Round 10 (2026-09-29 ~21:10 UTC)
Q1 Parity 1 (multi): Bonded sets / Alarms + sleep timer / Line-in sharing / Tone + loudness EQ -> **all four**
Q2 Parity 2 (multi): Room correction / Voice (HA Assist) / Announcements/ducking / Setup & onboarding -> **all four**
Q3 Order: Risk first (BRIEF) / Value first / Foundations first / Research and propose -> **Foundations first**
Q4 Clean-room: Docs only, no GPL source / Read, never paste (BRIEF) / Separate reader agents -> **Docs only, no GPL source**

## Round 11 (2026-09-29 ~21:20 UTC)
(Grounded in survey-homelab.md: no Proxmox; bare-metal Debian 13 + Compose; `make ci` needs Docker; host-network exception list plex+mdns-reflector; staged OPNsense VLANs; no audio services in HA.)
Q1 homelab gate (K28 conflict): Open only (Rec.) / Try rootless Docker / Self-merge on subset -> **Open only (Rec.)**
Q2 Host net: Grant host networking / Bridge + reflector / Research and propose -> **Grant host networking**
Q3 Speaker net: Research and propose / IoT VLAN + rule / Dedicated audio VLAN / Servers VLAN -> **Research and propose**
Q4 Tools (multi): Soldering + multimeter / Oscilloscope / Measurement mic / Woodworking shop -> **Soldering + multimeter, Woodworking shop**

## Round 12 (2026-09-29 ~21:30 UTC)
(Grounded in research-endpoint-hardware.md: S3 no APLL, no EMAC; P4 APLL + EMAC + IEEE 1588; core rig ~$380; TAS5825M boards non-US only.)
Q1 MCU: Research and propose (Rec.) / Keep ESP32-S3 / P4 is the reference -> **Research and propose (Rec.)**
Q2 Bench budget: Up to ~$500 / Up to ~$150 / Up to ~$1,000 / No cap; propose -> **No cap; propose**
Q3 HW sources: Permissive only / Any open design / Vendor refs only -> **Permissive only**
Q4 Access: Traefik + token / Traefik only / Research and propose -> **Traefik only**

## Round 13 (2026-09-29 ~21:40 UTC)
Q1 Releases: Tags + GH releases / Tags only / Never -> **Tags + GH releases**
Q2 Upstream: Drafts in chorus / Upstream PRs allowed / No upstream at all -> **Drafts in chorus**
Q3 PWA stack: Research and propose / Svelte + Vite / No-build vanilla -> **Research and propose**
Q4 Acoustics: New shopkit package / Inside chorus / Row to devices -> **New shopkit package**

## Round 14 (2026-09-29 ~21:50 UTC)
Q1 Bench data: Script writes a PR / Drop folder / Paste in a session -> **Script writes a PR**
Q2 Interfaces (multi): chorusctl CLI / Grafana dashboards / Claude Code skills / MQTT discovery -> **chorusctl CLI, Grafana dashboards, MQTT discovery**
Q3 BRIEF edits: Errata ADR, you edit / Dated amendments / Program owns BRIEF -> **Program owns BRIEF**
Q4 Loose ends (multi): Close PR #18 / Renumber ADRs / Stale docs + dead SHAs / Measurement honesty -> **all four**

## Round 15 (2026-09-29 ~22:00 UTC)
Q1 Access table (preview shown: chorus / shopkit / devices / 3d / inventory / homelab / home / super / upstream): Confirm / Change it -> **Confirm**
Q2 Success: Ready to go live / Measured at home / Research and propose -> **Ready to go live**
Q3 ESP-IDF: v5.3 now, propose upgrade / Separate firmware build / Upgrade now -> **v5.3 now, propose upgrade**
Q4 Docs: CLAUDE.md <= 120 lines / <= 200 lines / No limit -> **<= 200 lines**

## Round 16 (2026-09-29 ~22:05 UTC)
Q1 Non-goals (multi): Sonos protocol compat / Portable/battery / Bluetooth input / Cloud / remote access -> **Sonos protocol compat, Portable/battery, Bluetooth input**
Q2 Anything else: Nothing else / Yes, see my notes -> **Nothing else**

Interview closed 2026-09-29 ~22:05 UTC: 16 rounds, 53 questions (plus 4 between-round instructions).

## Interview reopened by the owner (2026-09-29 ~22:40 UTC): "I would like to talk through the ha, sendspin and music assistant to ensure it is actually what I want"
(Research presented in chat first: research-ha-ma-sources.md recommendation P1 Sendspin client bridge + MA hub, with the trade-offs.)

## Round 17 (2026-09-29 ~22:45 UTC)
Q1 MA's role: Yes, MA is the music hub / MA hub + native HA too / chorus stands alone / Not sure yet -> Other: **"This needs to be discussed further and in much more detail. Why should I use sendspin? Chorus should have all the functionality of sendspin. Please ask many more questions"**
Q2 The app: One chorus app / Split: MA + chorus / HA is the app -> Other: **"I want to be able to cast from apps like Spotify to zones or groups around the house. I want an app to be able to setup zones / groups, etc. The scope of what chorus can do needs to be massive. Ask me many, many follow up questions to understand"**
Q3 Grouping: MA groups rooms / chorus groups rooms / Both, kept in sync -> **chorus groups rooms**
Q4 Mixed groups: No, chorus-only / Yes, occasionally / Yes, important -> **No, chorus-only**

## Round 18 (2026-09-29 ~22:55 UTC)
(Explained in chat: why Sendspin is optional; Sendspin capability table vs chorus; casting receiver landscape: librespot MIT reverse-engineered + ToS; AirPlay 2 mature receivers GPL; UPnP/DLNA open; Google Cast impossible; Tidal/Qobuz Connect partner-only.)
Q1 Sendspin/MA: Neither; chorus complete / Native; MA adapter later / Research and propose -> **Neither; chorus complete**
Q2 Casting (multi): Spotify Connect / AirPlay 2 / UPnP/DLNA renderer / Anything legal (research) -> **Anything legal (research), UPnP/DLNA renderer** (Spotify Connect and AirPlay 2 NOT picked; conflicts with round-17 "cast from apps like Spotify": follow-up)
Q3 Receivers: Sidecar processes / Inside chorus-server / Research and propose -> **Research and propose**
Q4 Targets: Rooms + live groups / Rooms + saved groups / All three / Rooms only -> **All three**

## Round 19 (2026-09-29 ~23:05 UTC)
Q1 Legal bar: Personal use is fine / Strict: open only / Research and propose -> **Strict: open only**
Q2 Library (multi): Internet radio / Local music library / Plex/Jellyfin music / Podcasts -> **[No preference]** (unanswered; re-asked in round 20)
Q3 HA form: Custom integration / Core-quality + custom / MQTT only -> **Core-quality + custom**
Q4 Protocol 1 (multi): FLAC on the wire / Opus on the wire / Encryption + pairing / Browser/phone as speaker -> **FLAC on the wire, Opus on the wire, Encryption + pairing**

## Round 20 (2026-09-29 ~23:15 UTC)
Q1 Spotify under the strict bar: Certified streamer in / No Spotify / Research and propose -> Other: **"Research and prose. How does music assistant do it?"** (answered in chat from developer.spotify.com Soloist blog 2026-08-13 + getting-started, and music-assistant.io spotify-connect and airplay-receiver pages, all read 2026-09-29)
Q2 Library: Radio + local library / Everything listed / Nothing; inputs only / Research and propose -> **Nothing; inputs only**
Q3 Protocol 2 (multi): Metadata + artwork / Controller role / Visualizer / color / Any endpoint as source -> **all four**
Q4 Controls (multi): Buttons or touch / Status LED / Mic + mute switch / Small display -> Other: **"It depends on what and where the speaker is. Ask this questions again based on what the speaker is"** (re-asked per class in round 21-22)

## Round 21 (2026-09-29 ~23:25 UTC)
Q1 Soloist vs strict bar: Yes, it's official / Only if terms are clear / No -> **Yes, it's official**
Q2 Compact controls (multi): Buttons or touch / Status LED / Mic + mute switch / Small display -> **Buttons or touch, Status LED, Mic + mute switch**
Q3 Two-way controls (multi): Buttons or touch / Status LED / Mic + mute switch / None visible -> **None visible**
Q4 Sub controls (multi): Pairing button / Status LED / Level + phase knobs / Nothing physical -> **Level + phase knobs, Status LED, Pairing button**

## Round 22 (2026-09-29 ~23:35 UTC)
Q1 Amp (multi): Pairing button + LED / Front buttons / Line-in + optical in / Line/sub out -> **all four** + Other: **"I have a rack server and would love a streaming amp on my rack"**
Q2 Theater front (multi): Touch + status LED / Mic + mute switch / HDMI eARC + optical / None visible -> **all four** (conflict: "None visible" with "Touch + status LED"; follow-up round 23)
Q3 Voice: chorus HA integration / Emulate ESPHome API / Research and propose -> **Research and propose**
Q4 Wake word: On the server / On the speaker / In HA / Research and propose -> **On the server**

## Round 23 (2026-09-29 ~23:45 UTC)
Q1 Theater fix: Two variants / Soundbar with touch / Clean bar, hidden I/O -> **Two variants**
Q2 Rack zones: Research and propose / 2 zones / 4 zones / 8 zones -> **Research and propose**
Q3 Rack loads (multi): In-wall/ceiling / Existing passives / Outdoor speakers / Line out to AVR -> **In-wall/ceiling, Outdoor speakers, Line out to AVR**
Q4 Rack form: Research and propose / 1U / 2U / Shelf-mount box -> **2U**

## Round 24 (2026-09-29 ~23:50 UTC)
Q1 Scale: Up to 16 rooms / Up to 8 rooms / Up to 32 rooms / From my room list -> **Up to 8 rooms**
Q2 Streams: One per room / Up to 4 at once / Research and propose -> **Research and propose**
Q3 Group volume: Sonos-style / One shared level / Per-room only -> **Sonos-style**
Q4 Busy rooms: Take the room / Skip busy rooms / Ask in the app / Per-room priority -> **Take the room**

## Round 25 (2026-09-30 ~00:00 UTC)
Q1 Decoders: Research and propose / Vendor permissive libs / Pure-Rust Symphonia / Write them clean-room -> **Research and propose**
Q2 Alarms (multi): Built-in chimes / Spotify via Soloist / A stream URL / A line-in -> **all four**
Q3 Automation (multi): TV autoplay / Line-in autoplay / Volume limits / Quiet hours -> **all four**
Q4 Remote: LAN + HA only / Via your VPN / Keep open -> Other: **"LAN + HA or vpn"**

## Round 26 (2026-09-30 ~00:10 UTC)
Q1 HA entities (multi): Sound controls / Diagnostics / Button events / Firmware updates -> **all four**
Q2 HA UI: Custom chorus card / Standard cards only / Research and propose -> **Research and propose**
Q3 App users: Admin vs household / Everyone equal / Per-person profiles -> **Everyone equal**
Q4 App places (multi): Phones / Wall tablets / Desktop / TV -> **Phones, Wall tablets, Desktop**

## Round 27 (2026-09-30 ~00:20 UTC)
Q1 Tuning mic (multi): Speakers' own mics / Phone via the app / Measurement mic -> **Phone via the app**
Q2 Enclosures: Mixed per class / Wood everywhere / Printed where possible / Research and propose -> **Research and propose**
Q3 Compact budget: No cap; propose / Under ~$150 / Under ~$300 -> **Under ~$150**
Q4 Power: PoE+ small, mains big / PoE everywhere possible / Research and propose -> **PoE+ small, mains big**

## Round 28 (2026-09-30 ~00:30 UTC)
Q1 Wi-Fi rooms: Some, compact only / All wired / Research and propose -> **Some, compact only**
Q2 Pairing: Button + confirm / PIN on the LED/app / Auto-adopt on LAN -> **Auto-adopt on LAN** (option text stated K62's pairing becomes trust-on-first-use)
Q3 Updates: You approve each / Auto, quiet hours / Auto after soak -> **You approve each**
Q4 Line-in lag: Per-input setting / Always grouped / Automatic -> **Automatic**

## Round 29 (2026-09-30 ~00:40 UTC)
Q1 Architecture (preview of the revised picture shown): Yes, that's it / Mostly; notes / Not yet -> Other: **"Yes, thats it. I am not married to the tech stack. Change as you see fit for the new scope"**
Q2 Linux nodes: Research and propose / Product where it fits / Bench only -> **Product where it fits**
Q3 Anything else: Nothing else / Yes, see notes -> **Nothing else**

Reopened section closed 2026-09-30 ~00:40 UTC. Totals: 29 rounds, 104 questions (plus 4 between-round instructions and 1 reopen request).
