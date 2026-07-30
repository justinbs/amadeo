# ADR 0005 — Determinism is a hard invariant

**Status:** Accepted · **Date:** 2026-07-30

> The keystone decision. Almost every capability that makes Amadeo usable by an AI agent is downstream
> of this one.

## Context

An agent building a game must be able to answer *"did my change do what I intended, and did it break
anything that used to work?"* A human answers this by running the game and looking at it. An agent
cannot — and screenshots are expensive, ambiguous, and only tell you about one frame.

The mechanical answer is reproducibility. If the same inputs always produce the same state, then game
*behavior* becomes testable — and behavior is where game bugs live. Without determinism, only pure
functions are testable, which excludes essentially everything interesting.

Determinism must be designed in from the first tick. It is the most expensive property to retrofit in
this entire project, because every subsystem can silently violate it, and each violation is invisible
until something depends on it.

## Decision

**Determinism is invariant I3.** Same inputs plus same seed produce the same state, bit-identical,
across runs, processes, builds, and headless-vs-windowed execution.

### The time model

Fixed-timestep simulation, decoupled rendering:

```
loop {
    poll_platform_events();          // OS/window/raw input — NOT simulation
    accumulate_time();

    while accumulator >= FIXED_DT {  // ── deterministic zone ──
        sample_input_actions();      // from live device OR replay; indistinguishable
        run_schedule(PreSimulation);
        run_schedule(Simulation);
        run_schedule(PostSimulation);
        tick += 1;
        accumulator -= FIXED_DT;
    }                                // ── end deterministic zone ──

    run_schedule(Render(alpha));     // interpolated, variable rate, skippable
    run_schedule(Present);
}
```

### Rules inside the deterministic zone

1. **No wall-clock.** No `Instant::now()`, `SystemTime`, or frame delta. Read `tick` and `FIXED_DT`.
2. **No unordered iteration.** No `HashMap`/`HashSet` iteration in simulation paths. Use ordered maps.
3. **Seeded RNG only**, as a world resource. Systems needing randomness draw from a deterministic
   per-system or per-entity stream — never a shared mutable global, since that leaks system execution
   order into results.
4. **Explicit system ordering.** Labeled `before`/`after` constraints, never implicit registration
   order — otherwise determinism depends on module load order.
5. **Deterministic command merge.** Parallel execution is allowed only where the scheduler proves
   disjoint access, and deferred commands merge in a fixed order.
6. **Rendering never writes simulation state.**
7. **Asset load timing is not observable.** Simulation blocks on a declared asset set before a scene
   activates, so load completion order can never affect behavior.
8. **Input is sampled once per tick** from live device or replay stream; the simulation cannot tell
   which. This is precisely what makes replays work.

### Supporting machinery, built in M0

- **State hashing** — a stable hash over simulation state, excluding render caches and timings. This
  hash is the assertion in every golden replay test, so its definition is critical and must be pinned
  early.
- **Replay files** — a recorded action stream plus seed plus expected hashes at checkpoint ticks.
- **Snapshots** — full world serialization; the basis for save/load and time-travel debugging.
- **Golden replay test runner** in CI, from the first milestone onward.

### Headless equivalence (I7)

Headless mode drops `Render`/`Present` and runs as fast as it can. The simulation result must be
bit-identical to a windowed run. **There is a CI test for this**, because it's the property that makes
fast unattended verification possible.

### Scope of the guarantee, stated honestly

Committing to **same-machine, same-build reproducibility** now. Strict cross-platform determinism
additionally requires eliminating f32 variance across architectures and compilers, which realistically
means fixed-point math. Since Amadeo is Windows-first and single-platform for now, f32 is the right
call — but this is recorded so the future decision is informed rather than a surprise. Cross-platform
determinism would become necessary for deterministic-lockstep multiplayer or for verifying replays
across machines.

## Consequences

### What determinism buys

Every item here is an agent capability, which is the whole point:

| Capability | Mechanism |
|---|---|
| **Replay as behavioral regression test** | Record an input stream once, replay forever, assert the hash. Otherwise impossible for games. |
| **Fast headless verification** | 600 ticks with no GPU, hash, compare. Fast enough to run on every edit. |
| **Reproducible bug reports** | "It broke" becomes a replay file and a tick number. |
| **Save/load** | A save is a snapshot. Same machinery, no separate system. |
| **Time-travel debugging** | Snapshot every N ticks, rewind, single-step forward. |
| **Rollback netcode later** | Exactly the substrate it needs, already present. |

### Costs, accepted

- **Permanent discipline.** Every subsystem can leak nondeterminism, and every leak is silent. Guarded
  by golden replays in CI so a leak is caught within one commit rather than one milestone.
- **Throughput left on the table.** Parallelism is restricted to provably-disjoint access with ordered
  merges. Deliberate: a slower engine that reproduces is worth far more here than a fast one that
  doesn't.
- **Ordered collections in simulation paths**, which are slower than hash maps.
- **Q5 must be resolved in M0** — changing `FIXED_DT` later invalidates every recorded replay.
- **Physics must be verified, not assumed.** rapier's `enhanced-determinism` mode needs fixed iteration
  counts and ordered body insertion. Prove cross-run reproducibility with a test *before* building on
  it; this is a load-bearing assumption.

## Rejected alternatives

**Variable timestep with delta-time scaling.** Simpler, and what many engines do. Rejected outright: it
makes reproducibility impossible, which forfeits every capability in the table above. Non-starter.

**Determinism as an opt-in mode.** Attractive — full speed normally, reproducible when testing.
Rejected because a mode that isn't always on is always broken. The nondeterministic path becomes the
default, drifts, and the deterministic path silently rots.

**Fixed-point math from the start.** Would grant cross-platform determinism immediately. Rejected as
premature: significant ergonomic and performance cost for a guarantee no current requirement needs.
Revisit if multiplayer or cross-machine replay verification becomes a goal.
