# ADR 0007 — Fixed timestep is 60 Hz

**Status:** Accepted · **Date:** 2026-07-30 · **Resolves:** Q5

## Context

ADR 0005 requires a fixed simulation timestep. The rate itself was left open as Q5, flagged P1 because
**changing it later invalidates every recorded replay** — replays are keyed to a tick rate, and golden
replay tests are the project's primary behavioural regression mechanism. So this must be settled before
any simulation code exists.

Candidates were 60 Hz (conventional), 120 Hz (better physics fidelity and input latency at double the
cost), and decoupling the physics substep rate from the logic tick rate.

## Decision

**Logic tick rate: 60 Hz.** `FIXED_DT = 1/60` second, expressed exactly.

**Physics substeps are configurable** per project, defaulting to 1 substep per logic tick. A game
needing better collision fidelity raises the substep count without changing the logic tick rate, so
replays stay valid.

`FIXED_DT` is defined once, in `amadeo-core`, as an exact rational-derived constant — never as a
recomputed float and never configurable at runtime. A game may not change it.

## Rationale

1. **60 Hz is enough for every target game.** Palworld, Schedule I, and a horror slice are not
   frame-perfect competitive games. None needs 120 Hz simulation.
2. **Half the simulation cost of 120 Hz**, which matters because the deterministic zone is
   single-threaded by default (ADR 0005 restricts parallelism to provably-disjoint access).
3. **Rendering is already decoupled**, so display smoothness is not tied to this number. A 144 Hz
   monitor still renders at 144 Hz with interpolation between simulation ticks.
4. **Substeps solve the fidelity case** without touching the replay-relevant rate. This is the standard
   answer and it keeps the two concerns independent.
5. **Conventional**, so the number matches the majority of documentation, tutorials, and intuition that
   both authors bring.

## Consequences

- Input latency is up to ~16.7 ms of simulation granularity. Acceptable for these genres; not
  acceptable for a fighting game, which we are not making.
- **Every recorded replay embeds the tick rate in its header.** A replay recorded at a different rate
  must be rejected with a clear error rather than silently replayed wrong.
- Fast-moving objects may need substeps or continuous collision detection to avoid tunnelling. rapier
  supports CCD; this is a per-project tuning matter, not an engine-wide one.
- If 120 Hz is ever genuinely needed, it is a breaking change requiring re-recording every golden
  replay. Treat it as one, with a new ADR.

## Rejected alternatives

**120 Hz.** Better fidelity and latency. Rejected as paying double simulation cost for a benefit none
of the target games need.

**Configurable per project.** Superficially flexible. Rejected because it makes replays non-portable
across projects and makes the engine's own golden replay tests ambiguous — the tick rate would become
a variable in every determinism assertion. The substep knob covers the real use case without this cost.

**Variable timestep.** Already rejected outright in ADR 0005; incompatible with determinism.
