# 03 — AI-Native Design

> The highest-value document in this repository. Everything here is the difference between "an engine
> an AI can technically emit code for" and "an engine an AI can actually build games in."

## The core problem

An agent building a game needs to answer three questions that existing engines cannot answer:

1. **"What can I do?"** — what components, systems, node types, and assets exist, with what fields
   and valid ranges.
2. **"What did I just do?"** — after a change, what is the state of the running game, and did it
   change the way I intended.
3. **"Is it still right?"** — did this change break behavior that used to work.

Human developers answer these with documentation, running the game and looking at it, and intuition.
None of those are available to an agent. So Amadeo answers all three **mechanically**.

---

## Pillar 1 — Determinism (answers "is it still right?")

Fixed timestep, seeded RNG, ordered iteration, no wall-clock in simulation. Details in `adr/0005`
and `01-architecture.md`.

What determinism *buys*, which is why it's an invariant and not a nice-to-have:

| Capability | Falls out of determinism |
|---|---|
| **Replay as regression test** | Record an input stream once. Replay it forever. Assert a state hash. This is behavioral testing for games, and it is otherwise impossible. |
| **Reproducible bug reports** | "It broke" becomes a replay file and a tick number. |
| **Save/load** | A save is a state snapshot. Same machinery. |
| **Time-travel debugging** | Snapshot every N ticks, rewind, step forward one tick at a time. |
| **Headless verification** | Run 600 ticks with no GPU, hash the result, compare. Fast enough to run on every edit. |
| **Rollback netcode later** | The exact substrate it requires, already built. |

Every one of those is an agent capability. Determinism is not a purity exercise — it is the
mechanism by which an agent can see and verify.

**The discipline required.** Nondeterminism arrives quietly:
- `HashMap`/`HashSet` iteration order → use ordered maps in simulation paths.
- `Instant::now()`, `SystemTime`, frame delta in gameplay → read `tick` instead.
- Thread completion order affecting write order → deterministic command buffer merge.
- Uninitialized or NaN-producing float paths → validate at component construction.
- Asset load completion order affecting spawn order → simulation must not depend on load timing.

CI runs golden replays on every commit. A leak that lands is caught within one commit, not one
milestone.

---

## Pillar 2 — Reflection & schema (answers "what can I do?")

`amadeo-reflect` maintains a runtime type registry. Every component, resource, event, and node type
registers: field names, types, defaults, ranges, units, and doc text.

From one registry, three consumers:

```
                    ┌─▶  canonical text (de)serialization
  type registry ────┼─▶  editor inspector widgets (generated, not hand-written)
                    └─▶  machine-readable schema for the agent
```

`amadeo describe` emits the full API surface as JSON Schema. That means an agent can ask *"what
fields does a RigidBody have, what are the valid values, what does `ccd_enabled` mean"* and get an
authoritative answer generated from the code — never a stale doc, never a guess.

This also kills the "plausible but wrong" failure mode, which is the dominant way agents break game
code. If the schema says `Transform` has no `rotation_degrees` field, that's known before the code is
written, not after a confusing debug session.

**Consequence (I8):** if a type isn't reflected, it doesn't exist as far as the editor and the agent
are concerned. Registration is part of defining a component, not a follow-up task.

---

## Pillar 3 — The Agent Interface Layer (answers "what did I just do?")

`amadeo-agent` is a JSON-RPC server (stdio + local TCP) that both the CLI and the editor speak. It is
the engine's only control surface.

Capability groups, roughly:

**Introspect**
- `world.query` — entities matching a component filter, with component values
- `world.entity` — full component dump for one entity, with schema
- `world.resources` — global state
- `schedule.list` — systems, their stage, their ordering constraints, their last run cost
- `events.since(tick)` — semantic event log for a tick range
- `assets.list` / `assets.status` — what's loaded, what failed, why

**Control**
- `sim.step(n)` / `sim.pause` / `sim.resume` / `sim.set_speed`
- `world.spawn` / `world.despawn` / `world.set_component`
- `input.inject(action, value, tick)` — synthesize input deterministically
- `scene.load` / `scene.save` / `scene.reload`

**Observe**
- `render.capture(tick)` — deterministic screenshot at an exact tick. **This is the agent's eyes**,
  and it matters more here than it would in a browser-based engine because there's no devtools to
  fall back on.
- `render.describe` — structured description of what is on screen: visible entities, screen-space
  positions, layers, bounds. Far cheaper than an image and often sufficient to verify layout,
  overlap, or off-screen bugs without vision.
- `profile.frame` — per-system timings against declared frame budgets, so performance regressions
  are catchable, not discovered months later.

**Verify**
- `replay.record(path)` / `replay.play(path)` / `replay.hash(tick)`
- `snapshot.take` / `snapshot.restore` / `snapshot.diff(a, b)` — state diffing between two
  snapshots is an extremely high-signal way to answer "what did my change actually affect."

**Design notes:** JSON-RPC over stdio was chosen for being boring — inspectable by eye, scriptable
from a shell, no codegen to write a client. The protocol is versioned and specified in
`docs/protocol/` (to be written in M1) because the editor, the CLI, and the agent all depend on its
stability.

---

## Pillar 4 — Text as the authoring surface

Invariants I1 and I2, decided in `adr/0003`. The short version: scenes, prefabs, asset metadata, and
config are hand-writable text with canonical, byte-stable serialization and stable IDs.

Practical requirements this imposes:
- Stable, content-independent entity IDs so reordering doesn't churn diffs.
- Sorted keys, fixed number formatting, deterministic collection order.
- Round-trip test in CI: parse → serialize → assert byte-identical.
- `amadeo fmt` is the single formatting authority. The editor calls it. The agent calls it. Same
  bytes out.
- `amadeo check` validates against the schema registry and produces errors with file, line, and a
  suggested fix.

Where existing engines fail this specifically: editor-generated IDs that change on save, node
reordering on unrelated edits, and binary or semi-binary sidecar files. Those three failures are the
regression tests for this pillar.

---

## Pillar 5 — Diagnostics as an interface

An error message is an API for whoever reads it, and here that's often an agent with no ability to
ask a follow-up question. So error quality is a functional requirement:

- Structured, not just prose: which entity, which system, which asset path, which tick.
- Says what to do next, not only what went wrong.
- Panics in engine crates are bugs. Return typed errors.
- A system that fails should degrade and report, not take down the process, so the agent can inspect
  the broken state rather than a corpse.

Compare: `thread 'main' panicked at 'index out of bounds'` versus
`system 'resolve_collisions' (tick 412): entity 0x1A4 has Collider but no Transform; add a Transform
component or remove the Collider`. The second is actionable without a debugger. That's the standard.

---

## The agent's actual working loop

Once M1 lands, this is how I build a feature — and it's worth writing down because the whole
architecture exists to make these six steps possible:

```
1. amadeo describe                     → know the real API, not a guessed one
2. edit scene text / write systems     → author
3. amadeo check                        → schema + type errors, before running anything
4. amadeo run --headless --ticks 600   → behavior, fast, no GPU
   amadeo inspect world.query ...      → verify state matches intent
5. amadeo render.capture --tick 600    → look at it, when state isn't enough
6. amadeo test                         → golden replays confirm nothing else broke
```

Steps 1, 3, 4, and 6 need no human and no eyes, which is what makes unattended progress possible.
Step 5 is the expensive one, used to confirm, not to explore.

## How we'll know this worked

Concrete, falsifiable success criteria, checked at M1 and again at M4:

1. I can build a small complete game with **zero editor use** and zero screenshots — verified purely
   through headless runs and state queries.
2. Justin can build the equivalent game with **zero text file editing**.
3. Both scenes round-trip byte-identically and produce clean git diffs when edited by the other party.
4. A behavioral regression introduced anywhere in the engine is caught by golden replays within one
   commit.
5. `amadeo describe` output is sufficient to write correct engine code without reading engine source.

If any of those fail, the AI-native claim is not real yet, whatever the feature list says.
