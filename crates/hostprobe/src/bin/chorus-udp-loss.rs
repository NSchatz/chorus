//! `chorus-udp-loss`: a LAN's datagram loss, loss bursts and transit
//! variation at the TV path's cadence (goal 13, bench session S8).
//!
//! ```text
//! chorus-udp-loss recv --bind 0.0.0.0:47100 --seconds 86460      # on the receiving host, first
//! chorus-udp-loss send --to <receiver>:47100 --seconds 86400     # on the sending host
//! ```
//!
//! The sender sends one datagram of `--size` bytes (default 1472, the wire's
//! largest) every `--interval-us` (default 2500, one chunk of the low-latency
//! wire's defaults, ADR 0091), each carrying its sequence number and the
//! sender's monotonic stamp, on absolute deadlines so a late wakeup does not
//! slow the stream. The receiver tallies them (`chorus_hostprobe::udploss`):
//! received, lost, reordered, duplicated, the burst-length histogram the FEC
//! interleave depth is chosen from, and RFC 3550's transit variation with a
//! power-of-two table of it. It prints a summary line every
//! `--every-seconds` (default 600) and the whole report at the end. Nothing
//! is encrypted or authenticated: the probe carries no audio and no secret.
//!
//! Exit codes: 0 a run was measured; 2 a bad argument; 1 anything else failed
//! (a socket that would not open, a receiver that heard no probe at all).

use std::net::UdpSocket;
use std::process::ExitCode;
use std::time::Duration;

use chorus_hostprobe::monotonic_ns;
use chorus_hostprobe::udploss::{
    decode, encode, Report, Tally, DATAGRAM_LEN, HEADER_LEN, INTERVAL_US,
};

const EXIT_BAD_ARGUMENT: u8 = 2;

struct Args {
    send: bool,
    address: String,
    seconds: u64,
    interval_us: u64,
    size: usize,
    every_seconds: u64,
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let send = match it.next().as_deref() {
        Some("send") => true,
        Some("recv") => false,
        other => {
            return Err(format!(
                "the first argument is send or recv, not {:?}",
                other
            ))
        }
    };
    let mut a = Args {
        send,
        address: String::new(),
        seconds: 60,
        interval_us: INTERVAL_US,
        size: DATAGRAM_LEN,
        every_seconds: 600,
    };
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{} needs a value", flag));
        match flag.as_str() {
            "--to" if a.send => a.address = value()?,
            "--bind" if !a.send => a.address = value()?,
            "--seconds" => a.seconds = num(&value()?)?,
            "--interval-us" => a.interval_us = num(&value()?)?.max(1),
            "--size" => a.size = num(&value()?)? as usize,
            "--every-seconds" => a.every_seconds = num(&value()?)?.max(1),
            f => return Err(format!("unknown argument {}", f)),
        }
    }
    if a.address.is_empty() {
        return Err(if a.send {
            "send needs --to <host:port>"
        } else {
            "recv needs --bind <addr:port>"
        }
        .to_string());
    }
    if !(HEADER_LEN..=DATAGRAM_LEN).contains(&a.size) {
        return Err(format!(
            "--size is {} to {} bytes",
            HEADER_LEN, DATAGRAM_LEN
        ));
    }
    Ok(a)
}

fn num(s: &str) -> Result<u64, String> {
    s.parse()
        .map_err(|_| format!("'{}' is not a whole number", s))
}

fn send(a: &Args) -> Result<(), String> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("bind: {}", e))?;
    socket
        .connect(&a.address)
        .map_err(|e| format!("{}: {}", a.address, e))?;
    let mut buf = vec![0u8; a.size];
    let step_ns = a.interval_us * 1_000;
    let total = a.seconds * 1_000_000 / a.interval_us;
    let start = monotonic_ns();
    let mut errors = 0u64;
    for sequence in 0..total {
        let due = start + sequence * step_ns;
        let now = monotonic_ns();
        if due > now {
            std::thread::sleep(Duration::from_nanos(due - now));
        }
        encode(&mut buf, sequence, monotonic_ns());
        if socket.send(&buf).is_err() {
            errors += 1;
        }
    }
    println!(
        "udp-loss role=send to={} sent={} send_errors={} interval_us={} size={}",
        a.address, total, errors, a.interval_us, a.size
    );
    Ok(())
}

fn line(r: &Report, elapsed_s: u64) -> String {
    format!(
        "udp-loss role=recv elapsed_s={} received={} lost={} loss_ratio={:.6} reordered={} \
         duplicates={} foreign={} jitter_us={:.1} max_d_us={:.1} bursts={}",
        elapsed_s,
        r.received,
        r.lost,
        r.loss_ratio(),
        r.reordered,
        r.duplicates,
        r.foreign,
        r.jitter_ns as f64 / 1_000.0,
        r.max_d_ns as f64 / 1_000.0,
        r.bursts.iter().map(|(_, n)| n).sum::<u64>()
    )
}

fn recv(a: &Args) -> Result<(), String> {
    let socket = UdpSocket::bind(&a.address).map_err(|e| format!("bind {}: {}", a.address, e))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    let mut tally = Tally::new();
    let mut buf = vec![0u8; 2048];
    let start = monotonic_ns();
    let end = start + a.seconds * 1_000_000_000;
    let mut next_line = start + a.every_seconds * 1_000_000_000;
    loop {
        let now = monotonic_ns();
        if now >= end {
            break;
        }
        if now >= next_line {
            println!("{}", line(&tally.report(), (now - start) / 1_000_000_000));
            next_line += a.every_seconds * 1_000_000_000;
        }
        match socket.recv(&mut buf) {
            Ok(n) => {
                let at = monotonic_ns();
                match decode(&buf[..n]) {
                    Some((sequence, sent)) => tally.push(sequence, sent, at),
                    None => tally.foreign(),
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(format!("recv: {}", e)),
        }
    }
    let r = tally.report();
    println!("{}", line(&r, a.seconds));
    for (len, count) in &r.bursts {
        println!("udp-loss burst length={} count={}", len, count);
    }
    for (upper_us, count) in &r.d_buckets_us {
        println!(
            "udp-loss transit-variation below_us={} count={}",
            upper_us, count
        );
    }
    if r.received == 0 {
        return Err("no probe datagram arrived".to_string());
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("chorus-udp-loss: {}", e);
            return ExitCode::from(EXIT_BAD_ARGUMENT);
        }
    };
    let outcome = if args.send { send(&args) } else { recv(&args) };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("chorus-udp-loss: {}", e);
            ExitCode::FAILURE
        }
    }
}
