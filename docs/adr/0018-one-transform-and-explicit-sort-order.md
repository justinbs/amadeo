# ADR 0018 — One `Transform`, and an explicit sort order shared by 2D and 3D

**Status:** Accepted · **Date:** 2026-08-01 · **Partially resolves:** Q3

> **Clarification, 2026-08-01, after review by Justin. No decision below is changed.**
>
> This ADR justifies a 3D-first transform partly with "all three target games are 3D". Read alone,
> that reads as though 2D were being demoted. It is not. **Amadeo supports 2D and 3D as equal
> first-class capabilities** — decided in session 1 and restated here; see `docs/00-vision.md`
> § "The targets are a priority signal, not a scope limit".
>
> The target games order the *work*, not the *scope*. And the decision below is the 2D-friendly one
> on its own merits: two transform types would have meant two hierarchies in any world mixing them,
> making 2D-over-3D harder. One transform serves both better. Annotated rather than superseded
> because the reasoning stands and only its emphasis was wrong.

## Context

Q3 asks "how do 2D and 3D coexist in the renderer?" and `docs/04-subsystems.md` §4 frames it as a
choice between one orthographic pipeline, two pipelines sharing a render graph, or 2D composited over
3D. It is blocking M1's sprite renderer and `GlobalTransform` propagation, which has been waiting
since ADR 0015.

**Reading the code rather than the question shows it is three decisions, not one, and they have very
different reversal costs:**

| Decision | Blocks | Cost to reverse |
|---|---|---|
| Transform representation | `GlobalTransform` propagation, now | **Very high** — a component identity change, so every state hash containing it (ADR 0017) |
| Sort order for 2D and 3D | the sprite batcher, the scene format | **High** — a reflected field, so it lives in the schema, in scene files, and in state hashes |
| Pipeline architecture | sprite batcher internals only | **Moderate** — entirely behind `RenderBackend`; nothing outside the renderer observes it |

The question's own framing emphasises the *pipeline*, which is the cheapest of the three: the
`RenderBackend` trait already isolates it, no file mentions it, and no state hash depends on it. The
expensive decisions are about **data** — what a transform is, and what decides draw order — because
those appear in component schemas, in `.scene` files, and in replay hashes.

A spike that built three wgpu pipelines would therefore spend its effort measuring the part that is
easy to change. That is the failure Q2 already hit once, where the prescribed criterion turned out
not to discriminate between candidates.

**One fact weighs heavily on the transform decision.** All three target games — Palworld, Schedule I,
Inside the Backrooms (`docs/00-vision.md`) — are 3D, so 3D is what the engine needs to be good at
*first*. An engine whose only transform is 2D would be building the wrong thing first.

That is a statement about **ordering, not scope.** 2D remains a first-class capability (see the
clarification at the top of this ADR). It happens that the same decision serves both: one transform
means one hierarchy, which is what makes a 2D layer over a 3D world tractable at all.

## Decision

### 1. One `Transform`, always 3D. `Transform2d` is retired.

A single component. A 2D game leaves `z` at zero and rotates only about it.

- **`translation: [f32; 3]`** — world units.
- **`rotation: [f32; 3]`** — Euler angles in **degrees**, applied Z, then X, then Y.
- **`scale: [f32; 3]`**.

There is no `Transform2d` and there will be no `Transform3d`. One propagation system, one
`GlobalTransform`, and Q10 ("2D UI over a 3D world") stops being a transform problem entirely.

### 2. Rotation is authored and stored as Euler degrees — the judgment call in this ADR

**This is the one place where a reasonable engineer would disagree, so the reasoning is spelled out.**

The mature-engine answer is to store a quaternion and author Euler angles. That is rejected here
because it breaks the thing this project treats as invariant #1: **the file is the source of truth
and is hand-editable** (I1). If the component stores a quaternion and the file shows Euler, then
either the file is not what the component holds, or the round trip goes
quaternion → Euler → quaternion, which is not byte-stable and therefore violates I2.

So the component stores what the file stores, and the only question is which of the two is
authorable. `rotation 0 90 0` is; `rotation 0 0.707 0 0.707` is not.

**What this costs, stated plainly:** gimbal lock, and Euler triples interpolate wrongly through large
rotations. Both are real. Both are handled where they actually arise rather than by making every
hand-written scene file unreadable:

- **`GlobalTransform` is a computed 4×4 matrix**, never authored and never written to a scene file.
  Because it is derived, it is free to be whatever the maths wants. Gimbal lock is a property of
  *interpolating* Euler angles, not of converting them once.
- **Animation (M3) holds quaternions in its own components** and writes a resulting transform.
  Skeletal animation needs quaternions; hand-authored level geometry does not.
- **The camera rig is already a separate module** (`docs/00-vision.md`), which is where first-person
  pitch/yaw clamping belongs and where a quaternion or a pitch/yaw pair can live.
- **ADR 0006's `interpolate = "angular"` annotation** stays on rotation. Networked interpolation of a
  large rotation is a real problem and it is M6's, at which point the replication layer can choose to
  send a quaternion on the wire while the authored component stays Euler — the wire format and the
  authoring format are not required to match.

Degrees rather than radians for the same reason: a level designer and an agent both write `90`
correctly and `1.5707964` approximately.

### 3. Draw order is an explicit integer that dominates depth

A `SortOrder(i32)` component, shared by everything drawable — sprites and meshes alike. Higher draws
later, on top.

Within one `SortOrder`, 3D uses the depth buffer for opaque geometry and back-to-front for
transparent. 2D leaves everything at one depth and distinguishes purely by `SortOrder`.

This is one scheme that serves both without surprising either: 2D gets painter's order, 3D gets
depth, and "UI over the world" is expressed as a higher `SortOrder` rather than as a separate
concept. It also preserves the property `Quad::layer` already had — draw order is **explicit data**,
never iteration order, which invariant I3 requires and which `Quad::layer`'s own doc comment already
argues for.

`Quad::layer` is removed in favour of it. Keeping a per-primitive layer *and* a shared sort order
would mean two things deciding draw order, which is how "why is this behind that" becomes a
half-hour question.

### 4. The pipeline choice is explicitly deferred

Not decided here. One orthographic pipeline with a sprite batcher, two pipelines sharing a graph, or
compositing — all three remain open, and all three are compatible with everything above.

It is deferred because `RenderBackend` isolates it: no scene file, no component schema, and no state
hash can observe which was chosen. It should be decided **when the sprite batcher is actually being
written**, against a real throughput target (`docs/04-subsystems.md` §4 suggests 20k sprites at
60 fps), because that is a question measurement can answer and speculation cannot.

## Consequences

- **`Transform2d` → `Transform` is a rename, which under ADR 0017 changes its `ComponentId` and
  invalidates every replay containing it.** Done immediately after ADR 0017 and while exactly two
  replays exist, which is the cheapest this will ever be. Both are regenerated deliberately.
- Every scene file gains a third component on `translation` and `scale`, and two more on `rotation`.
  Slightly noisier for a 2D scene; honest about what the engine is. Whether `position 1 2` should
  default `z` to zero is a **reflection-layer leniency** question like the numeric leniency added in
  session 5, and is deliberately not decided here — it can be added later without changing stored
  data.
- `GlobalTransform` and its propagation system are now unblocked, which was the point.
- The 2D renderer can be written against `Transform` and `SortOrder` without waiting for the pipeline
  decision.
- A 2D-only game pays for four unused floats per entity (one translation, two rotation, one scale).
  Accepted: none of the three target games is 2D, and the alternative costs a second component type,
  a second propagation system, and a coexistence problem in every module.

## Rejected alternatives

**`Transform2d` and `Transform3d` as separate components.** The conventional answer, and it lets a 2D
game stay minimal. Rejected because it needs two propagation systems and two `GlobalTransform`s,
forces every module and every renderer path to pick or handle both, and turns Q10 into a real
problem: a 2D HUD over a 3D world would have two transform hierarchies in one world with no defined
relationship. The saving is sixteen bytes per entity in a game type this project does not target.

**One `Transform` storing a quaternion.** Mathematically correct, no gimbal lock, interpolates
properly. Rejected because a quaternion cannot be hand-written, and I1 is not negotiable for the
convenience of a subsystem (animation) that can hold its own quaternions.

**Store a quaternion, author Euler, convert on load and save.** What most engines do. Rejected
because the round trip is not byte-stable — I2 requires that saving an unchanged file reproduces it
exactly, and quaternion → Euler is not a function with one answer.

**Keep `Quad::layer` and add `SortOrder` alongside it.** Less churn now. Rejected because two
independent things deciding draw order is a bug factory, and the migration cost is one field on one
component today.

**Build the three-pipeline spike first.** The Q1 and Q2 precedent, and it would produce real numbers.
Rejected for *now* because it measures the cheapest-to-reverse of Q3's three decisions while the two
expensive ones stay open — and because a throughput comparison is far more meaningful against a real
sprite batcher than against three prototypes. The spike is deferred, not cancelled.
