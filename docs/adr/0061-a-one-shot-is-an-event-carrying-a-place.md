# ADR 0061 — A one-shot is an event carrying a place, played once per sequence number

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0009, ADR 0059, ADR 0060 ·
**Closes:** ADR 0059's named gap

## Context

ADR 0059 built audio as a collection pass over a *state*: an `AudioSource` says "this entity is
making this sound", and it stops when the entity does. It then named what that cannot express and
refused to guess:

> **A one-shot has no home.** `AudioSource` describes a *state*, and a footstep is not a state — it
> is an event that happens once and is over. […] the tempting fix is a `play_once` flag on the
> component and a system that clears it, which puts a write into gameplay state for something that
> must not be in the state hash at all.

M3's exit gate says *"horror lives or dies here"*, and a horror game with no footsteps is not one.

## Decision

**A one-shot is a reflected `SoundPlayed` event carrying a world position, drained by the collection
pass and played exactly once per sequence number.**

### 1. An event, which is what splits the decision from the playing

The interesting property is that this splits in exactly the right place with no new machinery:

- **Deciding** a footstep happened is gameplay. It depends on how far the character walked, it must
  reproduce in a replay, and a save must restore you mid-stride. `Event` requires `StableHash`
  already, precisely because *"queued events are part of simulation state at a tick boundary"* — so
  a queued `SoundPlayed` is in the state hash and two runs that disagree about footsteps have
  genuinely diverged.
- **Playing** it is machinery, and `Audio` is a `Service`, which ADR 0009 puts outside the hash.

Neither half needed anything invented. That is the argument for events over the alternatives below:
the boundary the problem has is already the boundary the mechanism has.

### 2. It carries a **place**, not an entity

A `Voice` is keyed on the entity making it, because a voice *continues* and a backend has to tell
"still going" from "started again". A one-shot has no such problem and is deliberately given no
identity — an identity would invite a backend to decide a footstep is "still playing" and decline the
next one.

The position is a plain `[f32; 3]` with a `spatial: bool` beside it, mirroring `AudioSource` exactly
rather than inventing a second spelling for one idea.

**Following an entity is refused for now, and the reasoning is worth keeping.** A one-shot is over in
a fraction of a second, so following would buy a pan change of about a metre at running speed and
cost a lifetime question nobody wants to answer: what a footstep does when the thing that made it is
despawned mid-sound. Every major engine draws the same line and offers both — Unreal's
`PlaySoundAtLocation` against `SpawnSoundAttached`, Unity's `PlayClipAtPoint` against
`PlayOneShot` on an existing source — with the fixed-position one as the common case. Adding a
`follow` field later is additive.

### 3. Played once per **sequence number**, not once per frame

This is the subtle part and it is a correctness requirement rather than an optimisation.

`collect_audio` runs in the `Render` stage; event buffers swap at the **tick** boundary; and the
windowed loop renders as fast as it can. So a single footstep event sits in the readable buffer
across *every frame drawn during that tick*. Reading it naively plays one footstep per rendered
frame — five overlapping copies at 300 fps, one at 60 fps. **A bug whose symptom depends on the
frame rate**, which is the worst kind to be handed.

`EventRecord::sequence` is strictly increasing across all event types, so a high-water mark on the
`Audio` service is enough. It lives on a service rather than in a resource deliberately: how many
times a frame happened to be drawn is exactly the sort of thing that must never reach a replay.

**The bound is half-open — "the lowest sequence not yet played" — and that is not a stylistic
choice.** `EventClock` hands out sequence numbers starting at **zero**, so a field meaning "the
highest already played", initialised to `0` and filtered with `sequence > mark`, drops event number
zero. It did. The symptom was the first sound a world ever makes being silent and every one after it
working, which is close to undiagnosable by ear.

### 4. `AudioFrame` gains a second list, and it is honest about what it is

`one_shots` sits beside `voices` and is documented as **the one part of the frame that is not a
state**. The rest of an `AudioFrame` can be handed to a backend twice and must produce the same sound
once; these must be played once per entry and never diffed. `VoiceTracker` does not look at them at
all.

Folding them into `voices` with a flag was considered and rejected: every reconciliation path in
`VoiceTracker` would then need a branch for entries it must not reconcile, which is where the
"restarted every frame" class of bug comes from in the first place.

## Consequences

**Where a footstep comes from is the game's business.** `games/atrium` owns `Stride` and
`play_footsteps`; `modules/amadeo-character` knows nothing about them. Invariant I4 one level up: the
module knows how to move, the game knows what moving sounds like. A floating drone and a person in
boots share a controller and should not share a gait.

**A game must register the event.** `App::register_event::<SoundPlayed>()` is what arranges the
buffer swap; without it a footstep is sent and never read. The engine does not register it centrally,
because a game with no one-shots should not pay for a queue.

**One tick of latency**, because events are double-buffered. That is 16 ms and inaudible.

**A one-shot in a world with no ears is dropped**, like every voice. Not "heard from nowhere" — a
footstep nobody was there for.

**Still missing: pooling and a voice cap.** Nothing limits how many one-shots a frame may contain, so
a game that emitted a thousand would ask the mixer for a thousand tracks and be refused, once per
sound, with `BadSound`. That is a survivable and reported failure rather than a crash, and a cap is a
tuning question best answered when something real hits it.

## Alternatives rejected

**A `play_once` flag on `AudioSource` plus a system that clears it.** Named and refused in ADR 0059:
it puts a write into gameplay state for something that must not be in the state hash, and it makes
every entity that has ever made a noise carry a field about it forever.

**A `play()` method on the `Audio` service.** ADR 0059 rejected this for looping sounds because a
sound started by a call needs a handle to stop it. A one-shot needs no handle, so the objection is
weaker here — but it would put the *decision* to make a sound outside the state hash, which is
exactly backwards: a footstep that does not reproduce in a replay makes the replay a worse record of
what happened.

**Deduplicating by content rather than by sequence** — "have I already played a footstep at this
position this tick?". Wrong in the case that matters: a burst of gunfire is several identical events
in one tick and must be several sounds.
