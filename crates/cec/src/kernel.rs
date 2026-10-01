//! The kernel's CEC adapter, `/dev/cecN`, through its ioctls.
//!
//! # Sources (clean room, BRIEF.md guardrail 1, `docs/clean-room.md`)
//!
//! Everything here comes from the kernel's CEC userspace API documentation
//! prose and from permissive source, all read 2026-10-01; the kernel's C
//! source and its uapi header were never opened:
//!
//! - opening: "Access mode must be O_RDWR"
//!   (<https://docs.kernel.org/userspace-api/media/cec/cec-func-open.html>);
//! - `CEC_ADAP_G_CAPS` and `struct cec_caps`
//!   (<https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-caps.html>);
//! - `CEC_ADAP_G_PHYS_ADDR`
//!   (<https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-phys-addr.html>);
//! - `CEC_ADAP_G/S_LOG_ADDRS` and `struct cec_log_addrs`
//!   (<https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-log-addrs.html>);
//! - `CEC_S_MODE` and what passthrough leaves to the follower
//!   (<https://docs.kernel.org/userspace-api/media/cec/cec-ioc-g-mode.html>);
//! - `CEC_TRANSMIT`, `CEC_RECEIVE` and `struct cec_msg`
//!   (<https://docs.kernel.org/userspace-api/media/cec/cec-ioc-receive.html>);
//! - `CEC_DQEVENT` and `struct cec_event`
//!   (<https://docs.kernel.org/userspace-api/media/cec/cec-ioc-dqevent.html>);
//! - `poll()`: POLLPRI means events are pending
//!   (<https://docs.kernel.org/userspace-api/media/cec/cec-func-poll.html>);
//! - the ioctl request numbers: the MIT `cec_linux` 0.2.2 crate's comments
//!   restate each one's direction, number and struct
//!   (<https://github.com/User65k/cec_linux>, `src/sys.rs`); goal 13's
//!   research (`cec.md` section 1.8) recomputed every number from the
//!   documented layouts with the asm-generic encoding (`dir << 30 | size <<
//!   16 | 'a' << 8 | nr`, read 2 and write 1), identical on x86_64 and
//!   aarch64, and the size of each struct below is asserted at compile time
//!   so a layout typed wrong cannot build. That crate declares `features` as
//!   `[[u8; 4]; 12]`, transposed against the documented `[4][12]`; chorus
//!   declares the documented shape.
//!
//! # Mode
//!
//! `CEC_MODE_INITIATOR | CEC_MODE_EXCL_FOLLOWER_PASSTHRU` (0x01 | 0x30): in
//! passthrough "only this file descriptor will receive CEC messages for
//! processing", and the core leaves Get CEC Version, Give Device Vendor ID,
//! Abort, Give Physical Address, Give OSD Name and the user control messages
//! to the follower, which is why the role answers all of them itself
//! ([K-MODE] in `cec.md` 1.5). The core still claims the logical address by
//! polling.
//!
//! # libc
//!
//! `ioctl` and `poll` are libc's own, declared here: the std-linked binary
//! already links libc, so nothing is added to the build (the same reason
//! `crates/hostctl` declares its few libc calls). `POLLPRI` is 0x2 on Linux
//! (the `libc` crate 0.2.189, MIT OR Apache-2.0, read 2026-10-01).

// Unsafe allowed in this module only: it is the ioctl binding to /dev/cecN; every block below says what it relies on.
#![allow(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::raw::{c_int, c_ulong};
use std::time::Duration;

use crate::adapter::{Adapter, AdapterError, Claim, Claimed, Event, TxStatus};
use crate::codec::{Message, PhysicalAddress, AUDIO_SYSTEM, MAX_MESSAGE_LEN};

extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn poll(fds: *mut PollFd, nfds: c_ulong, timeout: c_int) -> c_int;
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

const POLLPRI: i16 = 0x2;

/// `CEC_ADAP_G_CAPS`.
pub const CEC_ADAP_G_CAPS: c_ulong = 0xC04C_6100;
/// `CEC_ADAP_G_PHYS_ADDR`.
pub const CEC_ADAP_G_PHYS_ADDR: c_ulong = 0x8002_6101;
/// `CEC_ADAP_G_LOG_ADDRS`.
pub const CEC_ADAP_G_LOG_ADDRS: c_ulong = 0x805C_6103;
/// `CEC_ADAP_S_LOG_ADDRS`.
pub const CEC_ADAP_S_LOG_ADDRS: c_ulong = 0xC05C_6104;
/// `CEC_TRANSMIT`.
pub const CEC_TRANSMIT: c_ulong = 0xC038_6105;
/// `CEC_RECEIVE`.
pub const CEC_RECEIVE: c_ulong = 0xC038_6106;
/// `CEC_DQEVENT`.
pub const CEC_DQEVENT: c_ulong = 0xC050_6107;
/// `CEC_S_MODE`.
pub const CEC_S_MODE: c_ulong = 0x4004_6109;

/// The asm-generic encoding the numbers above follow, so a test can
/// recompute each from its struct's size.
pub const fn ioc(dir: u32, nr: u32, size: usize) -> c_ulong {
    ((dir << 30) | ((size as u32) << 16) | (0x61 << 8) | nr) as c_ulong
}

/// `CEC_CAP_LOG_ADDRS`.
const CAP_LOG_ADDRS: u32 = 0x2;
/// `CEC_CAP_TRANSMIT`.
const CAP_TRANSMIT: u32 = 0x4;
/// `CEC_CAP_PASSTHROUGH`.
const CAP_PASSTHROUGH: u32 = 0x8;
/// `CEC_MODE_INITIATOR | CEC_MODE_EXCL_FOLLOWER_PASSTHRU`.
pub const MODE_INITIATOR_PASSTHRU: u32 = 0x01 | 0x30;
/// `CEC_LOG_ADDR_TYPE_AUDIOSYSTEM`.
const LOG_ADDR_TYPE_AUDIOSYSTEM: u8 = 4;
/// `CEC_OP_PRIM_DEVTYPE_AUDIOSYSTEM`.
const PRIM_DEVTYPE_AUDIOSYSTEM: u8 = 5;
/// `CEC_OP_ALL_DEVTYPE_AUDIOSYSTEM`.
const ALL_DEVTYPE_AUDIOSYSTEM: u8 = 0x08;
/// `CEC_LOG_ADDR_INVALID`.
const LOG_ADDR_INVALID: u8 = 0xff;
/// `CEC_VENDOR_ID_NONE` (the `cec_linux` crate's `VendorID::NONE`).
const VENDOR_ID_NONE: u32 = 0xffff_ffff;
/// `CEC_TX_STATUS_*`.
const TX_OK: u8 = 0x01;
const TX_ARB_LOST: u8 = 0x02;
const TX_NACK: u8 = 0x04;
/// `CEC_EVENT_STATE_CHANGE`, `CEC_EVENT_LOST_MSGS`.
const EVENT_STATE_CHANGE: u32 = 1;
const EVENT_LOST_MSGS: u32 = 2;

/// `struct cec_caps`, 76 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct CecCaps {
    driver: [u8; 32],
    name: [u8; 32],
    available_log_addrs: u32,
    capabilities: u32,
    version: u32,
}

/// `struct cec_log_addrs`, 92 bytes (91 of fields, padded to 4).
#[repr(C)]
#[derive(Clone, Copy)]
struct CecLogAddrs {
    log_addr: [u8; 4],
    log_addr_mask: u16,
    cec_version: u8,
    num_log_addrs: u8,
    vendor_id: u32,
    flags: u32,
    osd_name: [u8; 15],
    primary_device_type: [u8; 4],
    log_addr_type: [u8; 4],
    all_device_types: [u8; 4],
    features: [[u8; 12]; 4],
}

/// `struct cec_msg`, 56 bytes (55 of fields, padded to 8).
#[repr(C)]
#[derive(Clone, Copy)]
struct CecMsg {
    tx_ts: u64,
    rx_ts: u64,
    len: u32,
    timeout: u32,
    sequence: u32,
    flags: u32,
    msg: [u8; 16],
    reply: u8,
    rx_status: u8,
    tx_status: u8,
    tx_arb_lost_cnt: u8,
    tx_nack_cnt: u8,
    tx_low_drive_cnt: u8,
    tx_error_cnt: u8,
}

/// `struct cec_event`, 80 bytes: the 64-byte union read as `state_change`
/// (`u16 phys_addr, u16 log_addr_mask, u16 have_conn_info`) or `lost_msgs`
/// (`u32 lost_msgs`).
#[repr(C)]
#[derive(Clone, Copy)]
struct CecEvent {
    ts: u64,
    event: u32,
    flags: u32,
    raw: [u32; 16],
}

const _: () = assert!(std::mem::size_of::<CecCaps>() == 76);
const _: () = assert!(std::mem::size_of::<CecLogAddrs>() == 92);
const _: () = assert!(std::mem::size_of::<CecMsg>() == 56);
const _: () = assert!(std::mem::size_of::<CecEvent>() == 80);
const _: () = assert!(CEC_ADAP_G_CAPS == ioc(3, 0, std::mem::size_of::<CecCaps>()));
const _: () = assert!(CEC_ADAP_G_PHYS_ADDR == ioc(2, 1, 2));
const _: () = assert!(CEC_ADAP_G_LOG_ADDRS == ioc(2, 3, std::mem::size_of::<CecLogAddrs>()));
const _: () = assert!(CEC_ADAP_S_LOG_ADDRS == ioc(3, 4, std::mem::size_of::<CecLogAddrs>()));
const _: () = assert!(CEC_TRANSMIT == ioc(3, 5, std::mem::size_of::<CecMsg>()));
const _: () = assert!(CEC_RECEIVE == ioc(3, 6, std::mem::size_of::<CecMsg>()));
const _: () = assert!(CEC_DQEVENT == ioc(3, 7, std::mem::size_of::<CecEvent>()));
const _: () = assert!(CEC_S_MODE == ioc(1, 9, 4));

fn zeroed_msg() -> CecMsg {
    CecMsg {
        tx_ts: 0,
        rx_ts: 0,
        len: 0,
        timeout: 0,
        sequence: 0,
        flags: 0,
        msg: [0; 16],
        reply: 0,
        rx_status: 0,
        tx_status: 0,
        tx_arb_lost_cnt: 0,
        tx_nack_cnt: 0,
        tx_low_drive_cnt: 0,
        tx_error_cnt: 0,
    }
}

fn zeroed_log_addrs() -> CecLogAddrs {
    CecLogAddrs {
        log_addr: [0; 4],
        log_addr_mask: 0,
        cec_version: 0,
        num_log_addrs: 0,
        vendor_id: 0,
        flags: 0,
        osd_name: [0; 15],
        primary_device_type: [0; 4],
        log_addr_type: [0; 4],
        all_device_types: [0; 4],
        features: [[0; 12]; 4],
    }
}

/// One `/dev/cecN`, held in passthrough as the Audio System.
#[derive(Debug)]
pub struct KernelAdapter {
    file: File,
    path: String,
    /// The driver's name from `CEC_ADAP_G_CAPS`.
    pub driver: String,
    /// The adapter's name from `CEC_ADAP_G_CAPS`.
    pub name: String,
}

fn io_error(what: &str) -> AdapterError {
    let e = io::Error::last_os_error();
    // ENODEV: the adapter went away (an unplugged USB dongle).
    if e.raw_os_error() == Some(19) {
        return AdapterError::Closed;
    }
    AdapterError::Io(format!("{}: {}", what, e))
}

fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

impl KernelAdapter {
    /// Open `path` (O_RDWR), check it can claim and transmit, and take it in
    /// passthrough. Refused by name: a device that is not there, one this
    /// user may not open (the group that owns `/dev/cecN`), one without
    /// `CEC_CAP_LOG_ADDRS` (a monitor-only adapter), or one another process
    /// holds exclusively (`EBUSY`).
    pub fn open(path: &str) -> Result<KernelAdapter, AdapterError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| AdapterError::Unusable(format!("{}: {}", path, e)))?;
        let fd = file.as_raw_fd();
        let mut caps = CecCaps {
            driver: [0; 32],
            name: [0; 32],
            available_log_addrs: 0,
            capabilities: 0,
            version: 0,
        };
        // SAFETY: `fd` is open for as long as `file` lives; CEC_ADAP_G_CAPS
        // writes exactly one `struct cec_caps`, whose 76-byte layout is
        // asserted above, into the valid, exclusively borrowed `caps`.
        let r = unsafe { ioctl(fd, CEC_ADAP_G_CAPS, &mut caps as *mut CecCaps) };
        if r < 0 {
            return Err(AdapterError::Unusable(format!(
                "{} is not a CEC adapter: {}",
                path,
                io::Error::last_os_error()
            )));
        }
        let needed = CAP_LOG_ADDRS | CAP_TRANSMIT;
        if caps.capabilities & needed != needed || caps.available_log_addrs == 0 {
            return Err(AdapterError::Unusable(format!(
                "{} cannot claim a logical address and transmit (capabilities 0x{:x})",
                path, caps.capabilities
            )));
        }
        if caps.capabilities & CAP_PASSTHROUGH == 0 {
            return Err(AdapterError::Unusable(format!(
                "{} has no passthrough mode (capabilities 0x{:x}), so the kernel would answer \
                 for the Audio System and the role could not",
                path, caps.capabilities
            )));
        }
        let mode: u32 = MODE_INITIATOR_PASSTHRU;
        // SAFETY: CEC_S_MODE reads one u32 from the pointer, which points at
        // `mode`, alive for the call.
        let r = unsafe { ioctl(fd, CEC_S_MODE, &mode as *const u32) };
        if r < 0 {
            return Err(AdapterError::Unusable(format!(
                "{}: initiator and exclusive passthrough follower refused: {}",
                path,
                io::Error::last_os_error()
            )));
        }
        Ok(KernelAdapter {
            file,
            path: path.to_string(),
            driver: text(&caps.driver),
            name: text(&caps.name),
        })
    }

    fn fd(&self) -> c_int {
        self.file.as_raw_fd()
    }

    fn physical_address(&self) -> Result<PhysicalAddress, AdapterError> {
        let mut pa: u16 = 0;
        // SAFETY: CEC_ADAP_G_PHYS_ADDR writes one u16 into `pa`, valid and
        // exclusively borrowed for the call.
        let r = unsafe { ioctl(self.fd(), CEC_ADAP_G_PHYS_ADDR, &mut pa as *mut u16) };
        if r < 0 {
            return Err(io_error("CEC_ADAP_G_PHYS_ADDR"));
        }
        Ok(PhysicalAddress(pa))
    }

    fn log_addrs(&self, request: c_ulong, la: &mut CecLogAddrs) -> Result<(), AdapterError> {
        // SAFETY: both log-address ioctls read and/or write exactly one
        // `struct cec_log_addrs` (92 bytes, asserted above) through the
        // pointer, which is the valid, exclusively borrowed `la`.
        let r = unsafe { ioctl(self.fd(), request, la as *mut CecLogAddrs) };
        if r < 0 {
            return Err(io_error(if request == CEC_ADAP_S_LOG_ADDRS {
                "CEC_ADAP_S_LOG_ADDRS"
            } else {
                "CEC_ADAP_G_LOG_ADDRS"
            }));
        }
        Ok(())
    }

    /// The device path, for status lines.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Adapter for KernelAdapter {
    fn claim(&mut self, claim: &Claim) -> Result<Claimed, AdapterError> {
        // "EBUSY if types are already set": clear any earlier claim first
        // (num_log_addrs = 0 clears all, [K-LOG]).
        let mut current = zeroed_log_addrs();
        self.log_addrs(CEC_ADAP_G_LOG_ADDRS, &mut current)?;
        if current.num_log_addrs != 0 {
            let mut clear = zeroed_log_addrs();
            self.log_addrs(CEC_ADAP_S_LOG_ADDRS, &mut clear)?;
        }
        let mut la = zeroed_log_addrs();
        la.cec_version = claim.cec_version;
        la.num_log_addrs = 1;
        la.vendor_id = claim.vendor_id.unwrap_or(VENDOR_ID_NONE);
        let name = claim.osd_name.as_bytes();
        let n = name.len().min(14);
        la.osd_name[..n].copy_from_slice(&name[..n]);
        la.primary_device_type[0] = PRIM_DEVTYPE_AUDIOSYSTEM;
        la.log_addr_type[0] = LOG_ADDR_TYPE_AUDIOSYSTEM;
        la.all_device_types[0] = ALL_DEVTYPE_AUDIOSYSTEM;
        // With a valid physical address this blocks until the core has
        // polled for and claimed the address ([K-LOG]).
        self.log_addrs(CEC_ADAP_S_LOG_ADDRS, &mut la)?;
        let logical_address = match la.log_addr[0] {
            LOG_ADDR_INVALID => None,
            AUDIO_SYSTEM => Some(AUDIO_SYSTEM),
            // 0xf (Unregistered fallback) or anything else: not the Audio
            // System, so the role cannot run.
            _ => None,
        };
        Ok(Claimed {
            logical_address,
            physical_address: self.physical_address()?,
        })
    }

    fn transmit(&mut self, m: &Message) -> Result<TxStatus, AdapterError> {
        let bytes = m
            .encode()
            .map_err(|e| AdapterError::Io(format!("not sent: {}", e)))?;
        let mut msg = zeroed_msg();
        msg.len = bytes.len() as u32;
        msg.msg[..bytes.len()].copy_from_slice(&bytes);
        // SAFETY: CEC_TRANSMIT reads and writes exactly one `struct cec_msg`
        // (56 bytes, asserted above) through the pointer, the valid,
        // exclusively borrowed `msg`. Blocking: it returns when the transmit
        // is done.
        let r = unsafe { ioctl(self.fd(), CEC_TRANSMIT, &mut msg as *mut CecMsg) };
        if r < 0 {
            return Err(io_error("CEC_TRANSMIT"));
        }
        let s = msg.tx_status;
        Ok(if s & TX_OK != 0 {
            TxStatus::Ok
        } else if s & TX_NACK != 0 {
            TxStatus::Nack
        } else if s & TX_ARB_LOST != 0 {
            TxStatus::ArbitrationLost
        } else {
            TxStatus::Failed(s)
        })
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Message>, AdapterError> {
        let mut msg = zeroed_msg();
        // 0 would mean "wait forever" ([K-RX]); at least 1 ms.
        msg.timeout = timeout.as_millis().clamp(1, u128::from(u32::MAX)) as u32;
        // SAFETY: CEC_RECEIVE reads and writes exactly one `struct cec_msg`
        // through the pointer, the valid, exclusively borrowed `msg`.
        let r = unsafe { ioctl(self.fd(), CEC_RECEIVE, &mut msg as *mut CecMsg) };
        if r < 0 {
            let e = io::Error::last_os_error();
            return match e.kind() {
                io::ErrorKind::TimedOut
                | io::ErrorKind::WouldBlock
                | io::ErrorKind::Interrupted => Ok(None),
                _ => Err(io_error("CEC_RECEIVE")),
            };
        }
        let len = (msg.len as usize).min(MAX_MESSAGE_LEN);
        Message::decode(&msg.msg[..len])
            .map(Some)
            .map_err(|e| AdapterError::Io(format!("received: {}", e)))
    }

    fn event(&mut self) -> Result<Option<Event>, AdapterError> {
        let mut p = PollFd {
            fd: self.fd(),
            events: POLLPRI,
            revents: 0,
        };
        // SAFETY: one valid `pollfd` for the call; a zero timeout returns at
        // once.
        let r = unsafe { poll(&mut p as *mut PollFd, 1, 0) };
        if r <= 0 || p.revents & POLLPRI == 0 {
            return Ok(None);
        }
        let mut ev = CecEvent {
            ts: 0,
            event: 0,
            flags: 0,
            raw: [0; 16],
        };
        // SAFETY: CEC_DQEVENT writes exactly one `struct cec_event` (80
        // bytes, asserted above) into the valid, exclusively borrowed `ev`;
        // poll said one is pending, so it does not block.
        let r = unsafe { ioctl(self.fd(), CEC_DQEVENT, &mut ev as *mut CecEvent) };
        if r < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(io_error("CEC_DQEVENT"));
        }
        let first = ev.raw[0].to_ne_bytes();
        Ok(match ev.event {
            EVENT_STATE_CHANGE => Some(Event::StateChange {
                physical_address: PhysicalAddress(u16::from_ne_bytes([first[0], first[1]])),
                log_addr_mask: u16::from_ne_bytes([first[2], first[3]]),
            }),
            EVENT_LOST_MSGS => Some(Event::LostMessages(ev.raw[0])),
            // Pin events: chorus does not ask for them (`cec.md` 1.7).
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_device_is_refused_by_name() {
        let e = KernelAdapter::open("/nonexistent/cec9").unwrap_err();
        assert!(e.to_string().contains("/nonexistent/cec9"), "{e}");
    }

    #[test]
    fn a_file_that_is_not_a_cec_adapter_is_refused() {
        // /dev/null takes the ioctl and fails it (ENOTTY).
        let e = KernelAdapter::open("/dev/null").unwrap_err();
        assert!(e.to_string().contains("not a CEC adapter"), "{e}");
    }

    #[test]
    fn the_request_numbers_are_the_documented_ones() {
        // cec.md section 1.8's table, recomputed independently.
        assert_eq!(CEC_ADAP_G_CAPS, 0xC04C6100);
        assert_eq!(CEC_TRANSMIT, 0xC0386105);
        assert_eq!(CEC_RECEIVE, 0xC0386106);
        assert_eq!(CEC_DQEVENT, 0xC0506107);
        assert_eq!(CEC_ADAP_S_LOG_ADDRS, 0xC05C6104);
    }
}
