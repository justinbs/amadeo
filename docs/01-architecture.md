# 01 — Architecture

## The shape of the thing

```
┌──────────────────────────────────────────────────────────────────────┐
│  AUTHORS                                                             │
│    Justin (editor / code)              Claude (text / CLI / RPC)      │
└────────────┬──────────────────────────────────────┬──────────────────┘
             │                                      │
             ▼                                      ▼
      ┌─────────────┐                        ┌─────────────┐
      │amadeo-editor│  ─── same protocol ──▶  │ amadeo-cli  │
      └──────┬──────┘                        └──────┬──────┘
             │                                      │
             └──────────────┬───────────────────────┘
                            ▼
                   ┌──────────────────┐
                   │  amadeo-agent    │   Agent Interface Layer
                   │  RPC · inspect   │   The ONLY control surface.
                   │  snapshot·replay │   Editor has no privileged path.
                   └────────┬─────────┘
                            ▼
                   ┌──────────────────┐
                   │   amadeo-app     │   lifecycle, plugin registry, main loop
                   └────────┬─────────┘
                            ▼
   ┌────────────────────────────────────────────────────────────┐
   │ SUBSYSTEMS  scene · script · ui · anim · physics · audio    │
   │             render · input · assets · events                │
   └────────────────────────┬───────────────────────────────────┘
                            ▼
                   ┌──────────────────┐
                   │    amadeo-ecs    │   archetype SoA storage, schedules
                   └────────┬─────────┘
                            ▼
              ┌──────────────────────────────┐
              │ amadeo-reflect · core · math │   foundation
              └──────────────────────────────┘

   modules/  ← optional genre vocabulary. Depends on subsystems. Nothing depends on it.
   games/    ← depends on modules and subsystems.
```

The load-bearing structural claim: **`amadeo-agent` sits above `amadeo-app` and below both the editor
and the CLI.** The editor is not "the app with tools attached" — it is a client. That single placement
is what makes invariant I5 (parity) structurally true rather than aspirational.

## Dependency rule

Strict DAG, ordered as listed in `CLAUDE.md` §4. A crate may depend only on crates above it. No
cycles, no exceptions. When a lower crate seems to need something from a higher one, the answer is
almost always a trait defined low and implemented high.

Enforced by CI (`cargo deny` / a workspace lint), not by good intentions.

## Scene tree for authoring, ECS for runtime

This is the most important design decision after determinism, and it resolves the tension in the
original requirement — Godot-like node authoring *and* a data-oriented core.

**Both, at different stages.** They are not competing models; they are different representations of
the same thing.

| | Authoring representation | Runtime representation |
|---|---|---|
| Shape | Tree of named nodes with children | Flat archetype tables of components |
| Lives in | `.ama` text scene files, editor viewport | Memory, `amadeo-ecs` |
| Optimized for | Human comprehension, nesting, reuse via prefabs | Cache locality, bulk iteration, uniform queries |
| Who reads it | Justin in the editor, Claude in a text file, git | Systems |

**The pipeline:**

```
scene file (.ama)  ──parse──▶  scene graph (nodes)  ──instantiate──▶  entities + components
      ▲                              ▲                                        │
      └────── canonical write ───────┴──────────── reconstruct ────────────────┘
```

Nesting survives into the runtime as data: a `Parent`/`Children` component pair plus
`LocalTransform` and a derived `GlobalTransform` updated by a transform propagation system. So the
hierarchy is real at runtime, but it's *components*, not an object graph with virtual dispatch.

**Why this works for both authors:**
- Justin drags a node into another node. The editor mutates the scene graph, then canonically writes
  the text file. Diff is minimal.
- Claude edits the text file directly, or issues an RPC that mutates the live scene graph and writes.
- Either way the same parse → instantiate path runs, so there is exactly one code path to be correct.

**Consequence to respect:** node types are *not* classes with inherited behavior. A node type is a
named bundle of components plus defaults — closer to a prefab archetype than to a Godot `Node`
subclass. This keeps the runtime data-oriented and keeps the format declarative. Behavior comes from
systems that query components, never from methods on nodes.

## The frame

Fixed-timestep simulation with decoupled rendering. Non-negotiable (see `adr/0005`).

```
loop {
    poll_platform_events();          // OS, window, raw input        — NOT simulation
    accumulate_time();

    while accumulator >= FIXED_DT {  // deterministic zone begins
        sample_input_actions();      // from recorded or live source
        run_schedule(PreSimulation);
        run_schedule(Simulation);    // gameplay, physics step, events drained
        run_schedule(PostSimulation);
        tick += 1;
        accumulator -= FIXED_DT;
    }                                // deterministic zone ends

    let alpha = accumulator / FIXED_DT;
    run_schedule(Render(alpha));     // interpolated, variable rate, may be skipped
    run_schedule(Present);
}
```

Rules that follow:
- Nothing inside the deterministic zone may read wall-clock time, unordered collections, thread
  completion order, or frame delta. It reads `tick` and `FIXED_DT`.
- Rendering may read simulation state but must never write it.
- Headless mode drops `Render`/`Present` entirely and runs the loop as fast as it can. The
  simulation result must be bit-identical to a windowed run. There is a CI test for this.

## Schedules and system ordering

Systems are registered into named schedule stages with explicit ordering constraints
(`before`/`after` a labeled system), never implicit registration order — implicit order makes
determinism depend on module load order, which is a trap.

Parallel execution is allowed *only* where the scheduler can prove disjoint component access, and the
merge of deferred commands must be applied in a deterministic order. Determinism beats throughput
every time there's a conflict; a slower engine that reproduces is worth vastly more here than a fast
one that doesn't.

## Events

Typed, double-buffered queues. Events written during tick N are readable during tick N+1 (or the
same tick, at a later stage, if declared so). No immediate-dispatch callbacks in simulation — they
make ordering implicit and reentrancy possible, which breaks both determinism and legibility.

Events are also the natural agent observability hook: the event log for a tick range is a compact,
semantic description of what happened, far cheaper to read than diffing full state. `amadeo-agent`
exposes it directly.

## Modules

A module is a crate that registers components, systems, node types, and assets into the app. Nothing
more privileged than that. Planned:

`mod-tilemap`, `mod-platformer2d`, `mod-topdown`, `mod-charcontroller3d`, `mod-dialogue`,
`mod-turnbased`, `mod-inventory`, `mod-vfx`.

Modules matter more than they look. **They are the vocabulary the agent composes with**, and composing
tested primitives is dramatically more reliable than generating novel subsystem code. Every module
added reduces how much has to be invented per game. Treat module quality and documentation as core
engine work, not as extras.

## Testing architecture

Four layers, each with a different job:

1. **Unit** — pure functions, math, parsing. Inline `#[cfg(test)]`.
2. **Headless behavior** — construct a world, run N ticks, assert on state. The workhorse.
3. **Golden replay** — a recorded input stream plus an expected state hash per checkpoint tick.
   Catches behavioral regressions across the entire engine. Any nondeterminism leak fails these.
4. **Round-trip** — parse a scene file, re-serialize, assert byte-identical. Guards I2, and therefore
   guards the human/agent collaboration story.

Every subsystem PR is expected to add to layers 2 and 3.
