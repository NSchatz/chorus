//! S0031-chorus-sync-4, impl gate ordinal 3, finding F9. The body below is the
//! verdict's artifact (`work/specs/S0031-chorus-sync-4/regress_0031_F9.rs`)
//! carried in unchanged from the first `use` onward; only this comment is this
//! repository's.
//!
//! Run it with `make one ARGS="-p chorus-client-linux --test regress_0031_F9"`.
//!
//! AC-15: "IF the server stops answering exchanges while audio keeps arriving
//! THEN THE SYSTEM SHALL keep playing, SHALL stop applying new correction once
//! its newest accepted sample is older than the configured staleness limit, and
//! SHALL report its offset as stale."
//!
//! **What it found.** `SyncLoop::observe` answered a device that reports a
//! delay of zero with `Correction::NoDeviceDelay` and returned BEFORE it looked
//! at the age of the newest accepted sample, so the offset went on being
//! published as fresh (`stale=0`) for as long as the run lasted, however long
//! ago the server had stopped answering. A device reporting zero is not a
//! hypothetical: it is the ALSA `null` device the repository's own
//! `make verify-null-device` runs the shipped binaries against, and a NEGATIVE
//! reported delay - what a real card can return in an xrun - took the same
//! branch.
//!
//! **What it guards now.** That the age of the newest accepted sample is what
//! decides the flag `Telemetry::stale` publishes - "Whether the newest accepted
//! sample is older than the staleness limit", which is the field's own
//! documentation - in every state, whatever the device says about its distance
//! from the DAC.
//!
//! The two halves below are the same silent server and the same sample age.
//! They differ only in what the DEVICE says about its distance from the DAC,
//! which AC-15 does not mention.

use chorus_client_linux::sink::{PcmSink, SinkError};
use chorus_client_linux::sync::{Correction, SyncConfig, SyncLoop};
use chorus_client_linux::SinkWrite;
use chorus_protocol::TimeSync;

/// A device that reports the delay it is asked for.
struct Device {
    name: &'static str,
    delay_frames: i64,
}

impl PcmSink for Device {
    fn device(&self) -> &str {
        self.name
    }
    fn frame_len(&self) -> usize {
        4
    }
    fn rate_hz(&self) -> u32 {
        48_000
    }
    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        Ok(SinkWrite {
            frames_written: (pcm.len() / 4) as u64,
            underran: false,
        })
    }
    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        Ok(self.delay_frames)
    }
    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        Ok(false)
    }
    fn drain(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
    fn frames_played(&mut self) -> Result<u64, SinkError> {
        Ok(0)
    }
}

/// One exchange that any window admits: 199 us of round trip, completed on the
/// client clock at 900 200 000 ns.
fn accepted_exchange() -> TimeSync {
    TimeSync {
        t0_ns: 900_000_000,
        t1_ns: 900_100_000,
        t2_ns: 900_101_000,
        t3_ns: 900_200_000,
    }
}

#[test]
fn a_silent_server_is_reported_stale_whatever_the_device_says_about_its_delay() {
    let config = SyncConfig::default();
    // One accepted exchange, and then the server goes quiet for a good deal
    // longer than the staleness limit. Audio keeps arriving throughout: the
    // device below is accepting frames in both halves.
    let quiet_for_ns = config.staleness_limit_ns + 60_000_000_000;
    let now_ns = accepted_exchange().t3_ns + quiet_for_ns;
    let next_write_ts_ns = now_ns + 40_000_000;

    // Half one: a device with a ring, which is the case the committed tests
    // cover. The offset ages past the limit and is reported stale.
    let mut with_ring = SyncLoop::new(config);
    with_ring.offer(&accepted_exchange());
    let mut ring = Device {
        name: "device-with-a-ring",
        delay_frames: 5_760,
    };
    let outcome = with_ring
        .observe(&mut ring, now_ns, next_write_ts_ns, 48_000)
        .expect("the device reports its delay");
    assert!(
        matches!(outcome, Correction::Stale { .. }),
        "the control half did not go stale at all, so this test is not about what it says it is: \
         got {:?}",
        outcome
    );
    let control = with_ring.telemetry(now_ns);
    assert!(control.stale, "the control half publishes stale=1");
    assert!(control.line().contains("stale=1"));

    // Half two: the same silent server, the same sample, the same age. The
    // only difference is that this device reports a delay of zero, which is
    // what the ALSA `null` device reports for ever and what a card in an xrun
    // can report.
    let mut with_zero = SyncLoop::new(config);
    with_zero.offer(&accepted_exchange());
    let mut zero = Device {
        name: "null",
        delay_frames: 0,
    };
    let outcome = with_zero
        .observe(&mut zero, now_ns, next_write_ts_ns, 48_000)
        .expect("a device that answers zero has not refused");
    assert_eq!(
        outcome,
        Correction::NoDeviceDelay,
        "a delay of zero is answered by correcting nothing, which is not what is under test here"
    );

    let published = with_zero.telemetry(now_ns);
    assert_eq!(
        published.age_ns,
        Some(quiet_for_ns),
        "the age of the newest accepted sample is published either way"
    );
    assert!(
        published.stale,
        "AC-15 requires the offset to be reported as stale once the newest accepted sample is \
         older than the configured staleness limit. The newest accepted sample is {} ns old \
         against a {} ns limit and the client publishes: {}",
        quiet_for_ns,
        config.staleness_limit_ns,
        published.line()
    );
    assert!(
        published.line().contains("stale=1"),
        "the published line says the offset is fresh: {}",
        published.line()
    );
}
