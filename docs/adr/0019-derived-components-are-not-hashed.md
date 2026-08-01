# ADR 0019 — Derived components are excluded from the state hash

**Status:** Accepted · **Date:** 2026-08-01

## Context

`GlobalTransform` — a child's transform composed with its parents' — has been waiting since ADR 0015,
blocked on knowing what a transform is. ADR 0018 settled that. Building it surfaces a question the
ECS has never had to answer.

**Every component is currently part of `World::state_hash`.** It walks each archetype's columns and
hashes every row. There is no way to opt out, and until now nothing wanted one: every component held
authored or simulated state, and I3 says that state is exactly what a replay asserts on.

`GlobalTransform` is different. It is **derived**: recomputed every tick from `Transform` and
`Parent`, carrying no information those two do not already contain. Hashing it would mean:

- **Every replay becomes sensitive to matrix arithmetic.** A change to how a rotation is composed —
  or a different instruction set evaluating the same expression — moves the hash of a value that is
  not simulation state. The failure looks exactly like a real regression.
- **The hash gets no new information.** The inputs are already hashed. A derived value agreeing with
  its inputs is a tautology; a derived value *disagreeing* is a bug in the propagation system, which
  is what its own tests are for.

Justin decided the question directly: `GlobalTransform` should not be in the state hash. This ADR
records the mechanism, because the ECS has no way to express it and the way it gets expressed is
easy to misuse.

## Decision

**`Component` gains an associated constant `DERIVED`, defaulting to `false`. Columns of derived
components are skipped by `World::state_hash`.**

```rust
pub trait Component: 'static + Send + Sync + fmt::Debug + StableHash + Reflect {
    /// Whether this component is recomputed from scratch every tick.
    const DERIVED: bool = false;
}
```

Mechanically: `Column` gains `fn is_derived(&self) -> bool`, `TypedColumn<T>` returns `T::DERIVED`
(monomorphised, so the flag is available through the type-erased trait object), and `state_hash`
skips those columns.

### Why `DERIVED` and not `HASHED`

Naming is the whole safety story here. `HASHED: bool = false` says *what it does* and invites anyone
who wants a quieter diff to reach for it. `DERIVED` says *what must be true* — and the correctness
rule follows from the name without having to be memorised: **set it only if the value is recomputed
from scratch every tick from other components.** If a system ever writes it and expects the value to
survive to the next tick, it is not derived and must not be marked so.

The hazard being guarded against is precise: marking real simulation state as derived silently drops
it from every replay assertion, and nothing fails. That is the same failure mode
`#[derive(StableHash)]` exists to prevent one level down — a hand-written hash that forgets a field
still compiles and still produces a plausible number.

### Why not the other two options

**Store it outside the ECS**, in a side table keyed by entity. Genuinely excluded from the hash by
construction. Rejected because the renderer needs it *in a query*, alongside `Quad` and `SortOrder`,
and a side table means every consumer does a lookup per entity instead of walking a column. It also
puts a second entity-indexed store next to the one the ECS already provides, which is exactly the
kind of parallel structure ADR 0015 avoided with `Children`.

**Recompute on demand** rather than storing anything. No storage, no hash, no new trait surface.
Rejected because it is O(depth) at every read, every frame, for every consumer — and the renderer,
physics, and culling will all read it.

### Why a flag rather than a second store

ADR 0009 solved the analogous problem for globals by splitting `Resource` (hashed) from `Service`
(not hashed) into two stores. That worked because nothing queries resources and services together.

Components are different: **archetypes are the storage**, and splitting them would mean an entity's
components live in two places and every query touching both pays for it. One store with a per-type
flag is the shape that fits.

## Consequences

- **The float-determinism pressure comes off the matrix maths.** `GlobalTransform` cannot move a
  replay hash, so the composition arithmetic is free to be whatever is clearest.

  **With one caveat that must be written down:** a gameplay system that reads `GlobalTransform` and
  writes the result back into a `Transform` (or any other hashed component) *reintroduces* float
  sensitivity through the back door. That is legitimate — "put this child where its parent's hand
  is" is a real thing to want — but it means the arithmetic still deserves to be deterministic. Hence
  plain scalar float maths, hand-written, rather than anything SIMD-accelerated.
- **`amadeo-math` is still not created, and glam is still not a dependency.** Propagation needs a 4×4
  compose-and-multiply and nothing else: about eighty lines of unremarkable scalar arithmetic, living
  in `amadeo-transform` next to its only caller. `amadeo-math` wrapping glam (`docs/02-tech-stack.md`)
  is a larger job with a wider surface, and it should be designed when something needs that surface —
  not reverse-engineered from the first caller.
- **The escape hatch exists now, so it can be misused.** `DERIVED` is documented at the trait, and the
  only type setting it today is `GlobalTransform`. A second one should be argued for.
- Golden replays are unaffected by adding `GlobalTransform` — which is the point, and is asserted by
  a test rather than assumed.

## Rejected alternatives

**Hash it anyway, and accept the coupling.** Simplest possible rule: everything is hashed, no
exceptions, no trait surface, no misuse risk. Genuinely tempting — the strength of I3 comes partly
from having no exceptions. Rejected because it makes every replay assert on a value that carries no
information, and turns any change in matrix composition into a false regression across every recorded
replay in the project. The exception is narrow and the rule for it is checkable.

**A marker component (`NotHashed`) rather than a trait constant.** Data rather than a type-level
flag, and inspectable at runtime. Rejected because it is per-*entity* where the property is per-*type*
— an entity could carry `GlobalTransform` without the marker and silently change the hash — and
because it would cost an archetype migration to express something that is a fact about the type.
