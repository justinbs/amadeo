# ADR 0059 — Audio is a collection pass behind a backend, and kira is what goes behind it

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0009, ADR 0031, ADR 0036 ·
**Settles:** `docs/04` §12's library choice

## Context

`docs/04-subsystems.md` §12 has carried audio as three ⚠️ marks since M0: a library choice ("leaning
kira"), a determinism rule, and a list of what it must do. M3's exit gate says *"horror lives or dies
here"*, which makes this the largest unbuilt thing between the engine and that gate.

## Decision

**An `AudioBackend` trait with a required `NullAudio`, fed by a collection pass that reads the world
— and `kira` behind the trait when a real backend lands.**

### The rule everything is shaped by, and it is structural

**Simulation never waits on audio; audio never changes simulation.** Mixing runs on its own thread at
its own rate, so if gameplay could ask *"has this finished playing?"* the answer would depend on how
fast a machine filled a buffer, and invariant I3 would be gone.

Nobody has to remember that, because `Audio` is a **Service** and ADR 0009 puts services outside the
state hash entirely. `audio_cannot_move_the_state_hash` pins it — which is worth having even though
it is structural, because `collect_audio` could easily reach into the world by accident, for instance
by clearing a `playing` flag.

### The third instance of the same shape

A trait, a null backend, a collection pass that hands over everything, a backend that never reaches
back into the world. `RenderBackend` and `PhysicsBackend` already work this way, and the repetition
is the point: it is what makes invariant I7 hold, so the engine runs with no window, no GPU **and no
sound card** — which is how CI runs it.

It is also the only way audio is testable. Nothing can assert on a sound; everything can assert on
the frame that would have produced one.

### An `AudioFrame` is a state, not a set of commands

*"These are the sounds that should be audible now"*, not *"start this, stop that"*. A backend diffs
it against what it is already playing.

That is what makes `AudioSource` declarative: a generator hums **because there is a generator**, and
the hum stops when the entity does, with nobody having remembered to stop it. A scene file can author
it, `describe` can see it, and a snapshot restores it correctly — because the sound is a function of
the world rather than of a call somebody made once. Same argument ADR 0031 made for the camera.

### kira, behind the trait

Put to Justin with the alternatives. `kira` ships tracks, effects, tweens, spatial emitters and
clock-based music scheduling; `rodio` is playback and little else; `cpal` is a raw device callback
with the mixer left to write.

The deciding argument is that **audio is outside the state hash**, so unlike physics there is no
determinism reason to own the mixing. ADR 0036 owns rapier's *interface* because a solver's results
reach a replay; nothing kira does can. So the engine owns the interface and delegates the work, and
**ADR 0036 §4's rule applies unchanged: no kira type may cross `AudioBackend`** — which is what keeps
the choice reversible.

### Gain is applied by the collection pass

A voice reaches a backend with its bus and master gain already multiplied in. A backend that applied
them would have to be told them, and two backends could disagree about the order — where a voice
arriving with its final gain cannot be misread.

### Buses are an enum, not strings

`Effects`, `Music`, `Dialogue`, `Interface`. A bus is the fixed set of things a player has volume
sliders for, not an open vocabulary — so a scene naming one that does not exist fails to load with the
list of ones that do, where a string would silently create a bus nothing can turn down. It also makes
ducking authorable later: *"quiet effects while dialogue plays"* is a rule about two named things.

`Interface` is separate from `Effects` for a reason worth stating: a menu click that gets quieter
because the player is standing near a waterfall is a menu that feels broken.

## Consequences

**No sound comes out yet.** This is the engine-owned half — the trait, the null backend, the
components, the collection pass, and sixteen tests. The kira backend is next, and it is deliberately
separate: the interface is the part that is expensive to change later, and it should be settled and
exercised before a dependency shapes it. Same reasoning that shipped `NullPhysics` before rapier and
the cascade fitting before the GPU half.

**A one-shot has no home.** `AudioSource` describes a *state*, and a footstep is not a state — it is
an event that happens once and is over. Events are what `amadeo-events` is for, and wiring them to
audio is the other half of this subsystem. Naming the gap now, because the tempting fix is a
`play_once` flag on the component and a system that clears it, which puts a write into gameplay state
for something that must not be in the state hash at all.

**A listener is required and there is no fallback.** A world with no `AudioListener` submits an empty
frame rather than guessing where to hear from — guessing is what produces a sound coming from the
wrong side. **The empty frame is still submitted**, deliberately: a backend holding voices from last
frame has to be told they are gone, and skipping the submission would leave a sound playing after
whatever made it stopped existing.

**Whether ears go on the camera or the character is the game's choice**, and it is audible. On the
camera, the viewer hears what they can see, which suits third person; on the character, the horror
case works when the camera swings away.

**A spatial sound should be mono**, which surprises people: a stereo recording already has its own
left and right, so a position has nothing left to decide. Documented on `SoundData::channels` rather
than enforced, because refusing to place a stereo sound would be refusing something that merely
sounds wrong.

## Alternatives rejected

**`cpal` and write the mixer.** The posture the engine takes for `Rng`, `StableHasher` and its own
trigonometry — and those are all things whose results reach the state hash. Audio's do not. A correct
mixer is resampling, click-free starts and stops, denormals and spatialisation, none of which is what
M3 is judged on.

**A `play()` call on the service rather than a collection pass.** Simpler for a footstep and wrong for
everything else: a sound started by a call has to be stopped by another call, which means a handle,
which means gameplay holding a reference to something outside the state hash. The declarative form has
no handle to leak.

**One list of sounds on the frame with a kind tag** rather than a listener plus voices. The listener
is one thing per frame and every voice needs it; folding them together would put an `Option` on every
voice for a fact that belongs to the frame.
