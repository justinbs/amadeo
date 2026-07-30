# ADR 0008 — Archetype ECS with safe type-erased columns

**Status:** Accepted · **Date:** 2026-07-30

## Context

`amadeo-ecs` is the most-used code in the engine and everything downstream depends on its shape. Three
constraints pull against each other:

1. **Data-oriented, structure-of-arrays storage** (goal G4) — components of one type stored
   contiguously, so systems iterate cache-friendly slices.
2. **`unsafe_code = "forbid"`** (ADR 0002) — the workspace forbids unsafe outside explicitly audited
   modules.
3. **Legible to a Rust-learning human** (`CLAUDE.md` §6, a hard requirement) — Justin must be able to
   read and fix this code.

The conventional high-performance approach conflicts with 2 and 3. Production archetype ECS
implementations store columns as raw byte buffers (`Vec<u8>` plus a layout and a drop function) and use
unsafe pointer casts to view them as `&[T]`. Fast, and effectively unreadable without deep Rust
knowledge.

## Decision

**Archetype-based storage where each column is a concrete `Vec<T>` behind a safe trait object,
downcast once per archetype per query.**

```
World
 └── archetypes: Vec<Archetype>              // one per unique component set
      └── columns: BTreeMap<ComponentId, Box<dyn Column>>
           └── TypedColumn<T> { values: Vec<T>, changed: Vec<Tick> }
```

- `Box<dyn Column>` is downcast to `&TypedColumn<T>` via `Any`, yielding a real `&[T]`.
- **The downcast happens once per (archetype, query) pair — not per entity.** The inner loop is plain
  slice iteration over contiguous typed memory.
- `BTreeMap` rather than `HashMap` for column and archetype lookup, so iteration order is deterministic
  (invariant I3).

So the cost is O(number of archetypes) per query, while the benefit — contiguous typed slices in the
hot loop — is O(number of entities). The ratio is overwhelmingly favourable: a query over 10,000
entities spread across 5 archetypes performs 5 downcasts.

## Rationale

1. **We get the cache behaviour that actually matters.** SoA layout is preserved exactly; each
   component type is a contiguous `Vec<T>`. The thing data-oriented design buys — sequential access in
   the inner loop — is fully intact.
2. **Zero unsafe.** No pointer arithmetic, no manual drop handling, no layout computation. The class of
   bug that is hardest to debug and most dangerous in generated code simply does not exist here.
3. **Readable.** `TypedColumn<T> { values: Vec<T> }` is comprehensible to anyone who knows what a `Vec`
   is. That directly serves the legibility requirement, which is not negotiable.
4. **The cost is measurable and bounded**, and it is in the right place — per-archetype, not per-entity.
5. **Reversible.** The public query API does not expose the storage representation. If profiling ever
   shows the dispatch matters, the internals can be replaced behind the same API. This is exactly the
   "measure before committing" posture `04-subsystems.md` §3 asked for.

## Consequences

- One virtual call and one type check per archetype per query. Negligible relative to the work done.
- `Component` requires `'static` (needed for `Any`). Standard for ECS, no practical restriction.
- Archetype fragmentation remains the real performance risk, exactly as flagged in `04-subsystems.md`
  §3 — not the downcast. **Measure fragmentation with a realistic component mix before optimising
  anything else.**
- Adding or removing a component moves an entity between archetypes, which is a row copy. Structural
  changes therefore go through deferred command buffers (already required by ADR 0005 for deterministic
  merge order), so this cost is batched.

## Rejected alternatives

**Raw byte columns with unsafe casts.** The conventional fast approach, as used by mature Rust ECS
implementations. Rejected on constraints 2 and 3: it would require carving out an audited unsafe module
in the single most-read crate in the engine, and would make that crate opaque to one of its two authors.
The performance difference does not justify that, and if profiling ever proves otherwise, this ADR can
be superseded with evidence rather than anticipation.

**Sparse sets per component type** (EnTT-style). Simpler than archetypes and fully safe. Rejected
because multi-component queries — the common case in every real system — require intersecting sparse
sets and produce scattered access, losing the contiguity that motivated data-oriented design.

**`Vec<Option<T>>` per component type, indexed by entity.** Simplest possible design. Rejected on memory
waste (one slot per entity per component type, whether present or not) and on iteration cost, since
queries must skip `None` holes.

**Start simple, add archetypes later.** Tempting for schedule reasons. Rejected because the query engine
would be written twice, and query iteration semantics are the part everything else depends on.
