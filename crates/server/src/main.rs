//! `chorus-server`: take PCM in, put timestamped chunks on a socket, under a
//! host contract it states out loud.
//!
//! # Exit codes, which are the contract a script grades
//!
//! - `0`  the source ended cleanly, the end-of-stream signal was sent, and the
//!        client was served.
//! - `2`  the configuration was refused, including an unsupported format.
//! - `3`  the host contract was refused: the granted rtprio ceiling is zero,
//!        the real-time policy was denied, or locking memory was denied. The
//!        message names the limit that was read and what was wanted.
//! - `4`  the socket could not be bound, or the client connection failed.
//! - `5`  the PCM source failed mid-stream. No end-of-stream signal is sent in
//!        this case, on purpose.
//! - `6`  a thread runs under a real-time policy that was not reported.
//! - `7`  this process could not take a complete inventory of its own threads,
//!        so it cannot say whether `6` holds. Refusing here rather than
//!        starting is the point: the contract is graded on that report, and a
//!        report built from a list that lost a thread reads clean for the
//!        wrong reason. A thread that was created and never reported itself is
//!        the same failure from the other end and exits the same way.
//!
//! # The shape of the process, and why the report can be taken once
//!
//! Every thread this process will ever run is created here, before the
//! scheduling report is taken, and none is created after it:
//!
//! - the **supervisor**, this thread, which opens a source for each stream and
//!   waits;
//! - the **audio** thread, which is the only one that touches PCM, the only
//!   one that asks for a real-time policy, and the only one that spawns
//!   nothing;
//! - the **acceptor**, which does nothing but accept connections and hand them
//!   to a slot;
//! - two **client** threads per slot, `--max-clients` slots of them, created
//!   whether or not anybody has connected.
//!
//! That shape is deliberate and it is a safety property rather than a style.
//! `std::thread::spawn` inherits the creating thread's scheduling policy
//! (`PTHREAD_INHERIT_SCHED`), so a connection handler spawned from a thread
//! holding `SCHED_FIFO` is real-time too, at the same priority, and
//! `deploy/run-server.sh` runs this binary with `--ulimit rtprio=20`. Taking
//! the real-time policy on the thread that does the audio work, and on no
//! thread that creates another, is what keeps the socket handlers ordinary.
//! Creating them all before the report is what lets one report describe the
//! whole run: `crates/hostctl` grades the report against `/proc/self/task`,
//! and a thread created after that comparison is a thread nobody checked.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_hostctl::ThreadRegistry;
use chorus_server::clients::ClientPool;
use chorus_server::config::ServerConfig;
use chorus_server::hostreport::{
    decide_memory_lock, register_ordinary_thread, scheduling_report, take_contract_for_this_thread,
    ContractRefused, RealTimeOutcome, SchedulingVerdict,
};
use chorus_server::serve::{serve_stream, ServeError, ServeParams, ServeReport};
use chorus_server::source::{self, PcmSource};
use chorus_server::stream::{Fanout, FanoutSink};

const EXIT_CONFIG: u8 = 2;
const EXIT_CONTRACT: u8 = 3;
const EXIT_TRANSPORT: u8 = 4;
const EXIT_SOURCE: u8 = 5;
const EXIT_UNDECLARED_THREAD: u8 = 6;
const EXIT_INCOMPLETE_INVENTORY: u8 = 7;

/// Every status line carries the contract phrases, so a run that is missing
/// part of the contract says so every time it says anything.
#[derive(Clone)]
struct Status {
    real_time: String,
    memory: String,
}

impl Status {
    fn say(&self, line: &str) {
        println!("chorus-server: {} {} {}", line, self.real_time, self.memory);
    }
}

fn report(what: &str, detail: &str) {
    let mut err = std::io::stderr();
    let _ = writeln!(err, "chorus-server: {}: {}", what, detail);
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match ServerConfig::from_args(args) {
        Ok(c) => c,
        Err(e) => {
            report("configuration refused", &e.to_string());
            return ExitCode::from(EXIT_CONFIG);
        }
    };

    // The format is validated before anything else happens, and before a
    // single chunk exists, so an unsupported one can never be interpreted
    // under an assumption.
    let format = match StreamFormat::new(
        config.sample_rate_hz,
        config.channels,
        &config.sample_format,
    ) {
        Ok(f) => f,
        Err(e) => {
            report("stream format refused", &e.to_string());
            println!("chorus-server: stopped reason=unsupported-format chunks_sent=0");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    if let Err(e) = format.frames_in(config.chunk_us) {
        report("chunk duration refused", &e.to_string());
        println!("chorus-server: stopped reason=unsupported-format chunks_sent=0");
        return ExitCode::from(EXIT_CONFIG);
    }

    // Every thread registers itself, from inside itself, exactly once. This one
    // does it here: it supervises, it does no audio work, and it asks for no
    // real-time policy, and a report with no row for a running thread is a
    // report that cannot be checked.
    let registry = Arc::new(ThreadRegistry::new());
    register_ordinary_thread("supervisor", &registry);

    let memory = match decide_memory_lock(
        config.lock_memory,
        config.memlock_wanted_bytes,
        config.allow_unlocked_memory,
    ) {
        Ok(m) => m,
        Err(e) => {
            report("host contract refused", &e.to_string());
            println!("chorus-server: stopped reason=memory-lock-denied chunks_sent=0 played=0");
            return ExitCode::from(EXIT_CONTRACT);
        }
    };

    // ONE timeline for the whole process, ONE fanout, and one source and one
    // chunker per stream. Two clients attached at the same time are on the same
    // timeline and get the same presentation timestamp for the same content; a
    // chunker each would give them two streams that merely sound alike.
    //
    // The timeline outlives a stream on purpose. A second stream on a fresh
    // epoch would step the presentation timestamps of anybody still attached
    // backwards, while the `t1`/`t2` stamps their exchanges are answered with
    // came from the epoch they attached on - two clocks in one connection, and
    // the one thing this whole phase exists to avoid. Monotonic time already
    // gives the next stream an origin later than the last one's, so nothing is
    // gained by restarting it.
    let timeline = MonotonicTimeline::new();
    let fanout = Arc::new(Fanout::new());
    let keep = Arc::new(AtomicBool::new(true));
    let params = ServeParams {
        format,
        chunk_us: config.chunk_us,
        rate_skew_ppm: config.rate_skew_ppm,
    };

    // One unit per thread that came up, so the report below is taken over a
    // thread population that is complete rather than one that is still
    // arriving. Every thread drops its sender the moment it has sent, so
    // reading this to exhaustion terminates whatever happens.
    let (ready, came_up) = mpsc::channel::<()>();

    // The audio thread: the only one that reads PCM, cuts chunks and paces
    // them, the only one that asks the host for a real-time policy, and the
    // only one that creates no thread. It takes the contract for ITSELF, which
    // is the difference between a real-time policy on the thread doing the
    // audio work and a real-time policy on a thread that goes on to make five
    // more that inherit it.
    let (sources, stream_jobs) = mpsc::channel::<Box<dyn PcmSource>>();
    let (outcomes, stream_outcomes) = mpsc::channel::<Result<ServeReport, ServeError>>();
    let (contract, contract_taken) = mpsc::channel::<Result<RealTimeOutcome, ContractRefused>>();
    {
        let registry = Arc::clone(&registry);
        let fanout = Arc::clone(&fanout);
        let keep = Arc::clone(&keep);
        let rt_priority = config.rt_priority;
        let rttime_us = config.rttime_us;
        let allow_non_realtime = config.allow_non_realtime;
        thread::spawn(move || {
            let taken = take_contract_for_this_thread(
                "audio",
                rt_priority,
                rttime_us,
                allow_non_realtime,
                &registry,
            );
            let refused = taken.is_err();
            if contract.send(taken).is_err() || refused {
                return;
            }
            for mut pcm in stream_jobs {
                let mut read = move |buf: &mut [u8]| pcm.read(buf);
                let mut sink = FanoutSink::new(Arc::clone(&fanout));
                let go = {
                    let keep = Arc::clone(&keep);
                    move || keep.load(Ordering::SeqCst)
                };
                let served = serve_stream(params, timeline, &mut read, &mut sink, &go);
                if outcomes.send(served).is_err() {
                    return;
                }
            }
        });
    }

    let real_time = match contract_taken.recv() {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(e)) => {
            report("host contract refused", &e.to_string());
            println!("chorus-server: stopped reason=real-time-denied chunks_sent=0 played=0");
            return ExitCode::from(EXIT_CONTRACT);
        }
        Err(_) => {
            report(
                "host contract refused",
                "the audio thread stopped before it could say whether it had taken the real-time \
                 policy, so this run cannot say that it holds",
            );
            println!("chorus-server: stopped reason=real-time-denied chunks_sent=0 played=0");
            return ExitCode::from(EXIT_CONTRACT);
        }
    };

    let status = Status {
        real_time: match &real_time {
            RealTimeOutcome::Granted {
                ceiling,
                priority,
                rttime_us,
                ..
            } => format!(
                "scheduling=real-time rtprio_ceiling={} rtprio_obtained={} rttime_us={}",
                ceiling, priority, rttime_us
            ),
            RealTimeOutcome::RunningWithout { ceiling, wanted } => format!(
                "scheduling=no-real-time-policy-by-configuration rtprio_ceiling={} \
                 rtprio_wanted={}",
                ceiling, wanted
            ),
        },
        memory: memory.phrase(),
    };

    // Every client thread the process will run, created now, while this thread
    // holds no real-time policy for any of them to inherit.
    let pool = ClientPool::spawn(
        config.max_clients,
        timeline,
        Arc::clone(&keep),
        Arc::clone(&registry),
        ready.clone(),
    );
    let client_threads = pool.threads();

    // The acceptor. It is handed the listener rather than binding one, so that
    // the socket is still bound after the host contract has been reported and
    // graded, exactly as it was when this loop ran on the main thread.
    //
    // One unit per attached client goes down `arrivals`, which is how the
    // supervisor waits for somebody to play to without owning the listener.
    let (arrived, arrivals) = mpsc::channel::<()>();
    let (bound, listening) = mpsc::channel::<TcpListener>();
    {
        let registry = Arc::clone(&registry);
        let fanout = Arc::clone(&fanout);
        let keep = Arc::clone(&keep);
        let ready = ready.clone();
        let status = status.clone();
        thread::spawn(move || {
            register_ordinary_thread("acceptor", &registry);
            if ready.send(()).is_err() {
                return;
            }
            drop(ready);
            let listener = match listening.recv() {
                Ok(listener) => listener,
                Err(_) => return,
            };
            while keep.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        if !attach(stream, peer, &pool, &fanout, &status) {
                            continue;
                        }
                        if arrived.send(()).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        report("a client connection failed", &e.to_string());
                        return;
                    }
                }
            }
        });
    }
    drop(ready);

    // The report is taken over the whole population or not at all.
    let expected = 1 + client_threads;
    let mut up = 0usize;
    while came_up.recv().is_ok() {
        up += 1;
    }
    if up != expected {
        report(
            "the thread inventory could not be completed",
            &format!(
                "{} of {} threads reported themselves before the scheduling report was taken, so \
                 this run cannot say whether a thread is real-time without having been reported, \
                 and it will not claim that it can",
                up, expected
            ),
        );
        println!("chorus-server: stopped reason=incomplete-thread-inventory chunks_sent=0 played=0");
        return ExitCode::from(EXIT_INCOMPLETE_INVENTORY);
    }

    let (lines, verdict) = scheduling_report(&registry);
    for line in &lines {
        println!("chorus-server: {}", line);
    }
    match verdict {
        SchedulingVerdict::Agreed => {}
        SchedulingVerdict::Undeclared { count } => {
            report(
                "the scheduling report and the kernel disagree",
                &format!(
                    "{} threads run under a real-time policy that was not reported",
                    count
                ),
            );
            return ExitCode::from(EXIT_UNDECLARED_THREAD);
        }
        SchedulingVerdict::InventoryIncomplete { reason } => {
            report(
                "the thread inventory could not be completed",
                &format!(
                    "{}; so this run cannot say whether a thread is real-time without having \
                     been reported, and it will not claim that it can",
                    reason
                ),
            );
            println!(
                "chorus-server: stopped reason=incomplete-thread-inventory chunks_sent=0 played=0"
            );
            return ExitCode::from(EXIT_INCOMPLETE_INVENTORY);
        }
    }

    status.say(&format!(
        "starting listen={} source={} rate_hz={} channels={} sample_format={} chunk_us={} \
         rate_skew_ppm={} max_clients={}",
        config.listen,
        config.source,
        config.sample_rate_hz,
        config.channels,
        config.sample_format,
        config.chunk_us,
        config.rate_skew_ppm,
        config.max_clients
    ));
    if memory.is_unlocked() {
        status.say("note this run holds no locked memory");
    }
    if matches!(real_time, RealTimeOutcome::RunningWithout { .. }) {
        status.say("note this run has no real-time policy");
    }

    let listener = match TcpListener::bind(&config.listen) {
        Ok(l) => l,
        Err(e) => {
            report(
                "the listen address could not be bound",
                &format!("{}: {}", config.listen, e),
            );
            return ExitCode::from(EXIT_TRANSPORT);
        }
    };
    let address = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| config.listen.clone());
    status.say(&format!("listening on={}", address));
    if bound.send(listener).is_err() {
        report(
            "a client connection failed",
            "the acceptor stopped before the listening socket reached it",
        );
        return ExitCode::from(EXIT_TRANSPORT);
    }

    // One stream, or one stream after another. `--serve-forever` clears
    // `config.once`, and `deploy/run-server.sh` and `deploy/Dockerfile` both
    // pass it, so this loop is the deployed shape and the single stream is the
    // default one every `tools/` entry point takes.
    loop {
        // A client is waited for before a chunk is cut, so that nothing is
        // produced into an empty room and the stream a listener joins starts
        // where the audio does.
        if arrivals.recv().is_err() {
            report(
                "a client connection failed",
                "the acceptor stopped before a client attached",
            );
            break ExitCode::from(EXIT_TRANSPORT);
        }

        let pcm = match source::open(&config.source, format, config.tone_ms) {
            Ok(s) => s,
            Err(e) => {
                report(
                    "the PCM source could not be opened",
                    &format!("{}: {}", config.source, e),
                );
                break ExitCode::from(EXIT_SOURCE);
            }
        };
        status.say(&format!("source {}", pcm.describe()));

        if sources.send(pcm).is_err() {
            keep.store(false, Ordering::SeqCst);
            report("the stream stopped", "the chunk emitter is gone");
            status.say("stopped reason=stream-failed");
            break ExitCode::from(EXIT_TRANSPORT);
        }

        let outcome = match stream_outcomes.recv() {
            Ok(o) => o,
            Err(_) => {
                keep.store(false, Ordering::SeqCst);
                report("the stream stopped", "the chunk emitter panicked");
                status.say("stopped reason=stream-failed");
                break ExitCode::from(EXIT_TRANSPORT);
            }
        };

        match outcome {
            Ok(served) => {
                status.say(&format!(
                    "stream done chunks_sent={} frames_sent={} bytes_discarded={} \
                     ended_cleanly={} final_sequence={} clients={} dropped_for_slow_clients={}",
                    served.chunks_sent,
                    served.frames_sent,
                    served.bytes_discarded,
                    u8::from(served.ended_cleanly),
                    served.final_sequence,
                    fanout.subscribers(),
                    fanout.dropped()
                ));
                if config.once {
                    keep.store(false, Ordering::SeqCst);
                    status.say("stopped reason=stream-ended");
                    break ExitCode::SUCCESS;
                }
                // The clients of the stream that just ended have had their
                // end-of-stream signal and are leaving, so the arrivals they
                // registered are spent. Discarding them is what makes the next
                // pass wait for a NEW listener rather than replay into a room
                // that has emptied.
                while arrivals.try_recv().is_ok() {}
                status.say("stream ended, waiting for the next client");
            }
            Err(e) => {
                keep.store(false, Ordering::SeqCst);
                report("the stream stopped", &e.to_string());
                status.say("stopped reason=stream-failed");
                break match e {
                    ServeError::Source(_) => ExitCode::from(EXIT_SOURCE),
                    _ => ExitCode::from(EXIT_TRANSPORT),
                };
            }
        }
    }
}

/// Attach one connection to the stream: audio out, requests in, replies back.
///
/// Both directions run on threads the pool created before this process bound a
/// socket. Nothing here creates one, which is what makes the scheduling report
/// printed at startup a description of the whole run.
fn attach(
    stream: TcpStream,
    peer: std::net::SocketAddr,
    pool: &ClientPool,
    fanout: &Arc<Fanout>,
    status: &Status,
) -> bool {
    let _ = stream.set_nodelay(true);
    let reader = match stream.try_clone() {
        Ok(r) => r,
        Err(e) => {
            report(
                "a client connection could not be split",
                &format!("{}: {}", peer, e),
            );
            return false;
        }
    };
    // A read timeout is what lets the request reader notice a stopped run
    // rather than sit in a blocking read forever.
    let _ = reader.set_read_timeout(Some(Duration::from_millis(200)));

    if !pool.attach(stream, reader, || fanout.subscribe()) {
        status.say(&format!(
            "client refused peer={} reason=no-free-client-slot max_clients={} clients={}",
            peer,
            pool.max_clients(),
            fanout.subscribers()
        ));
        return false;
    }
    status.say(&format!(
        "client connected peer={} clients={}",
        peer,
        fanout.subscribers()
    ));
    true
}
