# ADR 0041 — Parallelism is deterministic by construction, or it does not exist

**Status:** Accepted · **Date:** 2026-08-06 · **Resolves:** Q9 · **Builds on:** ADR 0005, ADR 0009,
ADR 0021, ADR 0036

## Context

The engine has been single-threaded everywhere until now — grepping for `std::thread` finds only
`thread::current().id()` inside error messages.

**Q9 has been open since session 2**, and says: *"Which pools exist, what runs off the simulation
thread, and exactly how results re-enter the deterministic zone in a fixed order. This is where
determinism is most commonly lost in real engines. Decide before adding the first background task,
not after."*

Justin asked for two things, which are the first background tasks: **multithreaded asset loading**,
and **parallel ECS queries for heavier per-entity work**. Behind both sits a bigger target — open
worlds, streamed terrain, surface-nets meshing — where the work genuinely does not fit on one thread.

**Invariant I3 is the keystone.** `CLAUDE.md` says so directly and lists what it buys: replay-as-test,
headless verification, snapshots, save/load, time-travel debugging. Trap 2 names "unsorted parallel
writes" as a nondeterminism leak. So this is the most expensive decision left in the project.

## What the research found

**Three specific ways parallelism destroys determinism**, all of them real and all of them
demonstrated in shipping engines:

1. **Completion order reaches the answer.** Bevy's own `ParallelCommands` documentation says command
   application order "will depend on how many threads are run", so non-commutative commands give
   non-deterministic results. Spawn order becoming thread-count dependent is enough to void a replay.
2. **Parallel reduction changes float results.** Floating-point addition is not associative, so
   summing across threads gives a different number than summing in order. This is the same class of
   problem ADR 0036 already hit, and it is why rapier's `parallel` is mutually exclusive with
   `enhanced-determinism`.
3. **Timing reaches the answer.** Whether a background job has finished by tick N depends on the wall
   clock. A simulation that reacts to "the mesh is ready" reacts on a different tick on a slower
   machine — and every individual computation was correct.

Avian, the other Rust physics engine, reaches the same conclusion rapier does: disable parallelism
for strict determinism.

**But ECS is a viable deterministic concurrency model**, and the condition is precise. Parallel
iteration is deterministic if and only if **each parallel unit writes only its own entity's
components, with no shared accumulator and no cross-entity reads of mutated data**. That is a
property an API can *enforce* rather than merely document.

**And one measurement decided the priority.** Gate 4 measured the Atrium's whole simulation tick at
**8.3 µs — 0.05% of a 60 Hz frame**. Parallel system execution would be optimising something that
currently costs nothing. What is actually expensive is asset loading and chunk meshing, and neither
is a gameplay system: both are *jobs*, pure functions from owned input to owned output.

## Decision

**Parallelism is allowed only in shapes where determinism is structural. The unsafe shapes are not
made discouraged — they are made unspellable.** The same move as `Component: Reflect` (ADR 0013), the
Resource/Service split (ADR 0009), and `move_shape` being a separate operation (ADR 0037).

### 1. `amadeo-jobs`: background work with two disciplined ways back

A `JobPool` of fixed worker threads, and an `Inbox` that results are delivered into. A job is
`FnOnce() + Send + 'static` — it **owns its inputs**, so it cannot borrow the world, and it cannot
reach anything the simulation is touching.

There are exactly two permitted routes for an answer to re-enter the simulation:

- **Wait at a barrier.** `JobPool::wait_for_idle()` blocks until everything submitted has finished.
  After it returns the world is in exactly the state it would have reached had every job run inline,
  in any order — so parallelism becomes purely a faster way of computing the same thing and nothing
  downstream can tell it happened. Asset loading is this. Terrain **collision** is this.
- **Deliver into something gameplay cannot observe.** Results land in a `Service`, which ADR 0009
  keeps structurally out of the state hash. A terrain chunk's **visual mesh** is this.

**`Inbox` drains in key order, never completion order.** That is the whole reason it exists rather
than a channel: a channel returns things in whatever order threads finished, which is exactly the
nondeterminism being prevented. `the_same_work_drains_identically_however_many_workers_run_it` pins
it by running the same jobs on a pool of 1 and a pool of 8 and requiring identical output.

**The key must be a stable property of the work** — an asset id, a chunk coordinate — not a
submission counter, which is only stable if submission order is.

**`JobPool::pending()` is diagnostics only.** A count that depends on machine speed is precisely the
input that would make a replay diverge, and the doc comment says so.

### 2. The visual/gameplay split is the load-bearing rule for streaming

This is the part that is easy to get wrong, and it is where an open world will actually break.

A streamed terrain chunk has two products. Its **mesh** is drawn and nothing else — it goes in a
Service and may arrive whenever it arrives. Its **collider** is gameplay: a character stands on it,
so *when* it becomes available changes where the character is.

So the collider cannot stream freely. The rule is: **which chunks are active is decided
deterministically** (a function of the player's position, which is deterministic), and **the
simulation blocks until the chunks it needs are ready**. That costs a frame hitch on a slow machine
and preserves the replay, which is the correct trade and the one deterministic-lockstep games have
always made.

ADR 0021 already established exactly half of this for assets: gameplay may not ask "has this
finished loading?" The generalisation is that gameplay may not observe *any* completion timing.

### 3. `par_for_each_mut` for within-system parallelism, with a constrained closure

Systems that genuinely do heavy per-entity work opt in. The closure sees **one entity's components
and nothing else** — no `&World`, no `Commands`, no captured accumulator. Writes are therefore
provably disjoint, and the result is bit-identical to the sequential version.

What that forbids is what makes it safe: no spawning, no despawning, no cross-entity reads, no sums.
A system needing those stays sequential, which costs nothing it was not already paying.

### 4. System *execution* stays sequential and ordered

The schedule keeps resolving to one total order with alphabetical tie-breaking. No access
declarations, no conflict analysis, no parallel dispatch.

Justified by measurement rather than taste: the simulation is 8.3 µs. Revisit when a profile says a
real game's systems are the bottleneck — `profile.frame` (ADR 0040) is how that would be noticed.

### 5. No `rayon`, no `tokio`, no async

`amadeo-jobs` has **no dependencies at all**, not even `thiserror`. A job pool is `std::thread`, a
channel and a mutex.

Reaching for a work-stealing runtime would put a scheduler the engine does not control underneath the
one thing that must be reproducible — and would make "why did this replay diverge" a question about
someone else's code. This may be revisited for `par_for_each_mut`, where scoped borrows across a
persistent pool are genuinely awkward without either `rayon` or `unsafe`, and `unsafe` is forbidden
by ADR 0008. That is a narrower decision than this one and can be made when the code is written.

## Alternatives rejected

**Bevy-style parallel system execution**, where systems declare their access and non-conflicting ones
run concurrently. The industry-standard design and the largest theoretical win. Rejected on three
counts: it requires every system to declare access where today a system is simply
`FnMut(&mut World)`; Bevy's own parallel command buffer is documented as non-deterministic, so the
deterministic command merge would be something this project had to invent; and gate 4 says the thing
it optimises currently costs 0.05% of a frame.

**Threads only outside the tick** — background jobs, and no parallel iteration at all. Genuinely
tempting, and the smallest possible surface. Rejected because it leaves a game with heavy per-entity
work no answer at all, and because `par_for_each_mut`'s constrained closure gets that case for a much
smaller risk than the rejected option above.

**Async/await throughout.** Rejected without much agonising: it colours every function it touches, it
needs a runtime, and the engine's work is CPU-bound batch computation rather than IO concurrency.

## Consequences

- **Q9 is resolved**, and Q12 (`Service: Send + Sync`) is *not* moot — but it is easier. Services
  stay `Send + Sync` and the bound is now earned rather than speculative.
- **A frame hitch is the accepted cost of a slow machine**, not a dropped guarantee. A game that
  cannot stream fast enough waits, and the replay still reproduces.
- **`available_parallelism` decides the pool size, leaving one core for the simulation** — which is a
  whole core doing real work throughout, permanently, by ADR 0036 and by §4 above.
- **A pool of one worker is a supported configuration**, not a degenerate case. It is the control
  case for "is this a threading bug?", answerable by changing one number.
- Nothing in the engine uses any of this yet. Background asset loading and terrain chunk meshing are
  the first two consumers, and both belong to the milestone this ADR was written for.
