//! The player threads: who writes into a player port (goal 16).
//!
//! `--players N` gives the server N network media players, `p0` to
//! `p<N-1>`. Each is a port (`crate::playerport`) and ONE ordinary thread,
//! `player-<i>`, created with the rest of the population before the
//! scheduling report and counted in it (`crates/server/src/main.rs`). A
//! thread per stream, made when a stream starts, is what the thread contract
//! forbids: a thread created after the report is a thread nobody checked.
//! So the threads exist from the start, and what a player does is the
//! [`PlayerDriver`] it was given.
//!
//! This module is the seam between the population and the work. It knows how
//! many players there are, what they are called and how their threads come
//! up; it knows nothing about networks, containers or codecs. The default
//! driver, [`IdleDriver`], waits until the run stops. The renderer (the
//! track after this one) hands [`spawn`] drivers that fetch, decode and write
//! into the port, and changes nothing here.
//!
//! Control code: the threads it starts are ordinary producers on the far
//! side of a port, and nothing here touches a chunk or a stamp.
//! `audio-path.conf` records it as excluded.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_hostctl::ThreadRegistry;

use crate::hostreport::register_ordinary_thread;
use crate::playerport::PlayerPort;

/// Most players one server runs (`--players`). ASSUMED: 16. Each is a thread
/// and a ring held for the life of the process whether or not it ever plays,
/// and a player with no stream slot to play on is no use, so the bound sits
/// at half the 32-slot ceiling: a house that plays sixteen different network
/// streams at once is past anything the brief describes.
pub const MAX_PLAYERS: usize = 16;

/// How long an idle player thread, or a writer waiting for room, sleeps
/// between looks at whether it is still wanted. ASSUMED: 5 ms, a quarter of
/// the default 20 ms tick, so a writer waiting on a full ring is back well
/// before the ring (a second) could run dry.
pub const POLL: Duration = Duration::from_millis(5);

/// What one player's thread does for the life of the run.
///
/// `run` is called once, on the player's own thread, after the thread has
/// registered itself and reported that it is up. It returns when `keep` says
/// stop (and may return earlier; the thread then ends, which is not an
/// error). It owns the producer's side of `port`: it is the only writer, the
/// only one that flushes, holds or finishes it.
pub trait PlayerDriver: Send {
    /// Drive `port` until `keep` is false.
    fn run(&mut self, port: Arc<PlayerPort>, keep: &AtomicBool);
}

/// The driver of a player nothing drives: it waits for the run to end and
/// writes nothing. The port stays empty, so a group that plays it hears
/// silence.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdleDriver;

impl PlayerDriver for IdleDriver {
    fn run(&mut self, _port: Arc<PlayerPort>, keep: &AtomicBool) {
        while keep.load(Ordering::SeqCst) {
            thread::park_timeout(Duration::from_millis(200));
        }
    }
}

/// One [`IdleDriver`] per player.
pub fn idle_drivers(players: usize) -> Vec<Box<dyn PlayerDriver>> {
    (0..players)
        .map(|_| Box::new(IdleDriver) as Box<dyn PlayerDriver>)
        .collect()
}

/// The id of player `index`, which a group's source names as `player:<id>`.
pub fn player_id(index: usize) -> String {
    format!("p{}", index)
}

/// Which of `players` players `id` names: `p<i>` with `i` written without a
/// sign or a leading zero and below `players`.
pub fn player_index(id: &str, players: usize) -> Option<usize> {
    let digits = id.strip_prefix('p')?;
    if digits.is_empty()
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
    {
        return None;
    }
    digits.parse::<usize>().ok().filter(|i| *i < players)
}

/// The ids of `players` players, for a refusal that says what there is.
pub fn player_list(players: usize) -> String {
    if players == 0 {
        return "none: this server was started without --players".to_string();
    }
    (0..players).map(player_id).collect::<Vec<_>>().join(", ")
}

/// Create the player threads, one per port, `player-<i>` driving port `i`
/// with `drivers[i]`. Each registers itself as an ordinary thread and sends
/// one unit down `ready` before its driver runs, as every thread of the
/// population does. Returns how many threads were created, which the caller
/// adds to the number it expects to report.
///
/// # Panics
///
/// When `drivers` and `ports` differ in length: a player with no driver, or
/// a driver with no port, is a wiring mistake in the binary, found at start.
pub fn spawn(
    ports: &[Arc<PlayerPort>],
    drivers: Vec<Box<dyn PlayerDriver>>,
    keep: &Arc<AtomicBool>,
    registry: &Arc<ThreadRegistry>,
    ready: &Sender<()>,
) -> usize {
    assert_eq!(
        ports.len(),
        drivers.len(),
        "one driver per player port: {} ports, {} drivers",
        ports.len(),
        drivers.len()
    );
    for (index, (port, mut driver)) in ports.iter().zip(drivers).enumerate() {
        let port = Arc::clone(port);
        let keep = Arc::clone(keep);
        let registry = Arc::clone(registry);
        let ready = ready.clone();
        thread::spawn(move || {
            register_ordinary_thread(&format!("player-{}", index), &registry);
            if ready.send(()).is_err() {
                return;
            }
            drop(ready);
            driver.run(port, &keep);
        });
    }
    ports.len()
}

/// Write all of `samples` (interleaved frames) into `port`, waiting for room
/// [`POLL`] at a time, for a producer with nothing better to do meanwhile.
/// Stops early when `keep` goes false or `abandon` says so (a stop or a seek
/// arrived: the caller flushes). Returns the frames written.
pub fn write_all(
    port: &PlayerPort,
    samples: &[f32],
    keep: &AtomicBool,
    abandon: &dyn Fn() -> bool,
) -> usize {
    let channels = port.channels();
    let frames = samples.len() / channels;
    let mut done = 0usize;
    while done < frames {
        if !keep.load(Ordering::SeqCst) || abandon() {
            break;
        }
        let n = port.write(&samples[done * channels..frames * channels]);
        done += n;
        if n == 0 {
            thread::sleep(POLL);
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_player_id_is_p_and_its_index() {
        assert_eq!(player_id(0), "p0");
        assert_eq!(player_id(15), "p15");
        assert_eq!(player_index("p0", 2), Some(0));
        assert_eq!(player_index("p1", 2), Some(1));
        for bad in ["p2", "p", "p01", "p-1", "p+1", "q0", "0", "", "p1x", "P0"] {
            assert_eq!(player_index(bad, 2), None, "{}", bad);
        }
        assert_eq!(player_index("p0", 0), None);
        assert_eq!(player_list(3), "p0, p1, p2");
        assert!(player_list(0).contains("--players"));
    }

    struct Writes(Vec<f32>, mpsc::Sender<usize>);

    impl PlayerDriver for Writes {
        fn run(&mut self, port: Arc<PlayerPort>, keep: &AtomicBool) {
            let wrote = write_all(&port, &self.0, keep, &|| false);
            let _ = self.1.send(wrote);
        }
    }

    #[test]
    fn each_player_thread_reports_itself_then_runs_its_driver_on_its_own_port() {
        let ports: Vec<Arc<PlayerPort>> = (0..2)
            .map(|_| Arc::new(PlayerPort::new(48_000, 1, 4)))
            .collect();
        let keep = Arc::new(AtomicBool::new(true));
        let registry = Arc::new(ThreadRegistry::new());
        let (ready, came_up) = mpsc::channel();
        let (done, wrote) = mpsc::channel();
        let drivers: Vec<Box<dyn PlayerDriver>> =
            vec![Box::new(Writes(vec![0.5; 6], done)), Box::new(IdleDriver)];
        assert_eq!(spawn(&ports, drivers, &keep, &registry, &ready), 2);
        drop(ready);
        assert_eq!(came_up.iter().count(), 2, "both reported themselves");
        let mut roles: Vec<String> = registry.snapshot().iter().map(|t| t.role.clone()).collect();
        roles.sort();
        assert_eq!(roles, ["player-0", "player-1"]);
        assert!(registry.snapshot().iter().all(|t| !t.wants_real_time));
        // The first driver's six frames do not fit a ring of four: it waits
        // for room, and the audio thread's side makes some.
        let mut out = [0u8; 4 * 4];
        let mut taken = 0;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while taken < 6 && std::time::Instant::now() < deadline {
            taken += ports[0].play(4, chorus_protocol::SampleFormat::PcmF32Le, &mut out);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(taken, 6);
        assert_eq!(wrote.recv_timeout(Duration::from_secs(10)), Ok(6));
        assert_eq!(ports[1].mark(), 0, "the idle driver writes nothing");
        keep.store(false, Ordering::SeqCst);
    }

    #[test]
    fn a_writer_waiting_for_room_gives_up_when_told_to() {
        let port = PlayerPort::new(48_000, 1, 2);
        let keep = AtomicBool::new(true);
        let asked = std::cell::Cell::new(0);
        let wrote = write_all(&port, &[0.1; 5], &keep, &|| {
            asked.set(asked.get() + 1);
            asked.get() > 3
        });
        assert_eq!(wrote, 2, "what fitted, then abandoned");
    }
}
