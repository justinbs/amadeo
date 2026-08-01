# ADR 0017 — `ComponentId` comes from the canonical name, not the Rust path

**Status:** Accepted · **Date:** 2026-08-01 · **Resolves:** Q13

## Context

ADR 0008 chose to derive `ComponentId` from a type's *name* rather than `std::any::TypeId`, because
`TypeId` carries no stability guarantee across builds and using it as a map key would make archetype
ordering — and therefore state hashes — differ between compilations of identical logic. That
reasoning holds and is not being revisited.

What it did not consider is *which* name. The implementation used `std::any::type_name::<T>()`, which
is the **fully-qualified path**. That couples a component's identity to where its code lives.

This was found in practice, not in theory. ADR 0015 moved `Transform2d` from
`amadeo_render::components` to `amadeo_transform`. That changed its `ComponentId`, which changed
archetype ordering, which would have changed every state hash containing it. It was free only because
nothing committed happened to assert a hash containing `Transform2d`.

So as written, **a pure refactor was a replay-invalidating change, and nothing warned you.** Moving a
type between crates, or even renaming a module, silently invalidated recorded behaviour. That is the
worst shape a trap can have: invisible, delayed, and indistinguishable from a real regression when it
finally fires.

There is a second, quieter cost. A `.scene` file writes `Transform2d`. `amadeo describe` prints
`Transform2d`. The ECS keyed on `amadeo_transform::Transform2d`. Three names for one thing, two of
which agreed.

**Timing.** Q13 was raised in session 5 with the note that it wanted a decision "before many replays
exist". Two now exist. It is decided now because the cost of deciding it grows monotonically and
nothing else about it improves with waiting.

## Decision

**`ComponentId` is the FNV-1a hash of `Reflect::type_name()`** — the canonical name, the same string
a scene file writes and `amadeo describe` prints.

This is available for free: ADR 0013 made `Component: Reflect` a compiler-enforced bound, so every
component already has a canonical name. `#[reflect(name = "...")]` therefore also becomes the way to
rename a Rust type without changing its identity.

Three consequences fall out deliberately:

1. **`ComponentId::of_name(&str)`** joins `ComponentId::of::<T>()`. A caller holding only a name —
   the scene loader, the agent layer — computes the same id as a caller holding the type. That these
   agree is the point, and there is a test asserting it.
2. **`ResourceId` and `ServiceId` keep the path.** Neither is reflected, so neither has a canonical
   name to use. Resources get this treatment when `Resource: Reflect` lands; services never will,
   since they are engine machinery that no file names.
3. **Moving a component between crates is now free**, which is what makes the transform rework Q3
   needs affordable.

## Consequences

- **Both committed replays were invalidated and regenerated.** `walk_and_jump.replay` and
  `wander.replay` changed only in their checkpoint lines; the input streams are byte-identical. That
  is exactly the signature of an identity change rather than a behaviour change, and it is worth
  recognising: same inputs, same code, different hashes means something about *identity* moved.
- **Two components with the same canonical name now collide**, where the full path previously made
  that impossible. This is the real cost, and it is mitigated in two places:
  - `ComponentRegistry::register` already refuses a duplicate name with a clear message. Invariant I8
    says every component is registered, so this covers the supported path.
  - For the gap — a component used but never registered — `World::insert` carries a **debug-build
    guard** recording which name first claimed each id and asserting on a mismatch. Debug-only
    because it costs a lookup and a string compare per new component type, and because CI runs the
    whole suite in debug *and* release.
- A component's identity now depends on its canonical name, so **renaming a component is a
  replay-invalidating change** where moving one no longer is. That is the right way round: a rename
  is a semantic change a human chose, and it is visible in the diff of every scene file that
  mentions it. A move is a refactor nobody expects to have consequences.
- `hash_type_name` remains for resources and services; `hash_name` is new and takes an
  already-chosen string.

## Rejected alternatives

**Keep the fully-qualified path.** Zero work, and no collision risk at all. Rejected because it
leaves a trap armed for every future refactor, and because the collision case it protects against is
already detected by the registry — trading a caught error for a silent one is the wrong direction.

**Keep the path, and add a lint or test that catches component moves.** Attractive briefly: it
preserves collision-freedom and warns about the trap. Rejected because such a test can only compare
against a committed list of paths, which is a second source of truth that has to be maintained by
hand and will drift. The name-based id needs no bookkeeping.

**Add an explicit `#[component(id = 0x1234)]` attribute.** Fully decoupled from both path and name,
and immune to renames. Rejected as too much ceremony for the problem — every component would carry a
magic number, hand-authored scene files could not be written without consulting it, and a copy-paste
of a component definition would silently duplicate an id.

**Defer until `Resource: Reflect` lands and change both together.** Tidier as one change. Rejected
because the cost of this decision grows with every recorded replay, and resources are not the ones
that move between crates or appear in scene files.
