# ADR 0072 — A loaded scene is composed, and a body is stored in its own space

**Status:** Accepted · **Date:** 2026-08-16 · **Builds on:** ADR 0015, ADR 0019, ADR 0036, ADR 0071 ·
**Closes:** the hazard `STATUS.md` opened in session 18 as "a capture at tick 0 is not a picture of
your game"

## Context

`GlobalTransform` is **derived** (ADR 0019): computed by `propagate_transforms`, kept out of the
state hash, never authored. Every consumer of a world pose — the mesh collector, the physics sync,
the interaction sweep — reads it *with a fallback to the entity's local `Transform` when it is
absent*. That fallback is correct for a root, where the two are the same thing, and silently wrong
for a child, where they are not.

Two facts about *when* it runs turn that into a defect:

1. **`propagate_transforms` is a `PostSimulation` system.** So a freshly loaded world has no
   composed transform at all, and during any tick the composed transform is one tick old.
2. **`step_physics` runs in `Simulation`**, before propagation.

ADR 0071's room pieces are what made this reachable. A prefab has exactly one root, so a piece with
more than one collider — a doorway with two jambs and a lintel, a room shell with a floor and a
ceiling — has no choice but to put them on **children**. Before those pieces existed, no game had a
reason to put a collider anywhere but a prefab root, and the whole class was unreachable.

It cost parts of two sessions in two disguises. Session 18 lost most of one to a torch beam authored
at `y = -0.1` under a camera: captured at tick 0 it was drawn a tenth of a metre *underground*,
correctly shadowing the room with the floor it was buried in, and the conclusion drawn was that the
renderer had two bugs. It did not (Q39, withdrawn). Session 19 hit the same root cause one tick
later, and found a second, worse defect underneath it.

## Decision

### 1. Instantiating a scene composes the hierarchy before it returns

`amadeo_scene::instantiate_with` calls `propagate_transforms` on success. A loaded world therefore
has a correct `GlobalTransform` on everything, immediately, with no tick required.

Placed there rather than in `App::load_scene` because `amadeo-scene` is the crate that *creates* the
hierarchy — it already depends on `amadeo-transform`, so this adds no edge to the crate graph — and
because instantiation is the operation whose result was incomplete.

Placed in the engine rather than asked of each game for the reason this project keeps returning to:
**the step that would be forgotten is the step whose absence has no symptom.** Nothing crashes; a
child is simply somewhere else for one tick.

**Safe by construction.** `GlobalTransform` is derived, so ADR 0019 keeps it out of the state hash
and computing it earlier cannot move a replay, a golden recording or a snapshot's integrity check.

### 2. A body's pose is written back in the space its `Transform` is written in

A `PhysicsBackend` is handed world poses and returns world poses (ADR 0036). A `Transform` on a child
is **relative to its parent**. `step_physics` wrote one straight into the other, so the world position
was stored as if it were local and propagation then applied the parent *again* — every tick, forever.

The result is a parented collider that walks away from its piece by the piece's own offset, once per
tick. In the Warren it read as a level scattered across a hundred metres, and it had been that way
since the day room pieces were written. **It was invisible on tick one**, because nothing had
propagated yet and the fallback to the local transform was still in play — which is exactly why a
capture taken at tick one looked fine.

So the write-back converts through the parent's inverse (`Mat4::inverse_rigid`), and:

- **A static body is not written back at all.** Its pose is authored, not simulated; the solver
  returns what it was given. This is right on its own terms and it covers every case in the
  repository, since a piece's colliders are level geometry.
- **A parent's scale is dropped**, as it is everywhere else in this crate. A collider carries its own
  size and nothing scales a shape, so a body under a scaled parent lands in the right place with an
  unscaled collider. A scaled physics body is ill-defined and reporting it would need an error
  channel `step_physics` does not have.

### 3. A root's pose is read from its own `Transform`, not from the composed one

For a root the two are equal whenever propagation is current — and the local one is **fresher**,
because the composed one is always a tick old. Reading the composed one meant that anything writing
a `Transform` between ticks was silently undone by the next step: a teleport, a level transition, or
a test standing the player somewhere.

That fault was *dormant* until decision 1 landed, because before it a freshly loaded world had no
composed transform to prefer. Fixing one exposed the other, which is the useful part of the story:
**the fallback was hiding both.**

## Consequences

- **A world is correct before its first tick**, so `--ticks 0` is once again a meaningful thing to
  ask for. `amadeo capture` still defaults to one tick and should stay that way — a tick is also
  where gameplay's own first frame happens — but the hazard behind that default is gone.
- **A prefab may hold as many colliders as it likes.** That is what ADR 0071's piece library needs
  and it now works rather than appearing to.
- **`amadeo check` remains not a load.** It validates text against a schema; neither of these
  defects was visible to it, and both were visible the moment something stood on the floor. The
  cheapest real check is still `amadeo capture --ticks 5`, and the honest one is a test that plays.
- **Two engine reads of `GlobalTransform` are now asymmetric on purpose** — child from the matrix,
  root from its own transform. Anywhere else that composes a world pose should follow the same rule,
  and `modules/amadeo-interaction` already does.

## Rejected alternatives

**Requiring a physics body to be a root.** What bevy_rapier effectively asks for, and it would have
been the cheaper fix. Rejected because it forbids ADR 0071's pieces: a prefab has one root, so a
doorway could not carry its own colliders and geometry would have to be emitted loose by the
generator — which is precisely what ADR 0071 §2 refuses.

**Running `propagate_transforms` inside `Simulation`, before physics.** Would fix the staleness for
children too, and costs a full hierarchy walk every tick in the middle of the simulation for the
benefit of a case that has no other symptom. Left open: if a *moving* parent with a physics child
ever appears, this is the answer.

**Leaving the composition to each game.** Every game already registers the system; the argument for
it is that it is explicit. Rejected on the same ground as `load_environments` and `load_meshes`,
which are also done for the caller: forgetting them is silent.
