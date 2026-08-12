# ADR 0060 — A missing sound is silence, and the backend's only test is a person

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0021, ADR 0036, ADR 0059 ·
**Answers:** Q12, in the negative

## Context

ADR 0059 built the engine-owned half of audio — the trait, `NullAudio`, buses, components, the
collection pass, `VoiceTracker` — and deliberately stopped before the kira backend, on the grounds
that the interface is the expensive part and should be settled first. This is the other half.

Three things had to be decided to write it, and none of them is about kira.

## Decision

### 1. A missing sound is silence, and there is no placeholder

ADR 0021 says a missing asset produces a **visible stand-in plus a structured report** rather than a
crash. `TextureCache` implements that in three steps, ending in a magenta check built in code so the
last resort cannot itself be missing.

**`SoundCache` deliberately implements only the second half.** There is no placeholder sound, and
there must not be one.

The reason is that the argument for a visible stand-in does not carry over. Magenta works because it
is *unmistakably not content* — nobody ships a magenta-and-black check. **Audio has no equivalent.**
A beep, a tone, a burst of noise: each is indistinguishable from something a game might legitimately
play, so a placeholder sound turns a broken asset into a design choice. Worse, it is played
repeatedly, at the volume and in the position the missing asset would have had.

So a sound that will not load is silent, and `SoundCache::failures` is the whole of the diagnosis.
That is a real weakening — a silent game is less obviously broken than a magenta one — and it is
accepted because the alternative is worse in the case that matters.

This is not "audio is special". It is the general rule made explicit: **ADR 0021 wants a stand-in
that is legible as a stand-in, and where no such thing exists, the report is the whole answer.**

### 2. The kira backend's verification is a person listening, and that is written down

**No test in this repository can tell you whether the kira backend works.** CI has no sound card,
neither does a headless run, and even on a machine with a device nothing in the process can read back
what left through the operating system.

Two things follow, and both are structural rather than aspirational:

- **The backend contains as little decision-making as possible.** Everything that could be wrong
  invisibly — which voices are new, which have gone, which merely moved — is in `VoiceTracker`, which
  ADR 0059 pulled out for exactly this reason and which is exercised headlessly. What is left in
  `kira_backend.rs` is the part that genuinely needs a device.
- **The listening procedure is committed, not remembered.**
  `crates/amadeo-audio/tests/you_can_hear_it.rs` holds two `#[ignore]`d tests that open a device,
  play for a few seconds, and print what you should hear. They are excluded from `cargo test` by the
  `#[ignore]`, so CI never runs them, and the invocation lives in the repository instead of in
  somebody's shell history.

They still assert everything up to the speaker — that a device opens, a sound uploads, and no frame
submission errors — so they are not decorative. The last step is the listener's, and each file says
so plainly rather than being named as though it covered more.

### 3. Q12 does not bite, and the reason generalises

Q12 has predicted since the Q1 spike that `kira::AudioManager` would be the first thing unable to
satisfy `Service: Send + Sync`, and proposed a separate `LocalService` store (the standing prior), a
relaxed bound, or a `Mutex`.

**In kira 0.12 the manager and every handle are `Send + Sync`**, checked by compiling the bound
rather than by reading the source, with a control case that fails. So none of the three was needed:
the manager goes into the service like any other value.

The reason is worth keeping. kira's desktop backend does not hold the `cpal` stream itself — it hands
it to a stream-manager thread and keeps a controller. **A library that already owns a thread has
usually had to become `Send + Sync` in order to.** That is a useful prior for the next candidate, and
it points the suspicion at libraries that expect to be driven from *your* thread rather than at
libraries that are "low level".

**Q12 therefore stays open**, with kira struck off its list of offenders. Deciding it now would be
deciding it speculatively, which is what its own entry has said to avoid since it was written.

## Consequences

**`games/atrium` enables `amadeo-audio/kira` by default**, the same trade it already makes for
rapier: a demo of the audio system that makes no sound is not a demo. Feature unification means
`cargo test --workspace` builds `cpal`, and on Linux that needs ALSA's headers — CI installs them.
This is a *build* requirement, not a runtime one; the runner still has no device and nothing plays.

**The Atrium's two `.wav` files are generated from text** by `cargo run -p atrium --bin tone`, which
is `games/vault`'s `pix` argument applied to audio: a `.wav` is not diffable, so the source is a
table of frequencies and the file is derived. It uses `amadeo_core::sin_cos_degrees` rather than
`f32::sin` — not for ADR 0044's reason, since nothing here reaches the state hash, but for the
mundane one that a generator whose output can differ from itself is not a build step.

**Two sounds, one spatial and one not**, deliberately. They are the two different paths through the
backend — a placed voice gets its own positioned track, a non-spatial one plays on its bus directly —
and no test can tell those paths apart. Hearing one move and the other stay put is the check.

**A one-shot still has no home**, unchanged from ADR 0059. Nothing here makes it closer or further.

**Per-bus effects and ducking are now cheap.** The backend creates one mixer track per `Bus` even
though gain is already applied per voice by the collection pass, so "quiet effects while dialogue
plays" is a change to one track's volume rather than a restructuring.

## Alternatives rejected

**A placeholder sound after all** — a short beep under the missing id. Rejected above: it is
indistinguishable from content, and unlike magenta it repeats.

**Raising an error when a sound fails to load**, rather than reporting it. This is ADR 0021's
survivable case: gameplay may not ask whether an asset has loaded, so a voice can legitimately name
something still on its way. Failing would make a headless run stricter than the game.

**Spatialising in the engine** — computing attenuation and panning ourselves and setting volume and
pan on a plain voice. This is the mixer-writing ADR 0059 rejected when it chose kira over `cpal`, one
layer up, and the collection pass already declines to pre-attenuate for exactly this reason: a
backend that does its own spatialisation would then do it twice.

**A `Mutex` around the manager anyway, for safety.** Nothing to be safe from — the bound is
satisfied, and a lock nothing contends is a lock that misleads the next reader about why it is there.
