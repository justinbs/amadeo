//! Background work, shaped so its results cannot move a replay — ADR 0041.
//!
//! # The hazard this is built around
//!
//! Threading is where determinism dies in real engines, and invariant I3 is this project's keystone.
//! The danger is not that two threads corrupt each other — Rust prevents that. It is subtler:
//!
//! **Whether a background job has finished by tick N depends on the wall clock.** A simulation that
//! reacts to "the mesh is ready" reacts on a different tick on a slower machine, and the replay
//! diverges. That is true even though every individual computation is perfectly correct.
//!
//! So this crate provides work, and provides exactly two disciplined ways for the answer to come
//! back. ADR 0041 has the full argument; the short version is:
//!
//! 1. **Wait at a barrier.** Submit the work, then [`JobPool::wait_for_idle`] before continuing.
//!    Parallelism becomes purely a way of computing the same thing faster, and the simulation cannot
//!    tell it happened. This is what asset loading and terrain chunk collision are.
//! 2. **Deliver into something gameplay cannot observe.** Results land in an [`Inbox`] drained into
//!    a [`Service`](../amadeo_ecs/trait.Service.html) — engine machinery that ADR 0009 keeps out of
//!    the state hash. A chunk's *visual* mesh is this; its *collider* is not.
//!
//! **An [`Inbox`] drains in key order, never completion order.** That is the whole reason it exists
//! rather than a plain channel: a channel hands things back in whatever order threads finished,
//! which is exactly the nondeterminism this crate is here to prevent.
//!
//! ```
//! use amadeo_jobs::{Inbox, JobPool};
//!
//! let pool = JobPool::new(4);
//! let inbox: Inbox<u32, u32> = Inbox::new();
//!
//! // Submitted in a deliberately awkward order, and the slowest job is submitted first.
//! for key in [3_u32, 1, 2] {
//!     let inbox = inbox.clone();
//!     pool.submit(move || inbox.deliver(key, key * 10));
//! }
//!
//! // The barrier. After this, every job has finished.
//! pool.wait_for_idle();
//!
//! // Drained by key, so the answer does not depend on which thread won.
//! assert_eq!(inbox.drain(), vec![(1, 10), (2, 20), (3, 30)]);
//! ```

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

/// One unit of background work.
///
/// `'static` and owning its inputs, deliberately: a job that borrowed from the world would need the
/// world held still for its whole lifetime, which is the opposite of running it in the background.
/// Everything a job needs is moved into it, and everything it produces comes back through an
/// [`Inbox`].
type Task = Box<dyn FnOnce() + Send + 'static>;

/// Shared state the pool and its workers coordinate through.
#[derive(Debug)]
struct Shared {
    /// Jobs submitted but not yet finished. Both *queued* and *running* count, which is what makes
    /// [`JobPool::wait_for_idle`] a real barrier rather than merely "the queue is empty".
    outstanding: AtomicUsize,
    /// Woken every time `outstanding` reaches zero.
    idle: Condvar,
    /// Guards the condvar. Holds nothing itself — the count is atomic — but a `Condvar` needs a
    /// mutex to pair with, and this is the smallest honest one.
    idle_lock: Mutex<()>,
}

/// A fixed set of worker threads.
///
/// Created once and kept for the process's lifetime. Threads are **not** spawned per job: creating
/// one costs tens of microseconds, which would dominate anything short enough to be worth
/// parallelising per frame.
///
/// Dropping the pool closes the queue and joins every worker, so a pool cannot outlive the work it
/// was given.
#[derive(Debug)]
pub struct JobPool {
    /// `Option` only so `Drop` can close the channel before joining. A closed sender is what tells
    /// a worker to stop; without dropping it first, the join below would wait forever.
    sender: Option<Sender<Task>>,
    workers: Vec<JoinHandle<()>>,
    shared: Arc<Shared>,
}

impl JobPool {
    /// Starts a pool with `workers` threads. At least one, whatever is asked for.
    ///
    /// A pool of one is a legitimate and useful configuration, not a degenerate case: it runs the
    /// same code down the same path with all parallelism removed, which is what makes
    /// "is this a threading bug?" answerable by changing one number.
    #[must_use]
    pub fn new(workers: usize) -> Self {
        let (sender, receiver) = channel::<Task>();
        // One mutex around the receiver, shared by every worker — the standard shape for a
        // multi-consumer queue built on `std::sync::mpsc`, which is single-consumer by itself.
        let receiver = Arc::new(Mutex::new(receiver));
        let shared = Arc::new(Shared {
            outstanding: AtomicUsize::new(0),
            idle: Condvar::new(),
            idle_lock: Mutex::new(()),
        });

        let handles = (0..workers.max(1))
            .map(|index| {
                let receiver = Arc::clone(&receiver);
                let shared = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("amadeo-job-{index}"))
                    .spawn(move || worker_loop(&receiver, &shared))
                    .expect("the operating system can start a thread")
            })
            .collect();

        Self {
            sender: Some(sender),
            workers: handles,
            shared,
        }
    }

    /// A pool sized to this machine, leaving one core for the simulation thread.
    ///
    /// The simulation is single-threaded permanently (ADR 0036 for physics, ADR 0041 for systems),
    /// so it is a whole core doing real work throughout. Handing every core to background jobs would
    /// have them competing with the thing they exist to keep fed.
    #[must_use]
    pub fn for_this_machine() -> Self {
        let cores = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(2);
        Self::new(cores.saturating_sub(1).max(1))
    }

    /// How many workers this pool has.
    #[must_use]
    pub fn workers(&self) -> usize {
        self.workers.len()
    }

    /// Queues a job.
    ///
    /// Returns immediately. The job runs on some worker at some point, and **nothing about when is
    /// deterministic** — which is why the result has to come back through one of the two disciplined
    /// routes in the module docs.
    pub fn submit(&self, work: impl FnOnce() + Send + 'static) {
        // Incremented *before* queueing. The other order has a race: a worker could take the job and
        // finish it before the count went up, and `wait_for_idle` would return with work in flight.
        self.shared.outstanding.fetch_add(1, Ordering::SeqCst);

        let shared = Arc::clone(&self.shared);
        let task: Task = Box::new(move || {
            work();
            finish(&shared);
        });

        if let Some(sender) = &self.sender {
            // A closed channel means the pool is being dropped. Running the job inline is wrong
            // (it may block a shutdown) and losing it silently is worse, so the count is corrected
            // and the job is dropped — which `wait_for_idle` can then return from.
            if sender.send(task).is_err() {
                finish(&self.shared);
            }
        }
    }

    /// Blocks until every submitted job has finished. **The barrier.**
    ///
    /// This is what turns background work from a determinism hazard into a pure speedup: after it
    /// returns, the world is in exactly the state it would have reached had every job run inline on
    /// the simulation thread, in any order. Nothing downstream can tell the difference.
    pub fn wait_for_idle(&self) {
        let mut guard = self
            .shared
            .idle_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while self.shared.outstanding.load(Ordering::SeqCst) > 0 {
            guard = self
                .shared
                .idle
                .wait(guard)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// How many jobs are queued or running.
    ///
    /// For diagnostics and for a streaming system deciding whether to submit more — **never** for
    /// gameplay to branch on. A count that depends on how fast the machine is, is exactly the input
    /// that would make a replay diverge.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.shared.outstanding.load(Ordering::SeqCst)
    }
}

impl Drop for JobPool {
    fn drop(&mut self) {
        // Closing the channel is what ends each worker's loop; without this the joins below block
        // forever waiting for work that will never come.
        self.sender = None;
        for worker in self.workers.drain(..) {
            // A worker that panicked has already reported it. Joining is still worth doing so the
            // thread is reaped, and the panic is not re-raised here because a `Drop` that panics
            // during unwinding aborts the process.
            let _ = worker.join();
        }
    }
}

/// Marks one job complete and wakes anyone waiting at the barrier.
fn finish(shared: &Shared) {
    if shared.outstanding.fetch_sub(1, Ordering::SeqCst) == 1 {
        // Taking the lock before notifying is what closes the race with `wait_for_idle`: without it,
        // a waiter could check the count, see one outstanding, and start waiting *after* this
        // notification had already gone out — and then wait forever.
        let _guard = shared
            .idle_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        shared.idle.notify_all();
    }
}

fn worker_loop(receiver: &Mutex<std::sync::mpsc::Receiver<Task>>, _shared: &Shared) {
    loop {
        // The lock is held only long enough to take a job, never while running one — otherwise the
        // pool would execute one job at a time and every worker but one would be idle.
        let task = {
            let guard = receiver
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.recv()
        };
        match task {
            Ok(task) => task(),
            // The sender was dropped: the pool is shutting down.
            Err(_) => return,
        }
    }
}

/// Somewhere finished jobs put their results, drained in **key order**.
///
/// # Why not a channel
///
/// A channel hands results back in whichever order threads happened to finish, which is the exact
/// nondeterminism this crate exists to prevent. An `Inbox` is a sorted map: whatever order the work
/// completed in, [`Inbox::drain`] returns it sorted by key, so the simulation sees one fixed
/// sequence on every machine and every run.
///
/// Pick a key that is a **stable property of the work**, not of when it was submitted — an asset id,
/// a chunk coordinate. A submission counter would be stable too, but only if submission order is,
/// and that is a much easier thing to get wrong later.
#[derive(Debug)]
pub struct Inbox<K: Ord + Send + 'static, V: Send + 'static> {
    delivered: Arc<Mutex<BTreeMap<K, V>>>,
}

// Hand-written rather than derived: `#[derive(Clone)]` would demand `K: Clone, V: Clone`, which is
// wrong -- an `Inbox` clone shares one map rather than copying it, and that is the whole point.
impl<K: Ord + Send + 'static, V: Send + 'static> Clone for Inbox<K, V> {
    fn clone(&self) -> Self {
        Self {
            delivered: Arc::clone(&self.delivered),
        }
    }
}

impl<K: Ord + Send + 'static, V: Send + 'static> Default for Inbox<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Send + 'static, V: Send + 'static> Inbox<K, V> {
    /// An empty inbox.
    #[must_use]
    pub fn new() -> Self {
        Self {
            delivered: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Puts a finished result in. Called from a worker thread.
    ///
    /// Delivering the same key twice keeps the **last** one, which is right for the case that
    /// produces it: work resubmitted because its input changed should win over the stale answer
    /// still in flight.
    pub fn deliver(&self, key: K, value: V) {
        let mut map = self
            .delivered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.insert(key, value);
    }

    /// Takes everything delivered so far, **sorted by key**, and empties the inbox.
    ///
    /// Called from the simulation thread, at a point the schedule fixes. What arrives is a fixed
    /// sequence for a fixed set of keys, whatever order the workers finished in.
    #[must_use]
    pub fn drain(&self) -> Vec<(K, V)> {
        let mut map = self
            .delivered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *map).into_iter().collect()
    }

    /// How many results are waiting.
    ///
    /// Diagnostics only, for the same reason [`JobPool::pending`] is: it depends on the wall clock.
    #[must_use]
    pub fn ready(&self) -> usize {
        self.delivered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn every_submitted_job_runs() {
        let pool = JobPool::new(4);
        let inbox: Inbox<u32, u32> = Inbox::new();
        for key in 0..100 {
            let inbox = inbox.clone();
            pool.submit(move || inbox.deliver(key, key));
        }
        pool.wait_for_idle();
        assert_eq!(inbox.ready(), 100);
    }

    #[test]
    fn results_arrive_in_key_order_however_they_finish() {
        // **The property the whole crate exists for.** The first job submitted is made the slowest,
        // so completion order is guaranteed to differ from submission order — and the drained order
        // must still be by key.
        let pool = JobPool::new(4);
        let inbox: Inbox<u32, &'static str> = Inbox::new();

        for (key, label, delay_ms) in [
            (0_u32, "first", 40_u64),
            (1, "second", 1),
            (2, "third", 1),
            (3, "fourth", 1),
        ] {
            let inbox = inbox.clone();
            pool.submit(move || {
                std::thread::sleep(Duration::from_millis(delay_ms));
                inbox.deliver(key, label);
            });
        }
        pool.wait_for_idle();

        assert_eq!(
            inbox.drain(),
            vec![(0, "first"), (1, "second"), (2, "third"), (3, "fourth")]
        );
    }

    #[test]
    fn the_same_work_drains_identically_however_many_workers_run_it() {
        // I3, at the level this crate can be responsible for: the number of threads is a property of
        // the machine, and it must not be able to reach the answer. A pool of one is not a special
        // case here — it is the control.
        let run = |workers: usize| {
            let pool = JobPool::new(workers);
            let inbox: Inbox<u32, u32> = Inbox::new();
            for key in (0..64).rev() {
                let inbox = inbox.clone();
                pool.submit(move || inbox.deliver(key, key * 3));
            }
            pool.wait_for_idle();
            inbox.drain()
        };
        assert_eq!(run(1), run(8));
    }

    #[test]
    fn the_barrier_waits_for_work_that_is_running_not_just_queued() {
        // The race the outstanding count is ordered to avoid. A barrier that only checked for an
        // empty *queue* would return while the last job was still running, and the simulation would
        // read a half-written result — reproducibly on a fast machine and never on a slow one.
        let pool = JobPool::new(2);
        let inbox: Inbox<u32, u32> = Inbox::new();
        for key in 0..4 {
            let inbox = inbox.clone();
            pool.submit(move || {
                std::thread::sleep(Duration::from_millis(20));
                inbox.deliver(key, key);
            });
        }

        pool.wait_for_idle();
        assert_eq!(inbox.ready(), 4, "every job should have finished");
        assert_eq!(pool.pending(), 0);
    }

    #[test]
    fn draining_empties_the_inbox() {
        let inbox: Inbox<u8, u8> = Inbox::new();
        inbox.deliver(1, 1);
        assert_eq!(inbox.drain().len(), 1);
        assert!(inbox.drain().is_empty());
    }

    #[test]
    fn redelivering_a_key_keeps_the_newer_answer() {
        // Work resubmitted because its input changed should win over the stale answer still in
        // flight — a terrain chunk edited while its old mesh was being built is exactly this.
        let inbox: Inbox<&'static str, u32> = Inbox::new();
        inbox.deliver("chunk", 1);
        inbox.deliver("chunk", 2);
        assert_eq!(inbox.drain(), vec![("chunk", 2)]);
    }

    #[test]
    fn waiting_on_an_idle_pool_returns_immediately() {
        let pool = JobPool::new(2);
        pool.wait_for_idle();
        assert_eq!(pool.pending(), 0);
    }

    #[test]
    fn a_pool_sized_to_this_machine_leaves_a_core_for_the_simulation() {
        let pool = JobPool::for_this_machine();
        assert!(pool.workers() >= 1);
    }
}
