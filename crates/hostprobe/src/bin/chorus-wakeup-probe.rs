//! `chorus-wakeup-probe`: how late does a periodic thread wake on this host?
//!
//! ```text
//! chorus-wakeup-probe --period-us 1000 --seconds 60                 # SCHED_OTHER, slack 1 ns
//! chorus-wakeup-probe --period-us 1000 --seconds 60 --policy fifo --rt-priority 20
//! ```
//!
//! It prints the host's facts, runs the loop in `chorus_hostprobe::wakeup` on the
//! main thread, and prints the lateness histogram (p50, p99, p99.9, max, count,
//! overruns) with the facts again after the run. `--policy fifo` applies the
//! CPU-time bound first and only then takes `SCHED_FIFO` through
//! `chorus_hostctl`, clamped to the host's rtprio ceiling, the same order the
//! server uses (`real-time-acquisitions.conf`).
//!
//! Exit codes: 0 a run was measured; 2 a bad argument; 3 `--policy fifo` and the
//! host granted no real-time priority (the missing-prerequisite exit); 1 anything
//! else failed.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::ExitCode;

use chorus_hostctl::{
    bound_real_time_cpu_time, current_policy, lock_memory, policy_name, take_real_time_policy,
};
use chorus_hostprobe::environment::{moving_facts, static_facts};
use chorus_hostprobe::histogram::{count_at_or_above, log2_buckets_us};
use chorus_hostprobe::wakeup::{apply_timer_slack, run, Config};

const EXIT_BAD_ARGUMENT: u8 = 2;
const EXIT_NO_PREREQUISITE: u8 = 3;

struct Args {
    period_us: u64,
    seconds: u64,
    slack_ns: Option<u64>,
    fifo: bool,
    rt_priority: u32,
    rttime_us: u64,
    lock_memory_bytes: Option<u64>,
    inject_every: usize,
    inject_us: u64,
    raw: Option<String>,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        period_us: 1_000,
        seconds: 60,
        slack_ns: Some(1),
        fifo: false,
        rt_priority: 20,
        rttime_us: 200_000,
        lock_memory_bytes: None,
        inject_every: 0,
        inject_us: 0,
        raw: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{} needs a value", flag));
        match flag.as_str() {
            "--period-us" => a.period_us = num(&value()?)?,
            "--seconds" => a.seconds = num(&value()?)?,
            "--slack-ns" => {
                let v = value()?;
                a.slack_ns = if v == "keep" { None } else { Some(num(&v)?) };
            }
            "--policy" => {
                a.fifo = match value()?.as_str() {
                    "other" => false,
                    "fifo" => true,
                    p => return Err(format!("--policy is other or fifo, not '{}'", p)),
                }
            }
            "--rt-priority" => a.rt_priority = num(&value()?)? as u32,
            "--rttime-us" => a.rttime_us = num(&value()?)?,
            "--lock-memory-mib" => a.lock_memory_bytes = Some(num(&value()?)? << 20),
            "--inject-every" => a.inject_every = num(&value()?)? as usize,
            "--inject-us" => a.inject_us = num(&value()?)?,
            "--raw" => a.raw = Some(value()?),
            other => return Err(format!("unknown argument '{}'", other)),
        }
    }
    if a.period_us == 0 || a.seconds == 0 {
        return Err("--period-us and --seconds must be positive".to_string());
    }
    Ok(a)
}

fn num(s: &str) -> Result<u64, String> {
    s.parse()
        .map_err(|_| format!("'{}' is not a non-negative integer", s))
}

fn us(ns: i64) -> String {
    format!("{:.3}", ns as f64 / 1_000.0)
}

/// Bound the thread's real-time CPU time, then take SCHED_FIFO, in that order.
fn take_fifo(priority: u32, rttime_us: u64) -> Result<String, String> {
    let bound = bound_real_time_cpu_time(rttime_us).map_err(|e| e.to_string())?;
    let grant = take_real_time_policy(priority).map_err(|e| e.to_string())?;
    Ok(format!(
        "{} priority {} (ceiling {}), RLIMIT_RTTIME {} us",
        policy_name(grant.policy),
        grant.priority,
        grant.ceiling,
        bound.soft_display()
    ))
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("chorus-wakeup-probe: {}", e);
            return ExitCode::from(EXIT_BAD_ARGUMENT);
        }
    };
    for (k, v) in static_facts() {
        println!("{:<22} {}", k, v);
    }
    if let Some(bytes) = args.lock_memory_bytes {
        match lock_memory(bytes) {
            Ok(l) => println!("{:<22} locked (limit {})", "memory", l.soft_display()),
            Err(e) => println!("{:<22} not locked: {}", "memory", e),
        }
    }
    let policy = if args.fifo {
        match take_fifo(args.rt_priority, args.rttime_us) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("chorus-wakeup-probe: --policy fifo was refused: {}", e);
                return ExitCode::from(EXIT_NO_PREREQUISITE);
            }
        }
    } else {
        format!("{} (as started)", policy_name(current_policy()))
    };
    println!("{:<22} {}", "policy", policy);
    let slack = match args.slack_ns {
        Some(want) => {
            let (read, err) = apply_timer_slack(want);
            match (read, err) {
                (Some(r), None) => format!("{} ns (asked {} ns)", r, want),
                (r, Some(e)) => format!("{:?} ns; setting {} ns failed: {}", r, want, e),
                (None, None) => format!("unreadable (asked {} ns)", want),
            }
        }
        None => format!("{:?} ns (kept)", chorus_hostprobe::timer_slack_ns().ok()),
    };
    println!("{:<22} {}", "timerslack", slack);
    let period_ns = args.period_us * 1_000;
    let wakeups = (args.seconds * 1_000_000_000 / period_ns) as usize;
    println!(
        "{:<22} {} us x {} wakeups ({} s)",
        "period", args.period_us, wakeups, args.seconds
    );
    if args.inject_every > 0 {
        println!(
            "{:<22} every {}th wakeup made late by >= {} us",
            "injection", args.inject_every, args.inject_us
        );
    }
    for (k, v) in moving_facts() {
        println!("{:<22} {}", format!("{}_before", k), v);
    }
    let config = Config {
        period_ns,
        wakeups,
        inject_every: args.inject_every,
        inject_ns: args.inject_us * 1_000,
    };
    let result = match run(&config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("chorus-wakeup-probe: the loop failed: {}", e);
            return ExitCode::FAILURE;
        }
    };
    for (k, v) in moving_facts() {
        println!("{:<22} {}", format!("{}_after", k), v);
    }
    let Some(s) = result.summary() else {
        eprintln!("chorus-wakeup-probe: no wakeups were recorded");
        return ExitCode::FAILURE;
    };
    println!("{:<22} {:.3} s", "elapsed", result.elapsed_ns as f64 / 1e9);
    println!("{:<22} {}", "count", s.count);
    println!("{:<22} {}", "overruns", result.overruns);
    println!(
        "lateness_us            min {} p50 {} p99 {} p99.9 {} max {} mean {}",
        us(s.min),
        us(s.p50),
        us(s.p99),
        us(s.p999),
        us(s.max),
        us(s.mean)
    );
    if args.inject_every > 0 {
        let injected = result.injected.iter().filter(|&&i| i).count();
        let seen = count_at_or_above(&result.lateness_ns, (args.inject_us * 1_000) as i64);
        println!(
            "{:<22} {} injected, {} wakeups at or above {} us",
            "injection_seen", injected, seen, args.inject_us
        );
    }
    println!("buckets_us (upper bound: count)");
    for (upper, n) in log2_buckets_us(&result.lateness_ns) {
        println!("  < {:>7} us: {}", upper, n);
    }
    if let Some(path) = args.raw {
        let written = File::create(&path).and_then(|f| {
            let mut w = BufWriter::new(f);
            for l in &result.lateness_ns {
                writeln!(w, "{}", l)?;
            }
            w.flush()
        });
        if let Err(e) = written {
            eprintln!("chorus-wakeup-probe: could not write {}: {}", path, e);
            return ExitCode::FAILURE;
        }
        println!("{:<22} {} (lateness ns, one per wakeup)", "raw", path);
    }
    ExitCode::SUCCESS
}
