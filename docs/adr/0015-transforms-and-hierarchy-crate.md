# ADR 0015 — Transforms and hierarchy live in `amadeo-transform`

**Status:** Accepted · **Date:** 2026-07-31

## Context

`amadeo-scene`'s `instantiate` can create a scene's entities but cannot record its hierarchy,
because there is no `Parent` component to put it in. A scene that loses its tree on load is not
finished, so this blocks real work.

Deciding where that component goes surfaced a **direct contradiction between two documents**:

- `CLAUDE.md` §4: *"`Transform2d` currently lives in `amadeo-render` because that is its only
  consumer. It moves to `amadeo-scene` in M1 along with the hierarchy components."*
- `docs/04-subsystems.md` §3, under `amadeo-ecs`: *"Hierarchy is data: `Parent`/`Children` +
  `LocalTransform` → `GlobalTransform` via a propagation system."*

**Those cannot both hold.** The crate order in `CLAUDE.md` §4 puts `amadeo-render` *below*
`amadeo-scene`, and invariant I6 says a lower layer never references a higher one. A `Parent` living
in `amadeo-scene` would be unreachable from the renderer — so M2's transform propagation, the single
biggest consumer of hierarchy, could not see it. `amadeo-physics` and `amadeo-anim` sit below
`amadeo-scene` too, and both want transforms.

The `CLAUDE.md` note was written when `Transform2d` had exactly one consumer and the observation
"this is not its permanent home" was correct. Its conclusion about *where* it should go was not.

## Decision

**A new crate, `amadeo-transform`, sitting directly above `amadeo-ecs`.** It owns the spatial
vocabulary that render, physics, animation, and scene all share:

| | |
|---|---|
| now | `Transform2d` (moved from `amadeo-render`), `Parent` |
| M2 | `Transform3d`, `GlobalTransform`, the propagation system |
| when needed | `Children` |

Updated crate order:

```
amadeo-derive → amadeo-core → amadeo-reflect → amadeo-ecs → amadeo-transform → amadeo-events → …
```

**This supersedes the `CLAUDE.md` §4 note**, which is updated to point here.

### `Parent` only, not `Parent` + `Children`

`Children` is a denormalised cache of what `Parent` already says. Keeping two representations
consistent is a classic source of bugs — a despawn that updates one and not the other leaves a
dangling reference — and nothing yet needs fast child iteration. It arrives when a system does, which
is the same on-demand policy the query API follows.

### Propagation is deferred to M2, deliberately

`GlobalTransform` and the system that computes it are not built here. Composing a child's transform
with its parent's is easy; deciding what the renderer *reads* is not, and that is entangled with Q3
(how 2D and 3D coexist), which has to be settled before M1's 2D renderer work anyway.

So `Parent` is currently data with no propagation behind it. That is a real limitation, and it is
still progress: ADR 0004 says hierarchy persists as components, and a scene can now round-trip its
tree instead of losing it.

## Rationale

1. **`amadeo-ecs` should stay the data *model*, not a component library.** It defines what an entity
   and a component *are*. `Transform2d` is a concrete, 2D-specific component, and M2 adds
   `Transform3d` beside it — at which point "the ECS crate holds the transform types" reads clearly
   wrong. A crate whose job is the shared spatial vocabulary is a nameable thing; a grab-bag inside
   the ECS is not.

2. **Everything that needs these types can reach them.** That is the requirement the previous plan
   failed, and it is not close: render, physics, anim, and scene are all above.

3. **The cost is one small crate, not a layer.** ADR 0011 made the shape of the crate graph
   load-bearing for compile times, so this is worth stating rather than waving through:
   `amadeo-transform` depends only on `amadeo-core`, `amadeo-reflect`, and `amadeo-ecs`, contains two
   types, and adds a node to a graph that is already shallow. The measured concern in ADR 0011 was
   heavy dependencies on the common path, not crate count.

## Consequences

- **Moving a component between crates changes its `ComponentId`, and therefore every state hash
  containing it.** `ComponentId` is the hash of `std::any::type_name::<T>()`, which is the
  *fully-qualified* path — so `amadeo_render::components::Transform2d` and
  `amadeo_transform::Transform2d` are different ids for the same type.

  Safe here: the committed golden replay uses the test file's own `Position`/`Velocity` and never
  touches `Transform2d`, so nothing needed regenerating. **It will not always be safe**, and the
  coupling between code organisation and behavioural fingerprints is a trap worth naming. Filed as
  **Q13** — using the reflection canonical name instead would decouple them, and would also make the
  ECS's identity and the scene file's identity literally the same string.

- **`Parent` holds an `Entity`, so `Entity` is now `Reflect` and `StableHash`.** It reflects as
  `{ generation, index }`. Those are runtime handle values and are meaningless in a saved file, which
  matters for the not-yet-built world-to-scene writer: it must derive nesting from `Parent` rather
  than serialising the component. Noted here so that path is not written the obvious wrong way.

- **`amadeo-render` no longer defines `Transform2d`** and does not re-export it. Two paths to one
  type is the sort of thing that makes people wonder whether they are the same type; the use sites
  are updated instead.

## Rejected alternatives

**Put them in `amadeo-ecs`.** Simplest — no new crate, everything can reach it, and `docs/04` §3
already implied it. Rejected because it makes the ECS crate the home for concrete components, which
it explicitly is not, and the problem compounds the moment `Transform3d` arrives in M2. Worth
recording that this is a close call and a perfectly defensible engine design; the deciding factor was
keeping `amadeo-ecs` describable in one sentence.

**Keep the `CLAUDE.md` plan and put them in `amadeo-scene`.** Rejected on invariant I6: the
renderer, physics, and animation all sit below `amadeo-scene` and all need transforms. This is not a
preference, it is a dependency-direction error in the original note.

**Put transforms in `amadeo-math`.** Superficially attractive — it is the "spatial types" crate.
Rejected because a transform *component* needs the `Component` trait, and `amadeo-math` sits below
`amadeo-ecs` with no engine dependencies at all. `amadeo-math` holds the maths; this holds the
components built from it.

**Leave `Transform2d` in `amadeo-render` and put only `Parent` in the new crate.** Avoids touching
working code. Rejected because it splits one concept across two crates for no reason other than
inertia, and every milestone that passes makes the move more expensive — M2's physics and animation
work would both build on the wrong location.
