# ADR 0024 — A component's id is a compile-time constant

**Status:** Accepted · **Date:** 2026-08-02 · **Resolves:** Q16

## Context

ADR 0017 decided that a `ComponentId` is the FNV-1a hash of a component's **canonical name** rather
than its Rust path, so that moving a type between crates stops being a silent, replay-invalidating
change. That decision is right and this ADR does not touch it.

What it did not consider is *how often that hash gets computed*.

`ComponentId::of::<T>()` called `Reflect::type_name()`, which returns a freshly allocated `String`,
and then hashed every byte of it. And it sits on the hot path of every component access: `World::get`,
`World::insert`, and every query call it to locate a column.

**Found by measurement, not by reading.** Benchmarking the sprite batcher (ADR 0023) produced a
number that did not add up: collecting 20,000 sprites took 5.1 ms — about 31% of a 60 Hz frame — for
work that is a handful of arithmetic per sprite. Removing the batcher's own trigonometry moved the
total by 4%, which ruled out the obvious suspect and pointed here. Each sprite does two
optional-component lookups (`SortOrder` and `GlobalTransform`), so a frame was allocating and hashing
`"SortOrder"` and `"GlobalTransform"` forty thousand times.

## What other engines do

**Bevy** assigns each component a **dense integer index** when it is registered with the `World`, and
stores a `TypeId → ComponentId` map for lookup. Ids are small, comparison is an integer compare, and
nothing is hashed per access.

That is not available here, and deliberately so: a registration-order index is not stable across
builds or across processes, and ADR 0017 needs a component's id to be the *same value* the scene file
and the agent protocol talk about. Amadeo's id has to be a function of the name.

But the useful half of Bevy's approach transfers exactly — **the id should be computed once, not per
access.** Bevy computes it at registration; Amadeo can do better and compute it at compilation,
because the name is a literal in the source.

## Decision

**`Reflect` gains two associated constants, and a component's id becomes a compile-time constant.**

```rust
pub trait Reflect: Sized + 'static {
    /// The canonical name as a constant, or "" when only known at runtime.
    const STATIC_NAME: &'static str = "";

    /// FNV-1a of STATIC_NAME, computed at compile time. Never override.
    const STATIC_NAME_HASH: u64 = amadeo_core::StableHasher::hash_str(Self::STATIC_NAME);

    fn type_name() -> String;
    // ...
}
```

`#[derive(Reflect)]` fills in `STATIC_NAME` for every struct and enum, honouring
`#[reflect(name = "...")]`. `STATIC_NAME_HASH` has a default derived from it, so setting the name is
the only thing a type does and the two cannot disagree.

`StableHasher::hash_str` is a new `const fn` — plain FNV-1a over the bytes, which is exactly what
`write_str` already did. `ComponentId::of::<T>()` becomes a constant load.

### The fallback is real, not defensive

A type whose `STATIC_NAME` is empty takes the old path. That covers the generic `Reflect` impls —
`[T; N]`, `Option<T>`, `Vec<T>` — whose names are built from their parameters and cannot be a single
constant.

Nothing that reaches `ComponentId` is generic today, because a component is a struct. The branch is
what makes that a *checked fact* rather than an assumption that fails strangely later.

## The measurements

Release build, AMD Ryzen 7 5700X3D. `cargo test -p amadeo-render --test sprite_throughput --release
-- --nocapture`.

| Scene | Before | After | Change |
|---|---|---|---|
| 20,000 sprites, 8 textures, 4 layers | 5.13 ms (31% of a frame) | **3.32 ms (20%)** | −35% |
| 50,000 sprites, one tilesheet | 11.55 ms | **6.77 ms** | −41% |

This is a whole-engine improvement rather than a rendering one. Every `World::get`, every
`World::insert`, and every query pays the same cost, so anything that touches components got faster —
the sprite batcher is simply where it was visible enough to catch.

**Byte-identical ids, verified.** The whole test suite including both golden replays passes
unchanged, and `amadeo replay` still matches all four checkpoints in a fresh process. That is the
assertion that matters: a different hash here would have changed every `ComponentId`, every state
hash containing a component, and every committed replay at once.

## Consequences

- **Every `Reflect` implementation now has a name available as a constant**, which is a small new
  obligation on hand-written impls and free for derived ones. Anything that skips it still works.
- **`StableHasher::hash_str` must stay identical to `write_str` + `finish`**, or ids move silently.
  Pinned by `const_hash_agrees_with_the_hasher`, which checks both against a list of names including
  the empty string.
- **The remaining sprite cost is now the archetype lookups themselves**, not the id. At 20,000
  sprites the batcher still does 40,000 individual entity lookups because the ECS has no
  optional-component query. See below.
- **A `static` cache inside a generic function is not a fix and must not be reintroduced.** It was
  tried first: a `static` declared inside `of::<T>()` is shared across every monomorphisation rather
  than instantiated per type, so every component collapsed onto one id. The archetype tests caught it
  instantly. Recorded in the function's own docs so the next person does not repeat it.

## What this does not fix, and what should come next

The research that informed this ADR turned up a second, larger finding that is deliberately left
alone here.

Archetype ECSs are fast because **a query matches whole archetypes, not individual entities** — the
column is resolved once per archetype and then iterated contiguously. Bevy caches that matching in
`QueryState` precisely because gathering archetypes is expensive.

Amadeo's batcher cannot do that for `SortOrder` and `GlobalTransform`, because both are **optional**
and the ECS has no way to express an optional component in a query. So it falls back to
`world.get::<T>(entity)` per entity, which is the anti-pattern this whole class of engine exists to
avoid. That is now the dominant remaining cost, and it is an ECS feature rather than a renderer fix:
an `Option<&T>` query term that resolves a column per archetype and yields `None` for archetypes
lacking it.

Worth doing before the sprite batcher reaches the GPU, and worth doing for its own sake — the
session-7 target games (Stellaris, Terraria, RimWorld) made ECS throughput a requirement rather than
a preference (`docs/00-vision.md` § Divergent).

## Rejected alternatives

**Dense integer ids assigned at registration, as Bevy does.** Fastest possible comparison, and no
hashing anywhere. Rejected because the id would no longer be a function of the name, which is exactly
what ADR 0017 bought: the ECS's identity and the scene file's identity are currently the same string,
and `amadeo describe` reports ids that mean something across processes. A registration-order index is
none of those things.

**A runtime cache keyed by `TypeId`.** A `RwLock<HashMap<TypeId, u64>>` consulted per lookup. Rejected
because it replaces an allocation with a lock and a hash lookup, which is a smaller win for more
machinery — and because `TypeId` is not build-stable, so it would need care to avoid leaking into
anything ordered (invariant I3).

**Making `type_name()` return `&'static str` instead of `String`.** Simpler in appearance, and removes
the allocation. Rejected because the generic impls genuinely need to build their names at runtime —
`array<f32, 2>` is composed from its parameters — so the signature cannot be narrowed without
special-casing them anyway. Adding a constant alongside leaves those impls untouched.
