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

use std::io::Write;
use std::net::TcpListener;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_hostctl::ThreadRegistry;
use chorus_server::config::ServerConfig;
use chorus_server::hostreport::{
    decide_memory_lock, scheduling_report, take_contract_for_this_thread, RealTimeOutcome,
};
use chorus_server::serve::{serve_stream, ServeParams};
use chorus_server::source;

const EXIT_CONFIG: u8 = 2;
const EXIT_CONTRACT: u8 = 3;
const EXIT_TRANSPORT: u8 = 4;
const EXIT_SOURCE: u8 = 5;
const EXIT_UNDECLARED_THREAD: u8 = 6;

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

    let (lines, agreed) = scheduling_report(&registry);
    for line in &lines {
        println!("chorus-server: {}", line);
    }
    if !agreed {
        report(
            "the scheduling report and the kernel disagree",
            "a thread runs under a real-time policy that was not reported",
        );
        return ExitCode::from(EXIT_UNDECLARED_THREAD);
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

    loop {
        let (stream, peer) = match listener.accept() {
            Ok(v) => v,
            Err(e) => {
                report("a client connection failed", &e.to_string());
                return ExitCode::from(EXIT_TRANSPORT);
            }
        };
        let _ = stream.set_nodelay(true);
        status.say(&format!("client connected peer={}", peer));

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

        let keep = Arc::new(AtomicBool::new(true));
        let go = {
            let keep = Arc::clone(&keep);
            move || keep.load(Ordering::SeqCst)
        };
        let mut read = move |buf: &mut [u8]| pcm.read(buf);
        let mut sink = stream;
        let params = ServeParams {
            format,
            chunk_us: config.chunk_us,
            rate_skew_ppm: config.rate_skew_ppm,
        };
        let outcome = serve_stream(params, MonotonicTimeline::new(), &mut read, &mut sink, &go);

        match outcome {
            Ok(served) => {
                status.say(&format!(
                    "stream done chunks_sent={} frames_sent={} bytes_discarded={} \
                     ended_cleanly={} final_sequence={}",
                    served.chunks_sent,
                    served.frames_sent,
                    served.bytes_discarded,
                    u8::from(served.ended_cleanly),
                    served.final_sequence
                ));
                if config.once {
                    status.say("stopped reason=stream-ended");
                    return ExitCode::SUCCESS;
                }
            }
            Err(e) => {
                report("the stream stopped", &e.to_string());
                status.say("stopped reason=stream-failed");
                return match e {
                    chorus_server::serve::ServeError::Source(_) => ExitCode::from(EXIT_SOURCE),
                    _ => ExitCode::from(EXIT_TRANSPORT),
                };
            }
        }
    }
}
