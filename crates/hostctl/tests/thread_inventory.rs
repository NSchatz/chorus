//! The thread inventory, one failure class at a time.
//!
//! Every case below is provoked through [`TaskSource`], which is the seam the
//! enumeration reads the kernel through. That matters for two reasons. The
//! first is that these classes cannot otherwise be produced without privilege
//! or a real fault: a test cannot make the kernel refuse to read one thread of
//! its own process, and a test that waited for a thread to vanish at the right
//! microsecond would be a coin toss. The second is that the errno injected
//! here is the errno the kernel returns, so the enumeration cannot tell the
//! difference between this and the real thing.
//!
//! One test reaches the real `/proc/self/task` on purpose: an inventory that
//! is right about a fake kernel and wrong about this one would be worthless.

use std::collections::HashMap;
use std::io;
use std::sync::mpsc;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use chorus_hostctl::{
    current_policy, thread_id, thread_inventory, thread_inventory_from, InventoryError,
    ProcTaskSource, TaskSource, ThreadName, ThreadRegistry, SCHED_FIFO, SCHED_OTHER,
};
use chorus_hostctl::{undeclared_real_time_threads_from, undeclared_real_time_threads_in};

/// `ENOENT`: the file is not there, because the thread is not there.
const ENOENT: i32 = 2;
/// `ESRCH`: the same fact, spelled about the task.
const ESRCH: i32 = 3;
/// `EIO`: a read that failed and says nothing about whether the thread exists.
const EIO: i32 = 5;
/// `EACCES`: the kernel refused.
const EACCES: i32 = 13;

/// A `/proc/<pid>/stat` line of the real shape, with the two fields the
/// enumeration reads placed where the kernel places them.
fn stat_line(tid: i32, comm: &str, policy: i32, rt_priority: u32) -> String {
    let mut fields: Vec<String> = Vec::new();
    for n in 4..=52 {
        fields.push(match n {
            40 => rt_priority.to_string(),
            41 => policy.to_string(),
            _ => "0".to_string(),
        });
    }
    format!("{} ({}) S {}", tid, comm, fields.join(" "))
}

/// A thread listing with an injectable fault at every point the real one has.
struct FakeTasks {
    listing: Result<Vec<String>, i32>,
    stat: HashMap<i32, Result<String, i32>>,
    comm: HashMap<i32, Result<String, i32>>,
}

impl FakeTasks {
    /// Threads that all exist and are all readable.
    fn healthy(threads: &[(i32, &str, i32, u32)]) -> FakeTasks {
        let mut stat = HashMap::new();
        let mut comm = HashMap::new();
        let mut listing = Vec::new();
        for (tid, name, policy, rt_priority) in threads {
            listing.push(tid.to_string());
            stat.insert(*tid, Ok(stat_line(*tid, name, *policy, *rt_priority)));
            comm.insert(*tid, Ok(format!("{}\n", name)));
        }
        FakeTasks {
            listing: Ok(listing),
            stat,
            comm,
        }
    }

    /// The three-thread process every case below starts from.
    fn three() -> FakeTasks {
        FakeTasks::healthy(&[
            (11, "chorus-main", SCHED_OTHER, 0),
            (12, "chorus-audio", SCHED_FIFO, 20),
            (13, "chorus-serve", SCHED_OTHER, 0),
        ])
    }

    fn listing_fails(mut self, errno: i32) -> FakeTasks {
        self.listing = Err(errno);
        self
    }

    fn listing_offers(mut self, entry: &str) -> FakeTasks {
        if let Ok(listing) = self.listing.as_mut() {
            listing.push(entry.to_string());
        }
        self
    }

    fn stat_fails(mut self, tid: i32, errno: i32) -> FakeTasks {
        self.stat.insert(tid, Err(errno));
        self
    }

    fn stat_reads(mut self, tid: i32, body: &str) -> FakeTasks {
        self.stat.insert(tid, Ok(body.to_string()));
        self
    }

    fn comm_fails(mut self, tid: i32, errno: i32) -> FakeTasks {
        self.comm.insert(tid, Err(errno));
        self
    }

    fn comm_reads(mut self, tid: i32, body: &str) -> FakeTasks {
        self.comm.insert(tid, Ok(body.to_string()));
        self
    }
}

/// A listing that loses one live thread on its first pass and tells the whole
/// truth afterwards.
///
/// This is not a hypothetical. `/proc/<pid>/task` is generated as it is read
/// and its iteration is not atomic, and it was measured doing exactly this on
/// this project's own suite: about 3 runs in 100, the listing came back without
/// the thread that was reading it, whose `stat` was readable throughout and
/// which a listing taken microseconds later contained.
struct LosesOneThreadOnce {
    inner: FakeTasks,
    lost: String,
    passes: Mutex<usize>,
}

impl TaskSource for LosesOneThreadOnce {
    fn describe(&self) -> String {
        self.inner.describe()
    }

    fn list(&self) -> io::Result<Vec<String>> {
        let mut passes = self.passes.lock().expect("the counter is not poisoned");
        *passes += 1;
        let names = self.inner.list()?;
        if *passes == 1 {
            return Ok(names.into_iter().filter(|n| n != &self.lost).collect());
        }
        Ok(names)
    }

    fn read_stat(&self, tid: i32) -> io::Result<String> {
        self.inner.read_stat(tid)
    }

    fn read_comm(&self, tid: i32) -> io::Result<String> {
        self.inner.read_comm(tid)
    }
}

fn answer(from: &Result<String, i32>) -> io::Result<String> {
    match from {
        Ok(body) => Ok(body.clone()),
        Err(errno) => Err(io::Error::from_raw_os_error(*errno)),
    }
}

impl TaskSource for FakeTasks {
    fn describe(&self) -> String {
        "a modelled /proc/self/task".to_string()
    }

    fn list(&self) -> io::Result<Vec<String>> {
        match &self.listing {
            Ok(names) => Ok(names.clone()),
            Err(errno) => Err(io::Error::from_raw_os_error(*errno)),
        }
    }

    fn read_stat(&self, tid: i32) -> io::Result<String> {
        match self.stat.get(&tid) {
            Some(answer_for) => answer(answer_for),
            None => Err(io::Error::from_raw_os_error(ENOENT)),
        }
    }

    fn read_comm(&self, tid: i32) -> io::Result<String> {
        match self.comm.get(&tid) {
            Some(answer_for) => answer(answer_for),
            None => Err(io::Error::from_raw_os_error(ENOENT)),
        }
    }
}

// --- one entry per live thread ----------------------------------------------

#[test]
fn inventory_lists_every_live_thread() {
    let inventory = thread_inventory_from(&FakeTasks::three())
        .expect("three readable threads make a complete inventory");
    assert_eq!(inventory.threads().len(), 3);
    assert_eq!(inventory.vanished(), 0, "nothing was dropped");

    let audio = &inventory.threads()[1];
    assert_eq!(audio.tid, 12);
    assert_eq!(audio.comm, ThreadName::Known("chorus-audio".to_string()));
    assert_eq!(audio.policy, SCHED_FIFO);
    assert_eq!(audio.rt_priority, 20);
    assert!(audio.is_real_time());

    let main = &inventory.threads()[0];
    assert_eq!(main.tid, 11);
    assert_eq!(main.policy, SCHED_OTHER);
    assert_eq!(main.rt_priority, 0);
    assert!(!main.is_real_time());

    // A listing that loses a live thread on one pass must not cost that thread
    // its entry. The listing is where this enumeration was actually losing
    // threads, and losing one there looks identical to the thread not existing.
    let lossy = LosesOneThreadOnce {
        inner: FakeTasks::three(),
        lost: "12".to_string(),
        passes: Mutex::new(0),
    };
    let survived = thread_inventory_from(&lossy)
        .expect("a listing that missed a thread once is not a failed enumeration");
    assert_eq!(
        survived.threads().len(),
        3,
        "a thread missing from one pass of the listing kept its entry"
    );
    assert!(survived.threads().iter().any(|t| t.tid == 12));
    assert_eq!(
        survived.vanished(),
        0,
        "and it is not counted as having exited, because it did not"
    );

    // And against the kernel itself, because an enumeration that is only right
    // about a modelled /proc is not the thing anyone needs. Four threads are
    // held alive on a barrier for the whole of the enumeration, which is what
    // "exists and is readable for the whole of that enumeration" means.
    let held = 4;
    let barrier = Arc::new(Barrier::new(held + 1));
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();
    for n in 0..held {
        let barrier = Arc::clone(&barrier);
        let tx = tx.clone();
        handles.push(
            thread::Builder::new()
                .name(format!("chorus-inv-{}", n))
                .spawn(move || {
                    tx.send(thread_id()).expect("the main thread is listening");
                    barrier.wait();
                })
                .expect("a thread can be spawned"),
        );
    }
    drop(tx);
    let live: Vec<i32> = (0..held)
        .map(|_| rx.recv().expect("each thread reports its own tid"))
        .collect();

    let real = thread_inventory().expect("this process can enumerate its own threads");
    for tid in &live {
        let found = real
            .threads()
            .iter()
            .find(|t| t.tid == *tid)
            .unwrap_or_else(|| panic!("thread {} is alive and is missing from the inventory", tid));
        assert!(
            found.comm.known().is_some_and(|n| n.starts_with("chorus-inv-")),
            "thread {} is reported as {}",
            tid,
            found.comm
        );
    }
    let me = real
        .threads()
        .iter()
        .find(|t| t.tid == thread_id())
        .expect("the thread doing the asking is in its own inventory");
    assert_eq!(me.policy, current_policy());

    barrier.wait();
    for handle in handles {
        handle.join().expect("every held thread ends cleanly");
    }
}

// --- a thread that ceased to exist ------------------------------------------

#[test]
fn vanished_thread_is_omitted_and_counted() {
    for errno in [ENOENT, ESRCH] {
        let inventory = thread_inventory_from(&FakeTasks::three().stat_fails(12, errno))
            .unwrap_or_else(|e| panic!("a thread that exited is not a failure, got {}", e));
        assert_eq!(inventory.threads().len(), 2);
        assert!(
            !inventory.threads().iter().any(|t| t.tid == 12),
            "the thread that exited is omitted"
        );
        assert_eq!(
            inventory.vanished(),
            1,
            "and the count says something was dropped and it had exited"
        );
    }

    // The count is what makes the two cases tellable apart, so the other side
    // of it is asserted here too.
    let clean = thread_inventory_from(&FakeTasks::three()).expect("all three are readable");
    assert_eq!(clean.vanished(), 0, "nothing was dropped");
}

// --- a read that failed for any other reason --------------------------------

#[test]
fn transient_read_failure_is_an_error_not_an_omission() {
    let result = thread_inventory_from(&FakeTasks::three().stat_fails(12, EIO));
    let error = match result {
        Err(e) => e,
        Ok(inventory) => panic!(
            "a transient read failure returned {} threads instead of an error",
            inventory.threads().len()
        ),
    };
    match &error {
        InventoryError::ThreadUnreadable { tid, errno, .. } => {
            assert_eq!(*tid, 12, "the error names the thread");
            assert_eq!(*errno, EIO, "and the reason the kernel gave");
        }
        other => panic!("expected ThreadUnreadable, got {:?}", other),
    }
    let said = error.to_string();
    assert!(said.contains("12"), "the message names the thread: {}", said);
    assert!(
        said.contains("Input/output error"),
        "the message names the reason: {}",
        said
    );
    assert!(
        !error.is_uninterpretable_record(),
        "a read that failed is not a record that could not be read"
    );
}

// --- the kernel refusing ----------------------------------------------------

#[test]
fn permission_denial_fails_the_enumeration() {
    // Refused the listing.
    let refused_listing = thread_inventory_from(&FakeTasks::three().listing_fails(EACCES));
    match refused_listing {
        Err(InventoryError::ListingUnreadable { errno, ref detail, .. }) => {
            assert_eq!(errno, EACCES);
            assert!(detail.contains("Permission denied"), "detail: {}", detail);
        }
        Err(other) => panic!("expected ListingUnreadable, got {:?}", other),
        Ok(inventory) => panic!(
            "a refused listing produced a list of {} threads",
            inventory.threads().len()
        ),
    }

    // Refused one listed thread.
    let refused_thread = thread_inventory_from(&FakeTasks::three().stat_fails(12, EACCES));
    match refused_thread {
        Err(InventoryError::ThreadUnreadable { tid, errno, .. }) => {
            assert_eq!(tid, 12);
            assert_eq!(errno, EACCES);
        }
        Err(other) => panic!("expected ThreadUnreadable, got {:?}", other),
        Ok(inventory) => panic!(
            "a refused thread produced a partial list of {} threads",
            inventory.threads().len()
        ),
    }
}

// --- a record that cannot be interpreted ------------------------------------

#[test]
fn unreadable_scheduling_record_is_an_error() {
    for body in ["not a stat line at all", "1234 (chorus) S 0 0 0"] {
        let result = thread_inventory_from(&FakeTasks::three().stat_reads(12, body));
        let error = match result {
            Err(e) => e,
            Ok(inventory) => panic!(
                "an uninterpretable record produced {} threads",
                inventory.threads().len()
            ),
        };
        match &error {
            InventoryError::RecordUninterpretable { tid, .. } => assert_eq!(*tid, 12),
            other => panic!("expected RecordUninterpretable, got {:?}", other),
        }
        assert!(error.is_uninterpretable_record());
        assert!(
            error.to_string().contains("12"),
            "the message names the thread: {}",
            error
        );
    }
}

#[test]
fn an_empty_record_is_never_reported_as_defaults() {
    // An empty scheduling record.
    let result = thread_inventory_from(&FakeTasks::three().stat_reads(12, ""));
    let error = match result {
        Err(e) => e,
        Ok(inventory) => {
            let defaulted = inventory
                .threads()
                .iter()
                .find(|t| t.tid == 12)
                .map(|t| format!("policy={} rt_priority={}", t.policy, t.rt_priority))
                .unwrap_or_else(|| "omitted entirely".to_string());
            panic!("an empty record was reported as {}", defaulted);
        }
    };
    assert!(
        error.is_uninterpretable_record(),
        "an empty record falls under the uninterpretable-record rule, got {:?}",
        error
    );
    assert!(
        error.to_string().contains("empty"),
        "the message says what was wrong: {}",
        error
    );

    // A listing entry that names no thread at all.
    let result = thread_inventory_from(&FakeTasks::three().listing_offers("not-a-thread"));
    let error = match result {
        Err(e) => e,
        Ok(inventory) => panic!(
            "an entry naming no thread produced {} threads",
            inventory.threads().len()
        ),
    };
    assert!(
        error.is_uninterpretable_record(),
        "an entry naming no thread falls under the same rule, got {:?}",
        error
    );
    match &error {
        InventoryError::EntryNamesNoThread { entry } => assert_eq!(entry, "not-a-thread"),
        other => panic!("expected EntryNamesNoThread, got {:?}", other),
    }
}

// --- an inventory with nothing in it ----------------------------------------

#[test]
fn an_empty_inventory_is_an_error() {
    let nothing = FakeTasks {
        listing: Ok(Vec::new()),
        stat: HashMap::new(),
        comm: HashMap::new(),
    };
    match thread_inventory_from(&nothing) {
        Err(InventoryError::Empty { listed, vanished }) => {
            assert_eq!(listed, 0);
            assert_eq!(vanished, 0);
        }
        Err(other) => panic!("expected Empty, got {:?}", other),
        Ok(inventory) => panic!(
            "an empty listing returned an inventory of {} threads",
            inventory.threads().len()
        ),
    }

    // And an inventory emptied by threads exiting is empty for the same
    // reason: the thread doing the asking cannot have been one of them.
    let all_gone = FakeTasks::healthy(&[(11, "chorus-main", SCHED_OTHER, 0)]).stat_fails(11, ENOENT);
    match thread_inventory_from(&all_gone) {
        Err(InventoryError::Empty { listed, vanished }) => {
            assert_eq!(listed, 1);
            assert_eq!(vanished, 1);
        }
        Err(other) => panic!("expected Empty, got {:?}", other),
        Ok(inventory) => panic!(
            "everything having exited returned {} threads",
            inventory.threads().len()
        ),
    }

    // The real one is never empty, which is the fact the rule rests on.
    let real = thread_inventory().expect("this process can enumerate its own threads");
    assert!(!real.threads().is_empty());
    assert!(ProcTaskSource.describe().contains("/proc/self/task"));
}

// --- a name that cannot be read ---------------------------------------------

#[test]
fn an_unreadable_thread_name_does_not_drop_the_thread() {
    for source in [
        FakeTasks::three().comm_fails(12, EIO),
        FakeTasks::three().comm_fails(12, EACCES),
        // A name read successfully and empty is a name nobody knows, not a
        // thread called "".
        FakeTasks::three().comm_reads(12, "\n"),
    ] {
        let inventory = thread_inventory_from(&source)
            .expect("a name nobody can read does not fail the enumeration");
        assert_eq!(inventory.threads().len(), 3, "the thread keeps its place");
        assert_eq!(inventory.vanished(), 0);
        let unnamed = inventory
            .threads()
            .iter()
            .find(|t| t.tid == 12)
            .expect("the thread whose name could not be read is still in the list");
        assert_eq!(unnamed.comm, ThreadName::Unavailable);
        assert_eq!(unnamed.comm.known(), None);
        assert_ne!(
            unnamed.comm.to_string(),
            "",
            "an unavailable name is never the empty string"
        );
        // Everything else about it is still the kernel's answer.
        assert_eq!(unnamed.policy, SCHED_FIFO);
        assert_eq!(unnamed.rt_priority, 20);
    }
}

// --- the question this all exists for ---------------------------------------

#[test]
fn no_clean_answer_from_an_incomplete_inventory() {
    let registry = ThreadRegistry::new();

    // Complete, and the undeclared real-time thread is found. This is the
    // positive control: the question IS answerable, so a refusal below is
    // about the inventory and not about the question.
    let answered = undeclared_real_time_threads_from(&registry, &FakeTasks::three())
        .expect("a complete inventory answers the question");
    assert_eq!(answered.len(), 1, "tid 12 is real-time and undeclared");
    assert_eq!(answered[0].facts.tid, 12);

    // Incomplete, in each of the ways it can be incomplete. None of them may
    // answer "zero undeclared real-time threads".
    let incomplete: Vec<FakeTasks> = vec![
        FakeTasks::three().listing_fails(EACCES),
        FakeTasks::three().stat_fails(12, EACCES),
        FakeTasks::three().stat_fails(12, EIO),
        FakeTasks::three().stat_reads(12, ""),
        FakeTasks::three().listing_offers("not-a-thread"),
    ];
    for source in &incomplete {
        match undeclared_real_time_threads_from(&registry, source) {
            Err(e) => assert!(!e.to_string().is_empty(), "the failure is reported: {}", e),
            Ok(found) => panic!(
                "an incomplete inventory answered with {} undeclared real-time threads",
                found.len()
            ),
        }
        assert!(
            thread_inventory_from(source).is_err(),
            "and there is no inventory to answer from"
        );
    }

    // The only way to reach the answer is through an inventory that completed,
    // and one of those cannot be built out of a shortened list.
    let complete = thread_inventory_from(&FakeTasks::three()).expect("this one completes");
    assert_eq!(
        undeclared_real_time_threads_in(&registry, &complete).len(),
        1
    );
}
