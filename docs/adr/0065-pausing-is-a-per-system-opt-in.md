# ADR 0065 — Pausing is a per-system opt-in, and the tick never stops

**Status:** Accepted · **Date:** 2026-08-13 · **Builds on:** ADR 0005, ADR 0009, ADR 0016, ADR 0063 ·
**Resolves:** Q35

## Context

M3's exit gate asks for *"title screen → playable loop → lose state → win state → pause → save → quit
→ resume"*. Everything a screen is **made of** exists after session 16 — a menu is a scene file,
`UiNode::visible` hides one, focus navigation works, `UiActivated` says a button was chosen — and
nothing exists above them. Q35 asked four questions: where the current screen lives, what a
transition does to the world, whether pausing stops the simulation, and whether any of this belongs
in the engine at all.

Only one of the four is expensive to get wrong, and it is the third. A pause menu has to keep running
while the world it is drawn over does not, so **something has to decide, per system, what still runs**
— and that decision reaches into the schedule, which every game and every module registers against.

## Decision

**A hashed `Paused` resource; the gameplay stages skip every system that did not ask to run while
paused; the tick counter never stops; and the engine has no concept of a screen.**

### 1. The tick keeps advancing, and that is what makes the rest work

Freezing the tick was the obvious reading of "pause" and it is wrong here, for a specific reason:
menu navigation is **hashed state driven by hashed input** (ADR 0063), and `amadeo-input` records
input **per tick**. Stop the tick and a keypress in a menu has nowhere in the replay to live.

So the tick advances, `PreSimulation` runs, and a paused tick is one that samples input, moves the
focus if asked, and does nothing else.

Two things fall out for free:

- **There is no burst on unpause.** Q35 worried that a paused game would bank real time and then run
  a flood of catch-up ticks. It cannot: `App::advance_real_time` keeps consuming its accumulator on
  cheap paused ticks, so the backlog is zero the whole time and **nothing in the loop changes**.
- **A pause is recorded and reproduces.** A replay containing a pause replays the pause, at its real
  length, with no change to the replay format.

The cost is honest and small: a replay of a session with a long pause carries those ticks. Input is
recorded as changes, so a paused tick with nothing pressed is nearly free.

### 2. Pause skips the *gameplay* stages, not all of them

`PreSimulation` always runs in full. It is definitionally the stage before gameplay, input must be
sampled or nothing could ever unpause, and a stage that could not run would make pausing
irreversible.

`Simulation` and `PostSimulation` are the gameplay stages and both are skipped. **Both**, and the
second one is not an afterthought: `games/atrium`'s `play_footsteps` runs in `PostSimulation` and
reads the character's velocity, which does not change while paused — leave that stage running and a
paused game taps out footsteps forever, in a room nobody is walking through.

`Render` and `Present` are untouched, which is the whole point: the menu is drawn.

### 3. A system opts in with `.while_paused()`

```rust
app.add_system(
    Stage::Simulation,
    system(NAVIGATE_FOCUS, navigate_focus).while_paused(),
);
```

One flag beside `before` and `after`, on the registration a game already writes. The menu therefore
stays in `Simulation`, where it belongs semantically — it is hashed state changing in response to
hashed input — rather than being exiled to a stage chosen for its scheduling side effects.

**This is what the other engines do.** Unreal has `bTickEvenWhenPaused` per actor; Godot 4 has
`process_mode` per node, with `Pausable`, `WhenPaused` and `Always` among its values. Two of the three
large engines make "does this keep running" a property of the *thing*, not of a global rule, and the
one that does not — Unity, where `Time.timeScale = 0` stops physics while `Update` keeps running —
is the one where every game re-implements a pause flag by hand.

`schedule.list` reports the flagged systems in a `runs_while_paused` array beside `systems`, because
"why did my system not run" is a question an agent must be able to answer without reading the game's
source. Additive: the existing `systems` array is unchanged.

### 4. `Paused` is hashed, and read once per tick

Hashed because whether you are paused is gameplay state: a save should restore it, and a replay must
reproduce it or the tick counts diverge. It is an ordinary reflected resource, so `amadeo query` sees
it, a snapshot restores it, and a scene file could author it.

Read **once at the top of `step`**, so pausing takes effect on the following tick rather than
half-way through the current one. Both are deterministic; this one is the one a person can reason
about, because "which systems ran this tick" does not then depend on where in the schedule the toggle
happened to sit.

**The engine never writes it.** A game decides that Escape means pause, which is invariant I4 one
level up — the same split ADR 0061 used for footsteps and ADR 0063 for buttons.

### 5. The engine has no concept of a screen

Q35's other three questions are all answered by not answering them. **What screens exist is genre
knowledge**: a horror slice has a title, a pause and a death screen; Stellaris has none of those in
that shape. Putting a `Screen` type below the module layer is what invariant I4 forbids, and the
engine cannot know a game's screen names anyway, so the type would have to be stringly-typed and that
string would go into the state hash.

A game declares its own enum resource, and everything already works: it is reflected, hashed,
snapshot-restored, and visible to `amadeo query`. `games/atrium` does exactly that.

The project's own rule decides when this changes: **something moves to `modules/` when a second game
wants it**, which is how `amadeo-camera` got there. One game is not evidence about shape.

### 6. What a transition does to the world stays undecided, deliberately

Q35's second question — whether loading a level despawns and rebuilds — is not answered here, and
nothing above forecloses either answer. That was the trap Q35 named: a screen system that despawns
the world cannot have a pause menu retrofitted onto it. Pausing does not touch entity lifetimes at
all, so the expensive mistake is not available.

## Consequences

**`Schedule::run` takes a `paused` flag.** Two callers, both in `App`. The `Render` and `Present`
stages always pass `false`, and a comment says why.

**A game with no pause pays nothing.** No `Paused` resource means never paused, checked with one
`Option` lookup per tick.

**A system that must run while paused and forgot to say so simply stops.** That is a visible failure
— the menu does not move — rather than a silent one, which is the right way round. The reverse
mistake, flagging something that should freeze, is the one to watch for: it looks like the game
carrying on quietly underneath the menu.

**Pausing is not a slow-motion or time-scale mechanism**, and should not become one. A time scale
that is not an exact multiple of the fixed timestep is a determinism problem (ADR 0005); if slow
motion is ever wanted, it is "run the simulation stages every *n*th tick", which is a separate
decision.

## Alternatives rejected

**A sixth stage that always runs.** Honest naming, and it makes the mistake in §3's consequences
impossible — a gameplay system cannot end up in a stage called `Interface` by accident. Rejected
because `Stage::name` is on the wire (ADR 0016), so a sixth stage is a protocol change that shows up
in `schedule.list`, in the CLI, in the docs, and in every game author's model of a tick, forever. It
is also coarse: a system wanted in both cases has to be registered twice.

**A full `Screen` state machine with general run-conditions**, which is Bevy's model and the closest
match to Amadeo's shape. It answers pause, title, game-over and loading with one mechanism. Rejected
for §5's reason — it puts genre knowledge in the engine — and because general run-conditions are
substantial new schedule machinery whose first and only caller would be a boolean.

**Overloading `PreSimulation` as "the stage that runs while paused"**, so the menu goes there and no
flag is needed. Free, and briefly tempting. Rejected because it gives an existing stage a second,
invisible meaning: a game putting a gameplay system in the stage documented as "anything that must be
settled before gameplay runs" would find it still running while paused, with nothing to explain why.
That is the defect shape Q32 named, and this project has been bitten by it three times.

**Freezing the tick counter.** Covered in §1: it strands menu input outside the replay format.
