# ADR 0023 — Sprites batch by sort order, then texture

**Status:** Accepted · **Date:** 2026-08-02 · **Resolves:** the remaining third of Q3

## Context

Q3 was three decisions wearing one question. ADR 0018 settled the two expensive ones — what a
transform is, and what decides draw order — and deliberately deferred the third, the *pipeline
shape*, because `RenderBackend` isolates it entirely and because `docs/06-open-questions.md` asked
for it to be **decided against a real throughput number rather than argued beforehand**.
`docs/04-subsystems.md` §4 named the figure: 20,000 sprites at 60 fps.

The question is a genuine trade-off, and it is the central one in every 2D renderer.

Drawing a textured rectangle is cheap. **Changing which texture is bound is not.** So a frame's cost
is decided by how many times state changes, not by how many rectangles are drawn — twenty thousand
sprites sharing one texture is one draw call, and twenty thousand sprites with their own textures is
twenty thousand. Collapsing a run of same-texture sprites into one call is *batching*.

But batching and draw order pull against each other:

- Sorting purely by `SortOrder` gives exactly the layering the author asked for, and starts a new
  batch every time the texture changes. Interleaved textures then produce one batch per sprite,
  which is the worst possible outcome and is what a tile-plus-item scene looks like.
- Sorting purely by texture batches perfectly and destroys layering, which is wrong the moment two
  sprites overlap.

## Decision

**Sort by `(sort order, texture)`. A batch is exactly one `(order, texture)` pair.**

Within one sort order, sprites group by texture. Across sort orders, the author's layering is exact.

### What this guarantees

- **Layering is never violated.** A sprite at order 5 always draws over one at order 3, even when
  that costs an extra batch. The tests pin this: one texture appearing in two orders produces two
  batches rather than being merged.
- **Within one order and one texture, order is stable and reproducible** — it follows entity
  iteration order, which is deterministic (invariant I3).

### What this costs, stated plainly

**Within a single sort order, the relative draw order of *different* textures is not guaranteed.**
It is decided by the texture id, not by the author.

Concretely: put a character and its drop shadow on the same `SortOrder` with different textures, and
which one draws first is not yours to choose. The fix is to give them different sort orders, which is
what `SortOrder` exists for — ADR 0018 introduced it precisely so that "what is in front" is explicit
data rather than an emergent property.

This is the same trade every 2D engine makes, and it is stated here because the failure is
confusing if you meet it without warning.

### Instances carry axes, not a size and an angle

A `SpriteInstance` stores the two world-space axes of the sprite's rectangle — the linear part of its
transform matrix — rather than a width, a height, and a rotation.

The decomposed form requires two `hypot` calls and an `atan2` per sprite on the CPU, and then a sine
and a cosine per sprite in the shader, to recover vectors the matrix already contained. Storing the
axes removes both ends of that round trip. It is also **strictly more expressive**: a size-and-angle
pair cannot represent a sheared or non-uniformly-scaled-then-rotated sprite, so the decomposition was
quietly lossy for any entity whose parent scaled it on one axis and turned it.

`QuadInstance` keeps the older shape for now, because the wgpu backend already renders it and nothing
was broken. It should follow when that backend is next touched.

## The measurements

Release build, AMD Ryzen 7 5700X3D. Reproduce with:

```
cargo test -p amadeo-render --test sprite_throughput --release -- --nocapture
```

| Scene | Sprites | Batches | Time | Share of a 60 Hz frame |
|---|---|---|---|---|
| 8 textures, 4 layers, fully interleaved | 20,000 | **32** | 5.1 ms | 31% |
| One tilesheet, one layer | 50,000 | **1** | 11.6 ms | — |
| Every sprite its own texture (worst case) | 1,000 | 1,000 | 0.42 ms | — |

**The batch counts are the result that matters, and they are exact.** Eight textures across four
layers produce exactly 32 batches from 20,000 fully interleaved sprites — the theoretical minimum
that preserves layering. A whole tilesheet, however many tiles, is one draw call. That is the
property Terraria and RimWorld need, and it is asserted rather than hoped for.

The batch counts are asserted in tests because they are a pure function of the world with no clock
involved. Times are printed rather than asserted tightly, since CI runners vary and `CLAUDE.md` §6
forbids tests that depend on wall-clock; the only timing assertion is a five-frame ceiling that would
catch an algorithmic collapse and nothing subtler.

### Two things the measurement changed

**The first working version was 55% slower**, at 7.9 ms, because it sorted 20,000 sprites by
`(order, &str)` — roughly 285,000 string comparisons. Collecting the distinct texture names into a
sorted table once and sorting by *index* into it made the sort integer-only. The table is sorted by
name, so the result stays a function of the names rather than of entity iteration order.

**The remaining cost is not in this module, and that is the more valuable finding.** Removing the
per-sprite trigonometry moved the total by only 4%, which pointed elsewhere:
`ComponentId::of::<T>()` allocates a `String` and FNV-hashes it **on every call**, and each sprite
does two optional-component lookups. A frame re-hashes `"SortOrder"` and `"GlobalTransform"` forty
thousand times. Filed as **Q16**, with the fix (an associated `&'static str` and a `const fn` hash)
and a note that the obvious caching shortcut does not work, because a `static` inside a generic
function is shared across instantiations rather than instantiated per type.

So: **the pipeline shape is not currently the limiting factor, and choosing it on today's absolute
timings would have been choosing on the wrong evidence.** That is worth recording, because it is the
opposite of what the question expected to find.

## Consequences

- **A tilesheet is the right way to author 2D content in this engine**, and `Sprite::region` exists
  to make it the easy way too. A tile is a sub-rectangle of a shared texture, which is also what lets
  thousands of tiles collapse into one batch.
- **`SortOrder` is now load-bearing for correctness, not just appearance.** Two sprites whose overlap
  matters need different orders. This should be said in the game-authoring docs, not just here.
- **20,000 sprites currently costs ~31% of a frame budget**, which is usable but not comfortable.
  Q16 is expected to remove most of it; the batching shape itself is a few hundred nanoseconds of
  sorting.
- **The choice is still cheap to revisit.** `RenderBackend` isolates the pipeline entirely, which was
  ADR 0018's reason for deferring this decision, and remains true after making it.
- **Nothing here can affect simulation.** Collection is read-only and writes into a `Service`, so
  invariant I7 holds and a headless run and a windowed run still agree.

## Rejected alternatives

**Sort by `SortOrder` alone, break the batch whenever the texture changes.** Preserves draw order
exactly, including between different textures in one layer, so it needs no caveat. Rejected because
it makes batching depend on how entities happen to be arranged: the interleaved 20,000-sprite scene
above would produce **20,000 batches instead of 32**. A renderer whose performance collapses because
a designer alternated two tilesheets is not one anybody can reason about.

**Sort by texture alone.** Fewest possible batches, and genuinely correct for a scene with no
overlapping transparency. Rejected because it silently destroys layering, and the failure appears as
"my UI is behind the world" with no mechanism to fix it. Layering is authored intent; batching is an
optimisation, and an optimisation must not override intent.

**Depth-buffer the sprites and let the GPU sort.** Draw in any order, write depth from `SortOrder`,
and batch perfectly. Genuinely attractive, and standard for opaque 3D. Rejected for 2D because
transparency does not work with a depth buffer — the overwhelming majority of sprites have soft or
cut-out edges, and a depth-tested transparent sprite erases what is behind it. Worth revisiting for
an explicitly opaque sprite path if one ever exists.

**Defer the decision again until the wgpu backend renders sprites.** Tempting, since the measurement
showed the pipeline is not currently the bottleneck. Rejected because the batching *rule* is what
game content gets authored against — a tilesheet convention, and the meaning of `SortOrder` — and
that has to be settled before content exists, not after. The GPU-side pipeline details it implies
remain deferrable, which is the part `RenderBackend` protects.
