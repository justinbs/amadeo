# ADR 0043 — Chunk residency is integer arithmetic, and colliders exist only at the finest level

**Status:** Accepted · **Date:** 2026-08-07 · **Builds on:** ADR 0006, ADR 0009, ADR 0021, ADR 0035,
ADR 0038, ADR 0041, ADR 0042 · **Amends:** ADR 0042 §2 (see §4)

## Context

M2.5 needs chunked streaming: a world larger than what is on screen, loaded around the player and
dropped behind them. ADR 0042 settled what terrain *is* — a generated base plus sparse hashed edits —
and ADR 0041 §2 settled the rule streaming has to obey:

> A chunk's **mesh** is drawn and nothing else, so it goes in a Service and may arrive whenever. Its
> **collider** is gameplay — a character stands on it — so *when* it arrives changes where the
> character ends up.

What was left open is everything that turns that rule into a design: how "which chunks are active" is
decided, how far collision extends, and what happens where chunks of different detail meet — **Q25**,
which ADR 0042 deliberately deferred on the grounds that the honest options depend on how streaming
ends up shaped.

## What the research found

**Zylann's `godot_voxel` is the closest production analogue** — smooth voxel terrain, streamed, with
LOD, inside an engine that has a scene format and an editor. Three of its choices decided three of
ours.

1. **It migrated away from an octree residency system to concentric axis-aligned boxes** ("clipbox"),
   for stated reasons that apply here unchanged: loading patterns become predictable, and several
   viewers are supported without split/merge logic having to agree about them. ADR 0006 reserves
   multiplayer and six of the eight target games are co-op or multiplayer, so more than one viewer is
   a requirement rather than a nicety.
2. **Its data boxes must exceed its mesh boxes by one block of padding, "for neighbour access during
   meshing."** That is ADR 0042's apron constraint, arrived at independently, and expressed at chunk
   granularity rather than sample granularity.
3. **Collision is a separate track from visuals**, with a per-viewer `requires_collisions` flag, and
   `collision_lod_count` defaults to **0** — collision is generated only for levels explicitly asked
   for. ADR 0041 §2 as shipped product rather than as our theory.

**And the classic cross-LOD fix for dual methods does not apply to us.** Nick Gildea's seam approach —
each chunk builds a small stitch mesh from its own boundary nodes plus its neighbours' — needs an
*adaptive* octree with variable-size leaf nodes. `amadeo-voxel` is a uniform grid. Transvoxel does not
apply either: it is for marching cubes, a *primal* method, and surface nets is *dual*. So the honest
options for Q25 are narrower than ADR 0042's list implied, and every one of them would have to be
derived here rather than ported.

## Decision

### 1. Residency is a union of integer boxes, one per viewer

`Residency::of(&[Viewer])` in `amadeo-voxel::chunk`. A viewer is a chunk coordinate and two radii;
residency is the union of their boxes, in `BTreeSet`s.

**Which chunks are active is gameplay state**, because a collider is, so this is part of invariant I3
rather than a performance detail. Accordingly it is decided by comparing `i32`s. There is exactly one
floating-point step in the module — `ChunkKey::containing`, turning a world position into a chunk
coordinate — and it uses only division, `floor` and a saturating cast, all exactly specified.

A cube rather than a sphere: one comparison per axis instead of a distance, it tiles the world
exactly, and the corners a sphere would exclude cost a few chunks rather than arithmetic on every one.

A union rather than anything cleverer, and asserted order-independent, because a set of players has no
inherent order and residency must not depend on how the world happened to iterate them.

### 2. Colliders exist only at the finest detail level

Distant chunks are drawn and are **not solid**.

The reason is not cost, it is determinism. If a chunk's collider changed resolution, and resolution
depends on viewer position, then the ground under a character would change shape because *another*
player walked toward it. That is gameplay state moving for a rendering reason, which is close to
exactly what ADR 0041 exists to prevent.

Keeping collision at one resolution has a consequence worth stating plainly: **the seam question
becomes purely visual**, and therefore sits entirely outside the state hash. That is what makes §3
affordable.

The cost is real and is named rather than hidden: anything that must interact with terrain far from a
viewer — a thrown object, an AI on the other side of the map, a projectile — falls through it. That is
a separate problem and it gets a separate answer when a game needs one.

### 3. Level of detail is built at one resolution now, with the level carried through the design

Terrain runs at detail level 0 only. **Q25 stays open**, which is what it asked for: its own text says
the answer depends on how streaming ends up shaped, and this is the only ordering that lets it be
decided against a running system rather than a drawing.

What is *not* deferred is the level itself. `ChunkKey` carries `lod` from the start, because it is part
of a chunk's **identity** — two chunks covering the same volume at different resolutions are different
chunks, with different meshes, different jobs, and different collider ids. `ChunkShape::cell_size_at`
and `chunk_size_at` are the arithmetic, and they are tested rather than left as an unused field.

This is ADR 0038's move exactly: one value now is a value of a field that has to exist anyway, not a
shortcut to undo. That ADR shipped `ShadowMode` with two variants so cascades could become a third;
this ships one detail level so more can become values of a field every function already takes.

### 4. A chunk needs an apron on **both** sides — amending ADR 0042 §2

ADR 0042 §2 says a chunk of `n` cells fills an `n + 1` sample grid, "reaching one step into the
neighbour on the high side". **That is correct and it is not sufficient**, and the difference is a
visible crack around every chunk.

Two different things need neighbour data, and the ADR described only the first:

- **Vertices.** A cell's vertex is decided by its eight corners, so meshing a chunk's last *cell*
  needs the next chunk's first *sample*. This is the high apron the ADR names.
- **Quads.** `surface_nets` emits a quad for a grid edge by looking at the four cells around it, and
  can only do so where all four have vertices. At a chunk's **low** face they do not — the cells on
  the other side belong to the previous chunk. So the quads *bridging* two chunks are emitted by
  neither, and the surface has a one-cell gap all the way around every chunk.

So a chunk of `n` cells fills an `n + 2` sample grid covering `n + 1` cells, starting one cell **below**
its own origin. Every quad in the world is then emitted exactly once, by the chunk on the high side of
it — no gaps and no duplicates. This is the convention `fast-surface-nets` documents as "faces are not
generated on the positive boundaries of a chunk".

**This was found by meshing two adjacent chunks and looking at the result, not by reading the ADR.**
The ADR was written before anything meshed two chunks, and the omission was invisible until something
did.

### 5. The apron becomes a checked property, not a remembered one

`Residency` carries **three** nested sets — `collision ⊆ visual ⊆ data` — where `data` is `visual`
grown by one chunk in every direction. A chunk that is drawn therefore always has loaded neighbours to
read its apron from, by construction.

Until now this was a sentence in `STATUS.md` that a future session had to remember. It is now
`the_data_box_always_exceeds_the_visual_box`, which fails naming the chunk and the missing neighbour.

## Alternatives rejected

**An octree for residency.** The textbook answer, and what `godot_voxel` shipped first. Rejected for
the reasons it moved away: split/merge decisions are harder to make predictable, and multiple viewers
require the tree to reconcile several opinions about the same node. Nothing here needs an octree's
adaptivity while every chunk is the same size.

**A spherical or distance-based radius.** More natural-looking loading, and it puts a `sqrt` and a
float comparison into a decision that is gameplay state. The corners of a cube are a few chunks;
the arithmetic is on every chunk, every tick, on every machine that has to agree.

**Skirts for cross-LOD seams, decided now.** A downward lip at each chunk border, hiding the crack
rather than closing it. Genuinely cheap and universally used, and it keeps a chunk's mesh a pure
function of its own coordinates. Rejected *for now* rather than on merit: with collision pinned to one
resolution the seam question is purely visual, so nothing forces the choice yet, and deciding it
against a running streaming system is strictly better information than deciding it against a design.

**Transition geometry, decided now.** Correct, and it makes a chunk's mesh depend on its neighbours'
chosen resolutions — so one chunk changing level dirties up to six others and meshing jobs gain a
dependency graph. Worth paying when there is something to look at that says it is needed.

## Consequences

- **Residency is reproducible across machines and across thread counts**, because it is integer set
  arithmetic over an ordered container. This is what M2.5's exit gate 2 will assert.
- **A chunk's mesh is a pure function of `(key, source, edits)`** — it reads no other chunk's *state*,
  only the same generator and the same edit overlay. That is what lets chunks be generated in any
  order, in parallel, without their results depending on that order, and it is why chunk meshing is
  the job `amadeo-jobs` was built for.
- **Edits are keyed by world sample coordinate, not by chunk.** A per-chunk edit store would let two
  chunks disagree about a shared sample, opening a seam exactly where somebody had been digging.
- **A terrain collider cannot be a `Collider` component.** `Shape` is `Copy` and `StableHash`, and a
  triangle mesh is neither cheap to copy nor something ADR 0042 will allow into the state hash. It has
  to reach the backend the way a texture reaches the GPU — by id, with the geometry derived. The
  operation that does so is not yet built and is the next thing.
- **Distant terrain is not solid**, per §2, and anything needing far-away terrain interaction needs a
  separate answer.
- **Q25 remains open and is now better posed**: not "which of four options", but "is a chunk's mesh
  allowed to depend on its neighbours' resolutions", with the seam confined to rendering by §2.
