# ADR 0009 — Simulation resources and engine services are separate, enforced by the type system

**Status:** Accepted · **Date:** 2026-07-30

## Context

`World` needed somewhere to keep single-instance global state: the simulation RNG, event queues, a
score. These became **resources**, and resources are included in `World::state_hash` so they
participate in golden replay assertions (ADR 0005). That is correct for all three examples.

The problem surfaced while writing the determinism test suite. A test asserted invariant I7 — that a
headless run and a windowed run reach identical simulation state — using a `RenderCount` resource
incremented by a render-stage system to prove rendering had actually happened.

**The test failed, and it was right to.** `RenderCount` was a resource, so it entered the state hash,
so the windowed run (count 120) legitimately disagreed with the headless run (count 0).

The `Resource` documentation already said engine machinery "lives in the app layer instead" — but no
such place had been built. The gap was real: there was nowhere to put a GPU device, an asset cache, an
audio mixer, or a frame counter. Everything reachable from a system had to go through `World`, and
everything in `World` was hashed.

This matters immediately, not eventually. M1 adds the asset cache and M2 adds the wgpu device; both
must be reachable from systems and neither can be hashed.

## Decision

**Two stores in `World`, with two traits, and the distinction enforced by trait bounds rather than by
discipline.**

| | `Resource` | `Service` |
|---|---|---|
| Holds | simulation state | engine machinery |
| Requires `StableHash` | **yes** | **no** |
| In `state_hash` | yes | **no** |
| Examples | `SimRng`, `Events<T>`, score, world clock | GPU device, asset cache, audio mixer, frame counter, file handles |

Both are reachable from a system through `&mut World`, so nothing is lost ergonomically. Accessors
mirror each other: `insert_resource`/`insert_service`, `resource`/`service`, and so on.

**The enforcement is the point.** `Resource` requires `StableHash`; `Service` deliberately does not.
A `wgpu::Device` therefore *cannot* be filed as a resource — it cannot implement the required trait.
The invariant is checked by the compiler, not remembered by whoever is writing the code at the time.

## Rationale

1. **It preserves I7 structurally.** A windowed run creates render-side state that a headless run
   never does. With one store, keeping those two in agreement would depend on every future author
   remembering which globals are "real" state. With two, misfiling is a type error.
2. **It keeps `World` fully hashable.** Everything in `resources` can be fingerprinted, so
   `state_hash` needs no allow-list, no opt-out attribute, and no per-type exceptions — all of which
   would be places for a mistake to hide.
3. **The naming carries the meaning.** "Is this simulation state, or is it machinery?" is a question
   with a clear answer for essentially every candidate, and the answer picks the trait.
4. **Discovered by a test rather than by a bug.** The determinism suite caught a design gap before
   any dependent code existed. That is the intended behaviour of the harness and worth noting as
   evidence it earns its keep.

## Consequences

- Two near-identical accessor sets on `World`. Mild duplication, accepted for the clarity.
- Authors must choose. The choice is usually obvious, and the compiler catches the common error
  (putting an unhashable thing in `resources`). The *reverse* error — filing genuine simulation state
  as a service — is **not** caught by the compiler and would silently exclude that state from replay
  assertions. Mitigated only by the naming and by review, and worth watching for.
- Snapshots and save/load will serialise resources and skip services, which is the correct split for
  both. A save file should not contain a GPU device handle.
- Services are not currently `StableHash`, so they cannot be diffed by the agent introspection layer.
  If that turns out to matter for debugging, add a separate optional describe/inspect trait rather
  than folding them back into the hash.

## Rejected alternatives

**One store with an opt-out attribute** (`#[not_simulation_state]` or similar). Fewer concepts.
Rejected because the default would be wrong for machinery and the opt-out would be easy to forget —
and forgetting it silently breaks I7, which is the hardest class of bug to notice in this project.

**Keep services out of `World` entirely, in `App`.** Architecturally tidiest, and what the original
`Resource` documentation implied. Rejected because systems receive only `&mut World`. Reaching `App`
state would mean a second system signature and a scheduler that knows about both — a large complexity
increase to avoid one extra map.

**Interior mutability or globals for machinery.** Rejected: hidden shared state is exactly what makes
execution order implicit, which ADR 0005 rules out.

**Fix the test instead.** Tracking the render count in a captured `Rc<Cell<u32>>` inside the test
closure would have made the failure go away in about a minute. Rejected because the failure was
pointing at a missing piece of the architecture, not at a bad test — and the missing piece was needed
two milestones from now regardless.
