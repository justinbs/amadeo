# ADR 0012 — Reflection is a value tree plus a derive, not dynamic field access

**Status:** Accepted · **Date:** 2026-07-31

## Context

`amadeo-reflect` is the keystone of M1. Invariant I8 says an unreflected type does not exist as far
as the editor and the agent are concerned, and `docs/04-subsystems.md` §8 marks it "build this
second" because serialization, the editor inspector, and agent introspection are all downstream of
one registry.

Four things had to be decided before writing it, all flagged in §8 as ⚠️. Each is cheap now and
expensive later, because every one of them would otherwise mean revisiting every component in the
engine.

## Decision

### 1. Reflection is a value tree, not dynamic field access

`Reflect` has four methods: `type_name`, `type_info`, `to_value`, `from_value`. A type converts to
and from a [`Value`] tree; nothing offers a live cursor into a running value.

The alternative — `fn field(&self, name: &str) -> Option<&dyn Reflect>` — is more powerful and
avoids allocating. It also requires `dyn Reflect`, object-safe accessors, and a downcast at every
level. That is precisely the Rust that `CLAUDE.md` §6 rules out, and this engine's three consumers
all want a *whole tree* at once — saving a scene, rendering an inspector, answering `world.entity`.
None of them wants a cursor.

The allocation happens when a scene is saved or an entity is inspected. Never in a tick.

**Consequence:** `Reflect` is not object-safe, because `from_value` returns `Self`. Type-erased
operations that genuinely need a trait object — "insert a component named `Health` onto this
entity" — are built from monomorphised function pointers in `amadeo-ecs`, which is where the entity
concept lives anyway.

### 2. `Value::Struct` is a `BTreeMap`, so canonical order is not a rule anyone has to follow

Struct fields come out sorted by name because the data structure cannot represent them any other
way. Invariant I2 (byte-stable serialization) therefore stops depending on every writer remembering
to sort.

`TypeInfo`, by contrast, keeps fields in **declaration order** — it is documentation, and a struct
reads best in the order its author wrote it. The two orderings serve different masters and that is
deliberate.

### 3. The metadata vocabulary, fixed now

Per §8, "decide the attribute vocabulary early — adding it later means touching every component."

| Attribute | On | Purpose |
|---|---|---|
| `name = "..."` | type | canonical name, when it should differ from the Rust identifier |
| `version = N` | type | schema version for migration; starts at 1 |
| `min` / `max` | field | **advisory** bounds — editor slider range, and a hint to an agent about what a sane value is |
| `unit = "..."` | field | `"m/s"`, `"rad"`, `"hp"` |
| `sync = "..."` | field | ADR 0006 replication policy: `never` (default), `on_change`, `always` |
| `interpolate = "..."` | field | ADR 0006 smoothing hint: `none` (default), `linear`, `angular` |
| `skip` | field | not authoritative state; excluded from schema, value, *and* hash |

Ranges are advisory and **not enforced on load**. A designer deliberately pushing a value past its
usual bounds is legitimate and must not be a load failure.

`sync` defaults to `never`. Opting in is a decision someone makes on purpose; opting out is
something they forget. A field that should have replicated and does not is a visible gameplay bug;
a cache that replicates because nobody annotated it is invisible bandwidth found months later.

Authority is deliberately **not** a field annotation. It belongs to an entity and already exists as
`amadeo_core::Authority`. Two places to say the same thing is two places to disagree.

### 4. `StableHash` is derived too, and sorts by field name

`amadeo-core` promised this for M1. It is more than convenience: a hand-written `stable_hash` that
forgets a field still compiles, still runs, and still produces a plausible number — while silently
excluding part of the simulation from every golden replay assertion. **That is the worst failure
shape available under invariant I3: the tests keep passing and stop testing.**

The derive hashes fields sorted by name, so reordering fields — a pure refactor — does not
invalidate every committed replay. Enums key on the variant *name*, not its index, so inserting a
variant in the middle leaves every other variant's fingerprint alone.

`#[reflect(skip)]` is honoured by both derives. A skipped field does not round-trip through
serialization, so including it in the hash would make a reloaded value disagree with the original
and break save/load comparison.

## Consequences

- **A new bottom-of-graph crate, `amadeo-derive`.** It depends on no engine crate, only `syn`,
  `quote`, and `proc-macro2`. That is what lets `amadeo-core` — the bottom of the *runtime* graph —
  re-export `#[derive(StableHash)]` without creating a cycle (invariant I6). A proc-macro crate is a
  compile-time tool; nothing it emits references it.

- **Trait and derive share a name on purpose.** `use amadeo_core::StableHash;` brings in both,
  exactly as it does for `Debug`. Rust keeps macros and types in separate namespaces so they cannot
  collide. Noted here because it is the kind of thing that looks like a mistake when you first meet
  it.

- **Compile cost.** `syn` was already in the workspace lock file via `thiserror`, so the marginal
  cost is the derive crate itself. ADR 0011 made rebuild times load-bearing, so this is worth
  watching rather than assuming: re-run `spikes/q1-game-logic/measure.ps1` if it feels slower.

- **Two gaps in `amadeo-core` were found and closed** while building this: `stable_hash_of` was
  `pub` but never re-exported from the crate root, and `[T; N]` had no `StableHash` impl at all —
  which is why `Transform2d` hand-rolls its array hashing today.

- **Existing components are not converted yet.** Deriving `StableHash` on `Transform2d` would change
  its fingerprint (its fields are not in alphabetical order, and arrays hash length-prefixed), which
  regenerates the golden replay. That is a deliberate, separate change — see below.

- **Not built here:** JSON Schema emission. `amadeo describe` needs a concrete output format, and
  that is a decision belonging to `amadeo-cli` and the protocol spec, not to the model. Letting a
  serializer's default output become the format is trap 4 in `CLAUDE.md` §7.

## The next two steps this sets up

1. **Make `Component: Reflect`.** I8 says an unreflected type does not exist; today that is a
   convention, and trap 5 ("skipping reflection registration") is still available. A trait bound
   would close it permanently, the same way ADR 0009 used `Resource: StableHash` to make misfiling a
   type error rather than a discipline problem. Cheapest now, with eight components in existence.
2. **Convert existing components to both derives** and regenerate the golden replay once,
   deliberately, in a commit that says so.

## Rejected alternatives

**Dynamic field access via `dyn Reflect`.** What Bevy does, and genuinely more capable — it supports
patching one field of a live value without rebuilding the whole thing. Rejected on legibility
(§6) and because no consumer here needs it. Revisit if the editor's inspector turns out to want
per-field writes badly enough to justify the machinery.

**Route `Component`'s `StableHash` through `Reflect`.** Tempting — `Value` already implements
`StableHash`, so `stable_hash_of(&self.to_value())` would be a one-line blanket impl and remove the
second derive entirely. Rejected on performance: `World::state_hash` visits every component of every
entity, and allocating a `Value` tree per component per hash would make the single hottest
diagnostic path in the engine allocate proportionally to world size. The derives are independent and
both cheap.

**Serde.** Mature, fast, and everyone knows it. Rejected because it solves serialization, not
reflection: it has no runtime type registry, no per-field metadata vocabulary, and no way to answer
"what fields does `RigidBody` have and what do they mean" without a value in hand. Pillar 2 needs the
schema *without* an instance. Adopting serde as well, purely for its format implementations, stays
open and is a separate question.

**Widening every integer to `i64` in `Value`.** Fewer variants. Rejected because signed and unsigned
genuinely differ for schema purposes, and because narrowing back has to be checked — a silently
truncated integer surfaces three subsystems from its cause.

**Widening `f32` to `f64` in `Value`.** The round trip is lossless, so this looked free. It is not:
formatting an `f32` that has been through `f64` can produce a different decimal string, which breaks
byte-stability (I2) in the text format even though the value is unchanged.
