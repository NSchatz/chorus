//! The socket syscalls the receive-timestamp check needs.
//!
//! Kernel software receive timestamps are requested with `SO_TIMESTAMPING`
//! (`SOF_TIMESTAMPING_RX_SOFTWARE | SOF_TIMESTAMPING_SOFTWARE`) and arrive as an
//! `SCM_TIMESTAMPING` control message on an ordinary `recvmsg`: "ts[0] holds a
//! software timestamp if set" (Documentation/networking/timestamping.rst section
//! 2.1.2, https://docs.kernel.org/networking/timestamping.html, read 2026-09-30).
//! Those stamps are system time, which `socket(7)` names for the sibling option:
//! "The clock used for the timestamp is CLOCK_REALTIME" (man-pages 6.19,
//! https://man7.org/linux/man-pages/man7/socket.7.html, read 2026-09-30). So this
//! unit reads `CLOCK_REALTIME`, the settable clock the audio path never reads, to
//! compare like with like; it is excluded from the audio path in
//! `audio-path.conf` for exactly that reason, and no unit of the server calls it.
//!
//! Constant values and struct layouts are the ones the MIT/Apache `libc` crate
//! 0.2.189 carries for 64-bit Linux GNU targets (`SO_TIMESTAMPING_NEW` 65,
//! `SO_TIMESTAMPING_OLD` 37, `SOF_TIMESTAMPING_RX_SOFTWARE` 1 << 3,
//! `SOF_TIMESTAMPING_SOFTWARE` 1 << 4, `msghdr`, `cmsghdr`). On those targets the
//! old and new layouts are the same: `struct timespec` is two 64-bit fields.

// Unsafe allowed in this module: it is the socket FFI boundary of the receive-
// timestamp check (docs/decisions/0000-host-probes.md); every block names its invariant.
#![allow(unsafe_code)]

use std::io;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::raw::{c_int, c_uint, c_void};

const CLOCK_REALTIME: c_int = 0;
const CLOCK_MONOTONIC: c_int = 1;
const SOL_SOCKET: c_int = 1;
const SO_TIMESTAMPING_OLD: c_int = 37;
const SO_TIMESTAMPING_NEW: c_int = 65;
const SOF_TIMESTAMPING_RX_SOFTWARE: c_uint = 1 << 3;
const SOF_TIMESTAMPING_SOFTWARE: c_uint = 1 << 4;
const AF_INET: c_int = 2;
const SOCK_DGRAM: c_int = 2;
const SOCK_CLOEXEC: c_int = 0o2000000;
const IPPROTO_ICMP: c_int = 1;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
struct Iovec {
    base: *mut c_void,
    len: usize,
}

#[repr(C)]
struct Msghdr {
    name: *mut c_void,
    namelen: u32,
    iov: *mut Iovec,
    iovlen: usize,
    control: *mut c_void,
    controllen: usize,
    flags: c_int,
}

#[repr(C)]
struct Cmsghdr {
    len: usize,
    level: c_int,
    kind: c_int,
}

#[repr(C)]
struct SockaddrIn {
    family: u16,
    port_be: u16,
    addr: [u8; 4],
    zero: [u8; 8],
}

extern "C" {
    fn clock_gettime(clock: c_int, ts: *mut Timespec) -> c_int;
    fn setsockopt(fd: c_int, level: c_int, name: c_int, value: *const c_void, len: u32) -> c_int;
    fn recvmsg(fd: c_int, msg: *mut Msghdr, flags: c_int) -> isize;
    fn socket(domain: c_int, kind: c_int, protocol: c_int) -> c_int;
    fn connect(fd: c_int, addr: *const SockaddrIn, len: u32) -> c_int;
}

fn clock_ns(clock: c_int) -> i64 {
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: &mut ts points at a live repr(C) timespec the kernel only writes
    // during the call; both clocks used here always exist.
    unsafe { clock_gettime(clock, &mut ts) };
    ts.tv_sec * 1_000_000_000 + ts.tv_nsec
}

/// Nanoseconds on `CLOCK_REALTIME`, the clock kernel software receive stamps are in.
pub fn realtime_ns() -> i64 {
    clock_ns(CLOCK_REALTIME)
}

/// Nanoseconds on `CLOCK_MONOTONIC`.
pub fn monotonic_ns() -> i64 {
    clock_ns(CLOCK_MONOTONIC)
}

/// Ask for software receive timestamps on `fd`. Returns the option number the
/// kernel accepted (65, `SO_TIMESTAMPING_NEW`, else 37, the old spelling).
pub fn enable_rx_software_stamps(fd: RawFd) -> io::Result<c_int> {
    let flags: c_uint = SOF_TIMESTAMPING_RX_SOFTWARE | SOF_TIMESTAMPING_SOFTWARE;
    let mut last = io::Error::from_raw_os_error(0);
    for option in [SO_TIMESTAMPING_NEW, SO_TIMESTAMPING_OLD] {
        // SAFETY: &flags points at a live unsigned int of the length passed, which
        // the kernel only reads during the call.
        let rc = unsafe {
            setsockopt(
                fd,
                SOL_SOCKET,
                option,
                &flags as *const c_uint as *const c_void,
                std::mem::size_of::<c_uint>() as u32,
            )
        };
        if rc == 0 {
            return Ok(option);
        }
        last = io::Error::last_os_error();
    }
    Err(last)
}

/// One `recvmsg` and the stamps around it.
#[derive(Debug, Clone, Copy)]
pub struct Received {
    /// Bytes received.
    pub len: usize,
    /// The kernel's software receive stamp (`ts[0]`), `CLOCK_REALTIME` nanoseconds,
    /// if the kernel delivered one.
    pub kernel_rt_ns: Option<i64>,
    /// `CLOCK_REALTIME` read first thing after `recvmsg` returned.
    pub user_rt_ns: i64,
    /// `CLOCK_MONOTONIC` read right after that.
    pub user_mono_ns: i64,
}

/// Block in `recvmsg` on `fd` and return what arrived with its stamps.
pub fn recv_stamped(fd: RawFd, buf: &mut [u8]) -> io::Result<Received> {
    // Room for one SCM_TIMESTAMPING record (16-byte header, 48-byte payload) and
    // anything else the kernel adds; u64 keeps it 8-byte aligned as cmsg(3) needs.
    let mut control = [0u64; 32];
    let mut iov = Iovec {
        base: buf.as_mut_ptr() as *mut c_void,
        len: buf.len(),
    };
    let mut msg = Msghdr {
        name: std::ptr::null_mut(),
        namelen: 0,
        iov: &mut iov,
        iovlen: 1,
        control: control.as_mut_ptr() as *mut c_void,
        controllen: std::mem::size_of_val(&control),
        flags: 0,
    };
    let n = loop {
        // SAFETY: msg, iov, buf and control are all live for the call; iov covers
        // exactly buf and controllen exactly control, so the kernel writes only
        // inside them.
        let n = unsafe { recvmsg(fd, &mut msg, 0) };
        if n >= 0 {
            break n as usize;
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    };
    let user_rt_ns = realtime_ns();
    let user_mono_ns = monotonic_ns();
    let kernel_rt_ns = parse_timestamping(
        &control,
        msg.controllen.min(std::mem::size_of_val(&control)),
    );
    Ok(Received {
        len: n,
        kernel_rt_ns,
        user_rt_ns,
        user_mono_ns,
    })
}

/// Walk the control messages and return `ts[0]` of the first `SCM_TIMESTAMPING`.
fn parse_timestamping(control: &[u64; 32], used: usize) -> Option<i64> {
    let base = control.as_ptr() as *const u8;
    let header = std::mem::size_of::<Cmsghdr>();
    let mut at = 0usize;
    while at + header <= used {
        // SAFETY: at + header <= used <= the size of control, and at is a multiple of
        // 8 (CMSG_ALIGN on 64-bit Linux), so the read is in bounds and aligned.
        let h = unsafe { &*(base.add(at) as *const Cmsghdr) };
        if h.len < header || at + h.len > used {
            return None;
        }
        if h.level == SOL_SOCKET
            && (h.kind == SO_TIMESTAMPING_NEW || h.kind == SO_TIMESTAMPING_OLD)
            && h.len >= header + 16
        {
            // SAFETY: the record is at least header + one timespec long and inside
            // control; the payload starts 8-byte aligned after the 16-byte header.
            let ts = unsafe { &*(base.add(at + header) as *const Timespec) };
            if ts.tv_sec != 0 || ts.tv_nsec != 0 {
                return Some(ts.tv_sec * 1_000_000_000 + ts.tv_nsec);
            }
            return None;
        }
        at += (h.len + 7) & !7;
    }
    None
}

/// An unprivileged ICMP echo ("ping") socket connected to `addr`. Linux allows one
/// for any group inside `net.ipv4.ping_group_range` (icmp(7)).
pub fn ping_socket(addr: [u8; 4]) -> io::Result<OwnedFd> {
    // SAFETY: no pointers; a negative result is checked below.
    let fd = unsafe { socket(AF_INET, SOCK_DGRAM | SOCK_CLOEXEC, IPPROTO_ICMP) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is a socket this call just created and nothing else owns.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let sa = SockaddrIn {
        family: AF_INET as u16,
        port_be: 0,
        addr,
        zero: [0; 8],
    };
    // SAFETY: &sa points at a live repr(C) sockaddr_in of the length passed, which
    // the kernel only reads during the call.
    let rc = unsafe { connect(fd, &sa, std::mem::size_of::<SockaddrIn>() as u32) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(owned)
}
