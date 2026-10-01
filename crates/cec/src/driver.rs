//! The role on an adapter: receive, handle, transmit, tick.
//!
//! [`Driver::step`] is one turn of the loop a CEC thread runs: wait for a
//! message at most a little while, hand it to the role, let time pass,
//! transmit what the role says to, record the TV's power in the shared
//! [`TvPower`], and hand back what is the caller's business (volume, mute,
//! System Audio Mode, ARC). The clock is the caller's: a monotonic
//! millisecond count (BRIEF.md guardrail 4), never a wall clock.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::adapter::{Adapter, AdapterError, Claim, Event, TxStatus};
use crate::codec::{opcode, Message, PhysicalAddress};
use crate::power::TvPower;
use crate::role::{AudioSystem, Config, Effect};

/// What the driver has done, for status lines and tests.
#[derive(Debug, Default)]
pub struct DriverCounters {
    /// Messages received.
    pub received: AtomicU64,
    /// Messages transmitted and acknowledged.
    pub sent: AtomicU64,
    /// Transmits not acknowledged or failed.
    pub unacknowledged: AtomicU64,
    /// Feature Aborts the hub sent.
    pub aborts_sent: AtomicU64,
    /// Messages the adapter reported lost.
    pub lost: AtomicU64,
}

/// The Audio System role running on an adapter.
pub struct Driver<A: Adapter> {
    adapter: A,
    role: AudioSystem,
    power: Arc<TvPower>,
    now_ms: Box<dyn Fn() -> u64 + Send>,
    log: Box<dyn FnMut(&str) + Send>,
    counters: Arc<DriverCounters>,
}

impl<A: Adapter> std::fmt::Debug for Driver<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Driver").finish_non_exhaustive()
    }
}

impl<A: Adapter> Driver<A> {
    /// Claim logical address 5 on `adapter` and start the role. Refused by
    /// name when the configuration is not carriable or address 5 is not
    /// claimed (another Audio System on the bus, or no physical address).
    pub fn start(
        mut adapter: A,
        config: Config,
        power: Arc<TvPower>,
        now_ms: Box<dyn Fn() -> u64 + Send>,
        log: Box<dyn FnMut(&str) + Send>,
    ) -> Result<Driver<A>, AdapterError> {
        config.check().map_err(AdapterError::Unusable)?;
        let claimed = adapter.claim(&Claim {
            osd_name: config.osd_name.clone(),
            vendor_id: config.vendor_id,
            cec_version: config.cec_version,
        })?;
        if claimed.logical_address.is_none() {
            return Err(AdapterError::Unusable(format!(
                "logical address 5 (Audio System) was not claimed at physical address {}: \
                 another Audio System holds it, or the adapter has no physical address yet \
                 (is the HDMI cable in and the TV's CEC on?)",
                claimed.physical_address
            )));
        }
        let mut driver = Driver {
            adapter,
            role: AudioSystem::new(config, claimed.physical_address),
            power,
            now_ms,
            log,
            counters: Arc::new(DriverCounters::default()),
        };
        (driver.log)(&format!(
            "cec claimed logical_address=5 physical_address={}",
            claimed.physical_address
        ));
        let now = (driver.now_ms)();
        let effects = driver.role.start(now);
        driver.apply(effects)?;
        Ok(driver)
    }

    /// The role, for reading its state.
    pub fn role(&self) -> &AudioSystem {
        &self.role
    }

    /// The counters.
    pub fn counters(&self) -> &Arc<DriverCounters> {
        &self.counters
    }

    /// The physical address in use.
    pub fn physical_address(&self) -> PhysicalAddress {
        self.role.physical_address()
    }

    /// The room's volume and mute from the server.
    pub fn room_state(&mut self, volume: u8, muted: bool) -> Result<Vec<Effect>, AdapterError> {
        let now = (self.now_ms)();
        let effects = self.role.room_state(volume, muted, now);
        self.apply(effects)
    }

    /// One turn: at most one message (waiting up to `wait`), the adapter's
    /// events, time passing. Returns the effects that are the caller's.
    pub fn step(&mut self, wait: Duration) -> Result<Vec<Effect>, AdapterError> {
        let mut effects = Vec::new();
        while let Some(event) = self.adapter.event()? {
            match event {
                Event::StateChange {
                    physical_address, ..
                } => {
                    (self.log)(&format!(
                        "cec state-change physical_address={}",
                        physical_address
                    ));
                    effects.extend(self.role.set_physical_address(physical_address));
                }
                Event::LostMessages(n) => {
                    self.counters
                        .lost
                        .fetch_add(u64::from(n), Ordering::Relaxed);
                    (self.log)(&format!("cec lost-messages count={}", n));
                }
            }
        }
        if let Some(m) = self.adapter.receive(wait)? {
            self.counters.received.fetch_add(1, Ordering::Relaxed);
            let now = (self.now_ms)();
            effects.extend(self.role.handle(&m, now));
        }
        let now = (self.now_ms)();
        effects.extend(self.role.tick(now));
        self.apply(effects)
    }

    fn transmit(&mut self, m: &Message) -> Result<(), AdapterError> {
        let status = match self.adapter.transmit(m) {
            Ok(s) => s,
            Err(AdapterError::Closed) => return Err(AdapterError::Closed),
            // One failed transmit (no physical address yet, a full queue) is
            // logged and the role carries on; the bus is best effort.
            Err(e) => {
                self.counters.unacknowledged.fetch_add(1, Ordering::Relaxed);
                (self.log)(&format!(
                    "cec transmit-failed message=\"{}\" detail=\"{}\"",
                    m, e
                ));
                return Ok(());
            }
        };
        if m.opcode == Some(opcode::FEATURE_ABORT) {
            self.counters.aborts_sent.fetch_add(1, Ordering::Relaxed);
        }
        if status == TxStatus::Ok {
            self.counters.sent.fetch_add(1, Ordering::Relaxed);
        } else {
            self.counters.unacknowledged.fetch_add(1, Ordering::Relaxed);
            (self.log)(&format!(
                "cec transmit status={} message=\"{}\"",
                status.name(),
                m
            ));
        }
        Ok(())
    }

    fn apply(&mut self, effects: Vec<Effect>) -> Result<Vec<Effect>, AdapterError> {
        let mut theirs = Vec::new();
        for e in effects {
            match e {
                Effect::Send(m) => self.transmit(&m)?,
                Effect::TvPower(state) => {
                    self.power.set(state);
                    (self.log)(&format!("cec tv-power state={}", state.name()));
                    theirs.push(Effect::TvPower(state));
                }
                other => theirs.push(other),
            }
        }
        Ok(theirs)
    }
}
