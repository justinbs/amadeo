# ADR 0070 — An item is an entity, and storing it removes its `Transform`

**Status:** Accepted · **Date:** 2026-08-15 · **Builds on:** ADR 0029, ADR 0037, ADR 0054, ADR 0063 ·
**Settles:** `docs/05`'s `mod-inventory`

## Context

`mod-inventory` is the last unbuilt named module, and four of the eight target games are
inventory-heavy. The fork `docs/05` records is whether an item is an **entity** or a **value**: a
stack of fifty arrows is one row in a list, a dropped arrow is a thing in the world with a collider,
and both have to be the same item.

Both designs ship real games. Minecraft and Terraria made an item a value and spawned an entity for
a dropped one; RimWorld made every item a `Thing` with a stack count and moved it between containers.
Minecraft has since moved its `ItemStack` to a **set of named data components**, which is worth
noticing: even the canonical value-based design converged on composition.

## The measurement that decided it

The obvious cost of "an item is always an entity" is that a stored item is an entity with no place
in the world, and the engine has never expressed that. So the three passes that would have to agree
to ignore one were read rather than reasoned about:

| Pass | Query |
|---|---|
| `collect_meshes` | `(&Mesh, &Transform, Option<&SortOrder>, Option<&GlobalTransform>)` |
| `step_physics` | `(&RigidBody, &Transform, Option<&Velocity>, …)` |
| `propagate_transforms` | `world.get::<Transform>(entity)`, `continue` when absent |

**All three already require a `Transform` and skip an entity that lacks one.** So the audit that was
this option's main cost does not exist, and the operation is a single component:

> **Removing an item's `Transform` takes it out of the world; putting one back drops it.**

Nothing else changes. Its mesh, its collider, its `Interactable`, its own per-item state all stay on
the entity untouched, so there is no second representation and nothing is ever converted between two.

## Decision

### 1. An item is an entity, always

`Item { kind, count, max_stack }` says what it is and how many. `StoredIn { container, slot }` says
where it is when it is not in the world. `Inventory { slots }` marks a container.

The reasons are this project's rather than the industry's:

- **The M3 exit gate needs a flashlight**, which is a light, a battery and a thing you can look at.
  As a value that is three new fields on the engine's item struct; as an entity it is a **prefab**,
  and ADR 0029 already built those. Every per-item property after it is free in the same way, which
  is ADR 0066's argument for animating a reflected field rather than a known one.
- **`ShapeHit::entity` and `Looking::at` already hand you an `Entity`.** Picking up what you are
  looking at is "remove a component"; with values it would be "read a component off that entity,
  build a value from it, despawn it" — a conversion, in the place conversions go wrong.
- **Trap 11.** Four target games are defined by their modding ecosystems, and composition is the
  modding answer. A closed `ItemStack` struct is a list of everything a mod may never add.

### 2. A stack is one entity with a count, not `n` entities

RimWorld's model. Fifty arrows is one entity with `count: 50`, so the thing that made the value
design attractive — a stack being one row — is not actually a property of values at all.

`max_stack` is per item rather than global, and `1` is how something says it does not stack.

### 3. A slot is authored data, not storage order

`StoredIn` carries a `slot`, and `contents` returns items **sorted by it**.

The alternative is to return whatever a query yields. That is reproducible — archetype order then row
order — but it is not *stable*: an item's position in the list would move when an unrelated component
was added to it, so "my sword is in the third slot" would depend on archetype churn. A grid inventory
needs a slot index anyway.

This is ADR 0063's reasoning one module along: **an order that is authored is identical everywhere,
and an order that is derived from layout is only accidentally so.**

### 4. The module knows nothing about what an item does

`store`, `take`, `drop_at` and `contents` are plain functions on a `&mut World`, not systems. There
is no "use item" hook and no registered behaviour, because what a key or a flashlight *means* is
genre knowledge and invariant I4 puts it above the engine — the same split ADR 0068 drew for
behaviour, where the game writes the facts and reads the state.

## Consequences

- **A stored item still exists.** It is in the state hash, it is in a snapshot, and `amadeo query`
  can read it — which is invariant I5 holding for a bag as well as for a room, and is what a value
  buried inside a component could not offer.
- **Entity count grows with stored items.** One entity per *stack*, so a chest of fifty arrows is
  one, not fifty. The case that would hurt is tens of thousands of distinct stacks — a RimWorld
  stockpile — and RimWorld does exactly this and is fine. If it ever does hurt, the fix is a
  different container representation behind these same four functions rather than a different item
  model.
- **A `GlobalTransform` left on a stored item goes stale.** Harmless, because it is `DERIVED` and out
  of the state hash and because every reader requires a `Transform` beside it — but it is the kind of
  thing that looks alarming in a dump, so `store` removes it too.
- **An item whose container is despawned is orphaned rather than destroyed.** The same call ADR 0015
  makes for a `Parent` pointing at a dead entity: the walk stops, nothing panics, and the item is
  still there. Whether that is a leak or a feature is the game's to decide — a dropped bag that
  spills its contents wants exactly this — so `orphaned` reports them and nothing acts on them.
  **`contents` keeps answering for a dead container**, which was found by a test written expecting
  the opposite. It is a lookup by *handle*, and filtering by liveness would make an orphan invisible
  to every function here while still existing — so a game emptying a dead chest onto the floor could
  not see what was in it. Nothing is ambiguous, because a handle carries a generation: an entity
  reusing the slot is a different handle and inherits nothing.
- **Nothing stops a container being stored inside itself.** A bag in a bag is a real feature and the
  cycle is the failure mode beside it. `store` refuses the direct case; a longer cycle is not checked
  and would be a bounded walk if a game ever nests containers.

## Rejected alternatives

**An item is a value in a list.** Rejected for §1's reasons. The honest point in its favour is
cache locality when scanning a large inventory — genuinely better — and it is not close to mattering
at the sizes any target game has.

**An item is an open value: a `BTreeMap<String, Value>` of component values**, which is Minecraft's
current model and would fit this codebase suspiciously well, since `SnapshotEntity` already holds
exactly that shape and both conversion directions already exist as functions. **Blocked, and worth
recording why:** `Component: StableHash`, and neither `Value` nor `BTreeMap` implements it — which is
a deliberate limit, not an oversight. Making them hashable would let any component become a bag of
dynamic values, and a schema that cannot describe its own contents is what invariant I8 exists to
forbid.

**Removing the collider and the mesh as well as the `Transform`.** Rejected as redundant once the
three passes were read: they key on `Transform`, so removing more is extra archetype churn that buys
nothing, and every component removed is one whose data has to be reconstructed on the way out.
