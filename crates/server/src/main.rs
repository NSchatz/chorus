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
//!        wrong reason.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_hostctl::ThreadRegistry;
use chorus_server::config::ServerConfig;
use chorus_server::hostreport::{
    decide_memory_lock, scheduling_report, take_contract_for_this_thread, RealTimeOutcome,
    SchedulingVerdict,
};
use chorus_server::serve::{serve_stream, ServeParams};
use chorus_server::source;
use chorus_server::stream::{read_requests, write_outbound, Fanout, FanoutSink};

const EXIT_CONFIG: u8 = 2;
const EXIT_CONTRACT: u8 = 3;
const EXIT_TRANSPORT: u8 = 4;
const EXIT_SOURCE: u8 = 5;
const EXIT_UNDECLARED_THREAD: u8 = 6;
const EXIT_INCOMPLETE_INVENTORY: u8 = 7;

/// Every status line carries the contract phrases, so a run that is missing
/// part of the contract says so every time it says anything.
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

    // This process has one thread and it does the audio work, so it is
    // registered exactly once, by whichever of the two paths below it takes.
    // Registering it twice, once as "main" and once as "audio", would put two
    // rows in the report for one thread and make the count meaningless.
    let registry = ThreadRegistry::new();

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

    // The audio thread is this one: it takes the real-time policy and its
    // CPU-time bound before it touches a byte of PCM.
    let real_time = match take_contract_for_this_thread(
        "audio",
        config.rt_priority,
        config.rttime_us,
        config.allow_non_realtime,
        &registry,
    ) {
        Ok(r) => r,
        Err(e) => {
            report("host contract refused", &e.to_string());
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
         rate_skew_ppm={}",
        config.listen,
        config.source,
        config.sample_rate_hz,
        config.channels,
        config.sample_format,
        config.chunk_us,
        config.rate_skew_ppm
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
    let bound = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| config.listen.clone());
    status.say(&format!("listening on={}", bound));

    // ONE source, ONE chunker, ONE timeline, fanned out. Two clients attached
    // at the same time are on the same timeline and get the same presentation
    // timestamp for the same content; a chunker each would give them two
    // streams that merely sound alike.
    let mut pcm = match source::open(&config.source, format, config.tone_ms) {
        Ok(s) => s,
        Err(e) => {
            report(
                "the PCM source could not be opened",
                &format!("{}: {}", config.source, e),
            );
            return ExitCode::from(EXIT_SOURCE);
        }
    };
    status.say(&format!("source {}", pcm.describe()));

    let timeline = MonotonicTimeline::new();
    let fanout = Arc::new(Fanout::new());
    let keep = Arc::new(AtomicBool::new(true));
    let params = ServeParams {
        format,
        chunk_us: config.chunk_us,
        rate_skew_ppm: config.rate_skew_ppm,
    };

    // The first client is waited for before a chunk is cut, so that nothing is
    // produced into an empty room and the stream a listener joins starts where
    // the audio does.
    let (first, peer) = match listener.accept() {
        Ok(v) => v,
        Err(e) => {
            report("a client connection failed", &e.to_string());
            return ExitCode::from(EXIT_TRANSPORT);
        }
    };
    attach(first, peer, &timeline, &fanout, &keep, &status);

    let producer = {
        let fanout = Arc::clone(&fanout);
        let keep = Arc::clone(&keep);
        let go = move || keep.load(Ordering::SeqCst);
        let mut read = move |buf: &mut [u8]| pcm.read(buf);
        let mut sink = FanoutSink::new(fanout);
        thread::spawn(move || serve_stream(params, timeline, &mut read, &mut sink, &go))
    };

    // Everything else that arrives joins the stream already running.
    let acceptor = {
        let fanout = Arc::clone(&fanout);
        let keep = Arc::clone(&keep);
        let status_real_time = status.real_time.clone();
        let status_memory = status.memory.clone();
        thread::spawn(move || {
            let status = Status {
                real_time: status_real_time,
                memory: status_memory,
            };
            while keep.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, peer)) => attach(stream, peer, &timeline, &fanout, &keep, &status),
                    Err(e) => {
                        report("a client connection failed", &e.to_string());
                        return;
                    }
                }
            }
        })
    };

    let outcome = match producer.join() {
        Ok(o) => o,
        Err(_) => {
            report("the stream stopped", "the chunk emitter panicked");
            status.say("stopped reason=stream-failed");
            return ExitCode::from(EXIT_TRANSPORT);
        }
    };
    keep.store(false, Ordering::SeqCst);

    match outcome {
        Ok(served) => {
            status.say(&format!(
                "stream done chunks_sent={} frames_sent={} bytes_discarded={} \
                 ended_cleanly={} final_sequence={} clients={}",
                served.chunks_sent,
                served.frames_sent,
                served.bytes_discarded,
                u8::from(served.ended_cleanly),
                served.final_sequence,
                fanout.subscribers()
            ));
            status.say("stopped reason=stream-ended");
            // The acceptor is parked in `accept`, which nothing here can
            // interrupt without a shutdown syscall this server does not
            // otherwise need; the process is exiting, so it is left to go with
            // it rather than joined.
            drop(acceptor);
            ExitCode::SUCCESS
        }
        Err(e) => {
            report("the stream stopped", &e.to_string());
            status.say("stopped reason=stream-failed");
            drop(acceptor);
            match e {
                chorus_server::serve::ServeError::Source(_) => ExitCode::from(EXIT_SOURCE),
                _ => ExitCode::from(EXIT_TRANSPORT),
            }
        }
    }
}

/// Attach one connection to the stream: audio out, requests in, replies back.
fn attach(
    stream: TcpStream,
    peer: std::net::SocketAddr,
    timeline: &MonotonicTimeline,
    fanout: &Arc<Fanout>,
    keep: &Arc<AtomicBool>,
    status: &Status,
) {
    let _ = stream.set_nodelay(true);
    let reader = match stream.try_clone() {
        Ok(r) => r,
        Err(e) => {
            report(
                "a client connection could not be split",
                &format!("{}: {}", peer, e),
            );
            return;
        }
    };
    // A read timeout is what lets the request reader notice a stopped run
    // rather than sit in a blocking read forever.
    let _ = reader.set_read_timeout(Some(Duration::from_millis(200)));

    let (tx, rx) = fanout.subscribe();
    status.say(&format!(
        "client connected peer={} clients={}",
        peer,
        fanout.subscribers()
    ));

    {
        let timeline = *timeline;
        let mut sink = stream;
        thread::spawn(move || {
            let _ = write_outbound(&mut sink, timeline, &rx);
        });
    }
    {
        let timeline = *timeline;
        let keep = Arc::clone(keep);
        let mut reader = reader;
        thread::spawn(move || {
            let go = move || keep.load(Ordering::SeqCst);
            read_requests(&mut reader, timeline, &tx, &go);
        });
    }
}
