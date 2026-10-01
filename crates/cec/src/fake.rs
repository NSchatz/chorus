//! A CEC bus in memory, an adapter on it, and a scripted TV, for tests on
//! a machine with no CEC hardware (`cec.md` section 4: the container has no
//! `/dev/cecN` and cannot load `vivid`).
//!
//! # What the bus models
//!
//! - Logical-address claims: an address another node holds is refused, as
//!   the kernel's polling claim finds it acknowledged ([K-LOG]).
//! - Directed messages: delivered to the node holding the destination and
//!   acknowledged, or NACKed when nobody holds it.
//! - Broadcasts: delivered to every other node, acknowledged.
//! - A bounded receive queue per node: when full the oldest message is
//!   dropped and a lost-messages event is queued, as the kernel reports an
//!   overflow ([K-EV], `CEC_EVENT_LOST_MSGS`). The depth is ASSUMED (the
//!   kernel promises "all messages received in the last two seconds", not a
//!   count).
//! - Every transmit, in order, with its status, for tests to read back.
//!
//! What it does not model: bit timing, arbitration, retries. They are the
//! kernel's and the bus's, and a bench session (`docs/cec.md`) checks them
//! on real hardware.
//!
//! # The scripted TV
//!
//! [`FakeTv`] holds logical address 0 at physical address 0.0.0.0 and
//! answers what a TV answers, in one of four characters ([`TvKind`]):
//! Roku-like (accepts System Audio Mode and ARC, broadcasts Active Source
//! when it turns on: LEAD, `cec.md` section 3 says Roku drives volume and
//! mute of a home theatre over CEC), a TV that Feature-Aborts everything
//! directed to it, a TV without ARC, and a slow TV that answers late.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::adapter::{Adapter, AdapterError, Claim, Claimed, Event, TxStatus};
use crate::codec::{
    build, opcode, AbortReason, Message, PhysicalAddress, PowerStatus, AUDIO_SYSTEM, BROADCAST, TV,
};

/// Messages a node's receive queue holds before the oldest is dropped.
/// ASSUMED (see the module documentation).
pub const QUEUE_DEPTH: usize = 64;

#[derive(Debug)]
struct Node {
    logical_address: Option<u8>,
    inbox: VecDeque<Message>,
    events: VecDeque<Event>,
}

#[derive(Debug, Default)]
struct Bus {
    nodes: Vec<Node>,
    sent: Vec<(Message, TxStatus)>,
    closed: bool,
}

/// The bus. Clones share it.
#[derive(Debug, Clone, Default)]
pub struct FakeBus {
    inner: Arc<(Mutex<Bus>, Condvar)>,
}

impl FakeBus {
    /// An empty bus.
    pub fn new() -> FakeBus {
        FakeBus::default()
    }

    fn lock(&self) -> MutexGuard<'_, Bus> {
        match self.inner.0.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn join(&self) -> usize {
        let mut bus = self.lock();
        bus.nodes.push(Node {
            logical_address: None,
            inbox: VecDeque::new(),
            events: VecDeque::new(),
        });
        bus.nodes.len() - 1
    }

    /// An adapter on this bus at physical address `pa`.
    pub fn adapter(&self, pa: PhysicalAddress) -> FakeAdapter {
        FakeAdapter {
            bus: self.clone(),
            node: self.join(),
            pa,
        }
    }

    /// Every message transmitted so far, in order, with its status.
    pub fn sent(&self) -> Vec<(Message, TxStatus)> {
        self.lock().sent.clone()
    }

    /// Close the bus: every adapter's next call fails with
    /// [`AdapterError::Closed`].
    pub fn close(&self) {
        self.lock().closed = true;
        self.inner.1.notify_all();
    }

    fn transmit(&self, from: usize, m: &Message) -> Result<TxStatus, AdapterError> {
        let mut bus = self.lock();
        if bus.closed {
            return Err(AdapterError::Closed);
        }
        let mut delivered = false;
        for (i, node) in bus.nodes.iter_mut().enumerate() {
            if i == from {
                continue;
            }
            let Some(la) = node.logical_address else {
                continue;
            };
            if m.destination == BROADCAST || la == m.destination {
                if node.inbox.len() >= QUEUE_DEPTH {
                    node.inbox.pop_front();
                    node.events.push_back(Event::LostMessages(1));
                }
                node.inbox.push_back(m.clone());
                delivered = true;
            }
        }
        let status = if m.destination == BROADCAST || delivered {
            TxStatus::Ok
        } else {
            TxStatus::Nack
        };
        bus.sent.push((m.clone(), status));
        drop(bus);
        self.inner.1.notify_all();
        Ok(status)
    }

    fn receive(&self, node: usize, timeout: Duration) -> Result<Option<Message>, AdapterError> {
        let deadline = Instant::now() + timeout;
        let mut bus = self.lock();
        loop {
            if bus.closed {
                return Err(AdapterError::Closed);
            }
            if let Some(m) = bus.nodes[node].inbox.pop_front() {
                return Ok(Some(m));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            bus = match self.inner.1.wait_timeout(bus, deadline - now) {
                Ok((g, _)) => g,
                Err(p) => p.into_inner().0,
            };
        }
    }

    fn claim(&self, node: usize, la: u8) -> Option<u8> {
        let mut bus = self.lock();
        if bus
            .nodes
            .iter()
            .enumerate()
            .any(|(i, n)| i != node && n.logical_address == Some(la))
        {
            return None;
        }
        bus.nodes[node].logical_address = Some(la);
        Some(la)
    }
}

/// An adapter on a [`FakeBus`].
#[derive(Debug)]
pub struct FakeAdapter {
    bus: FakeBus,
    node: usize,
    pa: PhysicalAddress,
}

impl FakeAdapter {
    /// Hot plug: the physical address changes and a state-change event is
    /// queued, as the kernel reports one.
    pub fn replug(&mut self, pa: PhysicalAddress) {
        self.pa = pa;
        let mut bus = self.bus.lock();
        let mask = bus.nodes[self.node]
            .logical_address
            .map(|la| 1u16 << la)
            .unwrap_or(0);
        bus.nodes[self.node].events.push_back(Event::StateChange {
            physical_address: pa,
            log_addr_mask: mask,
        });
    }
}

impl Adapter for FakeAdapter {
    fn claim(&mut self, _claim: &Claim) -> Result<Claimed, AdapterError> {
        Ok(Claimed {
            logical_address: self.bus.claim(self.node, AUDIO_SYSTEM),
            physical_address: self.pa,
        })
    }

    fn transmit(&mut self, m: &Message) -> Result<TxStatus, AdapterError> {
        m.encode()
            .map_err(|e| AdapterError::Io(format!("not sent: {}", e)))?;
        self.bus.transmit(self.node, m)
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Message>, AdapterError> {
        self.bus.receive(self.node, timeout)
    }

    fn event(&mut self) -> Result<Option<Event>, AdapterError> {
        let mut bus = self.bus.lock();
        if bus.closed {
            return Err(AdapterError::Closed);
        }
        Ok(bus.nodes[self.node].events.pop_front())
    }
}

/// The scripted TV's character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvKind {
    /// Accepts System Audio Mode and ARC, answers power polls, broadcasts
    /// Active Source when it turns on.
    RokuLike,
    /// Feature-Aborts [Refused] everything directed to it (power polls and
    /// System Audio Mode included) and announces nothing.
    FeatureAbortEverything,
    /// Like Roku but Feature-Aborts Initiate ARC [Unrecognized opcode]: its
    /// HDMI input has no ARC.
    NoArc,
    /// Like Roku, but every answer goes out this long after the request
    /// (past the 1 s response time) and it announces nothing on power on.
    Slow(Duration),
}

#[derive(Debug, Default)]
struct Heard {
    messages: Vec<Message>,
}

/// A TV on a [`FakeBus`]: logical address 0, physical address 0.0.0.0.
pub struct FakeTv {
    bus: FakeBus,
    node: usize,
    heard: Arc<(Mutex<Heard>, Condvar)>,
    on: Arc<AtomicBool>,
    keep: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for FakeTv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeTv").finish_non_exhaustive()
    }
}

impl FakeTv {
    /// Put a TV of `kind` on `bus`, in standby, answering from its own
    /// thread.
    pub fn start(bus: &FakeBus, kind: TvKind) -> FakeTv {
        let node = bus.join();
        bus.claim(node, TV);
        let heard: Arc<(Mutex<Heard>, Condvar)> = Arc::default();
        let on = Arc::new(AtomicBool::new(false));
        let keep = Arc::new(AtomicBool::new(true));
        let thread = {
            let bus = bus.clone();
            let heard = Arc::clone(&heard);
            let on = Arc::clone(&on);
            let keep = Arc::clone(&keep);
            thread::spawn(move || {
                while keep.load(Ordering::SeqCst) {
                    let m = match bus.receive(node, Duration::from_millis(20)) {
                        Ok(Some(m)) => m,
                        Ok(None) => continue,
                        Err(_) => return,
                    };
                    {
                        let mut h = lock(&heard.0);
                        h.messages.push(m.clone());
                    }
                    heard.1.notify_all();
                    if m.destination != TV {
                        continue;
                    }
                    if let Some(answer) = answer(kind, &m, on.load(Ordering::SeqCst)) {
                        if let TvKind::Slow(d) = kind {
                            thread::sleep(d);
                        }
                        let _ = bus.transmit(node, &answer);
                    }
                }
            })
        };
        FakeTv {
            bus: bus.clone(),
            node,
            heard,
            on,
            keep,
            thread: Some(thread),
        }
    }

    /// Send `m` from the TV (its initiator is set to 0).
    pub fn send(&self, mut m: Message) -> TxStatus {
        m.initiator = TV;
        self.bus
            .transmit(self.node, &m)
            .unwrap_or(TxStatus::Failed(0))
    }

    /// A remote key forwarded to the Audio System: pressed, then released.
    pub fn press(&self, key: u8) {
        self.send(build::user_control_pressed(TV, AUDIO_SYSTEM, key));
        self.send(build::user_control_released(TV, AUDIO_SYSTEM));
    }

    /// Turn on: power polls answer "on", and a Roku-like TV (or one without
    /// ARC) broadcasts Active Source 0.0.0.0, its own home screen.
    pub fn power_on(&self, kind: TvKind) {
        self.on.store(true, Ordering::SeqCst);
        if matches!(kind, TvKind::RokuLike | TvKind::NoArc) {
            self.send(Message::new(
                TV,
                BROADCAST,
                opcode::ACTIVE_SOURCE,
                &PhysicalAddress::TV.bytes(),
            ));
        }
    }

    /// Go to standby: power polls answer "standby" and Standby is broadcast.
    pub fn standby(&self) {
        self.on.store(false, Ordering::SeqCst);
        self.send(Message::new(TV, BROADCAST, opcode::STANDBY, &[]));
    }

    /// Everything the TV has received so far.
    pub fn heard(&self) -> Vec<Message> {
        lock(&self.heard.0).messages.clone()
    }

    /// Wait up to `timeout` for a message, received after the first `skip`
    /// the TV heard, that `matches`.
    pub fn expect(
        &self,
        skip: usize,
        timeout: Duration,
        matches: impl Fn(&Message) -> bool,
    ) -> Option<Message> {
        let deadline = Instant::now() + timeout;
        let mut h = lock(&self.heard.0);
        loop {
            if let Some(m) = h.messages.iter().skip(skip).find(|m| matches(m)) {
                return Some(m.clone());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            h = match self.heard.1.wait_timeout(h, deadline - now) {
                Ok((g, _)) => g,
                Err(p) => p.into_inner().0,
            };
        }
    }
}

impl Drop for FakeTv {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

/// What a TV of `kind` answers to `m`, directed to it.
fn answer(kind: TvKind, m: &Message, on: bool) -> Option<Message> {
    let op = m.opcode?;
    let to = m.initiator;
    if kind == TvKind::FeatureAbortEverything {
        return (op != opcode::FEATURE_ABORT)
            .then(|| build::feature_abort(TV, to, op, AbortReason::Refused));
    }
    match op {
        opcode::GIVE_DEVICE_POWER_STATUS => Some(build::report_power_status(
            TV,
            to,
            if on {
                PowerStatus::On
            } else {
                PowerStatus::Standby
            },
        )),
        opcode::INITIATE_ARC if kind == TvKind::NoArc => Some(build::feature_abort(
            TV,
            to,
            op,
            AbortReason::UnrecognizedOpcode,
        )),
        opcode::INITIATE_ARC => Some(Message::new(TV, to, opcode::REPORT_ARC_INITIATED, &[])),
        opcode::TERMINATE_ARC => Some(Message::new(TV, to, opcode::REPORT_ARC_TERMINATED, &[])),
        // Accepted silently: no Feature Abort is the acceptance (CTS 11.2.15-2).
        opcode::SET_SYSTEM_AUDIO_MODE => None,
        _ => None,
    }
}
