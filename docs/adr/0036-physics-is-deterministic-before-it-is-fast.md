# ADR 0036 — Physics is deterministic before it is fast

**Status:** Accepted · **Date:** 2026-08-05 · **Resolves:** Q24 · **Builds on:** ADR 0002, ADR 0005,
ADR 0009, ADR 0017

## Context

`amadeo-physics` does not exist, and two of M2's four exit gates depend on it. Gate 1 wants a
physics-driven character controller; **gate 3 wants a physics-heavy replay of 200+ bodies to
reproduce bit-identically across runs and processes**.

Physics is the largest body of floating-point arithmetic this engine will ever run, and **invariant
I3 is the keystone** — `CLAUDE.md` says so directly, and lists what it buys: replay-as-test, headless
verification, snapshots, save/load, and time-travel debugging. So the question is not a performance
tuning detail. It decides whether the project's entire testing story survives contact with its own
physics.

Raised as **Q24** when Justin asked whether the engine needs a physics engine at all — a question
whose answer turned out to be "yes, and in this milestone".

## What the research found

**Rapier can do exactly what gate 3 asks.** Its `enhanced-determinism` feature gives bit-level
cross-platform determinism: serialise the state after N steps on two machines with different CPUs and
operating systems, and the bytes match. Not "close enough" — identical.

Three conditions come with it, and they are the decision:

1. **`enhanced-determinism` cannot be enabled alongside `parallel`, `simd-stable` or `simd-nightly`.**
   Determinism and multi-threaded-or-vectorised physics are mutually exclusive. There is no
   configuration that takes both and no way to defer by taking both now.
2. **The target must comply strictly with IEEE 754-2008.** Fine for desktop, and fine for WASM —
   which matters twice over, since WASM is M5's web export *and* ADR 0011's reserved modding hatch.
3. **Determinism holds for one rapier version.** An upgrade may legitimately change results, which
   invalidates every committed replay containing physics.

**And the obvious compromise is not available, for a Cargo-specific reason.** Features are unified
across a build: if one game in this workspace asked for fast physics and another for deterministic
physics, `cargo test --workspace` would enable both at once — which rapier forbids. So this cannot be
a per-game setting in a repository that already has two games. It is one choice for the engine.

## Decision

### 1. `enhanced-determinism` is on, permanently

Physics is single-threaded and scalar. That is a property of the engine rather than a default a game
may override, and the mutual exclusion above is what makes "may override" incoherent anyway.

The argument is I3 and nothing else. Every mechanism this project has built its verification on —
golden replays, `amadeo replay` in a separate process, snapshots, the determinism CI job — assumes
that the same inputs produce the same state. Physics that does not honour that does not weaken those
mechanisms a little; it makes them silent about the largest thing in the simulation.

**The failure mode of the alternative is what settles it.** A replay that passes on the author's
machine and fails on someone else's does not look like a physics configuration problem. It looks like
a bug in the game, and it is close to unattributable — which is exactly the class of failure ADR 0005
exists to make impossible.

### 2. Physics state is in the state hash

Rigid body positions, orientations and velocities are simulation state and are hashed like any other.
This is what makes gate 3 meaningful, and it follows from 1 rather than being a separate choice:
determinism is only worth paying for if something checks it.

The alternative — physics as a `Service`, excluded from the hash the way asset caches are (ADR 0009)
— was considered and does not survive contact with gate 1. A physics-driven character controller
means the **player's position comes from physics**, and position is gameplay state that everything
else reads. Excluding it would mean a replay proved almost nothing about any game that used physics.

### 3. The rapier version is pinned exactly, and an upgrade is a deliberate replay regeneration

An `=x.y.z` dependency rather than a caret range. An upgrade is then a visible, intentional act with
a known consequence, handled under the procedure in `docs/07` — isolate the cause, confirm the hashes
return when reverted, then regenerate.

This is the same shape as ADR 0017's identity changes, with one difference worth stating: the trigger
is a *dependency* rather than something in this repository, so nothing here will remind anyone. The
pin is what turns "results moved mysteriously" into "we upgraded rapier".

### 4. No rapier type crosses the trait boundary

ADR 0002 already says rapier sits behind engine-owned traits. This makes the boundary testable rather
than aspirational: **no rapier type may appear in a component, a scene file, a snapshot, or the state
hash.** If one does, the wrapper is not a boundary — it is a re-export, and the version pin in 3
becomes load-bearing in places nobody expected.

## Consequences

**Good:**

- Gate 3 is achievable as written, rather than needing to be reworded to match what was built.
- I3 stays whole, so every existing verification mechanism keeps working when physics arrives —
  golden replays, separate-process replay, snapshots, the determinism job.
- M6's client-server prediction gets much easier. It is *not* deterministic lockstep (ADR 0006), but
  a client and server that compute the same result from the same inputs need no reconciliation for
  physics, which is the hardest part of prediction to get right.
- WASM is covered, so neither web export nor a future mod sandbox has a physics exception.

**Bad, and accepted:**

- **Physics never uses more than one core, and never uses SIMD.** On an eight-core machine that is
  most of the CPU left on the table, on the subsystem most likely to become the frame-time limit.
- **The target list contains cases that will feel it.** RimWorld and Project Zomboid are
  large-simulation games; a few hundred bodies is comfortable and a few thousand may not be.
- **Gate 4's frame-time budget must be set knowing this**, and measured rather than hoped for. If
  physics is the limit, the answers are fewer bodies, better broad-phase culling, or sleeping
  inactive bodies — **not** relaxing this decision.
- **Effectively irreversible once replays containing physics exist**, which is the point and also the
  risk. It is why this is decided before the crate rather than after.

## What was rejected

- **Parallel and SIMD physics, with physics excluded from the state hash.** Genuinely faster by a
  large factor, and it would keep rapier upgrades free. Rejected because it is not really excludable:
  a physics-driven character's position *is* gameplay state, so either the hash covers it and is
  nondeterministic, or it does not and a replay proves almost nothing about the game. This is the
  option that quietly guts the testing story rather than visibly narrowing it.
- **A per-game feature switch.** The reasonable-sounding compromise, and unavailable: Cargo unifies
  features across a build, so two games in one workspace cannot disagree about a feature that rapier
  forbids combining.
- **Deferring until something can be measured.** How ADR 0011 and ADR 0023 were both settled well,
  and it had a real case here. Rejected because the trade is not primarily about speed — it decides
  whether physics state is hashed, which shapes the trait surface, the snapshot format and the replay
  fixtures. A layer built before deciding tends to bake in the wrong assumption, and `CLAUDE.md`'s
  trap list puts retrofitting determinism first for exactly that reason.
