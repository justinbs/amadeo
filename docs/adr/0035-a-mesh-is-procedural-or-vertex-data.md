# ADR 0035 — A mesh asset is a procedural shape or vertex data

**Status:** Accepted · **Date:** 2026-08-05 · **Decides:** what a mesh asset is · **Builds on:**
ADR 0020, ADR 0026, ADR 0029, ADR 0033

## Context

`docs/05-roadmap.md` puts mesh rendering and glTF import in M2. Before either can be written,
something has to decide **what a mesh asset is**, because that is what content gets authored against
and therefore the only part that is expensive to change afterwards.

**The pattern holds for the fourth time.** ADR 0018, ADR 0031, ADR 0033 and ADR 0034 each found that
`RenderBackend` isolates rendering *mechanism* so completely that no file, schema or state hash can
observe it — so the mesh pass, the depth buffer and the PBR shader are all cheap. The expensive
decision is the data beside them.

Most of that data is already settled by precedent and needs no new argument:

- A mesh is an **asset with a declared id** (ADR 0020), like a texture, a prefab and a material.
- The `Mesh` component holds ids, not data — ADR 0033's worked example already spells it
  `override Mesh` / `material "stone_rough"`.
- Decoding happens **at load time behind a format tag** (ADR 0026), so a compiled mesh format later
  is an addition rather than a rewrite.

What is left open is narrower and sharper: **can a shape be described in text, or is a mesh always
vertex data?**

## What the research found

**glTF is unusual among interchange formats, and it matters.** It was designed for runtime delivery:
its binary blocks are laid out to be copied straight into GPU buffers with no unpacking step. That is
why Bevy loads glTF directly at runtime where most engines convert to an in-house binary at import.
So "load glTF at runtime" is a genuinely available option here in a way that "load FBX at runtime"
never is, and the eventual importer does not have to be a precondition for anything.

**Godot splits meshes in two**: `PrimitiveMesh` (BoxMesh, SphereMesh, PlaneMesh — procedural,
carrying only parameters) and `ArrayMesh` (actual vertex arrays). Unity ships built-in primitives;
Bevy ships shape primitives. Nobody treats "a cube" as something you must import.

That split matters more here than in any of those engines, because of **invariant I1**. A box
described as three numbers is hand-writable, diffable text. A `.glb` is opaque bytes. If the only
kind of mesh is vertex data, then the first thing M2's exit gate needs — a floor and some walls — is
the one thing a text file cannot express.

## Decision

### 1. A mesh asset is a scene file with one root, carrying either kind

No new format, exactly as ADR 0033 decided for materials and ADR 0029 for prefabs. So the parser, the
canonical writer, `amadeo fmt`, `amadeo check`, ADR 0032's nested values and ADR 0021's load barrier
all apply the day it exists.

```text
scene wall_panel
version 1

entity mesh "Wall panel"
  BoxMesh
    size 1.0 2.5 0.2
```

### 2. Procedural shapes are reflected components, tessellated at load

`BoxMesh`, `PlaneMesh` and the shapes that follow are ordinary reflected types. The schema knows
their fields, `describe` reports them, `describe --example` can spell one, and an agent can author a
3D level without a toolchain, a binary, or a build step.

They are turned into vertices **once, at load**, by engine code — never per frame. That is the same
place ADR 0026 puts image decoding, and for the same reason: the runtime holds one representation and
does not care where it came from.

### 3. Vertex data is the other kind, and the two meet immediately

Both kinds produce the same `MeshData` — positions, normals, texture coordinates and indices — and
nothing above the loader can tell which it came from. That is the whole point, and it is ADR 0026's
`PixelFormat` argument reused: **the shared type is the load-bearing part**, because it makes the
glTF importer a new *producer* rather than a change to the mesh component, the cache, the batcher,
the backend, and every test that asserts on geometry.

**The vertex layout is fixed**: position, normal, UV. Decided rather than made flexible, because a
configurable layout means a shader per permutation and this project has one person; ADR 0033 already
chose defines-plus-a-pipeline-cache over generality for the same reason. Tangents, when normal
mapping needs them, are **generated at load** from the UVs rather than added to the layout — which is
what glTF itself permits when a model omits them.

### 4. What this deliberately does not decide

**The glTF importer.** It is a producer of `MeshData` and nothing above it changes when it lands, so
it needs no decision now — which is the property this ADR exists to buy.

**The `Material` field list**, which ADR 0033 explicitly left to arrive with PBR. It arrives with the
mesh pass and still needs no ADR, because adding a field to a reflected type is the cheap change the
schema exists for.

## Consequences

**Good:**

- **3D content is authorable in text from the first day**, so I1 and I5 reach 3D the way ADR 0031
  made them reach the camera. An agent can build a room out of boxes with no tooling.
- M2's exit gate gets a floor and walls without the importer being finished first, so the milestone
  has a runnable slice long before it has a complete one.
- A procedural shape costs a handful of numbers in a file rather than a committed binary, which keeps
  the repository reviewable.
- The importer, when it comes, is additive.

**Bad, and accepted:**

- **Two runtime paths for "what is a mesh"**, so the loader branches once. Contained: everything
  above `MeshData` sees one thing.
- **Tessellation is engine code that has to be correct.** A wrong normal is a subtly wrong picture
  rather than an error, which is the hardest kind of bug to notice — so each shape needs a test
  asserting its winding and its normals, not just its vertex count.
- **"Which shapes ship" becomes a recurring small question.** Bounded by keeping the list to what a
  target game actually needs, starting with box and plane.
- A primitive's parameters are in the state hash, where imported vertex data is not — because the
  parameters are authored and the vertices are derived. Consistent with ADR 0019's treatment of
  `GlobalTransform`, and worth stating because the asymmetry is otherwise surprising.

## What was rejected

- **One kind only: vertex data, with primitives as a generator tool.** The `.pix` precedent the Vault
  set — hand-written text in, machine-readable output, one command between — and genuinely tempting
  for its single runtime concept. Rejected because that precedent was for *sprite art*, where nobody
  hand-edits pixels, and a box's three numbers are exactly what someone does want to edit in place. A
  build step between wanting a cube and seeing one is a real dent in I1 for the case where I1 is
  cheapest to honour.
- **Primitives built in Rust, assets meaning imports only.** The least machinery by a clear margin.
  Rejected because a scene file could then not author a cube, so the first thing M2's exit gate needs
  would be the one thing text cannot express — and the agent would have to edit Rust and recompile to
  place a box, which is the asymmetry I5 exists to prevent.
- **A configurable vertex layout.** See 3.
