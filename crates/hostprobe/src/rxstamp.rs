//! How far is a user-space receive stamp from the kernel's own receive timestamp
//! for the same packet?
//!
//! The server's time-sync stamps are taken in user space: `t1` when a request has
//! been read and decoded off the client's TCP connection, `t2` when the reply is
//! encoded (`crates/server/src/stream.rs`). Everything between the packet arriving
//! and that read returning (the socket wakeup, the scheduler, the read) is folded
//! into `t1`. The kernel can stamp the packet as it enters the receive stack
//! (`SOF_TIMESTAMPING_RX_SOFTWARE`: "generated just after a device driver hands a
//! packet to the kernel receive stack", Documentation/networking/timestamping.rst,
//! read 2026-09-30), and `recvmsg` hands that stamp back beside the data. The
//! difference between the two is exactly what a kernel stamp would take out of
//! `t1`.
//!
//! # The clock domain
//!
//! Kernel software receive stamps are `CLOCK_REALTIME`, which is settable; the
//! server's timeline is monotonic. This check compares like with like: the user
//! stamp here is `CLOCK_REALTIME` read first thing after `recvmsg` returns, so
//! `user - kernel` is one duration on one clock. A step of that clock between the
//! kernel's stamp and ours would corrupt a sample, so the offset between
//! `CLOCK_REALTIME` and `CLOCK_MONOTONIC` is read before and after every receive
//! and a sample across which it moved by more than [`STEP_GUARD_NS`] is discarded
//! and counted. A slew (a rate adjustment, which adjtime(3) describes as speeding
//! the clock up or slowing it down "by some small percentage") cannot be detected
//! that way; it scales a duration by that fraction, so at an ASSUMED 500 ppm a
//! 100 us duration moves by 50 ns. As a second, clock-domain-free view, the
//! difference between consecutive kernel stamps is compared with the difference
//! between consecutive monotonic user stamps.
//!
//! Three transports: TCP over loopback (the socket kind the server's stamps use
//! today), UDP over loopback (the alternative a protocol could use), and ICMP
//! echo to the container's default gateway through a real interface (`eth0`, a
//! veth), which is the one path here where a packet crosses a device driver.

use std::fs;
use std::io::{self, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::os::fd::AsRawFd;
use std::thread;
use std::time::Duration;

use crate::histogram::{self, Summary};
use crate::net;
use crate::sys;

/// A sample across which `CLOCK_REALTIME - CLOCK_MONOTONIC` moved by more than
/// this is discarded: the settable clock was stepped under it.
pub const STEP_GUARD_NS: i64 = 1_000_000;

/// Bytes per probe message.
const MESSAGE_LEN: usize = 16;

/// Which path the probe packets take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// A TCP connection over 127.0.0.1, `TCP_NODELAY`, one message per period.
    TcpLoopback,
    /// A UDP datagram over 127.0.0.1 per period.
    UdpLoopback,
    /// An ICMP echo to the default gateway per period; the reply is stamped.
    IcmpGateway,
}

impl Transport {
    /// The name the command line and the report use.
    pub fn name(self) -> &'static str {
        match self {
            Transport::TcpLoopback => "tcp-loopback",
            Transport::UdpLoopback => "udp-loopback",
            Transport::IcmpGateway => "icmp-gateway",
        }
    }

    /// Parse a name.
    pub fn parse(name: &str) -> Option<Transport> {
        match name {
            "tcp-loopback" => Some(Transport::TcpLoopback),
            "udp-loopback" => Some(Transport::UdpLoopback),
            "icmp-gateway" => Some(Transport::IcmpGateway),
            _ => None,
        }
    }
}

/// One check's parameters.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// The path the packets take.
    pub transport: Transport,
    /// How many packets to stamp.
    pub samples: usize,
    /// Nanoseconds between packets.
    pub period_ns: u64,
}

/// One check's result.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// `user - kernel` receive stamp, nanoseconds, per stamped packet.
    pub user_minus_kernel_ns: Vec<i64>,
    /// Consecutive-packet view: (user monotonic gap) - (kernel stamp gap), nanoseconds.
    pub gap_difference_ns: Vec<i64>,
    /// Receives that came back with no kernel stamp.
    pub unstamped: usize,
    /// Receives that carried more than one message (TCP coalescing); the kernel
    /// stamp is then the last packet's (socket(7)).
    pub coalesced: usize,
    /// Samples discarded because the settable clock moved under them.
    pub step_discards: usize,
    /// Probes that got no answer in time (ICMP only).
    pub lost: usize,
    /// The socket option number the kernel accepted.
    pub option: i32,
}

impl Outcome {
    /// Order statistics of `user - kernel`.
    pub fn delta_summary(&self) -> Option<Summary> {
        histogram::summarise(&mut self.user_minus_kernel_ns.clone())
    }

    /// Order statistics of the consecutive-packet view.
    pub fn gap_summary(&self) -> Option<Summary> {
        histogram::summarise(&mut self.gap_difference_ns.clone())
    }
}

/// Collects samples from successive receives.
struct Collector {
    out: Outcome,
    previous: Option<(i64, i64)>,
}

impl Collector {
    fn new(option: i32) -> Collector {
        Collector {
            out: Outcome {
                option,
                ..Outcome::default()
            },
            previous: None,
        }
    }

    fn offset() -> i64 {
        net::realtime_ns() - net::monotonic_ns()
    }

    /// Record one receive; `offset_before` was read just before blocking in it.
    fn take(&mut self, offset_before: i64, r: net::Received, one_message: usize) {
        if r.len > one_message {
            self.out.coalesced += 1;
        }
        let Some(kernel) = r.kernel_rt_ns else {
            self.out.unstamped += 1;
            self.previous = None;
            return;
        };
        let offset_after = r.user_rt_ns - r.user_mono_ns;
        if (offset_after - offset_before).abs() > STEP_GUARD_NS {
            self.out.step_discards += 1;
            self.previous = None;
            return;
        }
        self.out.user_minus_kernel_ns.push(r.user_rt_ns - kernel);
        if let Some((prev_kernel, prev_mono)) = self.previous {
            self.out
                .gap_difference_ns
                .push((r.user_mono_ns - prev_mono) - (kernel - prev_kernel));
        }
        self.previous = Some((kernel, r.user_mono_ns));
    }
}

/// Run one check.
pub fn run(config: &Config) -> io::Result<Outcome> {
    match config.transport {
        Transport::TcpLoopback => tcp_loopback(config),
        Transport::UdpLoopback => udp_loopback(config),
        Transport::IcmpGateway => icmp_gateway(config),
    }
}

/// Send `samples` messages, one per period, through `send`.
fn paced_sender<F: FnMut(&[u8]) -> io::Result<()>>(
    samples: usize,
    period_ns: u64,
    mut send: F,
) -> io::Result<()> {
    let mut deadline = sys::monotonic_ns() + period_ns;
    for i in 0..samples {
        sys::sleep_until_monotonic_ns(deadline)?;
        let mut message = [0u8; MESSAGE_LEN];
        message[..8].copy_from_slice(&(i as u64).to_le_bytes());
        send(&message)?;
        deadline += period_ns;
    }
    Ok(())
}

fn tcp_loopback(config: &Config) -> io::Result<Outcome> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    let (samples, period) = (config.samples, config.period_ns);
    let sender =
        thread::Builder::new()
            .name("rx-tcp-sender".into())
            .spawn(move || -> io::Result<()> {
                let mut stream = TcpStream::connect(address)?;
                stream.set_nodelay(true)?;
                paced_sender(samples, period, |m| stream.write_all(m))
            })?;
    let (receiver, _) = listener.accept()?;
    let option = net::enable_rx_software_stamps(receiver.as_raw_fd())?;
    let mut collector = Collector::new(option);
    let mut buf = [0u8; 4096];
    let mut bytes = 0usize;
    while bytes < samples * MESSAGE_LEN {
        let before = Collector::offset();
        let r = net::recv_stamped(receiver.as_raw_fd(), &mut buf)?;
        if r.len == 0 {
            break;
        }
        bytes += r.len;
        collector.take(before, r, MESSAGE_LEN);
    }
    join(sender)?;
    Ok(collector.out)
}

fn udp_loopback(config: &Config) -> io::Result<Outcome> {
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = receiver.local_addr()?;
    let option = net::enable_rx_software_stamps(receiver.as_raw_fd())?;
    receiver.set_read_timeout(Some(Duration::from_secs(2)))?;
    let (samples, period) = (config.samples, config.period_ns);
    let sender =
        thread::Builder::new()
            .name("rx-udp-sender".into())
            .spawn(move || -> io::Result<()> {
                let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
                socket.connect(address)?;
                paced_sender(samples, period, |m| socket.send(m).map(|_| ()))
            })?;
    let mut collector = Collector::new(option);
    let mut buf = [0u8; 2048];
    for _ in 0..samples {
        let before = Collector::offset();
        match net::recv_stamped(receiver.as_raw_fd(), &mut buf) {
            Ok(r) => collector.take(before, r, MESSAGE_LEN),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                collector.out.lost += 1;
                break;
            }
            Err(e) => return Err(e),
        }
    }
    join(sender)?;
    Ok(collector.out)
}

/// The default gateway of this network namespace, from `/proc/net/route`.
pub fn default_gateway() -> io::Result<(String, Ipv4Addr)> {
    let table = fs::read_to_string("/proc/net/route")?;
    parse_default_gateway(&table).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no default route in /proc/net/route",
        )
    })
}

/// Parse `/proc/net/route` text: the interface and gateway of the default route.
/// The kernel prints the addresses as host-order hex of the network-order bytes.
pub fn parse_default_gateway(table: &str) -> Option<(String, Ipv4Addr)> {
    table.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 || fields[1] != "00000000" {
            return None;
        }
        let raw = u32::from_str_radix(fields[2], 16).ok()?;
        if raw == 0 {
            return None;
        }
        Some((fields[0].to_string(), Ipv4Addr::from(raw.to_le_bytes())))
    })
}

/// The ones' complement checksum ICMP carries (RFC 792).
pub fn icmp_checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    for pair in bytes.chunks(2) {
        let word = u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]);
        sum += u32::from(word);
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn icmp_gateway(config: &Config) -> io::Result<Outcome> {
    let (_, gateway) = default_gateway()?;
    let socket = UdpSocket::from(net::ping_socket(gateway.octets())?);
    let option = net::enable_rx_software_stamps(socket.as_raw_fd())?;
    socket.set_read_timeout(Some(Duration::from_millis(500)))?;
    let mut collector = Collector::new(option);
    let mut buf = [0u8; 2048];
    let mut deadline = sys::monotonic_ns() + config.period_ns;
    for i in 0..config.samples {
        sys::sleep_until_monotonic_ns(deadline)?;
        deadline += config.period_ns;
        // Echo request: type 8, code 0, checksum, identifier (the kernel sets it
        // for a ping socket), sequence, payload.
        let mut request = [0u8; 8 + MESSAGE_LEN];
        request[0] = 8;
        request[6..8].copy_from_slice(&(i as u16).to_be_bytes());
        let sum = icmp_checksum(&request);
        request[2..4].copy_from_slice(&sum.to_be_bytes());
        let before = Collector::offset();
        socket.send(&request)?;
        match net::recv_stamped(socket.as_raw_fd(), &mut buf) {
            Ok(r) => collector.take(before, r, request.len()),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                collector.out.lost += 1;
                collector.previous = None;
            }
            Err(e) => return Err(e),
        }
        let now = sys::monotonic_ns();
        if now > deadline {
            deadline = now + config.period_ns;
        }
    }
    Ok(collector.out)
}

fn join(sender: thread::JoinHandle<io::Result<()>>) -> io::Result<()> {
    sender
        .join()
        .map_err(|_| io::Error::other("the sender thread panicked"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_route_parses() {
        let table = "Iface\tDestination\tGateway \tFlags\n\
                     eth0\t00000000\t010200C0\t0003\n\
                     eth0\t000200C0\t00000000\t0001\n";
        // 192.0.2.1 is a documentation address (RFC 5737), never a real one.
        let (iface, gw) = parse_default_gateway(table).unwrap();
        assert_eq!(iface, "eth0");
        assert_eq!(gw, Ipv4Addr::new(192, 0, 2, 1));
        assert_eq!(parse_default_gateway("Iface\n"), None);
    }

    #[test]
    fn the_rfc_1071_example_checksums() {
        // RFC 1071 section 3's worked example: the sum of these words is 0xddf2.
        let bytes = [0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7];
        assert_eq!(icmp_checksum(&bytes), !0xddf2);
    }
}
