//! What the role needs of a CEC adapter, whichever it is: the kernel's
//! `/dev/cecN` ([`crate::kernel::KernelAdapter`]) or the fake bus
//! ([`crate::fake::FakeAdapter`]).

use std::fmt;
use std::time::Duration;

use crate::codec::{Message, PhysicalAddress};

/// What a claim asks for: one Audio System logical address, and the
/// answers the kernel's core would give in a mode where it answers for us
/// (chorus runs in passthrough, where the role answers them itself; the
/// values are handed over anyway so a `cec-ctl` reading the adapter sees
/// the same ones).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// 1 to 14 printable ASCII bytes.
    pub osd_name: String,
    /// A 24-bit OUI, or `None`.
    pub vendor_id: Option<u32>,
    /// The `<CEC Version>` operand.
    pub cec_version: u8,
}

/// What was claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claimed {
    /// The logical address, `None` when none could be claimed (another Audio
    /// System holds 5, or the physical address is not known yet and the
    /// kernel claims later: a [`Event::StateChange`] says when).
    pub logical_address: Option<u8>,
    /// The physical address in use.
    pub physical_address: PhysicalAddress,
}

/// How a transmit ended, from the adapter's own report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxStatus {
    /// Acknowledged (a broadcast is acknowledged unless someone NACKs it).
    Ok,
    /// Nobody acknowledged a directed message: no device there.
    Nack,
    /// Lost arbitration on every retry.
    ArbitrationLost,
    /// Any other failure the adapter reports (low drive, error, timeout,
    /// aborted, retries exhausted), with its status bits.
    Failed(u8),
}

impl TxStatus {
    /// The status-line word.
    pub fn name(self) -> &'static str {
        match self {
            TxStatus::Ok => "ok",
            TxStatus::Nack => "nack",
            TxStatus::ArbitrationLost => "arbitration-lost",
            TxStatus::Failed(_) => "failed",
        }
    }
}

/// An adapter event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The physical or logical addresses changed (a hot plug, a claim).
    StateChange {
        /// The physical address now.
        physical_address: PhysicalAddress,
        /// One bit per claimed logical address.
        log_addr_mask: u16,
    },
    /// Received messages were lost because the queue overflowed.
    LostMessages(u32),
}

/// Why an adapter call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    /// The device could not be opened or does not do what chorus needs.
    Unusable(String),
    /// A call failed.
    Io(String),
    /// The adapter is gone (the fake bus closed, the device unplugged).
    Closed,
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Unusable(s) => write!(f, "the CEC adapter is unusable: {}", s),
            AdapterError::Io(s) => write!(f, "a CEC adapter call failed: {}", s),
            AdapterError::Closed => write!(f, "the CEC adapter is gone"),
        }
    }
}

impl std::error::Error for AdapterError {}

/// A CEC adapter as the role uses it.
pub trait Adapter: Send {
    /// Claim the Audio System's logical address.
    fn claim(&mut self, claim: &Claim) -> Result<Claimed, AdapterError>;
    /// Send one message and wait for its transmit status.
    fn transmit(&mut self, m: &Message) -> Result<TxStatus, AdapterError>;
    /// The next received message, waiting at most `timeout`.
    fn receive(&mut self, timeout: Duration) -> Result<Option<Message>, AdapterError>;
    /// The next pending event, without waiting.
    fn event(&mut self) -> Result<Option<Event>, AdapterError>;
}
