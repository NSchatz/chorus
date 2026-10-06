# chorus card

Multiroom and surround audio: server, sync protocol, speaker firmware. For sibling repos' agents.

## Offers

- The server: `crates/server`, image `deploy/Dockerfile`, host contract `deploy/compose.yaml` and
  `deploy/README.md` (host networking, control plane on port 4020, state in `/var/lib/chorus`).
- Release artifacts (server binary and OCI image, Soloist image, ESP32-S3 images, Linux endpoint
  `.deb`s, `SHA256SUMS`): `docs/release.md`, `tools/release.sh --list`.
- The control plane (JSON, rooms, groups, alarms; `GET /api/state`, `/metrics`):
  `docs/control-plane.md`, `crates/control`; the `chorusctl` CLI: `crates/ctl`, `docs/chorusctl.md`.
- MQTT topics for the house: `docs/mqtt.md`. Home Assistant integration:
  `integrations/homeassistant`, `docs/home-assistant.md`.
- Speaker acoustic designs (drivers, box, crossover, the tolerances an enclosure must hold):
  `docs/hardware/compact-speaker.md`, `docs/hardware/twoway-speaker.md`; enclosure proposal
  `docs/proposals/P12-enclosures.md`.
- Pure Rust cores to reuse: `crates/protocol`, `crates/sync`, `crates/dsp`, `crates/schedule`.

## Hand it work

`goals task add chorus "<title>" --project <id> --body-file <spec>`

A spec for chorus holds, under these headings:

```
# Spec: <title>
Why: <the need, in one or two lines, and which repo asks>
What: <the exact change: behaviour, inputs, outputs>
Where: <crates, docs or files it touches, if known>
Interface: <contract added or changed (catalog, protocol, topic, fixture), and its version>
Done when: <runnable checks: a test, a command and its expected output>
Owner steps: <anything only the owner does (flash, deploy, measure), or "none">
```

chorus will not start a spec that needs a GPL source read, a timing claim without a measurement,
or an install on hardware by an agent.

## Interfaces

- Protocol v2 (wire format, Noise handshake, firmware transfer): `docs/protocol.md`, vectors in
  `fixtures/protocol/`.
- Control plane catalog v1 and v2: `docs/control-plane.md`, vectors in `fixtures/control/`.
- MQTT topics and payloads: `docs/mqtt.md`.
- Speaker design record (`speaker-design-record` v1, exported by shopkit and committed byte for
  byte): `fixtures/design-record/`, read by `crates/dsp/src/design_record.rs`.
- The server container's contract (ports, state path, rtprio, memlock): `deploy/README.md`.
- Pins: toolchain `rust-toolchain.toml`, tools `mise.toml` and `mise.lock`, ESP-IDF
  `firmware/config/endpoint.conf`.

## Not here

- Enclosure models, drawings, print files, PCBs and build logs: devices.
- Printing and slicing: 3d. Shared Python libraries: shopkit.
- Part and supplier records: inventory. The homelab's compose and its deploy: homelab.
- The house record: home. The task tracker and harness: goals.
