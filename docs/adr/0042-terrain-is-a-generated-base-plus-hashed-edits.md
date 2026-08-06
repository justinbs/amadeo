# ADR 0042 — Terrain is a generated base plus hashed edits

**Status:** Accepted · **Date:** 2026-08-06 · **Builds on:** ADR 0009, ADR 0019, ADR 0021, ADR 0035,
ADR 0041

## Context

M2.5 needs terrain: worlds larger than a room, streamed, and for three of the eight target games —
Minecraft, Terraria, Project Zomboid — **destructible**.

The algorithm is the easy half and is nearly a solved problem. The hard half is the question this
project keeps finding underneath rendering questions: **what is the data, and what of it is in the
state hash?** Getting that wrong is not a performance problem or a visual one. It decides whether a
terrain game can be replayed or snapshotted at all, and `CLAUDE.md`'s trap list puts retrofitting
determinism first.

## What the research found

**Surface nets and marching cubes both turn a signed-distance field into a surface, and neither can
represent a sharp edge.** Surface nets is simpler, produces fewer triangles for the same field, and
places one vertex per surface cell rather than up to five triangles.

**Marching cubes needs TransVoxel to stitch chunks with differing detail levels.** Surface nets does
not need an equivalent, because a chunk that samples one cell into its neighbour already agrees with
that neighbour about where the surface is.

**Surface nets is not local**: a cell's vertex depends on all eight of its corners, so meshing the
last cell of a chunk requires samples belonging to the next chunk. This is the constraint most likely
to be got wrong, and it manifests as cracks between chunks that look like a rendering bug.

## Decision

### 1. Surface nets, and the trade is stated rather than assumed

Smooth terrain you can dig into. What is given up is sharp features — a 90° corner comes out
rounded — which is the right trade for ground and the wrong one for architecture. Buildings stay
`BoxMesh` and imported glTF, and the engine already draws both.

### 2. A field is sized in **samples**, not cells, and a chunk meshes with an apron

`Field::new(n)` holds `(n + 1)³` samples and meshes `n³` cells. A caller meshing a chunk fills the
extra layer from its neighbour's data.

Stated in the type rather than left to a comment, because a chunk that samples only its own volume
produces a mesh with a visible crack at every seam, and the symptom points at the renderer.

### 3. **The field is a generated base plus a sparse overlay of edits, and only the edits are hashed**

This is the decision. Three options were real:

- **Procedural only** — the field is `f(seed, position)`. Nothing in the state hash but the seed,
  infinite worlds for free, zero storage. **Not destructible.**
- **Stored voxels** — every chunk's samples are authored state. Destructible, and the state hash
  would have to walk megabytes of voxel data on every snapshot and every replay checkpoint.
- **Generated base plus sparse edits** — the base is `f(seed, position)`; a *change* to a sample is
  stored and hashed. What Minecraft, Astroneer and No Man's Sky all do.

**The third.** An untouched world costs nothing to store and nothing to hash. A dug tunnel costs
exactly the samples that were dug. And critically, **only the edits are simulation state**: the
generated base is a pure function of the seed and the coordinate, so it is *derived* and belongs
outside the hash.

That is ADR 0019's rule for `GlobalTransform` applied to terrain — computed, therefore excluded — and
ADR 0035's rule for meshes applied one level down: **the parameters are authored data, the vertices
they produce are not.**

The consequence worth being explicit about: a snapshot of a terrain world stores the seed and the
edits, and regenerates everything else. That is what makes a save file kilobytes rather than
gigabytes, and it falls out of the data model rather than needing a compression scheme.

### 4. The generated field lives in a `Service`; edits live in a `Component`

- **Edits** are a reflected, hashed component on a chunk entity. Sparse — a coordinate and a value.
  They are authored (by a player digging), they are gameplay, and they must survive a snapshot.
- **The generated field and the mesh** live in a `Service`, which ADR 0009 keeps structurally out of
  the state hash. Both are re-derivable from the seed and the edits, so nothing is lost by throwing
  them away — which is exactly what makes them safe to compute on a background thread (ADR 0041).

### 5. Where meshing happens, and the rule from ADR 0041 that governs it

`amadeo-voxel` is a pure function from a field to a mesh, with **no dependencies at all**. It sits at
the bottom of the graph beside `amadeo-image` and `amadeo-gltf`, for the same reason all three do: it
knows nothing about worlds, entities or rendering, so it can be tested with no engine.

That purity is what makes chunk meshing the ideal job for `amadeo-jobs`: it owns its input, touches
nothing shared, and returns owned output. ADR 0041 §2 then decides how the answer comes back, and it
is the rule most likely to be got wrong:

> A chunk's **mesh** is drawn and nothing else, so it goes in a Service and may arrive whenever. Its
> **collider** is gameplay — a character stands on it — so *when* it arrives changes where the
> character ends up.

So which chunks are active is decided **deterministically** from the player's position, and the
simulation **blocks** on colliders it needs.

### 6. It is a crate, not a module

`modules/` is for genre knowledge, and trap 10 says the engine must not assume a game has a
character. Terrain looks like it might be the same shape — Stellaris has none — but it is not.

The line is what a thing has to *know*. Meshing a field is geometry, exactly as sweeping a capsule is
(ADR 0037 put that in `amadeo-physics`). Chunking and streaming are mechanisms for "load what is
near", with no more opinion about genre than frustum culling has. What *would* be a module is
anything that knows about biomes, ore distribution, or where trees go — because that is a game
deciding what its world is made of.

## Alternatives rejected

**Marching cubes.** Better documented, more widely implemented, and it needs TransVoxel for
chunk seams and level of detail — machinery surface nets does not require. It also produces more
triangles for the same surface. Neither algorithm gives sharp features, so the usual reason to prefer
marching cubes does not apply.

**Dual contouring**, which *can* represent sharp features by storing a normal per edge crossing and
solving a least-squares fit per cell. Rejected as more storage, more arithmetic, and a
quadratic solve whose numerical behaviour would have to be made deterministic across machines — for a
feature terrain does not need and buildings get from meshes instead.

**Storing whole chunks of voxels.** Simplest to implement and simplest to reason about, and it makes
the state hash walk the entire world. Rejected on the grounds that it would make snapshots and
replays — the mechanisms every verification claim in this project rests on — quietly unaffordable for
exactly the games that most need terrain.

## Consequences

- **A terrain world is snapshot-able and replayable with nothing extra built**, because the seed and
  the edits are the whole of its state. The same property ADR 0036 bought for rigid bodies.
- **Editing is a sparse write**, so digging one tunnel does not make the whole chunk authored data.
- **A chunk needs its neighbours' samples to mesh**, which means chunk generation has to run before
  chunk meshing, and by one chunk's margin. That ordering is real and is where seam bugs will come
  from.
- **Level of detail is unsolved here.** Surface nets avoids marching cubes' seam problem *between
  chunks at the same resolution*; chunks at different resolutions still need something. It is
  deliberately not decided now, because the honest options depend on how the streaming system ends up
  shaped and guessing would be the thing this ADR exists to avoid.
- Nothing above `amadeo-voxel` knows a mesh came from a field, which is ADR 0035's bet paying out a
  third time.
