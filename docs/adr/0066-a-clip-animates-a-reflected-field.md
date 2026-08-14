# ADR 0066 — A clip animates a reflected field, and a player's clock is gameplay

**Status:** Accepted · **Date:** 2026-08-14 · **Builds on:** ADR 0012, ADR 0019, ADR 0020, ADR 0021,
ADR 0027, ADR 0032 · **Settles:** `docs/04` §14's first two questions

## Context

`amadeo-anim` was the last named M3 subsystem with nothing in it. `docs/04` §14 carried three
warnings: that sprite animation, skeletal animation, blend trees and tweens are four fairly separate
systems and somebody has to say which are engine and which are modules; that animation state machines
and gameplay state machines "want to be the same abstraction"; and that all of it must be
tick-deterministic, because animation driving gameplay — hitboxes on frames — is common.

The decision that is genuinely expensive is none of those three directly. It is **how a clip names
the thing it animates**, because that is the file format, and by invariant I2 a file format is a
designed, byte-stable artifact that every `.anim` ever written depends on.

## Decision

### 1. A track names a **component and a field**, and the engine writes through reflection

```
track
  component "Transform"
  field "translation"
  interpolation Linear
  keys
    - time 0.0
      value 0.0 1.0 0.0
    - time 1.5
      value 0.0 2.5 0.0
```

This is Godot's, Unity's and Bevy's model, and here it costs almost nothing to build: a `Component`
is `Reflect` by trait bound (ADR 0013), so reading one as a `Value`, patching one field and rebuilding
it is machinery that already exists — it is exactly what ADR 0029's prefab overrides do.

**The payoff is that adding an animatable property is never engine work.** A light's intensity, a
material's colour, a sprite's region on a tilesheet, a UI panel's paint — all of them animate today,
and so will everything added later, because a clip names fields rather than knowing types. That is
invariant I8's claim being spent rather than merely held.

Rejected: a **closed set of typed tracks** (translation, rotation, scale, colour…). Faster, and it is
the shape skeletal animation will need — but it makes every new animatable property a feature request
in a project whose whole argument is that the schema describes itself. Also rejected: **both**, which
gives two spellings for animating a translation and forces `amadeo fmt` to pick one, silently
rewriting whichever a person hand-wrote.

The honest cost: a read-patch-write round trip per animated component per tick. Fine for the tens of
entities property animation involves, hopeless for hundreds of skinned bones — so **skeletal
animation will need its own typed path**, and §5 says why that is fine.

### 2. A key's value is a list of numbers, and its width comes from the target

A key carries `Vec<f32>` and the track does not declare a width. Sampling coerces into whatever shape
the field already holds: one number into an `F32`, three into a three-element list, a rounded number
into an integer field. A mismatch is reported by name rather than guessed at.

That is what lets one track type animate a scalar, a vector, a colour and a tilesheet index without a
variant per shape — and it means the *component's* schema stays the single description of what its
fields are, rather than being restated in every clip.

### 3. The player's clock is hashed, and so is everything it writes

`AnimationPlayer` holds a clip id, a time, a speed, whether it loops and whether it is playing —
`AudioSource`'s shape, and hashed for the same kind of reason and a stronger one. Animation here
**writes gameplay components**: a clip that moves a `Transform` is a moving platform you can stand on,
and `docs/04` §14 requires hitboxes on frames to reproduce exactly.

So there is no derived half in this ADR, and that is worth stating plainly because the reflex from
`GlobalTransform` and `ComputedRect` points the other way. Property animation *is* simulation. The
derived half appears with skinning, where a pose becomes joint matrices that only a shader reads.

Time advances by `FIXED_DT * speed` and interpolation is linear — `+ - * /` only, so ADR 0044's ban
on transcendentals is respected without trying.

**Rotation is interpolated as Euler degrees**, because ADR 0018 says that is what a `Transform` holds.
It takes the long way round past 180°, which is right for a door and wrong for a tumbling object; the
answer when it matters is a quaternion track, and it is additive.

### 4. A clip may only touch components a game has allowed

`Animatable` is a service holding the component types animation may write, each registered with
`allow::<T>()`.

This exists because of a structural fact rather than a design preference: **`ComponentRegistry` is
owned by `App`, not by the `World`, so a system cannot reach it.** An allow-list was the way to write
this without moving the registry, and it turns out to be worth having on its own merits — animation
cannot reach into `RigidBody::kind` or `Collider::shape` and produce a world the solver disagrees
with, because a game never allows those.

A track naming a component that is not allowed, or a field that does not exist, is **recorded and
reportable**, never silent. ADR 0060's rule: a subsystem that can produce nothing must be able to say
why.

### 5. What is deliberately not here

- **Skeletal animation and skinning.** A different data path — joint palettes, inverse bind
  matrices, a vertex shader — and it needs a rigged model the repository does not have. Nothing here
  blocks it, and §1's cost note says where it will diverge.
- **Blending and blend trees.** One clip per player. Blending needs a pose to blend *into*, which is
  the skeletal representation above.
- **A state machine, and it is not going to be shared with AI.** `docs/04` §14 suggested animation
  and gameplay state machines want to be one abstraction. They do not, and the industry is one-sided
  about it: Unreal ships AnimBlueprints and Behavior Trees as separate systems on purpose, and Unity
  and Godot have animation state machines and nothing at all for AI. The reason is concrete — **an
  animation transition is a blend over time and an AI transition is instantaneous with side
  effects** — and a shared abstraction is clumsy at both. Keeping them apart is also the reversible
  direction: two systems can be unified later, and a unified one cannot be split without breaking
  everything built on it.

## Consequences

**A missing clip changes the state hash.** This is the first asset whose absence alters *simulation*
rather than presentation — a missing texture draws magenta, a missing sound is silence, a missing clip
means a platform does not move. ADR 0021's barrier means the answer is known before the first tick and
is the same on every machine that has the same files, so a replay is safe; but a machine missing the
file simulates a different world. `ClipCache::failures` and `App::asset_problems` are what make that
visible rather than mysterious, and it is the strongest reason yet to run `amadeo check` on a game's
assets.

**Animation runs in `Simulation`.** Not `PostSimulation` — a clip writes a `Transform` that physics,
the character controller and `propagate_transforms` all read this tick.

**Nothing installs itself.** A game inserts `Animatable`, allows its component types, and adds the
system. `amadeo-anim` sits below `amadeo-app`, so it cannot offer the `install(&mut app)` that a
module can (invariant I6). The allow-list being explicit is what keeps that from being an invisible
setup step: an unallowed component is reported by name.
