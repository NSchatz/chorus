//! The receiver pool: which receiver serves which target.
//!
//! A fixed number of receivers (one container each) serve the targets of
//! the moment: every room, every saved group, every live group. The pool is
//! pure bookkeeping: the caller hands it the target list whenever the state
//! changes (and when a deadline it returned has passed), and gets back the
//! `assign` and `release` messages to send.
//!
//! The rules (the design of goal 17, section 2.5; proposal P7):
//!
//! - **Priority.** Rooms first, then saved groups, then live groups. Rooms
//!   and saved groups rank in the order the caller lists them; live groups
//!   by age, oldest first, where age is the order the pool first saw them.
//!   When receivers run out, the lowest-ranked targets have none: the
//!   newest live groups first. They are reported in [`Update::exhausted`].
//! - **Stable.** A target that has a receiver keeps that receiver for as
//!   long as it holds one: the pool never moves a target between receivers.
//!   A new target takes the lowest free index.
//! - **Grace.** A live group that dissolves keeps its receiver while the
//!   receiver is busy (still playing), and for the grace period after it is
//!   both dissolved and idle; then the receiver is released. If the group
//!   forms again in that time it simply carries on. A lingering group ranks
//!   below every present target: a new target that needs a receiver takes a
//!   lingering group's at once. (A group that forms again after its
//!   receiver went gets whichever receiver is free; its data directory is
//!   chosen by key, [`crate::keydir`], so its Spotify Connect identity
//!   survives either way.)
//! - **Removed.** A room or saved group that is no longer listed loses its
//!   receiver at once.
//! - **Renamed.** A target whose name changed is assigned again, on the same
//!   receiver, with the new name (the supervisor restarts Soloist under it).
//!
//! Time is whatever the caller's monotonic clock says, as a [`Duration`]
//! since an origin of the caller's choosing (`Instant::duration_since`); it
//! must never go backwards. The pool reads no clock.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::keydir::{kind_of, Kind};

/// A room, saved group or live group that wants a receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The target's key: `room:<id>`, `group:<id>`, `live:<a>+<b>`.
    pub key: String,
    /// The Spotify Connect device name: the target's display name.
    pub name: String,
}

impl Target {
    /// A target from its key and name.
    pub fn new(key: &str, name: &str) -> Target {
        Target {
            key: key.to_string(),
            name: name.to_string(),
        }
    }
}

/// What a receiver is assigned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    /// The target's key.
    pub key: String,
    /// The device name it was assigned with.
    pub name: String,
    /// Whether the target is a dissolved live group kept through its grace
    /// period.
    pub lingering: bool,
}

/// A message to send to a receiver's supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Send `assign` (a new target, or the same target under a new name).
    Assign {
        /// The receiver's index.
        receiver: usize,
        /// The target's key.
        key: String,
        /// The device name.
        name: String,
    },
    /// Send `release`.
    Release {
        /// The receiver's index.
        receiver: usize,
    },
}

/// What one [`Pool::update`] decided.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Update {
    /// The messages to send, by receiver index, at most one per receiver.
    pub changes: Vec<Change>,
    /// The listed targets that have no receiver, highest rank first.
    pub exhausted: Vec<String>,
    /// When to call [`Pool::update`] again even if nothing changes: the
    /// earliest end of a grace period.
    pub next_deadline: Option<Duration>,
}

#[derive(Debug, Clone)]
struct Held {
    key: String,
    name: String,
    /// Since when the target is both unlisted and idle; `None` while it is
    /// listed or busy.
    idle_since: Option<Duration>,
    lingering: bool,
}

/// The pool.
#[derive(Debug, Clone)]
pub struct Pool {
    slots: Vec<Option<Held>>,
    grace: Duration,
    /// The age of every live group that is listed or lingering: lower is
    /// older.
    ages: BTreeMap<String, u64>,
    next_age: u64,
}

impl Pool {
    /// A pool of `receivers` receivers, all free, with the grace period a
    /// dissolved, idle live group keeps its receiver for.
    pub fn new(receivers: usize, grace: Duration) -> Pool {
        Pool {
            slots: vec![None; receivers],
            grace,
            ages: BTreeMap::new(),
            next_age: 0,
        }
    }

    /// How many receivers the pool has.
    pub fn receivers(&self) -> usize {
        self.slots.len()
    }

    /// What each receiver is assigned to, by index.
    pub fn assignments(&self) -> Vec<Option<Assignment>> {
        self.slots
            .iter()
            .map(|slot| {
                slot.as_ref().map(|held| Assignment {
                    key: held.key.clone(),
                    name: held.name.clone(),
                    lingering: held.lingering,
                })
            })
            .collect()
    }

    /// The receiver a target holds, if it holds one.
    pub fn receiver_of(&self, key: &str) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|held| held.key == key))
    }

    /// The target a receiver is assigned to, if any.
    pub fn target_of(&self, receiver: usize) -> Option<&str> {
        self.slots
            .get(receiver)?
            .as_ref()
            .map(|held| held.key.as_str())
    }

    /// Bring the pool up to date with the targets that exist now.
    ///
    /// `targets` is every room, saved group and live group, in the state's
    /// order; a key that is none of the three kinds is ignored, and of two
    /// entries with one key the first counts. `busy[i]` says whether
    /// receiver `i` is playing (a missing entry is "idle"). `now` is the
    /// caller's monotonic time.
    pub fn update(&mut self, targets: &[Target], busy: &[bool], now: Duration) -> Update {
        let before: Vec<Option<(String, String)>> = self
            .slots
            .iter()
            .map(|s| s.as_ref().map(|h| (h.key.clone(), h.name.clone())))
            .collect();

        // The listed targets by rank: rooms, saved groups, then live groups
        // by age.
        let mut listed: BTreeMap<&str, &Target> = BTreeMap::new();
        let mut ranked: Vec<(Kind, &Target)> = Vec::new();
        for target in targets {
            let Some(kind) = kind_of(&target.key) else {
                continue;
            };
            if listed.contains_key(target.key.as_str()) {
                continue;
            }
            listed.insert(&target.key, target);
            if kind == Kind::Live && !self.ages.contains_key(&target.key) {
                self.ages.insert(target.key.clone(), self.next_age);
                self.next_age += 1;
            }
            ranked.push((kind, target));
        }
        // A stable sort: rooms and saved groups keep the caller's order.
        ranked.sort_by_key(|(kind, target)| {
            let age = match kind {
                Kind::Live => self.ages.get(&target.key).copied().unwrap_or(u64::MAX),
                _ => 0,
            };
            (*kind, age)
        });

        // What is held by a target that is no longer listed.
        for (receiver, slot) in self.slots.iter_mut().enumerate() {
            let Some(held) = slot else { continue };
            if listed.contains_key(held.key.as_str()) {
                held.lingering = false;
                held.idle_since = None;
                continue;
            }
            if kind_of(&held.key) != Some(Kind::Live) {
                *slot = None;
                continue;
            }
            held.lingering = true;
            if busy.get(receiver).copied().unwrap_or(false) {
                held.idle_since = None;
                continue;
            }
            let since = *held.idle_since.get_or_insert(now);
            if now.saturating_sub(since) >= self.grace {
                *slot = None;
            }
        }

        // The listed targets that get a receiver, and how many lingering
        // groups may stay beside them.
        let capacity = self.slots.len();
        let winners = &ranked[..ranked.len().min(capacity)];
        let exhausted: Vec<String> = ranked[winners.len()..]
            .iter()
            .map(|(_, t)| t.key.clone())
            .collect();
        let is_winner = |key: &str| winners.iter().any(|(_, t)| t.key == key);

        // A listed target outside the winners gives its receiver up.
        for slot in &mut self.slots {
            if slot
                .as_ref()
                .is_some_and(|h| !h.lingering && !is_winner(&h.key))
            {
                *slot = None;
            }
        }
        // Lingering groups stay only in what the winners leave over: the
        // busy ones first, then the most recently dissolved.
        let room_for_lingering = capacity - winners.len();
        let mut lingering: Vec<(usize, bool, Duration)> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let held = s.as_ref().filter(|h| h.lingering)?;
                Some((i, held.idle_since.is_none(), held.idle_since.unwrap_or(now)))
            })
            .collect();
        lingering.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
        for (receiver, _, _) in lingering.into_iter().skip(room_for_lingering) {
            self.slots[receiver] = None;
        }

        // Winners keep what they hold (a new name is taken), and the rest
        // take the lowest free index, in rank order.
        for (_, target) in winners {
            match self.receiver_of(&target.key) {
                Some(receiver) => {
                    if let Some(held) = &mut self.slots[receiver] {
                        held.name.clone_from(&target.name);
                    }
                }
                None => {
                    // There is a free slot: winners are at most `capacity`,
                    // and everything else was just evicted to make room.
                    if let Some(free) = self.slots.iter().position(Option::is_none) {
                        self.slots[free] = Some(Held {
                            key: target.key.clone(),
                            name: target.name.clone(),
                            idle_since: None,
                            lingering: false,
                        });
                    }
                }
            }
        }

        // Ages are kept only for live groups that are listed or lingering.
        let slots = &self.slots;
        self.ages.retain(|key, _| {
            listed.contains_key(key.as_str()) || slots.iter().flatten().any(|held| &held.key == key)
        });

        let mut changes = Vec::new();
        for (receiver, (was, slot)) in before.iter().zip(&self.slots).enumerate() {
            let is = slot.as_ref().map(|h| (h.key.clone(), h.name.clone()));
            match (was, is) {
                (was, Some((key, name))) if was.as_ref() != Some(&(key.clone(), name.clone())) => {
                    changes.push(Change::Assign {
                        receiver,
                        key,
                        name,
                    });
                }
                (Some(_), None) => changes.push(Change::Release { receiver }),
                _ => {}
            }
        }
        let next_deadline = self
            .slots
            .iter()
            .flatten()
            .filter_map(|held| held.idle_since)
            .map(|since| since + self.grace)
            .min();
        Update {
            changes,
            exhausted,
            next_deadline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = Duration::from_secs(60);

    fn t(key: &str) -> Target {
        Target::new(key, &key.to_uppercase())
    }

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn keys(pool: &Pool) -> Vec<Option<String>> {
        pool.assignments()
            .into_iter()
            .map(|a| a.map(|a| a.key))
            .collect()
    }

    fn some(keys: &[&str]) -> Vec<Option<String>> {
        keys.iter()
            .map(|k| (!k.is_empty()).then(|| k.to_string()))
            .collect()
    }

    #[test]
    fn rooms_then_saved_groups_then_live_groups() {
        let mut pool = Pool::new(4, GRACE);
        let update = pool.update(
            &[t("live:a+b"), t("group:down"), t("room:b"), t("room:a")],
            &[],
            secs(0),
        );
        assert_eq!(
            keys(&pool),
            some(&["room:b", "room:a", "group:down", "live:a+b"])
        );
        assert_eq!(update.exhausted, Vec::<String>::new());
        assert_eq!(update.next_deadline, None);
        assert_eq!(
            update.changes[0],
            Change::Assign {
                receiver: 0,
                key: "room:b".into(),
                name: "ROOM:B".into()
            }
        );
        assert_eq!(update.changes.len(), 4);
        assert_eq!(pool.receiver_of("group:down"), Some(2));
        assert_eq!(pool.target_of(3), Some("live:a+b"));
        assert_eq!(pool.target_of(9), None);
        assert_eq!(pool.receivers(), 4);
    }

    #[test]
    fn nothing_changes_when_nothing_changed() {
        let mut pool = Pool::new(3, GRACE);
        let targets = [t("room:a"), t("room:b"), t("live:a+b")];
        pool.update(&targets, &[], secs(0));
        let again = pool.update(&targets, &[true, false, true], secs(1000));
        assert_eq!(again, Update::default());
        // The caller's order of rooms may change without moving anything.
        let reordered = [t("live:a+b"), t("room:b"), t("room:a")];
        assert_eq!(pool.update(&reordered, &[], secs(2000)), Update::default());
        assert_eq!(keys(&pool), some(&["room:a", "room:b", "live:a+b"]));
    }

    #[test]
    fn a_target_keeps_its_receiver_and_a_newcomer_takes_the_lowest_free() {
        let mut pool = Pool::new(4, GRACE);
        pool.update(&[t("room:a"), t("room:b"), t("room:c")], &[], secs(0));
        // room:a goes; b and c stay where they are.
        let update = pool.update(&[t("room:b"), t("room:c")], &[], secs(1));
        assert_eq!(update.changes, vec![Change::Release { receiver: 0 }]);
        assert_eq!(keys(&pool), some(&["", "room:b", "room:c", ""]));
        // A saved group arrives: index 0, the lowest free.
        pool.update(&[t("room:b"), t("room:c"), t("group:g")], &[], secs(2));
        assert_eq!(keys(&pool), some(&["group:g", "room:b", "room:c", ""]));
    }

    #[test]
    fn a_renamed_target_is_assigned_again_on_its_receiver() {
        let mut pool = Pool::new(2, GRACE);
        pool.update(&[t("room:a"), t("room:b")], &[], secs(0));
        let update = pool.update(
            &[t("room:a"), Target::new("room:b", "Bedroom")],
            &[],
            secs(1),
        );
        assert_eq!(
            update.changes,
            vec![Change::Assign {
                receiver: 1,
                key: "room:b".into(),
                name: "Bedroom".into()
            }]
        );
        assert_eq!(pool.assignments()[1].as_ref().unwrap().name, "Bedroom");
    }

    #[test]
    fn a_dissolved_idle_live_group_keeps_its_receiver_for_the_grace_period() {
        let mut pool = Pool::new(2, GRACE);
        let rooms = [t("room:a")];
        pool.update(&[t("room:a"), t("live:a+b")], &[], secs(0));
        // Dissolved at 10 s, idle: lingering until 70 s.
        let update = pool.update(&rooms, &[], secs(10));
        assert_eq!(update.changes, vec![]);
        assert_eq!(update.next_deadline, Some(secs(70)));
        assert!(pool.assignments()[1].as_ref().unwrap().lingering);
        let update = pool.update(&rooms, &[], secs(69));
        assert_eq!(update.changes, vec![]);
        assert_eq!(update.next_deadline, Some(secs(70)));
        let update = pool.update(&rooms, &[], secs(70));
        assert_eq!(update.changes, vec![Change::Release { receiver: 1 }]);
        assert_eq!(update.next_deadline, None);
        assert_eq!(keys(&pool), some(&["room:a", ""]));
    }

    #[test]
    fn a_group_that_forms_again_in_its_grace_period_carries_on() {
        let mut pool = Pool::new(2, GRACE);
        let both = [t("room:a"), t("live:a+b")];
        pool.update(&both, &[], secs(0));
        pool.update(&[t("room:a")], &[], secs(10));
        let update = pool.update(&both, &[], secs(40));
        assert_eq!(update, Update::default());
        assert!(!pool.assignments()[1].as_ref().unwrap().lingering);
        // The clock of the first dissolution is gone: a second one starts
        // its own grace period.
        let update = pool.update(&[t("room:a")], &[], secs(100));
        assert_eq!(update.next_deadline, Some(secs(160)));
    }

    #[test]
    fn a_busy_dissolved_group_is_kept_and_its_grace_starts_when_it_goes_idle() {
        let mut pool = Pool::new(2, GRACE);
        pool.update(&[t("room:a"), t("live:a+b")], &[], secs(0));
        let rooms = [t("room:a")];
        let update = pool.update(&rooms, &[false, true], secs(10));
        assert_eq!((update.changes.len(), update.next_deadline), (0, None));
        let update = pool.update(&rooms, &[false, true], secs(500));
        assert_eq!((update.changes.len(), update.next_deadline), (0, None));
        let update = pool.update(&rooms, &[false, false], secs(600));
        assert_eq!(update.next_deadline, Some(secs(660)));
        // Busy again resets it.
        pool.update(&rooms, &[false, true], secs(650));
        let update = pool.update(&rooms, &[], secs(655));
        assert_eq!(update.next_deadline, Some(secs(715)));
        let update = pool.update(&rooms, &[], secs(715));
        assert_eq!(update.changes, vec![Change::Release { receiver: 1 }]);
    }

    #[test]
    fn a_zero_grace_releases_at_once() {
        let mut pool = Pool::new(1, Duration::ZERO);
        pool.update(&[t("live:a+b")], &[], secs(0));
        let update = pool.update(&[], &[], secs(0));
        assert_eq!(update.changes, vec![Change::Release { receiver: 0 }]);
    }

    #[test]
    fn when_receivers_run_out_the_newest_live_groups_have_none() {
        let mut pool = Pool::new(3, GRACE);
        let rooms = [t("room:a"), t("room:b")];
        let with = |live: &[&str]| {
            let mut all = rooms.to_vec();
            all.extend(live.iter().map(|k| t(k)));
            all
        };
        pool.update(&with(&["live:a+b"]), &[], secs(0));
        // A second and a third live group: no receiver for either, and the
        // older of them is listed first.
        let update = pool.update(&with(&["live:c+d", "live:a+b", "live:e+f"]), &[], secs(1));
        assert_eq!(update.changes, vec![]);
        assert_eq!(
            update.exhausted,
            vec!["live:c+d".to_string(), "live:e+f".to_string()]
        );
        // The oldest dissolves: it lingers nowhere, because a listed target
        // is waiting; the next oldest takes its receiver.
        let update = pool.update(&with(&["live:e+f", "live:c+d"]), &[], secs(2));
        assert_eq!(
            update.changes,
            vec![Change::Assign {
                receiver: 2,
                key: "live:c+d".into(),
                name: "LIVE:C+D".into()
            }]
        );
        assert_eq!(update.exhausted, vec!["live:e+f".to_string()]);
        assert_eq!(update.next_deadline, None);
    }

    #[test]
    fn a_new_room_outranks_a_live_group() {
        let mut pool = Pool::new(2, GRACE);
        pool.update(&[t("room:a"), t("live:a+b")], &[], secs(0));
        let update = pool.update(&[t("room:a"), t("live:a+b"), t("room:b")], &[], secs(1));
        assert_eq!(
            update.changes,
            vec![Change::Assign {
                receiver: 1,
                key: "room:b".into(),
                name: "ROOM:B".into()
            }]
        );
        assert_eq!(update.exhausted, vec!["live:a+b".to_string()]);
    }

    #[test]
    fn of_two_lingering_groups_the_idle_and_older_one_goes_first() {
        let mut pool = Pool::new(3, GRACE);
        pool.update(&[t("live:a+b"), t("live:c+d"), t("live:e+f")], &[], secs(0));
        // a+b dissolves at 1 s (idle), c+d at 2 s (idle), e+f at 3 s (busy).
        pool.update(&[t("live:c+d"), t("live:e+f")], &[], secs(1));
        pool.update(&[t("live:e+f")], &[], secs(2));
        pool.update(&[], &[false, false, true], secs(3));
        assert_eq!(keys(&pool), some(&["live:a+b", "live:c+d", "live:e+f"]));
        // One room: the longest-idle lingering group gives way.
        pool.update(&[t("room:x")], &[false, false, true], secs(4));
        assert_eq!(keys(&pool), some(&["room:x", "live:c+d", "live:e+f"]));
        // A second: the other idle one, not the busy one.
        pool.update(&[t("room:x"), t("room:y")], &[false, false, true], secs(5));
        assert_eq!(keys(&pool), some(&["room:x", "room:y", "live:e+f"]));
        pool.update(
            &[t("room:x"), t("room:y"), t("room:z")],
            &[false, false, true],
            secs(6),
        );
        assert_eq!(keys(&pool), some(&["room:x", "room:y", "room:z"]));
    }

    #[test]
    fn keys_of_no_kind_and_repeats_are_ignored() {
        let mut pool = Pool::new(4, GRACE);
        let update = pool.update(
            &[
                t("player:1"),
                t("room:a"),
                Target::new("room:a", "Second"),
                t("soloist:r0"),
                t(""),
            ],
            &[],
            secs(0),
        );
        assert_eq!(keys(&pool), some(&["room:a", "", "", ""]));
        assert_eq!(update.exhausted, Vec::<String>::new());
        assert_eq!(pool.assignments()[0].as_ref().unwrap().name, "ROOM:A");
    }

    #[test]
    fn a_pool_of_no_receivers_reports_everything_exhausted() {
        let mut pool = Pool::new(0, GRACE);
        let update = pool.update(&[t("room:a"), t("live:a+b")], &[], secs(0));
        assert_eq!(update.changes, vec![]);
        assert_eq!(
            update.exhausted,
            vec!["room:a".to_string(), "live:a+b".to_string()]
        );
    }

    /// A small deterministic generator (xorshift64*, Marsaglia's shifts):
    /// the property tests need variety, not quality, and the same sequence
    /// on every run.
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn rank(pool: &Pool, key: &str, order: &[Target]) -> (Kind, u64) {
        let kind = kind_of(key).unwrap();
        let within = match kind {
            Kind::Live => pool.ages[key],
            _ => order.iter().position(|t| t.key == key).unwrap() as u64,
        };
        (kind, within)
    }

    /// Random histories of targets appearing, vanishing, being renamed and
    /// receivers going busy and idle, against every invariant in the module
    /// documentation.
    #[test]
    fn the_invariants_hold_over_random_histories() {
        let universe = [
            "room:a",
            "room:b",
            "room:c",
            "room:d",
            "group:g",
            "group:h",
            "live:a+b",
            "live:a+c",
            "live:b+c",
            "live:b+d",
            "live:c+d",
            "live:a+b+c",
        ];
        for seed in 1..=300u64 {
            let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let receivers = random.below(7) as usize;
            let mut pool = Pool::new(receivers, GRACE);
            let mut present: Vec<Target> = Vec::new();
            let mut now = Duration::ZERO;
            // What a supervisor would be running if it applied the changes.
            let mut applied: Vec<Option<(String, String)>> = vec![None; receivers];
            // Since when each lingering key has been idle, by this test's
            // own account.
            let mut idle_since: BTreeMap<String, Duration> = BTreeMap::new();
            for step in 0..120 {
                now += Duration::from_secs(random.below(40));
                match random.below(4) {
                    0 | 1 => {
                        let key = universe[random.below(universe.len() as u64) as usize];
                        match present.iter().position(|t| t.key == key) {
                            Some(at) => {
                                present.remove(at);
                            }
                            None => present.push(t(key)),
                        }
                    }
                    2 if !present.is_empty() => {
                        let at = random.below(present.len() as u64) as usize;
                        present[at].name = format!("renamed at step {step}");
                    }
                    _ => {}
                }
                let busy: Vec<bool> = (0..receivers).map(|_| random.below(3) == 0).collect();
                let held_before = pool.assignments();
                let update = pool.update(&present, &busy, now);
                let held = pool.assignments();
                let context = format!("seed {seed} step {step}");

                // The changes are exactly the difference, one per receiver.
                for change in &update.changes {
                    match change {
                        Change::Assign {
                            receiver,
                            key,
                            name,
                        } => applied[*receiver] = Some((key.clone(), name.clone())),
                        Change::Release { receiver } => {
                            assert!(
                                applied[*receiver].is_some(),
                                "{context}: release of a free receiver"
                            );
                            applied[*receiver] = None;
                        }
                    }
                }
                let now_held: Vec<Option<(String, String)>> = held
                    .iter()
                    .map(|a| a.as_ref().map(|a| (a.key.clone(), a.name.clone())))
                    .collect();
                assert_eq!(
                    applied, now_held,
                    "{context}: the changes do not give the assignments"
                );
                let mut touched: Vec<usize> = update
                    .changes
                    .iter()
                    .map(|c| match c {
                        Change::Assign { receiver, .. } | Change::Release { receiver } => *receiver,
                    })
                    .collect();
                let count = touched.len();
                touched.dedup();
                assert_eq!(
                    touched.len(),
                    count,
                    "{context}: two changes for one receiver"
                );

                // No key on two receivers.
                let mut seen: Vec<&str> = held.iter().flatten().map(|a| a.key.as_str()).collect();
                seen.sort_unstable();
                let unique = seen.len();
                seen.dedup();
                assert_eq!(seen.len(), unique, "{context}: a key on two receivers");

                // A target never moves between receivers.
                for (receiver, was) in held_before.iter().enumerate() {
                    let Some(was) = was else { continue };
                    if let Some(at) = pool.receiver_of(&was.key) {
                        assert_eq!(at, receiver, "{context}: {} moved", was.key);
                    }
                }

                // Listed targets: names are current; lingering is exactly
                // "held and not listed", and only a live group lingers.
                for assignment in held.iter().flatten() {
                    match present.iter().find(|t| t.key == assignment.key) {
                        Some(target) => {
                            assert!(!assignment.lingering, "{context}");
                            assert_eq!(assignment.name, target.name, "{context}: a stale name");
                        }
                        None => {
                            assert!(assignment.lingering, "{context}");
                            assert_eq!(kind_of(&assignment.key), Some(Kind::Live), "{context}");
                        }
                    }
                }

                // Exhausted is exactly the listed targets without a
                // receiver; then no receiver is free, none is lingering, and
                // every holder outranks every exhausted target.
                let without: Vec<&str> = present
                    .iter()
                    .filter(|t| pool.receiver_of(&t.key).is_none())
                    .map(|t| t.key.as_str())
                    .collect();
                let mut reported: Vec<&str> = update.exhausted.iter().map(String::as_str).collect();
                let ranks: Vec<(Kind, u64)> =
                    reported.iter().map(|k| rank(&pool, k, &present)).collect();
                assert!(
                    ranks.windows(2).all(|w| w[0] < w[1]),
                    "{context}: exhausted out of rank order"
                );
                reported.sort_unstable();
                let mut expected = without.clone();
                expected.sort_unstable();
                assert_eq!(reported, expected, "{context}");
                if let Some(best_exhausted) = ranks.first() {
                    for assignment in held.iter() {
                        let assignment = assignment.as_ref().unwrap_or_else(|| {
                            panic!("{context}: a free receiver beside an exhausted target")
                        });
                        assert!(
                            !assignment.lingering,
                            "{context}: a lingering group beside an exhausted target"
                        );
                        assert!(
                            rank(&pool, &assignment.key, &present) < *best_exhausted,
                            "{context}: {} holds a receiver that a higher-ranked target lacks",
                            assignment.key
                        );
                    }
                }

                // The grace period: a lingering group is never kept past it
                // when idle throughout, and the deadline returned is the
                // earliest such end.
                let mut earliest = None;
                idle_since.retain(|key, _| pool.receiver_of(key).is_some());
                for (receiver, assignment) in held.iter().enumerate() {
                    let Some(assignment) = assignment else {
                        continue;
                    };
                    if !assignment.lingering || busy[receiver] {
                        idle_since.remove(&assignment.key);
                        continue;
                    }
                    let since = *idle_since.entry(assignment.key.clone()).or_insert(now);
                    assert!(
                        now - since < GRACE,
                        "{context}: {} kept past its grace",
                        assignment.key
                    );
                    earliest =
                        Some(earliest.map_or(since + GRACE, |e: Duration| e.min(since + GRACE)));
                }
                assert_eq!(update.next_deadline, earliest, "{context}");
            }
        }
    }
}
