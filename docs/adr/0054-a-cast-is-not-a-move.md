# ADR 0054 — A cast is not a move

**Status:** Accepted · **Date:** 2026-08-11 · **Builds on:** ADR 0036, ADR 0037 · **Closes:** Q34

## Context

ADR 0037 gave `PhysicsBackend` a second operation beside `step`: `move_shape`, which moves a shape and
**slides** along whatever it hits. That is what a character walking into a wall should do, and it is
what a character controller is built on.

It was also, until now, the only query the engine had. So anything wanting a different question had to
borrow it, and **Q34** recorded that this was a workaround rather than an answer — naming the callers
that would eventually want the real thing: a bullet, a line of sight, a placement check, a mouse pick
in an editor.

### The workaround failed twice, in the same session, and the second time was visible

`modules/amadeo-camera` sweeps a sphere from a pivot along an arm to find how far back the camera can
sit. That is a cast question asked of a move.

**First failure.** Measuring the straight-line distance travelled counted a *sideways slide* as
progress, so a camera brushing a slope got a distance with little to do with the axis it was pointed
along, and it swung as the player moved. Session 14 fixed that by projecting the travel onto the arm.
A correction on top of an operation that was answering a different question.

**Second failure, which the correction could not survive.** With the view tilted up, the arm points
down *and back*. It hits the ground, slides **backward** along it — and backward is 0.87 of the arm's
direction, so the projection counted nearly the whole slide as progress along the arm. Measured in
`games/scarp`: the arm shortened from 7.0 to 6.86 for a shape that had gone essentially nowhere in the
direction asked for, and the camera was placed **0.057 m below the terrain surface** where its own
0.35 m probe radius should have held it clear.

What that looks like is not a subtle offset. The camera is under the ground looking up at its
underside, which is unlit (ADR 0052) — a dark mass filling the frame over a band of sky. Justin
reported it as *"pointing the camera upwards, the view always ends up showing the skybox"*, and
guessed correctly that it was waiting on other work.

**The general shape is worth keeping**: when a borrowed operation needs a correction, the correction
usually has its own failure mode, and finding it is a matter of time rather than of luck. By the end
the camera carried two — project onto the axis, and exclude the parent body — which is the signal that
the wrong question was being asked.

## Decision

**`PhysicsBackend` gains a third operation: `cast_shape`.**

```rust
fn cast_shape(&self, cast: &ShapeCast) -> Option<ShapeHit>;
```

Sweep a shape along a straight line; report the first thing in the way, or `None` for a clear line.
Nothing slides, nothing steps, nothing snaps to the ground.

`ShapeHit` carries the **fraction** of the motion travelled, the resulting **position**, and the
surface **normal**. The fraction is the unit-free answer and multiplying by a known length gives a
distance directly — which is what removed the camera's projection rather than replacing it with
different arithmetic. The position is on the line **by construction**, computed as start plus fraction
times motion, which is precisely the guarantee `move_shape` cannot make.

Three decisions inside it:

**A separate method, not `slide: bool` on `ShapeMove`.** Q34 listed that option and it is worse:
`step_height`, `snap_distance`, `max_slope_degrees` and `up` are all meaningless to a cast, so the
flag would silently void four fields of the type it was added to.

**`&self`, where its neighbours take `&mut self`.** A cast is a question. Saying so in the signature
lets a system reach it through `World::service` and ask alongside a query, rather than taking the
service mutably to ask something read-only.

**`stop_at_penetration: false`.** rapier's default reports an immediate hit whenever the sweep starts
in contact, whatever direction it was going. A shape resting on a surface and asked to move *away*
from it is not blocked — and the case is real rather than hypothetical: a follow camera's pivot can
be squeezed against a ceiling while its arm points down and back into open air. The default would
report a block and glue the camera to its minimum for as long as you stood under something.

The normal is not read by anything today. It is included because every caller Q34 named needs it, and
adding it later would change a returned type rather than extend one.

## Consequences

**The camera has no workarounds left.** Both of its sweeps are casts; the projection is gone and the
`.ignoring()` filter is now what it should always have been — a statement about which body the sweep
starts inside, rather than a way of dodging an unstable answer.

**`NullPhysics` finds nothing, ever**, which is ADR 0037 §5's posture applied to a third operation.
Pointed at it, a follow camera keeps its full arm inside solid rock — and that is what makes the
rapier tests evidence that the sweep is consulted, rather than evidence that the authored distance
happened to be right.

**It must run after `step`**, for exactly `move_shape`'s reason: both answer from an index the step
builds, and an empty index reports everything clear. This is now the third operation carrying that
constraint, which is an argument for the engine enforcing it rather than documenting it — not built,
and noted.

**It is in the state hash's path**, because the camera writes the result into a hashed `Transform`.
ADR 0036's `enhanced-determinism` is what makes that safe, and it is the same guarantee `move_shape`
already relied on.

**A zero-length sweep reports nothing rather than a hit at zero.** There is no line to ask about, so
there is no direction a normal could point along, and a caller dividing by the motion's length does
not get an infinity.

## Alternatives rejected

**Leave it, and let each caller project onto its own axis.** This *is* the option that was in place,
and this ADR exists because it produced a visible bug that a second correction would not have fixed
either — the projection is wrong whenever the slide has a large component along the query direction,
which is a geometry the caller does not control.

**A raycast instead.** A zero-radius ray slips through the crack between two triangles at a chunk
boundary and reports open space where there is rock, which `FollowCamera::radius` already documents.
A camera needs thickness. A ray is the degenerate case of this and can be added as one later.

**Expose rapier's `QueryPipeline` directly.** Fastest to write and it breaks ADR 0036 §4 — no rapier
type may cross `PhysicsBackend`, because the version is pinned exactly and a leaked type would put an
upgrade into the scene format and the state hash at once.
