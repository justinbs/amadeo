# ADR 0011 — Game logic is Rust; WASM is the reserved escape hatch

**Status:** Accepted · **Date:** 2026-07-31 · **Resolves:** open question Q1

## Context

Q1 has been the highest-priority unresolved question since session 1, and it blocks M1. It asks how
gameplay code is authored and how fast a change becomes visible — which sets the ceiling on how
useful an agent can be on this project.

The question was framed around a specific fear, quoted from `docs/06-open-questions.md`:

> If changing an enemy's speed requires a 30-second engine rebuild, iteration dies — tolerable for a
> human, corrosive for an agent doing twenty iterations to tune one behavior.

A prior was recorded at the same time — embedded Luau for gameplay, plus the ability to "graduate"
hot logic into Rust systems — and explicitly marked as a hypothesis to be overridden by evidence.

**The spike ran. The evidence overrides it.** Method, raw numbers, and re-run instructions are in
`spikes/q1-game-logic/README.md`; the prototypes are committed alongside.

All four candidates ran the same benchmark: a three-state enemy AI over 64 entities for 1800 ticks,
differing only in how the AI system is authored and reloaded. Agreement between candidates is
measured by `World::state_hash`, so "these compute the same simulation" is a fact rather than an
impression.

### What was measured

Ryzen 7 5700X3D, Windows 11, rustc 1.97.1, 2026-07-31.

| | edit → observe | state survives reload | hash vs native Rust | µs/tick (release) |
|---|---|---|---|---|
| **A** pure Rust | **0.95 s** (2.1 s in the real game) | no | reference | 4.6 |
| **B** cdylib | **0.69 s** | yes | ✅ identical | 4.6 |
| **C** Luau | **0.4 ms** | yes | ❌ **differs** | 109.7 (24×) |
| **D** WASM | **0.63 s** | yes | ✅ identical | 5.7 (1.24×) |

### The three results that decided it

**1. The premise was wrong at this scale.** A one-line gameplay edit rebuilds in **0.9 s** in a game
crate and **2.0 s** in `quad-demo`, which links wgpu and winit. Editing a core engine crate and
rebuilding everything downstream is **3.2 s**. The 30-second rebuild the question was written to
avoid does not exist, because the crate graph is small and shallow — which `CLAUDE.md` §4 named as
the mitigation and which turns out to have worked.

**2. Luau does not compute what Rust computes.** Luau numbers are `f64`; components are `f32`. The
two implementations agree at tick 1 and diverge at tick 2, permanently. Luau is *not*
nondeterministic — five separate processes produced the same hash every time — it is simply a
different computation.

This is fatal to the recorded prior specifically. The prior's value came from graduating hot logic
from Luau into Rust once it mattered. **Graduating a system changes its numbers, which invalidates
every golden replay recorded before the move.** The mechanism that made Luau attractive is the
mechanism that breaks.

**3. Luau's cost is the binding, not the language.** Running the benchmark with a do-nothing script
isolates it: 84.0 µs/tick is marshalling, leaving **22.9 µs for Luau to execute the AI** — the same
order as native Rust's entire 4.6 µs tick, which for an interpreted language is a good showing.
Recovering that overhead means building a userdata-based binding, which is precisely the "every
engine API must be bound and kept in sync" tarpit `docs/04-subsystems.md` §10 warns about.

## Decision

**Game logic is written in Rust, compiled into the game, with no scripting layer and no dynamic
reload. `amadeo-script` is not built in M1.**

**WASM (candidate D) is the reserved escape hatch**, pre-selected so this question is not
re-litigated from scratch. It is adopted if — and only if — a measured threshold is crossed:

| Trigger | Threshold |
|---|---|
| A one-line gameplay edit's rebuild-to-runnable | sustained above **5 s** |
| Getting back to the state of interest after a restart | sustained above **2 s** *after* snapshots exist |

Re-run `spikes/q1-game-logic/measure.ps1` to check. Numbers, not impressions.

**Two supporting decisions:**

- **A's real weakness is re-simulation, not compilation, and snapshots fix it.** Restarting and
  re-simulating costs 47 ms to reach 30 s of simulated time, 382 ms to reach 5 minutes — linear, and
  the only cost that grows with session length. `snapshot.take` / `snapshot.restore` are already in
  M1's `amadeo-agent` scope. **They are hereby the highest-value iteration-loop investment in M1**,
  ahead of any reload mechanism. Restoring to tick N beats re-simulating to tick N, and it helps
  every candidate including this one.

- **Luau is not rejected everywhere — only inside the deterministic zone.** Its numeric divergence
  only matters where `state_hash` matters. Menu behaviour, quest triggers, dialogue, and tuning
  tables sit outside the simulation and could reasonably be Luau at M3, where its 0.4 ms reload is a
  real gain and its precision is irrelevant. That is a future ADR, not a commitment here.

## Rationale

1. **Do not pay a permanent architectural cost for a problem that does not exist.** Every
   alternative buys back 0.3–1.5 s per iteration and charges for it forever: B in `unsafe` and an
   unenforced layout contract, C in determinism-equivalence and a 24× ceiling, D in a 200-crate
   dependency and a hand-maintained boundary. At 0.9 s measured, none of those trades is currently
   worth making.

2. **A has the best schema story available, by a distance.** The Rust types *are* the schema. There
   is no second definition to keep in sync, no marshalling layer to teach about a new component, and
   `amadeo describe` (M1) can emit it straight from the reflection registry. Every other candidate
   introduces a boundary that every future component must be taught to cross — B duplicates the
   signature by hand, C duplicates field names as Lua strings, D duplicates a byte layout. That cost
   is invisible at one component and dominant at eighty.

3. **A is the only candidate that touches no invariant.** B requires `unsafe` in the host, and
   `unsafe_code = "forbid"` is not a style preference here — ADR 0008 chose an entire ECS storage
   design to preserve it. C and D both need a runtime that is not `Send + Sync` and therefore cannot
   live in the world at all (see Consequences).

4. **D is genuinely good, which is why it is the named successor rather than a rejected option.** It
   is bit-identical to native Rust — verified across two optimisation levels, because WebAssembly
   specifies `f32` exactly and LLVM does not reassociate floats without fast-math. It costs 24% at
   runtime, its boundary is memory-safe on both sides, and it is the same artefact M5's web export
   needs. If the threshold is ever crossed, this is the answer.

5. **The spike is committed, so the decision is falsifiable.** `measure.ps1` re-runs the whole
   comparison. A future session that thinks this ADR is wrong can produce evidence rather than an
   argument — which is the entire point of resolving Q1 this way.

## Consequences

- **M1 loses a planned crate and gains nothing to build.** `amadeo-script` is not created. The
  roadmap's "Game logic layer, per the Q1 decision" is satisfied by "game logic is a Rust module in
  the game crate", which is what `games/quad-demo` already does.

- **Snapshot/restore is promoted within M1** from a listed RPC method to the iteration-loop
  priority. Its acceptance test should be: restore to tick N is measurably faster than re-simulating
  to tick N, at N = 18 000.

- **Keeping the crate graph small and shallow is now load-bearing, not hygiene.** The measured
  rebuild times are the justification for this decision, and they degrade if the graph grows wide or
  deep. A new engine crate that pulls a heavy dependency into the common path is a decision that
  moves this ADR closer to being wrong.

- **`Service: Send + Sync` is now a known blocker, filed rather than fixed.** Neither `mlua::Lua` nor
  `wasmtime::Store` is `Sync`, so neither candidate could put its runtime in the world; both had to
  hide it in an `Rc<RefCell<..>>` captured by the system closure, where world introspection cannot
  see it. This does not bite today — but it will block an audio mixer and an asset loader in M3 as
  surely as it blocked these. Recorded in `docs/06-open-questions.md` as **Q12**.

- **The two-component query limit is confirmed as a real constraint.** The benchmark needed three
  components at once and could not express it, so every candidate collects into a `Vec` and writes
  back by handle. Under this decision that overhead is on the critical path of the *shipping* code,
  not just a benchmark artefact, which strengthens the case for widening queries in M1.

- **No new dependencies, no new toolchain requirements, and CI is unchanged.** The
  `wasm32-unknown-unknown` target was added to this machine for the spike; it is harmless, and M5
  needs it regardless.

## Rejected alternatives

**B — Rust game logic as a hot-reloaded cdylib.** The fastest thing that is also exact: 0.69 s,
bit-identical, 0% runtime cost, and state survived the swap perfectly. Rejected on safety, not
performance.

It requires `unsafe` in the host to resolve symbols and to pass `&mut World` across a boundary whose
layout nothing verifies. The exported `amadeo_abi_version` check catches a stale library — but not
the dangerous case: **edit a component's fields, rebuild only the library, and the host reinterprets
old memory as the new shape, silently.** For a project whose stated standard is that a bad error
message is a real defect (`docs/03` Pillar 5), a failure mode with no error message at all is the
wrong trade for 0.3 s.

Worth recording that it worked *because of* ADR 0008: `ComponentId` hashes the type name rather than
using `TypeId`, so the host and the library agreed on component identity across compilation units. A
`TypeId`-based design would have failed here silently — the downcast would return `None` and the AI
would have done nothing.

**C — embedded Luau.** The recorded prior. Rejected for the deterministic zone on the numeric
divergence above, and on a 24× runtime cost whose fix is a binding layer that is itself a named
project trap. Explicitly **not** rejected outside the simulation; see the Decision.

**D — WASM, adopted now rather than reserved.** Genuinely tempting: bit-exact, sandboxed,
memory-safe, 24% overhead, and it doubles as the M5 web path. Rejected *for now* only because it
buys 0.3 s against candidate A while adding ~200 crates, a second build step, a second target, and a
byte-layout contract that every future component must be taught. Adopting it today would be
optimising an iteration loop that is not currently slow. Reserved rather than rejected because if
that changes, it is the right answer and the spike already proves it works.

**Split the difference: Rust now, decide again at M3.** Rejected as a non-decision. The point of
resolving Q1 is to stop it being re-argued; a deferral without a threshold guarantees the opposite.
The threshold table above is what makes this a decision rather than a postponement.
