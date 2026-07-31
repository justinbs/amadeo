# ADR 0013 — `Component` requires `Reflect`, so invariant I8 is structural

**Status:** Accepted · **Date:** 2026-07-31 · **Follows:** ADR 0012

## Context

Invariant I8 says reflection is not optional: *"Every component and resource registers a
machine-readable schema. If it can't be reflected, it can't be serialized, inspected, or edited."*

Until now that was a **convention**. Nothing stopped anyone writing `impl Component for Health {}`
and moving on. Everything would work — the game would run, tests would pass, the golden replay would
be happy — and the type would simply be invisible to the editor and the agent. That is trap 5 in
`CLAUDE.md` §7, listed in the project's own risk register precisely because the cost surfaces
milestones later, at the point when someone tries to save a scene or ask what a component does.

`docs/03-ai-native-design.md` Pillar 2 is blunt about the consequence: *"if a type isn't reflected,
it doesn't exist as far as the editor and the agent are concerned."*

ADR 0009 already solved a structurally identical problem. The engine needed non-simulation state kept
out of the state hash, and rather than trusting authors to remember, it made `Resource` require
`StableHash` and `Service` not — so misfiling became a type error. The same move is available here.

## Decision

**`Component: 'static + Send + Sync + Debug + StableHash + Reflect`.**

An unreflectable type can no longer be a component. Not discouraged — impossible.

`amadeo-ecs` gains a dependency on `amadeo-reflect`, which is where `CLAUDE.md` §4 already places it
in the crate order.

**Every existing component is converted** to `#[derive(StableHash, Reflect)]`, and the hand-written
`StableHash` impls are deleted.

**`Resource` is deliberately left alone for now.** I8 names resources too, and they should follow —
but the resource set drags in work this change does not need: `SimRng` wraps `Rng`, whose fields are
private and which lives in `amadeo-core` *below* `amadeo-reflect` (so its `Reflect` impl has to be
written in `amadeo-reflect` and needs accessors that do not exist yet), and `InputState` holds maps
that `Reflect` does not cover. Recorded as the next step rather than bundled in here.

## Rationale

1. **A convention that is invisible when broken is not a control.** Skipping registration produces
   no error, no warning, and no behavioural difference — the failure is silence, discovered by
   someone else, later. That is the exact profile ADR 0009 was written to eliminate.

2. **This is the cheapest it will ever be.** Eight components exist. Every later milestone adds more,
   and M3's genre modules add them in bulk.

3. **The cost is one derive per component**, and that derive is one people want anyway: it supplies
   the schema, the units, the ranges, and the ADR 0006 replication annotations that would otherwise
   be a separate sweep.

## Consequences

- **The golden replay survived, and it is worth recording why rather than filing it under luck.**
  Converting to `#[derive(StableHash)]` changes a type's fingerprint whenever its fields were not
  already in alphabetical order, because the derive sorts by name. The committed fixture uses only
  `Position { x, y }` and `Velocity { x, y }` — alphabetical already, scalar fields, no arrays — so
  the derived hash is byte-identical and the fixture needed no regeneration.

  `Transform2d`, `Quad`, and `Camera2d` **did** change fingerprint (arrays now hash length-prefixed,
  and `Quad`'s fields sort to `color, layer, size`). Nothing asserts on those, so nothing broke. A
  later conversion would not have been so cheap.

- **Every crate that defines a component now depends on `amadeo-reflect`** to name it in the derive's
  generated paths — `amadeo-render`, `amadeo-app` (tests), `games/quad-demo`. Normal for derive
  macros, and worth knowing before wondering why a new game crate will not compile.

- **`Component` is now a fairly heavy bound.** Five supertraits is a lot to ask, and each one is
  load-bearing: drop `StableHash` and replays stop seeing the type, drop `Reflect` and the editor and
  agent stop seeing it, drop `Send + Sync` and the scheduler can never parallelise. The weight is the
  point — it encodes what a component *is* in this engine.

- **Marker components pay a small tax.** `struct Player;` now needs two derives to carry no data.
  Accepted: uniformity is worth more than saving a line, and a marker that the editor cannot see is
  as much of a hole as a data component it cannot see.

## Rejected alternatives

**Leave it a convention and rely on review.** Zero work. Rejected on the evidence of the project's
own risk register: `CLAUDE.md` §7 already lists skipped registration as a named trap, which means it
was foreseen as a thing that happens rather than a thing that might.

**A runtime check — panic or warn on inserting an unregistered component.** Catches the mistake, but
only once the code runs, and only on a path that executes. Engine crates do not panic
(`CLAUDE.md` §6), and a warning is something to ignore. A compile error is neither.

**Require `Reflect` only for components that are actually serialized.** Superficially reasonable —
some components genuinely are transient. Rejected because "actually serialized" is not knowable when
the component is written: a component nobody saves today is one someone saves in M3, and by then the
annotation context is gone. It also splits components into two classes for the editor and the agent,
which is exactly the second-class-citizen outcome `CLAUDE.md` §1 rules out.

**Convert `Resource` in the same change.** Correct eventually, and I8 names it. Rejected as scope:
it requires exposing `Rng`'s internal state through `amadeo-core`, writing `Reflect` for it inside
`amadeo-reflect` (since `amadeo-core` sits below and cannot depend upward), and adding map support to
`Reflect` for `InputState`. Each is defensible on its own and none of it is needed to close the
component-shaped hole in I8.
