# ADR 0074 — Geometry is parametric, and authored as text

**Status:** Accepted · **Date:** 2026-08-17 · **Builds on:** ADR 0014, ADR 0026, ADR 0035, ADR 0039 ·
**Supersedes the unbuilt half of:** ADR 0035 · **Answers:** `docs/12-the-bar.md` §3's worst row

## Context

Session 20's engine review measured the thing five sessions of renderer work had been sitting on:

- **23 of 23 `.mesh` assets in the repository are `BoxMesh`.** `PlaneMesh` is registered by three
  games and used by none; `ArchMesh` by none; `GltfPart` by none.
- **12 of 12 materials have every texture slot empty**, so mipmaps, 16× anisotropy, normal mapping
  and metallic-roughness PBR have never drawn a textured pixel in a game.
- ADR 0035 promised a mesh asset would carry *"either a procedural shape or vertex data"*. **The
  vertex-data half was never built**, and `App::load_meshes`' own doc comment admitted it.

The consequence is the whole reason `games/warren` reads as an engine test. The renderer has
cascaded shadows, spot shadows, IBL, PBR, MSAA, bloom and fog. **The content language has one noun.**

`docs/12-the-bar.md` §3 raises this from an aesthetic problem to a requirement: the agent must be
able to produce a game's assets, and *"an engine where Claude can author a level but has to ask a
human for a model is an engine that has offloaded the expensive half."*

## Decision

**A mesh is described by parameters, in text, and the description is the source of truth.**

Justin chose this over three alternatives (see below).

### 1. A primitive set, not a triangle list

Reflected components in `amadeo-render`, each tessellating to `MeshData` exactly as `BoxMesh` does
today: **cylinder, cone/frustum, sphere, capsule, torus, wedge, prism, stair, tube** — alongside the
existing `BoxMesh`, `PlaneMesh` and `ArchMesh`.

Every one is a handful of numbers a person can read and change, and every one is something an agent
can *reason about* rather than emit.

### 2. Composition by assembly, and CSG deferred deliberately

A `CompoundMesh` is **a list of parts, each a primitive with its own transform**, tessellated into
one `MeshData`.

Union is concatenation and needs no boolean at all. That covers most of what architecture and props
actually are — a table is five parts, a lamp fitting is a cylinder and a cage of thin bars, a
generator is a bolted assembly — and it is what the games already do by hand with prefab children,
except that it produces **one mesh, one draw call, one asset** instead of an entity per part.

**Subtraction and intersection are a separate decision and are not in this one.** A robust triangle
boolean is a genuinely hard piece of computational geometry with a long tail of degenerate cases, and
shipping a fragile one would be worse than not having it. It gets its own spike when something needs
a hole that a compound cannot fake.

### 3. Modifiers, which is where the leverage is

`array` (count and step, in one or two axes), `mirror`, and `taper`. A run of racking is one part and
an array; a symmetrical fitting is one half and a mirror.

This is the difference between "a format that can express a model" and "a format an agent is
productive in": a door is `width` and `height`, not two hundred vertices, so a wider door is one
number rather than a new file.

### 4. Raw vertex data exists, as the escape hatch and not as the path

ADR 0035's promised `vertices`/`indices` form is built too — but as the **dump target** for
importers and generators, not as the way anything is authored by hand.

That is what keeps `amadeo-gltf` honest (an imported model has somewhere to land) and gives any
future tool an exit, without inviting anybody to hand-write a triangle soup into a scene file.

## Consequences

- **The generator pattern extends to geometry.** `pix` writes textures, `tone` and `sounds` write
  audio, `sky` and `gloom` write environment maps — all from a readable table of numbers. Meshes
  join that set, which is the property `docs/12` §3 says the engine must have.
- **`amadeo fmt`, `amadeo check`, prefab overrides, snapshots and the future editor all work on
  models for free**, because a model is now a scene document like everything else. That is I1 holding
  under geometry, and it is the same argument ADR 0071 makes for levels.
- **It will never produce an organic shape.** No faces, no creatures, no cloth, no folds. That is a
  real and permanent limitation of this decision, it is chosen with open eyes, and it is why
  `docs/12` §3 states low poly as a first-class art direction rather than a fallback. Organic work,
  when it is needed, arrives through the glTF importer and §4's escape hatch.
- **Backwards compatible.** `BoxMesh`, `PlaneMesh` and `ArchMesh` keep working unchanged; every
  existing `.mesh` file still loads. The mesh loader gains branches, which is exactly the property
  ADR 0035 was written to buy and the first time it has been cashed.

## Rejected alternatives

**Raw vertex data as the primary authoring path.** The cheapest option by a wide margin, and it does
not solve the problem: a five-thousand-triangle model as text is unreviewable, it bloats the
repository, it is not parametric so a wider door is a whole new file, and it helps tools *emit*
rather than helping an agent *author*. Kept as §4's escape hatch, where it is genuinely useful.

**A glTF writer, so generators emit `.glb`.** Real geometry in a standard format — and a `.glb` is
neither diffable nor hand-editable, so it breaks the exact property that makes the whole generator
pattern work: the source is text and the asset is derived. It would buy geometry by giving up
invariant I1 for models, which is the trade this project exists to refuse.

**Blender through an MCP tool.** A real DCC with real modelling, and it genuinely wins for specific
one-off assets. Rejected as *the answer* on three grounds: the output is a frozen triangle soup with
no parametric source, so it is the glTF option's downside plus a dependency; the published experience
of LLM-driven Blender work is that quality converges or degrades after about three critique
iterations; and it falls back to primitives-and-booleans for most work anyway. **If
primitives-and-booleans is what the agent is actually good at, put them in the engine's own format**
— where they are deterministic, diffable, parametric and editable by hand. Blender stays available as
a supplement for the cases where it wins.

## Sources

- [3Dify: LLM-assisted procedural 3D generation via MCP](https://arxiv.org/pdf/2510.04536)
- [Claude + Blender MCP: real-world performance](https://www.mindstudio.ai/blog/claude-blender-mcp-real-world-performance)
- [Development of No Man's Sky](https://en.wikipedia.org/wiki/Development_of_No_Man%27s_Sky) — whose
  creatures are recombinations of parametric artist-made parts rather than per-creature models, which
  is the same bet at a larger scale
