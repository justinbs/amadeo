# ADR 0031 — Two passes in one render graph, and the camera is an entity

**Status:** Accepted · **Date:** 2026-08-03 · **Resolves:** Q3 (last third), Q10 · **Completes:** ADR 0018

## Context

`docs/05-roadmap.md` makes this M2's first item and requires it **before any code**. It is the
question `docs/04-subsystems.md` §4 calls "the real decision of this subsystem", and ADR 0018
deliberately left it open after settling everything around it.

**The framing needs correcting first, and this is the second time.** §4 asks which *pipeline* shape
to use — one orthographic pipeline, two pipelines sharing a graph, or 2D composited over 3D. ADR 0018
opened by observing that Q3's framing emphasised the pipeline while the expensive decisions were
*data*: what a transform is, and what decides draw order. It settled those and said so.

The same is true again. ADR 0023 recorded that the pipeline "is still cheap to revisit" because
`RenderBackend` isolates it entirely — no scene file, no component schema, and no state hash can
observe which was chosen. So the pipeline is a consequence, not a decision.

**The expensive decision hiding inside "how do 2D and 3D coexist" is the camera model**, and nothing
had framed it as the question. A camera is reflected data: it lives in the schema, it can live in a
scene file, and today it lives in the state hash.

Some vocabulary, since it decides things below. A **render graph** is the declared list of passes and
what each one reads and writes. A **pipeline** is the GPU state for one kind of draw — a shader plus
its blending and depth settings. **Rendering to a texture** means drawing the scene into an image
instead of into the window.

## What the research found

**Bevy** runs separate `Core2d` and `Core3d` subgraphs inside one render graph, and runs a subgraph
**once per camera**: a camera says which subgraph to run and where to draw. They call it camera-driven
rendering, and they *migrated to it* — one of the larger rendering changes in the project's history.
That is evidence both that the shape is right and that retrofitting it is expensive. Their recorded
cost for the 2D/3D split is duplication: the two graphs each need their own UI subgraph.

**Godot** keeps 2D and 3D genuinely separate, composited through viewports. The cost surfaces when
you want to mix them — putting 2D content *into* a 3D world means rendering to a SubViewport and using
it as the albedo of a material on a plane mesh. That is friction to avoid, not to copy.

## Decision

### 1. Two passes, one render graph

The sprite pass and the mesh pass are separate pipelines that share one graph, one device, and one
frame. Neither is built on top of the other.

**One unified orthographic pipeline was not actually available.** ADR 0023 already rejected
depth-buffering sprites — the overwhelming majority have soft or cut-out edges, and a depth-tested
transparent sprite erases what is behind it. So "one pipeline" would have meant a 3D pipeline with
depth writes switched off for sprites: two pipelines wearing one coat, with the honesty removed.

**Compositing 2D over 3D was rejected** because it forecloses a 3D object drawn in *front* of a 2D
layer, and because it is the arrangement Godot needs the plane-mesh workaround to escape.

It is also what M2's exit gate 2 asks for — "a 2D scene from M1 still renders correctly and
unchanged, proving coexistence, not replacement". Keeping the sprite pass intact is the most direct
way to be able to claim that.

### 2. A camera is an entity, not a resource

A camera is a `Camera` component beside a `Transform`. A world may hold any number.

`Camera2d` is retired as a resource. This is the expensive half of this ADR and the reason it is
being decided now rather than when meshes land:

- **M4's editor needs a camera the game does not own.** By invariant I1 the editor holds no private
  state, so its viewport camera has to be *in the world*, alongside the game's. A resource cannot
  hold two, so deferring this makes M4 a migration that moves the scene format, the schema, the state
  hash, and a new GUI at once.
- **Rendering to a texture** is a target setting on a camera and impossible without several. Inside
  the Backrooms and Schedule I want security monitors; RimWorld and Zomboid want minimaps.
- **Project Zomboid is isometric**, which is neither cleanly 2D nor cleanly 3D — an orthographic
  projection feeding sprite drawing with Y-sorting. That only works if the projection is a property
  of the *camera* rather than baked into a pipeline.

Position and orientation come from `Transform`, per ADR 0018's one-transform rule. A camera is
therefore an ordinary member of the hierarchy: parenting one to a character is what a follow camera
is, with no special case.

### 3. The camera's fields are flat, and that is a compromise

```text
  Camera
    projection Orthographic     # or Perspective
    height 8.0                  # orthographic: world units tall
    fov 60.0                    # perspective: vertical degrees
    near 0.1
    far 1000.0
    target ""                   # empty = the window; otherwise an asset id
    viewport 0.0 0.0 1.0 1.0    # normalised sub-rectangle of the target
    order 0                     # between cameras, low to high
    active true
```

The natural design is `Projection::Orthographic { height }` — an enum whose payload carries exactly
the fields that projection needs, with nothing meaningless representable. **The scene format cannot
express that** (Q21, found by probing it while designing this): a payload enum emits a Rust `Debug`
form that nothing parses, as do nested structs, and `Option::None` fails to parse outright.

So the fields are flat, and a perspective camera carries a `height` that means nothing. That is
precisely the "two booleans instead of an enum" shape the Vault's `Phase` comment argues against, and
it is accepted here rather than solved, because fixing it properly is a change to ADR 0014's grammar
and belongs in its own decision. **Q21 is filed at P1 and should be settled before M2's material
model**, where the same problem arrives at a type nobody would want to flatten.

`target` is a plain string with empty meaning the window, rather than an `Option` or an enum, for the
same reason — and it matches `Sprite::texture`, which is already an asset id in a string.

### 4. `render.describe` answers for one camera at a time

The agent's primary verification channel (gate 2) has to keep working, and "what is on screen" stops
having a single answer once there are several cameras. It reports the camera with the lowest `order`
drawing to the window by default, and takes an optional camera to ask about instead.

## Consequences

**Good:**

- A scene file can author a camera, so the view is part of the level rather than something code sets
  up. That is invariant I1 reaching a subsystem it had not reached.
- The editor's viewport, render-to-texture, split views and minimaps all become the same mechanism.
- Isometric stops being a third case: it is an orthographic camera feeding the sprite pass.
- 2D is not built on 3D or vice versa, so neither can be made worse by work on the other — trap 9.

**Bad, and accepted:**

- **Both golden replays have to be regenerated.** Resources are in the state hash, so removing
  `Camera2d` from them moves it. This is the change `docs/07` warns hardest about, and it is done
  under the procedure recorded there: isolate the cause, confirm the hashes return when the change is
  reverted, and only then regenerate.
- **The camera type is worse than it should be**, per 3 above. Recorded as Q21 rather than hidden.
- **`FrameData` stops being one view.** The backend trait takes a list of views rather than a single
  camera, which touches the wgpu backend and the null backend.
- **More machinery than one game needs today.** quad-demo and the Vault each want exactly one camera,
  and both now have to author one.

## What was rejected

- **`Camera3d` beside `Camera2d`, both resources.** No churn and enough for M2's exit gate. Rejected
  because it hard-caps the engine at one camera and hands M4 the migration — the same shape of trap
  ADR 0006 reserved multiplayer hooks to avoid.
- **A camera component without a target or viewport**, adding them when something needs them. Pays
  the expensive part now and defers the machinery. Rejected narrowly: adding a field later to a
  component that scene files already author is a second migration, and render-to-texture is the
  feature most likely to be wanted first.
- **Deciding after 3D meshes land**, from real code rather than reasoning — which is how ADR 0011 and
  ADR 0023 were both settled well. Rejected because the render graph's shape depends on the answer:
  whether a pass runs once or once per camera is structural, so building it first means building it
  twice.
