# ADR 0050 — Amadeo's own content is low-poly, and the renderer stays general

**Status:** Accepted · **Date:** 2026-08-11 · **Builds on:** ADR 0035, ADR 0039, ADR 0045, ADR 0049

## Context

Justin asked whether a low-poly look would be the right direction, and whether it would be *easier to
produce* than a hyper-realistic one. Both halves deserve an honest answer, because the second one is
the reason the first is a good idea.

## What can actually be produced here, and what cannot

**No art can be authored in the conventional sense.** There is no modelling, no sculpting, no texture
painting, no photogrammetry. What *can* be written is **generators** — code that produces geometry and
images — and the project already has three: `games/vault`'s `bin/pix` turns hand-written character
grids into sprites, `games/scarp`'s `bin/turf` turns a noise formula into ground textures, and its
`bin/sky` turns a sun angle into an HDR environment.

That capability maps onto the two looks completely differently:

| | Low-poly | Hyper-realistic |
|---|---|---|
| A rock is | ~40 vertices, describable as text | a photogrammetry scan plus a 4K PBR texture set |
| Quality comes from | form, silhouette, colour | surface detail, measured materials, density |
| Produced by | a generator, or a few minutes in any modeller | scanning, sculpting, baking — none available here |
| In the repository | hand-editable, diffable text (invariant I1) | binary blobs with licences to track |

So the answer is **yes, substantially easier** — and not as a compromise. Low-poly's quality lives in
exactly the properties code can express, which is why it is the aligned choice rather than the
cheap one.

## Decision

### 1. Amadeo's own demos and assets are low-poly

`games/scarp`, `games/atrium`, and the M3 slice adopt it. Content generators stay per-game (the
`bin/turf` precedent) until a second game wants the same one, which is the moment to promote it.

### 2. **The renderer does not narrow.** This is a content decision only

`CLAUDE.md` trap 8 is explicit: *"Baking an art style into the renderer"* is a trap, because the
target games span stylised-realistic outdoors, low-poly, and dark atmospheric interiors. Choosing a
look for the *demos* must not become choosing a look for the *engine*.

Concretely: nothing gets removed. Normal mapping, the metallic-roughness BRDF and image-based lighting
all stay and all still work on imported art. What changes is the **order** the remaining work happens
in, below.

### 3. The remaining renderer order is re-ranked, and ADR 0045's reasoning is why

ADR 0045 ordered tier 1 by visual return **for a stylised-realistic target**. Low-poly changes what
returns most, and three items move:

- **Anti-aliasing moves up, sharply.** Low-poly is nothing but hard silhouette edges, and jagged edges
  are the single loudest tell that something is not finished. ADR 0045 put this in tier 2 with the
  note that they "read as amateur faster than almost anything else" — under low-poly that is not a
  note, it is the top of the list.
- **Image-based lighting was more important than it looked, and is already built.** Flat-shaded facets
  each catching a different part of the sky is precisely what makes good low-poly read as solid rather
  than as flat colour. ADR 0049 landing before this decision was luck, but it is the feature this look
  most depends on.
- **Normal mapping matters much less.** Low-poly generally uses flat shading and no normal maps.
  ADR 0047's work is not wasted — imported art still uses it, and it stays for the non-low-poly
  targets — but it should not have been ranked second for *this* content, and nothing further should
  be built on it for now.

Unchanged: **shadow cascades** still matter for any outdoor scene, and the metallic-roughness model
still earns its place, because stone against metal against wood reads perfectly well at low polygon
counts.

### 4. Flat shading needs to be a first-class thing a mesh can ask for

Low-poly depends on **per-face normals** — the faceting is the look. `BoxMesh` already does this
correctly, tessellating twenty-four vertices rather than eight so each face carries its own normal
(and `a_box_has_flat_faces_rather_than_averaged_corners` pins it). Imported glTF and generated meshes
have no such guarantee: a model exported with smooth normals will shade as a blob.

This is a gap rather than a decision, and it is recorded as **Q33**.

## Consequences

**Good.**

- The content path stops depending on capabilities that do not exist here. A generator is the same
  kind of artefact as everything else in the repository: text, diffable, re-runnable.
- Invariant I1 gets easier rather than harder — low-poly geometry *can* be a text file in a way a
  scanned rock cannot.
- The look is coherent with what the engine is good at today: hard edges, flat facets, strong lighting
  from a sky. Those are the parts that are finished.

**Bad, and accepted.**

- **The absence of anti-aliasing becomes the most visible flaw**, where before it was one of several.
  That is a real regression in perceived quality until it is built.
- ADR 0047's normal mapping is now a feature for content this project will not mostly produce. It was
  the right build under ADR 0045's assumptions and it is not wasted — but it is honestly ranked lower
  now, and saying so is better than pretending the order was right all along.
- Low-poly is *not* automatically easier to make look good. It is unforgiving about silhouette, colour
  and proportion, and it fails as flat and toy-like rather than as muddy. The saving is in production,
  not in judgement.

**Explicitly not decided here.** Whether flat shading is a mesh flag, an import setting, or a
generator's responsibility (**Q33**); which anti-aliasing technique; and anything about the target
games' individual looks, which `docs/00-vision.md` still governs.
