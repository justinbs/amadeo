# ADR 0029 — Prefabs are assets, and an override reaches only the instance root

**Status:** Accepted · **Date:** 2026-08-03 · **Resolves:** Q7 · **Supersedes:** ADR 0014's `from` grammar

## Context

`games/vault` found the gap by using the engine rather than by reasoning about it. A wall tile costs
ten lines of scene text; the arena has forty-four of them, which is four hundred lines of
near-identical text to author and to re-read on every diff. `entity w1 "Wall" from wall_tile` is one
line, and prefab instancing was refused outright.

Two things blocked it, and the second is the interesting one.

**The `from` conflict.** ADR 0014's grammar says a path (`from prefabs/door_metal`); ADR 0020's
worked example says an asset id (`from wall_concrete`). Not reconcilable: `is_usable_id` rejects `/`
precisely so a bare id in a scene line is unambiguous.

**Override semantics**, which `docs/06-open-questions.md` called the hardest problem in the scene
subsystem and said to study Unity's and Godot's failure modes before designing. That research is
what produced the design below.

## What the research found

**Unity.** Overrides live in editor state, are easy to create by accident, and are hard to find —
"modifications by mistake on a prefab instance… a source of bugs that are pretty hard to track
down". With nesting they **evaporate**: on Apply All, when a nested prefab loses the object an
override targeted, or when stored file IDs desync. Practitioners advise keeping nesting under two
levels for exactly that reason.

**Godot.** Editing an instanced child through "editable children" can inadvertently write back to
the source scene and every other instance.

Both failure classes come from the same root: **an override that names something *inside* a prefab
has to keep track of that thing across every future edit of the prefab.** Unity's file IDs are an
attempt at that, and they desync.

## Decision

### 1. `from` holds an asset id

Superseding ADR 0014's grammar. A prefab is an asset like any other, which means everything already
built applies unchanged: `amadeo check` validates the reference and offers "did you mean" on a typo,
ADR 0021's barrier makes it resident before the first tick, `amadeo assets` lists it, and moving the
file breaks nothing.

A `from` line is **itself the declaration** — `required_assets()` includes it, so a prefab does not
have to be repeated in the `assets` block.

### 2. An override reaches the instance root and nothing else

**This is the decision the research bears on, and it is what makes nesting safe.** There is no syntax
that can name anything inside a prefab, so there is nothing for an override to lose track of. Unity's
nesting pain is structurally impossible here rather than merely avoided — which is why
`nesting_is_safe_because_overrides_cannot_reach_inside` is a passing test and not a hope.

The cost is real: you cannot nudge one child of a prefab from an instance. You make a variant prefab
instead, which is more files.

Two shapes on an instance, and the distinction is explicit in the file:

- `override Foo` — **replaces** what the prefab put on the root. If the prefab does not have `Foo`,
  that is an error (see 4).
- `Foo` — **adds** something the prefab lacks. If the prefab does have it, that is an error too,
  because the author meant `override` and silently picking one would hide it.

### 3. An override is a patch, not a replacement

Only the named fields change; the rest come from the prefab.

```text
entity w1 "Wall" from wall_tile
  override Transform
    translation 3.0 0.0 0.0
```

Restating every field still works — a full override is a patch that happens to cover everything — so
this is strictly more permissive than replacement. **Only the top level merges**; a field whose value
is itself a struct is replaced whole, because recursive merging makes "how much did that touch?"
unanswerable by looking.

### 4. A dangling override refuses to load

If the prefab no longer has the component an override names, loading fails, naming the entity, the
component, and the prefab. This is the direct counter to Unity's worst behaviour: nothing is silently
lost, and the failure arrives when the prefab changed rather than months later as a value that
mysteriously reverted.

The friction is real and bought deliberately — editing one prefab can break every scene using it at
once, and `amadeo check` will list them all before anything runs.

### 5. Nesting is allowed; cycles are refused

A prefab may instance another. Safe because of 2. A cycle is detected and reported with the chain
(`loop_a -> loop_b -> loop_a`) rather than expanded forever.

### 6. A prefab has exactly one root

An instance *is* its prefab's root. With none there is nothing to be; with several there is no way to
say which one the overrides apply to.

**Prefab-internal ids are not registered** in `Instantiated::entities`. Two instances of one prefab
would otherwise collide on every internal id — and nothing can refer to them anyway, by 2. That makes
the collision structurally impossible rather than a rule to remember.

## Proof

`games/vault` converted its six sigils and two traps to prefab instances. The scene went from **223
lines to 142**, each sigil from fourteen lines to three, and **`collect-three.replay` matched all
four checkpoints unchanged** — the same world, authored differently, which is the strongest available
evidence that the refactor is behaviour-preserving.

## Consequences

**Good:**

- Repeated designed content stops being a copy-paste exercise.
- A prefab is an asset, so the whole asset toolchain applies to it for nothing.
- Override state is entirely visible in the file, which is what `docs/06` demanded of this question.

**Bad, and accepted:**

- **A prefab's id shares one namespace with every other asset.** The Vault hit this immediately: a
  prefab named `sigil.scene` collides with the `sigil` texture. Renaming to `sigil_pickup` fixed it,
  but the papercut is inherent to "a prefab is an asset" and will recur.
- **`amadeo import` could not import a prefab.** A bootstrapping deadlock, found by hitting it:
  `import` launches the game (ADR 0016), and the game refuses to start while a prefab it needs has no
  sidecar. The Vault.s two sidecars were written by hand. **Fixed later the same session** by
  `amadeo import --assets <dir>`, which names the directory instead of asking the game — Q19.
- **You cannot override a prefab's child.** By design, but it will be asked for, and the answer is a
  variant prefab.
- **A prefab edit can break every scene using it.** By design, per 4.

## What was rejected

- **`from` holds a path.** Keeps ADR 0014 and needs no sidecar, but contradicts ADR 0020's reasoning
  exactly — a path is a location, and prefab folders get reorganised more than most.
- **Accepting both, told apart by the slash.** Reintroduces the ambiguity `is_usable_id` exists to
  prevent, and means two resolution paths and two names for one prefab.
- **Path-addressed child overrides** (`override handle/Transform`). Full power, and precisely Unity's
  design and Unity's failure. Needs stable per-child identity, which is a whole design of its own.
- **Dropping a stale override and reporting it.** The game keeps running — and this is Unity's
  failure mode, where the report is only useful if someone reads it.
- **Keeping a stale override as dead data.** Nothing is lost, and the file would claim something is
  overridden while the running game disagrees — hidden state in a different coat.

## What this does *not* fix

**The Vault's forty-four wall tiles are still spawned from a `MAP` constant in code**, and that is
the right answer. As prefab instances they would be 44 × 4 = 176 lines of scene text against a
seven-line picture of the level. Prefabs fix repeated **designed** content; a *grid* wants a tilemap,
which is `mod-tilemap` in M7. Worth stating because "prefabs will fix the walls" was the obvious
expectation and it is wrong.
