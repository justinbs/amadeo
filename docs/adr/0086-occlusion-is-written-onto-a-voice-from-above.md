# ADR 0086 — Audio occlusion is a value written onto a voice from above

**Status:** proposed (session 26) — **awaiting the critic's approval, which Justin made a condition**
**Extends:** ADR 0059's collection pass and ADR 0009's Service rule. Supersedes nothing.
**Closes:** Q44.

## Context

`docs/11` §9 makes occlusion a **gameplay** requirement rather than polish:

> *"A warden exactly as loud through a wall as through a doorway makes the whole mechanic a lie."*

The game is made of corridors and its antagonist is found by ear, so this is the mechanic rather than
a garnish on it. It is row **F6** of `docs/13` §1b, third in the order.

**The obstacle is the crate graph, and it is invariant I6.** `crates/amadeo-audio/Cargo.toml` depends
on assets, core, ecs, events, reflect and transform — **and not on `amadeo-physics`**. So nothing
inside `collect_audio` can ask whether a wall stands between the listener and a source. Every answer
requires changing something structural, and a new edge in a strict DAG is close to the hardest thing
in this repository to take back out.

`CLAUDE.md` §5 therefore makes it Justin's, and it was put to him as Q44 with three shapes.

## Decision

**A voice carries an `occlusion` value, and something above both crates writes it.**

`amadeo-audio` gains **no dependency**. `AudioSource` gains a reflected, hashed field — a scalar in
`0.0..=1.0`, where `0.0` is a clear line and `1.0` is fully blocked — and the collection pass folds it
into the gain it already computes, in the same place and the same order as `Bus` gain, so two backends
cannot disagree about it.

**The engine ships the system that fills it**, in a crate that may depend on both. It is not left to
each game to remember.

### Why this shape rather than a dependency edge

The inversion is not novel here; it is **this project's established answer to exactly this problem,
and it has been used four times**: `Overlay`, `TextureCache`, `MeshCache` and `SkyCache` all have the
lower crate own the slot and something higher fill it. `amadeo-ui` sits *above* `amadeo-render` for
precisely this reason — the renderer cannot look for a `UiNode`, so it owns a slot and the higher
crate fills it.

A direct edge would make `amadeo-audio` unbuildable without a physics crate, for a query most games
never issue — a 2D platformer, `games/vault`, and every one of `docs/12`'s grand-strategy targets want
spatial sound and do not want a solver in the graph to get it. And ADR 0036 §4's rule that no rapier
type may cross `PhysicsBackend` would need restating in a second crate.

### Why not the level generator's room graph

`layout.rs` already knows which sections share a door, so occlusion could be a lookup with no cast at
all — exact for the Warren and nearly free. It is also **one game's trick rather than an engine
capability**, and `docs/05` M4b's walls are not generated the same way, so it would be rebuilt within
two milestones. Rejected on the same reasoning that put `FollowCamera` in `modules/` once a second
game wanted it.

## Consequences

- **`occlusion` is hashed**, because `AudioSource` is a component and a save must restore it, and
  because gameplay may legitimately read it (an AI that knows it cannot be heard). It is **not**
  derived: nothing recomputes it if it is dropped, which is ADR 0063's `Focus` call rather than
  `Looking`'s.
- **It is written in `PostSimulation`, after `propagate_transforms` and after `step_physics`**, for
  `move_shape`'s documented reason — a cast answers from an index `step` builds, so asking earlier
  queries an empty world on tick 1.
- **A game that installs nothing gets `0.0` everywhere**, which is exactly today's behaviour. So this
  cannot change any existing capture, test or state hash until something opts in.
- **The cast is a question, so it takes `&self`** — `cast_shape` (ADR 0054) already does, and it
  already reports *what* it hit via `ShapeHit::entity`, which is what distinguishes a wall from the
  listener's own body.
- **The attenuation curve is genre knowledge and does not live in the engine.** The engine supplies
  "how blocked is this", not "how much quieter should that be". `docs/11` §9's number — a voice behind
  a bore wall reads **≤ 0.30×** against a clear line at the same distance — is the Warren's, authored
  in the Warren.
- **Cost is one shape cast per audible voice per tick**, bounded by ADR 0059's eight-voice cap. It is
  not free, and it is measured rather than assumed before F6 closes.

## What was rejected, recorded so it is not relitigated

| Option | Why not |
|---|---|
| `amadeo-audio → amadeo-physics` | Unbuildable without a solver, for a query most games never issue; restates ADR 0036 §4 in a second place; hardest of the three to reverse |
| The level generator's room graph | One game's trick; M4b's walls are not generated the same way; rebuilt within two milestones |
| A `LocalService` holding a solver handle | Q12's shape, and it was already found not to bite. Adds a second world of geometry that can disagree with the first |
