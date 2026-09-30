//! Audit A-4: `chorus-client --free-run` forms the sync loop's error and
//! applies nothing, and says where on the server timeline it formed it.
//!
//! The same loop and the same exchange, once with correction enabled and once
//! with it disabled: the enabled loop answers with a correction, the free-run
//! loop answers `FreeRun` with the identical error, never touches the servo,
//! and publishes a correction of zero.

use chorus_client_linux::sink::{PcmSink, SinkError};
use chorus_client_linux::sync::{Correction, SyncConfig, SyncLoop};
use chorus_client_linux::SinkWrite;
use chorus_protocol::TimeSync;

struct Device;

impl PcmSink for Device {
    fn device(&self) -> &str {
        "fixture"
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
        // 120 ms at 48 kHz.
        Ok(5_760)
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

/// Server clock 1 s ahead of the client's, 200 us round trip.
fn exchange() -> TimeSync {
    TimeSync {
        t0_ns: 900_000_000,
        t1_ns: 1_900_100_000,
        t2_ns: 1_900_100_000,
        t3_ns: 900_200_000,
    }
}

#[test]
fn free_run_forms_the_same_error_and_corrects_nothing() {
    let config = SyncConfig::default();
    let now_ns = 901_000_000;
    // Playout 1 ms early against the timeline: a correction the servo would act on.
    let next_write_ts_ns = 1_901_000_000 + 120_000_000 - config.playout_latency_ns + 1_000_000;

    let mut corrected = SyncLoop::new(config);
    corrected.offer(&exchange());
    let enabled = corrected
        .observe(&mut Device, now_ns, next_write_ts_ns, 48_000)
        .unwrap();
    let Correction::Fine {
        error_ns: enabled_error,
        correction_ppm,
        ..
    } = enabled
    else {
        panic!("the enabled loop corrects: {:?}", enabled);
    };
    assert!(correction_ppm != 0.0);

    let mut free = SyncLoop::new(config);
    free.set_free_run(true);
    assert!(free.free_run());
    free.offer(&exchange());
    let outcome = free
        .observe(&mut Device, now_ns, next_write_ts_ns, 48_000)
        .unwrap();
    let Correction::FreeRun {
        error_ns,
        server_now_ns,
    } = outcome
    else {
        panic!("the free-run loop answers FreeRun: {:?}", outcome);
    };
    assert_eq!(error_ns, enabled_error);
    assert_eq!(server_now_ns, now_ns as i64 + 1_000_000_000);
    assert_eq!(free.servo_updates(), 0, "the servo is never updated");
    let telemetry = free.telemetry(now_ns);
    assert_eq!(telemetry.correction_ppm, 0.0);
    assert_eq!(telemetry.hard_resyncs, 0);
}
