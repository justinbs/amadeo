# ADR 0037 — A character is a move-and-slide query, and the character itself is a module

**Status:** Accepted · **Date:** 2026-08-05 · **Builds on:** ADR 0002, ADR 0005, ADR 0009, ADR 0036

## Context

M2's exit gate 1 wants "a physics-driven character controller you can walk around with". Nothing in
the engine can express one yet, and the reason is precise rather than a matter of missing convenience.

`PhysicsBackend` has exactly one operation: `step(bodies, gravity) -> results`. For a `Kinematic`
body that means *put it exactly where gameplay said*. Rapier's kinematic bodies push dynamic bodies
around but pass **straight through static geometry** — so a character built on today's trait would
walk through every wall in the level. Something has to be added to the trait, and what gets added is
the expensive part: every backend must implement it, `NullPhysics` must have an honest answer for it,
and every game writes its movement code against its shape.

This is the fifth time in this project a subsystem question has looked like it was about the
mechanism and turned out to be about the data next to it. The pipeline is isolated behind a trait; the
decision is what crosses the trait.

## The vocabulary, because it is what the choice turns on

- A **shape cast** sweeps a shape (here, a capsule) through space and reports the first thing it hits.
- **Move-and-slide** is that in a loop: hit a wall and slide along it rather than stopping dead; walk
  into a small step and rise over it; walk down a ramp and stay stuck to it rather than launching off.
- **Depenetration** is pushing a shape back out when it starts a tick already overlapping something.

Each of those three is individually subtle and each is a well-known source of bugs.

## What the research found

**Every mature engine ships an explicit kinematic controller as its primary answer**, and treats a
dynamic-rigid-body character as the alternative for physics-heavy cases:

| Engine | Primary | Mechanism |
|---|---|---|
| Unity | `CharacterController` | kinematic capsule, `Move()`, collide-and-slide, `isGrounded` |
| Unreal | `CharacterMovementComponent` | kinematic capsule, sweep-based, own step-up/step-down |
| Godot | `CharacterBody3D` | `move_and_slide()` over `move_and_collide()` |
| PhysX | character controller module | kinematic, "movement computed outside the simulation" |

Godot's documentation states the split most usefully: *RigidBody is for simulation, CharacterBody is
for custom physics* — a platformer is not simulating reality, it is designing an experience.

The line that decided it for **this** engine, though, is not about feel. The recurring advice is to
choose an explicit controller **when deterministic replay and network sync are priorities**. That is
invariant I3 and ADR 0006, which are not preferences here.

**And rapier already implements the hard parts.** `KinematicCharacterController::move_shape` handles
auto-stepping, slope-climb limits, ground snapping and the sliding iteration.

## Decision

### 1. `PhysicsBackend` gains one query: move a shape and slide

A new method taking a shape, a start pose and a desired motion, returning where the shape actually
ended up and whether it is standing on something. Every type crossing the boundary is one
`amadeo-physics` defines — **ADR 0036 §4 is unchanged**, and no rapier type appears in a component, a
scene file, a snapshot or the state hash.

The whole move-and-slide is the query, **not** a raw shape cast with sliding left to the caller. Two
reasons:

- Rapier's implementation of stepping, slope limits and ground snapping is already written and
  already careful. Reimplementing it above the trait would mean writing several hundred lines of
  numerically delicate geometry in a module, and writing it a second time for `NullPhysics`.
- It is honestly genre-agnostic. "Move this shape through the world and slide along what it hits"
  describes a lift, a moving platform, a projectile, and a camera that must not clip through a wall.
  It says nothing about characters.

### 2. The query is a **query**, so it adds no persistent state

Rapier's `broad_phase.as_query_pipeline(...)` is a *borrowed view* over the body and collider sets
`RapierPhysics` already owns. It allocates nothing that outlives the call and caches nothing between
calls, so it adds **no new determinism surface** and nothing new for `PhysicsBackend::reset` to clear.
That was checked in rapier's source before this ADR was written, not assumed.

### 3. It runs **after** the step, and that ordering is load-bearing

The query reads a spatial index that `pipeline.step` builds. Running the character move *before* the
step would query last tick's index — which is harmless for static level geometry, wrong by one tick
for moving platforms, and **completely wrong on tick 1, where the index is empty and the character
would walk through the level exactly once**.

So the character system is registered `.after(STEP_PHYSICS)`, and the module's registration helper
does that rather than leaving it to each game to remember. A rule a game can forget is a rule that
produces a bug on the first tick only, which is the hardest kind to notice.

### 4. The mechanism is in `amadeo-physics`; the *character* is in `modules/`

`CLAUDE.md` trap 10 says the engine must not assume a game has a character, and invariant I4 says
genre knowledge lives above the crate layer. The split follows exactly:

- **`amadeo-physics`** owns the query. It has no concept of a character, a walk speed or a jump.
- **`modules/amadeo-character`** owns `CharacterController` (walk speed, acceleration, jump speed,
  slope limit, step height) and `CharacterMotion` (velocity and grounded state), reads named input
  actions, calls the query, and writes the result back to `Transform`.

This is the same line Godot draws between `PhysicsBody3D::move_and_collide` and
`CharacterBody3D::move_and_slide`.

**This creates the `modules/` layer**, which `CLAUDE.md` §4 has reserved since session 1 and nothing
has occupied. Two rules come with it, and they are just I6 restated one level up:

- A module may depend on engine crates. **No engine crate may depend on a module**, ever.
- A module may depend on another module, but the module graph is a DAG like the crate graph.

The `modules/` path is what marks a crate as a module; there is no name prefix. A `use` site reads
`amadeo_character::CharacterController`, and `Cargo.toml` is where the layer is visible.

### 5. The module is **not** gated behind the `rapier` feature

`NullPhysics` implements the query degenerately: it applies the requested motion unmodified and
reports not-grounded. That is the same posture it already takes for `step` — a backend without a
*solver*, not a stub — and it means the module compiles and runs in every build (I7).

It also buys the evidence check this project learned the hard way last session: **a test is not
evidence until you have watched it fail.** Pointing the character tests at `NullPhysics` makes the
character walk straight through the wall, which is what proves the passing rapier test is measuring
collision response rather than an accidentally-correct constant.

## Alternatives rejected

**A dynamic rigid body with locked rotation.** By far the cheapest — one `lock_rotation` flag on
`RigidBody` and no trait change at all — and it gives emergent interaction for free: an explosion
shoves the character with no code. Rejected because movement becomes a negotiation with the solver
rather than an instruction to it: stairs are genuinely hard, slope limits are unstable at the
boundary, and tuning is indirect. The deciding argument is the one the research kept repeating, which
is about replay and netcode rather than feel.

Worth recording that this stays available. A `RigidBody` character needs nothing this ADR forbids, so
a game wanting ragdoll-ish physical comedy can still have one.

**Raw casts only, with the module implementing slide, step and snap.** The most general option and the
most work, and it duplicates in a module — twice, counting `NullPhysics` — geometry rapier already
ships. Rejected as the option that looks principled and costs the most for the least.

A raycast query is still worth having and the M2 build list names one. It should land with its first
real consumer rather than being invented alongside this, which has one.

## Consequences

- `PhysicsBackend` is wider by one method, and a future backend has one more thing to implement. The
  trait is small enough that this is a real cost worth naming rather than a rounding error.
- A character is a `Kinematic` body, so it is still handed to `step` and still known to the solver —
  which is what lets dynamic bodies collide with it.
- **Pushing dynamic bodies is not fully wired.** The character's new position reaches rapier through
  `set_translation`, which is a teleport rather than a kinematic sweep, so the character does not
  impart velocity to a crate it walks into. Gate 1 does not ask for this. When something does, the fix
  is `set_next_kinematic_translation` inside the rapier backend and nothing above it changes.
- The character's velocity lives in a reflected, hashed component, so a character-driven game is
  snapshot-able and replayable for nothing — the same property ADR 0036 bought for rigid bodies.
