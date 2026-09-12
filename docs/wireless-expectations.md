# What a wireless zone is promised

This is the published expectation for a zone declared wireless: what it is held
to, what buffer it gets, and what has actually been measured. It is written for
somebody deciding whether to put a room on Wi-Fi or run a cable to it.

**Read the last section first if you are in a hurry.** The numbers below are
what this system is DESIGNED to hold. What has been MEASURED over a real radio
in a real house is nothing at all, and this document says so rather than letting
the table above it be mistaken for evidence.

Every number here is committed in `config/transport.conf` and nowhere else, and
`tools/wireless-expectations-check.sh` (run by `make verify`) fails when this
document and that file disagree. The reasoning behind each is in
`docs/decisions/0021-the-wireless-tier.md`.

## The two tiers

A zone is DECLARED wired or wireless, on the server's own command line, where
the set of zones is declared:

```
chorus-server --zone kitchen --zone bedroom=wireless
```

The transports a zone may be declared with are `transports = wired wireless`. A
zone declared with anything else stops the server at start, naming the zone, the
value it read and the permitted transports; a zone that declares no transport is
`default_transport = wired`, which is recorded rather than inferred, so no
zone's tier is implicit.

A zone's transport is CONFIGURED and not commanded, exactly as the set of zones
is. No control message changes it, and one that tries is refused and told where
it is declared. The reason is the one `docs/control-plane.md` already gives for
the set of zones: the rooms in a house are a fact about a building, and which
wire or radio a room is on is the same kind of fact.

## What each tier is held to

| tier | inter-device bound | where it comes from |
|---|---|---|
| wired | `wired_bound_us = 500`, which is 0.5 ms | BRIEF.md section 2.2, "Stereo pair / same room" |
| wireless | `wireless_bound_us = 5000`, which is 5 ms | BRIEF.md section 2.2, "Multiroom music, different rooms" |

A wireless zone is held to the multiroom bound and is **not** held to the wired
one. That is the whole of the tier: ten times the room the wired tier has,
because a radio has jitter a cable does not and the honest answer to that is a
looser expectation and a deeper buffer.

**A group cannot be half wired.** A group is the unit a stream is served to and
every endpoint in it plays one timeline, so one wireless zone makes the whole
group wireless. The server reports the tier of every group it serves and names
the zone whose declaration set it, so a group on the loose bound is a fact
somebody declared rather than a surprise.

## The wireless buffer policy

Every endpoint in a group held to this policy applies all of it:

| value | committed as | what it is |
|---|---|---|
| buffer floor | `wireless_min_us = 120000` | the smallest occupancy a run is graded against |
| buffer ceiling | `wireless_max_us = 900000` | the largest occupancy a run is graded against |
| start fill | `wireless_start_fill_us = 400000` | buffered before the first frame is written |
| device delay target | `wireless_device_target_us = 300000` | the reported delay the playout loop holds |
| playout latency | `wireless_playout_latency_us = 500000` | content due at t is audible at t + this, at every endpoint in the group |

The wired tier's equivalent playout latency is `playout_latency_us = 180000`,
committed in `config/sync.conf`, and this phase did not move it or any other
servo constant. That is deliberate and it is the most important sentence in this
document: BRIEF.md section 9 answers "Wi-Fi jitter defeats the servo" with
"bigger buffers or wired-only for that zone; never chase Wi-Fi with servo
aggression". So the whole of this system's answer to a wireless link is the
table above, and a bad jitter figure is answered with a deeper buffer or a cable
and never with a more aggressive servo.

**An endpoint that cannot apply the declared latency stops.** Every endpoint in
a group applies the same latency or they are not aligned with each other, so an
endpoint in a wireless group that would play at the wired latency refuses to
play that stream at all and reports both numbers.

## What the endpoint does about its radio

An ESP32-S3 endpoint on a wireless link SETS its Wi-Fi power save mode from
committed configuration rather than inheriting the platform's. The platform
default is `WIFI_PS_MIN_MODEM`, and under modem sleep the delay in receiving
Wi-Fi data may be as long as the access point's DTIM cycle, which is a delay no
servo can correct because it is not this endpoint's to choose. The endpoint
publishes the mode it set and the mode the platform reports, on the same
`key=value` line it already publishes its link and its amplifier on.

Two things stop the wireless bound from being published even when the mode was
set:

- the platform reports a different mode than the one that was set;
- the radio is in a coexistence mode, where the platform sleeps outside its
  Wi-Fi time slice even with modem sleep disabled.

In both cases the link still comes up and the zone still plays, and the
endpoint's published line says `wireless_bound=withheld`.

**A Linux endpoint in a wireless zone gets the buffer policy and reports
`power_save=unknown`.** Turning Linux modem power save off needs privilege and a
different authority than the one this tier cites, and an endpoint that cannot
say what its radio is doing says so rather than claiming a mode.

**No credential is in this repository.** The endpoint's network name and its
secret are declared `unknown` in `firmware/config/endpoint.conf` and will stay
that way. An endpoint whose network is unknown refuses to join, names which
value is unknown and where to set it, and reports the link down; there is no
default network anywhere in this tree to fall back to.

## What has actually been measured

**Nothing, over a radio. Not one figure in this document is a measurement.**

The two criteria that would measure a wireless zone need an ESP32-S3 endpoint on
a real wireless link playing a grouped stream beside a second endpoint in
another room, with both line outputs captured by the RIG-3 rig, and the second
of them needs that rig run twice. This repository has no ESP32-S3, no capture
rig, no second machine and no wireless link. Both criteria are recorded in
`docs/verification-record.md` as **NOT passed, NOT skipped-green and NOT
satisfied**, and `make verify-wireless` exits non-zero naming what is missing.

What IS saved in `docs/measurements/` is two jitter reports, one per power-save
mode, computed from **committed modelled series** rather than from any radio.
They exist so the report's shape and its arithmetic are gradeable with no
hardware present, they say "NOT A MEASUREMENT" in their own text, and no claim
about Wi-Fi rests on them.

So: the bounds above are what a wireless zone is PROMISED, and the promise has
not been checked. If you are deciding between Wi-Fi and a cable for a room that
matters, the cable is the one this repository has evidence for.
