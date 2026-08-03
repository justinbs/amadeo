# ADR 0028 — A snapshot is a text file, and it captures the entity allocator

**Status:** Accepted · **Date:** 2026-08-03 · **Depends on:** ADR 0027 · **Delivers:** the priority ADR 0011 identified

## Context

ADR 0011's spike measured what actually degrades the agent's iteration loop, and the answer was not
what the question assumed. It was **not compile time** — a gameplay edit rebuilds in 0.9 s. It was
**re-simulation**: getting back to the moment of interest costs about 21 µs per tick and grows
linearly with session length, 382 ms to reach five simulated minutes and worse every minute after.

ADR 0011 named snapshots as the answer and deferred them. ADR 0027 unblocked them by making every
resource reflectable — `snapshot.restore` is exactly `from_value(to_value(x))` with a file in the
middle, so a world with an unreflectable resource is a world that cannot be restored.

## Two things this decision did not get to choose

**A snapshot must be a file, not an in-memory handle.** ADR 0016 makes every CLI invocation a fresh
process that exits. An in-memory snapshot would die with the process that took it, so it could never
help the loop it exists to help. That is forced by an earlier decision, and it was not obvious until
traced back.

**A snapshot must capture the entity allocator, not just the entities.** This is the part that would
have been easy to get wrong and impossible to notice:

> `World::state_hash` deliberately excludes the free list — correctly, since that is allocator
> bookkeeping rather than simulation state. Which means **two worlds can hash identically and then
> hand out different entity handles on the very next `spawn`**: one has a slot to reuse, the other
> does not. Both halves of a handle are hashed, so those worlds diverge a few ticks later, and the
> divergence looks like a simulation bug rather than a restore bug.

A snapshot that captured only the live entities would therefore restore a world that passed **every
available check** and was still wrong. Two consequences follow, and the second matters more:

1. the format records the free stack, in order;
2. **hash equality at the moment of restore is necessary but not sufficient**, so correctness is
   tested by *running on* after a restore rather than by comparing hashes.

`restore_continues_identically.rs` is built around that. Delete `free_slots` from the format and
exactly one test fails — `a_restored_world_spawns_the_same_handles` — while every other assertion in
the file still passes.

## Decision

### 1. The `.snapshot` format is text

```text
amadeo-snapshot 1
tick 240
state-hash c57ef7a71849c56a

resources
  SimRng
    increment 1
    state 6364136223846793006

entities
  0:0
    Transform
      translation -4.0 -4.0 0.0

free
  4:2
  3:1
```

Two spaces per level, like every other format here. Blocks are omitted when empty, so there is one
representation per state — which is what byte-stability (I2) needs.

**The speed objection does not survive the numbers.** Re-simulation costs ~21 µs/tick; writing a few
dozen entities as text is well under a millisecond. Text is cheaper by orders of magnitude at every
scale this engine has today. What it buys is that an agent can *read* a snapshot and diff two of
them — Pillar 3, not a nicety — and that a broken world can be repaired by hand.

Justin chose this over binary. Revisit if a measurement ever says otherwise; the writer now exists,
so that measurement is nearly free.

### 2. It is its own crate, and it borrows one thing

`amadeo-snapshot`, sitting above `amadeo-scene`. **Reusing the `.scene` format was rejected outright**
— that is `CLAUDE.md` §7 trap 4 almost exactly. A scene describes *authoring* structure with string
ids and nesting; a snapshot needs exact `index:generation` handles and allocator state. Bending one
around the other is how the authoring format stops being hand-writable.

Justin chose a separate crate over a module inside `amadeo-scene`, for maximum separation. The
consequence flagged when that was put to him is handled rather than accepted: the crate **borrows
`amadeo-scene`'s scalar encoding** rather than copying it. `format_float` has three requirements that
are each easy to get subtly wrong — shortest round-trip, visibly a float, not absurdly long — and
byte-stability depends on all three. Two copies would be two chances to get one of them wrong
differently. The two *formats* stay in separate crates, which is what the separation was about.

### 3. Restoring is a launch flag; capturing is a method

`--snapshot <path>` restores **before the first tick**, exactly as `--replay` does and for the same
reason: a snapshot says what the world *is*, so by the time a method could be called, the pre-roll
has already run and the moment it was meant to replace is gone. There is deliberately **no
`snapshot.restore` method**.

`snapshot.take` returns the snapshot's **text**, and the CLI writes the file — the same division
`amadeo check` and `amadeo import` use. The game knows what the world is; the CLI is the side that
touches the filesystem.

The two compose, which is the point: `amadeo status --from mid.snapshot --ticks 30` lands on the
recorded moment from a file and then runs thirty more.

### 4. A resource registers its own constructor

`World::insert_resource` records how to rebuild that type as it goes. **No `register_resource` call
for a game to forget** — which is the failure ADR 0016 found the hard way, when `quad-demo`
registered no components and `describe` would have reported an empty schema. A resource that exists
in a world is structurally a resource a snapshot can put back.

A resource in a file that this build does not have is **skipped**: dropping a subsystem should not
make old snapshots unloadable. A resource the game does not create is one a snapshot has no business
inventing.

### 5. Restore checks its own work

The last thing `restore` does is compare the rebuilt world's state hash against the one recorded at
capture. That turns "the restore silently produced a slightly different world" — the failure that
would poison every subsequent assertion — into an error at the moment it happens, with a message
that says plainly not to trust a run continued from there.

## What running it against a real game changed

The format was written, tested, and complete before it was ever pointed at `quad-demo`. Doing so
found a defect that no unit test had:

**`InputState` is two maps, and maps had no representation.** Its value fell out in `Display` form —
`{8831028638596390904 => {value: 0}}` — which no parser here reads back. Since `InputState` is a
resource in every game, **a snapshot of anything real would capture and then refuse to restore**:
the worst kind of broken, because it looks like it worked until you need it.

Fixed by giving the format proper nesting, which it should have had from the start — it is an
indentation-based format, and a nested block is the obvious shape. Worth recording because the unit
tests were thorough and all passed; only a real world had the shape that mattered.

## Consequences

**Good:**

- `amadeo snapshot --ticks 600 mid.snapshot` then `amadeo status --from mid.snapshot` reaches tick
  600 by reading a file rather than simulating 600 ticks.
- Snapshots are diffable. Two of them side by side show what changed between two moments, which is
  a debugging tool the engine did not have.
- A hand-edited snapshot is a legitimate way to construct a world state, and the reader validates it
  properly — a slot that is neither live nor free is refused rather than restored, because it could
  never be allocated again and nothing else would report it.

**Bad, and accepted:**

- **A snapshot is not portable across builds.** Rename a component and old snapshots stop loading.
  Deliberate: the alternative is a migration system for an artefact that captures one moment of one
  run, and the error says exactly what happened.
- **Services are not captured**, so restoring does not put back asset caches or GPU state. Correct
  per ADR 0009 — a restore puts the *simulation* back and the engine around it carries on — but it
  means a snapshot restored into a process with different assets loaded will draw differently while
  simulating identically.
- **Text will eventually be the wrong choice**, at some entity count nobody has measured. The
  threshold is guessed, and the escape hatch is a binary encoding behind the same reader and writer.
- **`InputState`'s keys are still unreadable** in a snapshot, for the Q18 reason. A snapshot is
  faithful, so it inherits that.

## What was rejected

- **Binary.** Compact and fast, and it optimises a cost currently ~400× smaller than the one
  snapshots exist to remove. It would also be the first opaque format in a project that has
  deliberately hand-rolled text formats for scenes, replays, and sidecars.
- **Reusing `.scene`.** Trap 4. See above.
- **A `snapshot.restore` method.** Cannot work: by the time a method runs, the pre-roll has happened.
- **Capturing only live entities.** The whole subject of this ADR.
- **A diff against a base snapshot.** Smaller files, and it needs a base to exist and stay valid.
  Reconsider when a snapshot is large enough for the size to matter, which is the same threshold that
  would trigger the binary question.
