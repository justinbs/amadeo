# ADR 0086 — Audio occlusion is a value written onto a voice from above

**Status:** accepted (session 26) — chosen by Justin, **approved with changes by engine gate review 29**; the changes are the amendment at the end
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

---

## Amendment — review 29's three required changes (session 26)

Approved with changes. The shape and both rejections stand; three things had to be said before F6
starts, and this is them.

### 1. The crate that owns the filling system is `amadeo-app`

The first draft said *"a crate that may depend on both"* and never named one, which is how a system
lands somewhere by accident. `amadeo-audio` cannot own it and `amadeo-physics` cannot; `modules/`
would contradict this ADR's own *"it is not left to each game to remember."*

**`amadeo-app` already owns cross-subsystem wiring** — the schedule, the agent host, the profiler —
and depends on everything. `occlude_voices` is registered there, and it is registered **automatically
whenever a world has both a `Physics` and an `Audio` service**, so no game installs it and no game can
forget it. A world with one and not the other gets nothing, costs nothing, and is unchanged.

### 2. One cast per tick cannot satisfy F6's own clause (a), and the scalar is eased

F6 (a) requires the transition across a cross-passage opening to be continuous, **no tick changing
gain by more than 0.15×**. A binary cast steps 1.0 → 0.0 in a single tick every time it crosses an
edge, so the first draft cost the row its close condition and did not know it.

**`occlusion` is eased toward the cast's answer at a bounded rate per tick** rather than being
assigned. Deterministic (a fixed rate over a fixed timestep), nearly free, and physically honest —
sound diffracts around an edge and the ear integrates, so an instantaneous step is the *less* correct
answer as well as the unusable one. The rate is a constant here, not authored: it is a property of
hearing rather than of a level.

Rejected: N casts to fixed offsets around the source, which is deterministic and correct and costs N
times as much for a smoothness the ease already supplies.

### 3. The intended consumer is a low-pass filter; gain alone is the descope

**A wall does not make a sound quieter. It makes it dull** — it removes the top two octaves. A voice
attenuated in gain alone tells the player *"that is further away"*, and `docs/11` §4's mechanic needs
them to distinguish *far* from *behind that bulkhead*. Those are the same cue if occlusion is a volume
knob.

So the scalar's intended consumer is a **low-pass cutoff** on the voice, with gain as a secondary
term. This ADR's abstraction was already right for it — the field says *how blocked*, not *how much
quieter* — so this costs the decision nothing and only had to be written down.

**Gain-only is named here as the budget descope**, not as the design. `docs/13` §1b already allows F6
to descope; if it does, this is the half that goes, and the field does not change when it comes back.
