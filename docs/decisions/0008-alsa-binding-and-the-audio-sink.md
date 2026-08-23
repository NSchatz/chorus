# 0008: how the client reaches ALSA

- Status: decided
- Recorded by: umbrella spec S0015-chorus-sound-2 (roadmap phase SOUND-2)
- Follows 0002 on dependencies, and BRIEF.md 3.2

## Decision

The Linux client binds `libasound.so.2` at **run time**, with `dlopen` and
`dlsym`, from `crates/alsa`. It adds no crate, no `-dev` package and no build
script, and `cargo build` works on a machine with no audio stack at all.

## Why this needed deciding

ALSA is not a choice. The roadmap phase names it as the output path and the
client has to play through a real audio device. What needed deciding is how to
reach it, and there were three routes:

1. **A crate from the registry** (`alsa`, or `alsa-sys`). The obvious route,
   and the one that lands the first external dependency this repository has
   ever had. 0002 records that "both crates are `std` only, with zero external
   dependencies", that `Cargo.lock` is therefore not committed, and that both
   of those decisions "flip the day the first external dependency lands, which
   is itself a decision-log entry". So this route is available and it is
   expensive: it is a change to the repository's dependency posture, not a
   change to the client.
2. **Link `libasound` directly**, with `extern "C"` declarations and
   `-lasound`. No crate, but a build-time dependency on the ALSA development
   package, which means CI and every contributor's machine needs it installed
   before `cargo build` works at all, including for the crates that have
   nothing to do with audio.
3. **Load `libasound.so.2` at run time.** No crate, no build-time dependency,
   and a missing ALSA runtime becomes a start-up error rather than a link
   error.

## Reasoning

Route 3 wins on the axis this repository already cares about, and it wins a
second time on an axis that only became visible once the failure paths were
written.

**Dependency posture.** BRIEF.md 3.2 says fewer dependencies is a feature and
0002 turned that into a property the build has. Route 3 keeps it exactly:
`cargo build --workspace` on a machine with no ALSA at all still builds every
crate and runs every test that does not need a device. Route 1 would have
landed a dependency and a lockfile for a binding that is roughly two hundred
lines of function signatures; route 2 would have made an audio library a
build-time requirement for a protocol crate that has never seen a sample.

**The failure the client has to have anyway.** The client already has to
report, and exit non-zero on, "the configured audio device cannot be opened".
Under route 3 "there is no ALSA runtime on this machine" is the same kind of
event, arrives on the same path, and is reported the same way. Under route 2 it
is not an event at all: it is a link failure at build time on a developer's
machine and a missing shared object at exec time in production, neither of
which the client gets to say anything about.

**The cost, stated plainly.** A wrong signature in a `dlsym` binding is
undefined behaviour, and nothing checks these against a header, because there
is no header. That is why every one of the ten functions is written out by
hand with a comment rather than generated, why only opaque pointers cross the
boundary, and why the only non-opaque values are the three stable enum
constants ALSA has carried for twenty years. It is also why the surface is
kept at ten functions: `snd_pcm_set_params` does the whole hardware-parameter
negotiation, so no `snd_pcm_hw_params_t` layout is ever assumed.

The second-order cost is real and should be said: this is not the route to
take if the client later needs the parts of the API that hand structs across
the boundary. If that day comes, route 1 is the successor, and the entry that
records it is the entry that flips 0002's lockfile decision.

## Consequences

- `crates/alsa` is the only unsafe code in the audio path and the only place
  that names a shared library.
- The three formats `docs/protocol.md` defines map to `SND_PCM_FORMAT_S16_LE`,
  `SND_PCM_FORMAT_S24_3LE` and `SND_PCM_FORMAT_FLOAT_LE`. The middle one is the
  trap worth writing down: ALSA's `S24_LE` is 24 bits in a **four**-byte
  container, and the protocol's `pcm_s24le` is three packed bytes, which is
  `S24_3LE`. A test asserts the mapping.
- The underrun count comes from the device's own signal, `-EPIPE` from
  `snd_pcm_writei` and `SND_PCM_STATE_XRUN` from `snd_pcm_state`, never from
  the reported delay. The ALSA documentation is explicit that on underrun the
  delay "will not necessarily got down to 0", so a client inferring health from
  a shrinking delay would read an underrun as healthy.
- The client keeps exactly one sink implementation and no configuration
  selects it. A modelled device exists under `crates/client-linux/tests/` and
  cannot be reached from a binary.

## Revisit when

The client needs an ALSA call that passes a struct by value or by layout, or
the first external dependency lands for some other reason and the cost of route
1 has already been paid.
