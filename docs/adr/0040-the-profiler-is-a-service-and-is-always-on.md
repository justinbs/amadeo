# ADR 0040 — The profiler is a service, and it is always on

**Status:** Accepted · **Date:** 2026-08-06 · **Builds on:** ADR 0009, ADR 0016

## Context

M2's exit gate 4 asks for "frame time within a declared budget at a declared scene complexity,
numbers written down". It was the last part of the milestone with no work against it.

Writing frame time down needs measuring it, and measuring *per system* — which is what
`docs/04-subsystems.md` §18 asks for, and what makes a number actionable rather than alarming —
needs the tick loop to read a clock.

**`CLAUDE.md` trap 2 names `Instant::now()` in gameplay as a nondeterminism leak**, second on a list
of things that will quietly destroy this design. So how timing gets in is a real decision rather than
an implementation detail.

Two things were also found while looking, and both are the same shape as findings this project has
had before:

- **`docs/04` §18 marked `profile.frame` ✅ done. It did not exist.** `docs/protocol/v1.md` correctly
  listed it as pending. A claim written into a doc and never executed — the same class of error as
  the CI comment that asserted GPU tests did not run.
- **There was no per-system timing of any kind**, so "which system got slower" could only be answered
  with an external profiler.

## Decision

### 1. Timing goes into a `Service`, which is what makes the clock read safe

`Profiler` is a [`Service`], not a `Resource`. **ADR 0009's split exists for exactly this shape of
thing**: a resource is simulation state and is in the state hash; a service is engine machinery and
is structurally excluded from it.

So nothing the profiler records can reach a state hash, a snapshot or a replay — not by convention
but because `World::state_hash` cannot see the service store at all. That is the same mechanism that
already lets `Renderer` hold a GPU device and `Assets` hold file bytes.

`profiling_does_not_move_the_state_hash` checks it rather than asserting it: two worlds, one profiled
and one with the profiler removed, run 180 ticks and must agree exactly.

**The residual risk is named rather than hidden.** A gameplay system *could* read the profiler
service and branch on a duration, which would make a replay diverge. Nothing structural prevents
that — it is true of every service, and the golden replays are the guard.

### 2. It is always on, not behind a feature

**An agent cannot feel a frame-rate problem.** `docs/04` §18 says so directly, and it is the whole
reason `profile.frame` is on the protocol's list. A profiler compiled out of the build a game ships
would report on a build nobody runs, and would make the agent's introspection depend on how a game
was compiled rather than on what the engine is — which is the wrong side of invariant I5.

The cost is two clock reads per system per tick. Measured in `games/atrium`: the whole simulation
tick is 8.3 µs in release across four systems, so the timing overhead is a low-single-digit
percentage of a number that is itself 0.05% of a frame.

A world with **no** profiler installed pays nothing — the schedule checks once per stage, not once
per system — which keeps this honest for a `World` assembled by hand rather than through `App`.

### 3. Both the mean and the worst run are kept

A system averaging 0.1 ms with a 9 ms spike drops a frame every time it spikes, and an average hides
that completely. Measured in the Atrium: `step_physics` averages 4.4 µs and its worst single run was
102.8 µs, 23× its own mean.

Kept as a running total plus a count plus a maximum, rather than a list of samples — a list grows
without bound over a long session, and those are the two questions worth asking.

### 4. Times are reported, never asserted

`profile.frame` returns numbers. The gate-4 test prints a table. **Nothing fails CI on a timing
regression**, and that is deliberate:

- `CLAUDE.md` §6 forbids tests that depend on wall-clock.
- CI runners are shared and variable.
- `crates/amadeo-render/tests/sprite_throughput.rs` already settled this pattern for the sprite
  batcher, with the same reasoning.
- **A flaky performance gate is one people learn to ignore**, which is worse than not having one.
  This project has already spent two CI failures' worth of trust on flakiness it did not choose.

What *is* asserted is everything that is not a clock: scene complexity, run counts, and one
deliberately enormous ceiling at half a frame that catches an algorithmic collapse. Plus a loose
scaling ratio across four body counts an order of magnitude apart, which can tell a slow constant
from a bad complexity class where a single measurement cannot.

`docs/04` §18's ambition — "a regression is an automatic failure rather than a slow realization" — is
right, and it needs dedicated hardware and a baseline history. Until this project has those, the
honest version is a committed measurement anyone can re-run.

## Alternatives rejected

**A `profile` cargo feature, off by default.** Zero cost and zero risk in a shipped build, and no
wall-clock in the tick unless asked for. Rejected because the numbers would then describe a build
nobody ships, and because `profile.frame` appearing or vanishing depending on compilation flags makes
agent introspection a build-time property rather than an engine one.

**Whole-tick timing from outside the deterministic zone.** Provably safe — no clock inside the tick at
all — and enough to declare a budget. Rejected because it cannot answer *which system*, so
`docs/04`'s per-system budgets stay unbuilt and every investigation still starts by reaching for an
external profiler. The safety it buys is already bought by ADR 0009's split.

## Consequences

- **Wall-clock genuinely runs inside the tick loop now.** That is a real widening of what the
  deterministic zone contains, and it is safe by structure rather than by care. Worth remembering
  when reading trap 2, which is otherwise still correct.
- `profile.frame` joins the protocol, and `docs/04` §18's ✅ becomes true rather than aspirational.
- The gate's numbers live in `docs/10-frame-budget.md`, regenerated by
  `cargo test -p atrium --release --test frame_budget -- --nocapture`.
- **GPU execution time is still not measured.** The profiler covers systems, and CPU-side frame
  preparation is timed separately, but how long the GPU takes to run the commands it is handed needs
  timestamp queries the wgpu backend does not have. That gap is stated in `docs/10` rather than
  papered over.
