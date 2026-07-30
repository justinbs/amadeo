# ADR 0004 — Scene tree for authoring, ECS for runtime

**Status:** Accepted · **Date:** 2026-07-30

## Context

Two requirements appear to conflict:

- Godot-like authoring, where a game is a tree of named nodes that a human can nest, name, and reuse.
  Intuitive, spatially natural, and what Justin expects from an editor.
- A **data-oriented** engine core, per the original brief. Archetype ECS with structure-of-arrays
  storage, for cache behavior and — equally important — for the *uniformity* that makes automated
  reasoning about game state tractable.

Node trees are objects with identity, nesting, and often inherited behavior. ECS is flat tables of
plain data. Picking one appears to mean losing something real: choose nodes and the runtime stops being
data-oriented; choose raw ECS and authoring becomes a flat list of entities, which is miserable for
humans and not much better for agents.

## Decision

**Have both, at different stages.** They aren't competing models — they're different representations
of the same thing at different points in the pipeline.

| | Authoring representation | Runtime representation |
|---|---|---|
| Shape | Tree of named nodes with children | Flat archetype tables of components |
| Lives in | `.ama` text files, editor viewport | Memory, `amadeo-ecs` |
| Optimized for | Human comprehension, nesting, reuse | Cache locality, bulk iteration, uniform query |
| Read by | Justin, Claude, git | Systems |

```
scene file  ──parse──▶  scene graph (nodes)  ──instantiate──▶  entities + components
     ▲                         ▲                                       │
     └──── canonical write ────┴─────────────── reconstruct ────────────┘
```

**Hierarchy survives into the runtime as data**, not as an object graph: a `Parent`/`Children`
component pair, plus `LocalTransform` and a derived `GlobalTransform` maintained by a transform
propagation system. The tree is real at runtime; it just isn't made of objects with virtual dispatch.

**A node type is a named bundle of components plus defaults** — closer to a prefab archetype than to a
Godot `Node` subclass. Critically:

- **No inherited behavior.** No `_process()` override, no method dispatch on nodes.
- **Behavior lives in systems** that query components. Always.
- Node types are registered in the reflection registry like everything else, so the editor and the
  agent both discover them the same way.

## Rationale

1. **It resolves the conflict without compromise.** Humans get nesting and names; the runtime stays
   flat and data-oriented. Neither side pays.
2. **One code path.** Editor drags and hand-edited text both go through parse → instantiate. Exactly
   one implementation to keep correct, which matters enormously for ADR 0003's parity guarantee.
3. **Data-oriented serves the agent, not just the cache.** Uniform layout with known schemas is what
   makes `world.query`, `snapshot.diff`, and state hashing possible. An object graph with per-node
   behavior would be far harder to introspect mechanically.
4. **Declarative files.** Because nodes carry no behavior, scene files describe *what exists*, not
   *what happens*. That keeps them readable, diffable, and safe to generate.
5. **Prior art works.** Godot 4 moved substantially toward this; Unity's DOTS conversion workflow is
   the same shape. Neither designed it in from the start, and both show the seams.

## Consequences

- Node types must be registered as component bundles. Adding one is data, not a subclass — cheap, and
  discoverable by both authors.
- Two ID spaces exist: stable authoring IDs in files, generational handles at runtime, with a mapping.
  Called out in ADR 0003 §3; they must be designed together.
- Reconstruction (runtime → scene graph → file) must be lossless, or editor saves lose data. This is
  covered by the round-trip byte-stability test.
- Someone arriving from Godot will expect to attach a script to a node and write `_process()`. They
  can't. The equivalent is a component plus a system. **This is the main conceptual adjustment the
  design demands** and it should be prominent in the eventual user documentation.
- Deeply nested transform hierarchies cost a propagation pass. Standard, and optimizable via change
  detection.

## Rejected alternatives

**Pure ECS, flat entity lists, no tree.** Simplest runtime, cleanest data orientation. Rejected
because authoring becomes hostile to humans, and the editor requirement is real.

**Node tree at runtime (classic Godot/Unity model).** Familiar and pleasant to write. Rejected on the
brief's data-oriented requirement, and because per-node behavior with virtual dispatch is markedly
harder to introspect, snapshot, and hash — it would undercut the entire observability story.

**Nodes as a thin view over ECS, maintained live in both directions.** Superficially the best of both.
Rejected as a synchronization nightmare: two authoritative representations that must agree every frame
is a reliable source of subtle bugs. The pipeline above has a single direction of authority at each
stage, which is why it's tractable.
