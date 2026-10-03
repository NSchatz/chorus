//! `chorus-soloistd`: the supervisor of one Spotify Soloist receiver.
//!
//! One receiver container holds PipeWire, WirePlumber, one Soloist process
//! (the owner's binary, mounted; chorus ships none) and this program. It
//! shares one directory with chorus-server, the receiver directory, and
//! nothing else: no network path exists between them (`docs/soloist.md`).
//!
//! What it does, in order:
//!
//! 1. claims a receiver index by taking an exclusive lock on `r<i>.lock`
//!    (the lowest free index below `--receivers`, or the one `--receiver`
//!    names), and makes the PCM FIFO `r<i>.pcm`;
//! 2. unless `--pipewire none`, writes PipeWire's configuration with a
//!    pipe-tunnel sink `chorus-r<i>` that writes the FIFO, and runs
//!    PipeWire and WirePlumber;
//! 3. reads `soloist --version` and works out when the build expires;
//! 4. listens on `r<i>.sock` and follows the server's `assign`, `release`,
//!    `command` and `restart` (the protocol is `chorus_soloist::protocol`);
//! 5. runs Soloist for the assigned target, connects to its WebSocket API
//!    with the port from `ws.port`, and relays every event;
//! 6. treats exit code 10 as "expired" (no restart until told), any other
//!    exit as a failure retried with a capped backoff, and never writes the
//!    API key to a log or a message.
//!
//! std only: threads and blocking I/O with timeouts, no async runtime. The
//! main thread owns every decision ([`supervisor`]); the other threads read
//! one thing each and send what they read to it.
//!
//! Exit codes: 0 stopped by SIGTERM or SIGINT; 2 a usage error; 3 no
//! receiver index is free (or the one named is held); 4 the receiver
//! directory cannot be used (the lock file, the FIFO or the socket); 5
//! PipeWire could not be started.

#![warn(missing_docs)]

pub mod args;
pub mod log;
pub mod pipewire;
pub mod supervisor;
pub mod sys;
pub mod wsclient;

/// The exit code of a usage error.
pub const EXIT_USAGE: i32 = 2;
/// The exit code when no receiver index could be claimed.
pub const EXIT_NO_INDEX: i32 = 3;
/// The exit code when the receiver directory cannot be used.
pub const EXIT_DIRECTORY: i32 = 4;
/// The exit code when PipeWire could not be started.
pub const EXIT_PIPEWIRE: i32 = 5;

/// Run the supervisor with the command line's arguments (the program name
/// left out) until it is told to stop; returns the exit code.
pub fn run(arguments: Vec<String>) -> i32 {
    let config = match args::parse(&arguments) {
        Ok(args::Parsed::Run(config)) => config,
        Ok(args::Parsed::Help) => {
            println!("{}", args::USAGE);
            return 0;
        }
        Ok(args::Parsed::Version) => {
            println!("chorus-soloistd {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        Err(why) => {
            eprintln!("chorus-soloistd: {why}\n\n{}", args::USAGE);
            return EXIT_USAGE;
        }
    };
    supervisor::run(*config)
}
