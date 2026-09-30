//! `chorus-rx-stamps`: user-space receive stamps against the kernel's software
//! receive timestamps, per transport.
//!
//! ```text
//! chorus-rx-stamps --transport tcp-loopback --samples 5000 --period-us 2000
//! chorus-rx-stamps --transport udp-loopback --samples 5000 --period-us 2000
//! chorus-rx-stamps --transport icmp-gateway --samples 2000 --period-us 10000
//! ```
//!
//! See `chorus_hostprobe::rxstamp` for what is compared and how the clock domain
//! is handled. Exit codes: 0 measured; 2 a bad argument; 4 the kernel delivered
//! no receive stamp at all on that transport; 1 anything else failed.

use std::process::ExitCode;

use chorus_hostprobe::environment::{moving_facts, static_facts};
use chorus_hostprobe::histogram::Summary;
use chorus_hostprobe::rxstamp::{default_gateway, run, Config, Transport};

const EXIT_BAD_ARGUMENT: u8 = 2;
const EXIT_NO_STAMPS: u8 = 4;

fn us(ns: i64) -> String {
    format!("{:.3}", ns as f64 / 1_000.0)
}

fn line(label: &str, s: &Summary) -> String {
    format!(
        "{:<22} n {} min {} p50 {} p99 {} p99.9 {} max {} mean {}",
        label,
        s.count,
        us(s.min),
        us(s.p50),
        us(s.p99),
        us(s.p999),
        us(s.max),
        us(s.mean)
    )
}

fn main() -> ExitCode {
    let mut transport = Transport::TcpLoopback;
    let mut samples = 5_000usize;
    let mut period_us = 2_000u64;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next();
        let ok = match (flag.as_str(), value.as_deref()) {
            ("--transport", Some(v)) => Transport::parse(v).map(|t| transport = t).is_some(),
            ("--samples", Some(v)) => v.parse().map(|n| samples = n).is_ok(),
            ("--period-us", Some(v)) => v.parse().map(|n| period_us = n).is_ok(),
            _ => false,
        };
        if !ok || samples == 0 || period_us == 0 {
            eprintln!(
                "chorus-rx-stamps: bad argument '{}' (--transport tcp-loopback|udp-loopback|icmp-gateway, --samples N, --period-us N)",
                flag
            );
            return ExitCode::from(EXIT_BAD_ARGUMENT);
        }
    }
    for (k, v) in static_facts() {
        println!("{:<22} {}", k, v);
    }
    println!("{:<22} {}", "transport", transport.name());
    if transport == Transport::IcmpGateway {
        // The interface only: the gateway's address stays out of anything committed.
        match default_gateway() {
            Ok((iface, _)) => println!("{:<22} the default gateway via {}", "peer", iface),
            Err(e) => println!("{:<22} unreadable ({})", "peer", e),
        }
    }
    println!("{:<22} {} x {} us", "packets", samples, period_us);
    for (k, v) in moving_facts() {
        println!("{:<22} {}", format!("{}_before", k), v);
    }
    let outcome = match run(&Config {
        transport,
        samples,
        period_ns: period_us * 1_000,
    }) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("chorus-rx-stamps: {} failed: {}", transport.name(), e);
            return ExitCode::FAILURE;
        }
    };
    for (k, v) in moving_facts() {
        println!("{:<22} {}", format!("{}_after", k), v);
    }
    println!("{:<22} {}", "socket_option", outcome.option);
    println!(
        "{:<22} stamped {} unstamped {} coalesced {} step_discards {} lost {}",
        "receives",
        outcome.user_minus_kernel_ns.len(),
        outcome.unstamped,
        outcome.coalesced,
        outcome.step_discards,
        outcome.lost
    );
    let Some(d) = outcome.delta_summary() else {
        println!(
            "{:<22} none: the kernel delivered no software receive stamp",
            "user_minus_kernel_us"
        );
        return ExitCode::from(EXIT_NO_STAMPS);
    };
    println!("{}", line("user_minus_kernel_us", &d));
    if let Some(g) = outcome.gap_summary() {
        println!("{}", line("gap_difference_us", &g));
    }
    ExitCode::SUCCESS
}
