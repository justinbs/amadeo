# ADR 0027 — Resources and events require `Reflect`, and a map is a value

**Status:** Accepted · **Date:** 2026-08-03 · **Completes:** ADR 0013 · **Supersedes nothing**

## Context

Invariant I8 says a type that cannot be reflected cannot be serialised, inspected, or edited.
ADR 0013 made that a **compiler-enforced bound** for components rather than a convention, on the
argument that a component invisible to the agent still works perfectly at runtime — so the omission
is not found until three milestones later, by someone wondering why `describe` is missing something.

ADR 0013 deliberately left resources out, and said why: it was not yet possible. Two of the four
resources could not reflect at all.

- **`SimRng`** wraps `Rng`, whose state is private to `amadeo-core`. `Reflect` lives in
  `amadeo-reflect`, which sits **above** `amadeo-core`, so `Rng` cannot implement it (invariant I6).
- **`InputState`** is two `BTreeMap`s, and the value tree had no way to represent a map.

The visible cost was that `world.resources` could not be implemented — `docs/protocol/v1.md` listed
it as blocked on exactly this. Components were introspectable and everything else in the world was
not, which is half an answer to "what is in this world".

## Decision

### 1. `Resource: Reflect`, and `Event: Reflect`

Both bounds, for the same reason ADR 0013 gave. Events were not in the original scope and were added
once the work started, because they hit the bound transitively (`Events<T>` is a `Resource`) and
because the argument is *stronger* for them: **the event log is how an agent answers "what did I just
do?"** — Pillar 3 of `docs/03-ai-native-design.md`. A queue nobody can read cannot serve its most
valuable purpose.

### 2. A map is a first-class value, with string keys

```rust
enum Value {
    // ...
    Struct(BTreeMap<String, Value>),
    Map(BTreeMap<String, Value>),   // new
}
```

`Struct` and `Map` hold the same shape and are kept apart deliberately. A struct's field set is
**fixed and known**, so an unrecognised name is a typo worth reporting. A map's keys are **data**, so
an unrecognised one is the entire point. Keeping them distinct is what lets `from_value` be strict
about one and permissive about the other, and what lets an editor render a fixed inspector for a
struct and an add-and-remove list for a map.

Keys are strings. A key type implements `ReflectKey` — `to_key` and `from_key` — and the trait's
contract is that `to_key` is **injective**. That cannot be reported as an error, because `to_value`
returns no `Result`, so `BTreeMap`'s impl carries a `debug_assert` that the entry count survived the
conversion.

### 3. `Rng` exposes its state; the reflection is written one layer up

`Rng::state() -> [u64; 2]` and `Rng::from_state`. Three independent things need to see inside a
generator and none can be served by drawing from it, because drawing *consumes* it: reflection,
hashing, and the snapshots ADR 0011 identified as the real iteration-loop cost.

For a type that sits below the reflection layer but has **public** state — `Tick` — the answer is
different and simpler: `amadeo-reflect` implements `Reflect` for it directly. The impl goes where the
*trait* lives rather than where the type does, which is legal in both directions.

### 4. `SimRng` stops hashing its `Debug` output

It used to hash `format!("{:?}", rng)`, reasoning that a derived `Debug` is a faithful function of
the fields. It is — but it made **every committed replay depend on the exact text of a `Debug`
impl**. Renaming a private field would have invalidated all of them, for a reason nobody would
connect to the failure. Now the two state words are hashed directly.

## What this cost, and how it was verified

**Both committed replays were regenerated.** That is the expensive part of the decision and it was
taken deliberately — see the question put to Justin, who chose to pay it now rather than leave the
landmine armed for whoever touched it next.

**The diagnosis was verified rather than assumed.** With *only* the `SimRng` hash reverted to its old
form — but `Resource: Reflect` in force, `EventClock` derived, `Events`/`InputState`/`Tick` all newly
reflected, `Rng::state` added, and the new `Map` variant in the value tree — **both replays matched
their committed hashes exactly**. So the entire reflection change is invisible to the state hash, and
the only thing that moved it is the one change intended to.

That is the check `docs/07-working-with-the-code.md` asks for before regenerating a golden file, and
it is worth doing rather than skipping: it distinguishes "I changed the thing I meant to" from "I
changed something else as well".

## Consequences

**Good:**

- **`world.resources` exists**, and answers from a real game: `amadeo call world.resources` reports
  `Camera2d`, `InputState`, and `SimRng` with their live state.
- Snapshots are now reachable. `snapshot.restore` is `from_value(to_value(x))` with a file in the
  middle, and every resource round-trips — asserted, including that a restored generator produces the
  *same next numbers*, which equality alone does not prove.
- A `Debug` impl is no longer load-bearing for replay validity.
- Maps are available to every future component. A stats block, an inventory, or a tag set now has a
  representation, and it reads naturally in an indented text format.

**Bad, and accepted:**

- **A resource cannot be a type whose state is private to a lower crate.** That is the invariant
  doing its job rather than a burden, but it is a real constraint and it will be met again.
- **A non-string map key round-trips through text.** `BTreeMap<u32, T>` is stringly-typed on disk.
- **`InputState`'s reflected keys are unreadable.** An `ActionId` is a hash whose name is not kept, so
  `world.resources` reports `"8831028638596390904"` rather than `"move_x"`. This is faithful and
  useless, it is a **known gap rather than an oversight**, and it is filed as **Q18**. The names exist
  — the input driver holds a table of them, which is how a `.replay` file writes readable action
  names — but they are outside `InputState` on purpose, since a resource participates in the state
  hash and two runs that registered different names for the same actions must not diverge. The fix
  belongs at the presentation layer.
- **The scene format still cannot express a map.** A field with no inline value parses as a *list*,
  so there is no nested-block syntax for either a map or a struct. The writer treats a map exactly as
  it already treats a nested struct — written bare so a round trip fails loudly rather than quietly
  changing shape. Nothing authors a map in a scene today, since resources are not scene-authorable at
  all. This ADR records the gap; it does not widen it.

## What was rejected

- **Arbitrary map keys, as Bevy and Godot both have.** Most general, and nothing stringifies.
  Rejected on two counts: `Value` contains floats and therefore has no total order to sort keys by,
  so it would need a hand-written one plus a rule banning float keys; and the scene format would need
  a syntax for a struct-as-a-key, which is genuinely hard to keep hand-writable. The case it buys —
  a tilemap keyed by coordinates — will use a bespoke chunk format rather than reflection.
- **No map variant, representing a map as a list of `{key, value}` pairs.** Zero new machinery, and
  arbitrary keys for free. Rejected because hand-authoring becomes `- key strength` / `value 10`,
  which is markedly worse in an indentation-based format, and because it is exactly the shape
  [Unity forces its users into](https://docs.unity3d.com/2020.1/Documentation/Manual/JSONSerialization.html) —
  the single most-complained-about hole in that serialiser.
- **Adding `Reflect` but keeping the `Debug`-based hash.** No replay regeneration, and the
  hash-affecting change stays separate from the reflection work. Rejected because it leaves a
  landmine flagged as inelegant since M0 still armed, and the next person to touch it pays the
  regeneration anyway — with less reason to be careful about it.
- **Making `ActionId` carry its name** so map keys read well. Rejected: it would make the id
  non-`Copy` and heap-allocated on a path walked every tick, to fix a presentation problem that
  belongs at the presentation layer.
