# ADR 0068 — AI behaviour is a state machine over named facts

**Status:** Accepted · **Date:** 2026-08-14 · **Builds on:** ADR 0011, ADR 0029, ADR 0037, ADR 0066 ·
**Settles:** `docs/05`'s `mod-behaviour`

## Context

M3's exit gate asks for "at least one pursuing entity with distinct AI states (idle, search, pursue,
lose interest)". `docs/05` names `mod-behaviour` and says nothing about its shape, so this is open.

The four architectures that exist in practice are a **finite state machine**, a **behaviour tree**
(standard since Halo 2, and what Unreal ships), **utility scoring** (The Sims, RimWorld), and
**goal-oriented planning** (F.E.A.R.). Unity and Godot ship none of them; games roll their own, and
most small games roll a state machine.

## Decision

### 1. A state machine, and the reasons are this project's rather than the industry's

Behaviour trees are the better answer at scale and this is not at that scale. What decides it here:

- **Legibility.** `CLAUDE.md` makes "Justin can read, debug and fix this himself" a hard requirement.
  In a state machine, *"why is it doing that"* is **one field you can read**. In a behaviour tree it
  is a path through a tree, which needs real tooling before it is answerable — and the same is true
  for an agent (invariant I5).
- **The gate describes one.** Idle, search, pursue, lose interest *is* a state machine. Building a
  tree to express four states is machinery in search of a problem.
- **A stalker is stateful anyway.** "I have been searching for eight seconds" is state, and a
  behaviour tree needs a blackboard to hold it — so the tree buys composability and still pays for
  state.
- **Being wrong is cheap**, which §2 is the reason for.

Utility scoring is rejected for the opposite of its strength: it is the least predictable and the
hardest to debug, and a horror stalker needs the player to be able to **read** what it is doing.
Planning is rejected as far more machinery than four states can justify.

### 2. The expensive part is the boundary, and it is the same for all four

This is Q3's lesson again, which `STATUS` records as a pattern: the pipeline is usually the cheap
question, and the expensive one is what *data* the choice implies. Whichever sequencer sits on top,
the module needs the game to answer "can it see the player" and to carry out "walk to the door".

So the boundary is designed and the sequencer is not load-bearing:

- **The game writes named facts.** [`Facts`] is a map of `String -> bool` a game's own systems fill —
  `"sees_player"`, `"heard_something"`, `"at_waypoint"`. The module never computes one and has no
  idea what any of them mean, which is invariant I4.
- **The game reads the current state and acts.** There are no callbacks and no registered action
  functions. A game system matches on `Behaviour::state` and does whatever that means, exactly as
  `games/atrium` matches on its own `Screen`.

Swapping a behaviour tree in later replaces the sequencer and touches neither half of that.

### 3. A transition tests facts and time, and nothing else

```
- to "pursue"
  when "sees_player"
- to "idle"
  after 6.0
```

Three optional conditions, ANDed: `when` a fact is true, `unless` a fact is true, and `after` a
number of seconds in the current state. **The first transition in authored order whose conditions all
hold wins**, which is what makes the result a pure function of the file rather than of a search.

**There is deliberately no expression language.** No comparisons, no arithmetic, no boolean algebra.
Every one of those is a small language to design, document, parse and debug, and each is a step
towards the scripting layer ADR 0011 measured and rejected. A game that wants `health < 0.3` writes
one line of Rust that sets `"badly_hurt"`, where it can be typed, tested and read.

`after 0.0` needs no sentinel for "no time requirement", because "at least zero seconds have passed"
is always true.

### 4. The machine is a component, and prefabs are how it is shared

Not an asset with a cache. Twenty monsters of one kind are twenty instances of one **prefab**
(ADR 0029), which is already the engine's answer to "many things with the same components" — so this
needs no new asset kind, no fifth cache, and no fifth instance of the missing-asset hazard that ADR
0066 had to reason about.

[`BehaviourMachine`] is authored and never written; [`Behaviour`] holds the current state and how
long it has been there, and is written every tick. That is ADR 0037's `CharacterController` /
`CharacterMotion` split for the third time, and the same test applies: one is what a person types
into a scene file and the other is what a person has no business typing.

### 5. What is hashed

**`Behaviour` and `Facts` both are.** Which state a monster is in is gameplay and a save must restore
it; and a fact is hashed rather than derived because a game may legitimately **latch** one — "has
seen the player at least once" is a fact that must survive, and a derived map could not hold it.

The cost is that a game recomputing every fact each tick hashes something it could have recomputed.
That is small, and the alternative loses a real capability.

## Consequences

**A state named by a transition and not defined is reported, never silent.** A typo in a state name
otherwise produces a monster that stops transitioning, which looks like an AI bug rather than a
spelling one — the failure shape this project keeps meeting. `BehaviourMachine::problems` names it,
the same way `AnimationClip::problems` does.

**Entering and leaving a state is an event**, not a callback. `BehaviourChanged` carries both ends, so
a game can play a roar on entering `pursue` without the module knowing what a roar is — ADR 0059's
split, and it keeps execution order explicit.

**Hierarchical states are not built.** They are the standard answer to transition growth and they are
additive: a state gains a parent, and a transition on the parent applies to its children. Nothing
here forecloses it, and four states do not need it.

**This module is designed against one game.** `games/atrium` gets a pursuer so the design meets a real
user before it is finished, which is the check `modules/amadeo-camera` had and
`modules/amadeo-interaction` did not.

## Alternatives rejected

**A behaviour tree**, covered in §1. The honest summary is that it wins at a scale this project has
not reached and costs legibility now, and §2 is what makes changing our mind cheap.

**Registering condition and action functions by name** — a table of `fn(&World, Entity) -> bool` the
game fills, which is `Animatable`'s shape from ADR 0066. It works and it is *less* introspectable than
a map of facts: a registry of function pointers cannot be read by `amadeo query`, and "why did it not
transition" becomes "read the game's source", which is what invariant I5 exists to prevent. Facts are
data, and data can be looked at.

**An expression language on transitions**, covered in §3.
