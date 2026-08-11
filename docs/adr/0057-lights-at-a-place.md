# ADR 0057 — Lights at a place: point and spot, eight per view

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0031, ADR 0048, ADR 0049

## Context

The engine has had exactly **one kind of light** since M2: `DirectionalLight`, which has no position.
Every surface in the world is lit from the same angle. That is right for the sun and it is the only
thing the engine could express, so nothing in a scene could be lit *from somewhere* — no lamp, no
torch, no window.

M3's exit gate is a first-person atmospheric horror slice whose renderer exam is, verbatim, *"a dark
corridor with a moving flashlight that reads as genuinely atmospheric"*. A flashlight is a spot
light. This is the last structural gap in the renderer before that gate is buildable.

## Decision

**`PointLight` and `SpotLight` as authored components, up to eight per view, evaluated in the mesh
shader's forward pass.**

### Forward with a fixed array, not clustered and not deferred

**Deferred is out.** It shades from a G-buffer, which fights the 4× MSAA that ADR 0051 chose
specifically because low-poly's only aliasing is silhouettes — multisampled deferred means resolving
per-sample or giving up the anti-aliasing that ADR 0050's art direction depends on. It also needs a
separate forward pass for transparency anyway, so it buys many lights at the cost of the two things
this engine has already committed to.

**Clustered forward is the upgrade path, not the starting point.** It sorts lights into regions of
the screen so a pixel pays only for the ones near it, and it is worth its machinery — a cluster
build, a light index list, a compute pass or CPU binning — somewhere north of a few dozen lights. A
lit room or a corridor with a torch is under eight. Building the machinery first would be building
for a scene nobody has.

The choice is cheap to revisit because `RenderBackend` isolates it completely, which is the property
ADR 0031 demonstrated in a three-row table when an entire render restructuring moved no simulation
state.

### Eight, and the ninth is dropped by distance

Every pixel evaluates every light in the list, so the cap is a direct cost. When a scene has more,
the **nearest to the camera** win — measured to the light's *reach* (its position minus its range),
so a large distant lamp outranks a small near one, which is what "affects this view most" means.

**The cut is silent.** A frame that quietly drops the ninth light is a lit scene with one light
missing, which an author can see and fix. Refusing to draw would be worse, and logging every frame
would be noise. The count is visible through `render.describe`, which is where the question gets
answered.

### A point light is a spot light whose cone is the whole sphere

Both collapse into one `PunctualLight` crossing the backend boundary, with `cone_outer_cos = -1` for
a point light — everything is inside a cone that wide, so the shader needs no branch on the kind. A
per-pixel per-light branch to save two multiplies would cost more than it saves, and it would let the
two kinds shade differently by accident.

They stay **two components**, because that is what an author writes: a point light with an
`inner_angle` field would be a component half of whose fields are meaningless.

### Inverse square, windowed to zero at the range

Falloff is `1 / d²`, which is how light actually behaves, multiplied by a window that reaches exactly
zero at `range`. The window is not physical — real light never quite stops — and it is there for two
reasons: a light with no bound would have to be evaluated by every pixel in the world, and an artist
placing a lamp wants to know what it touches. Without the window there is a visible circle where the
light is cut off.

### The BRDF was extracted rather than copied

`fs_main` computed Cook-Torrance inline for the sun. A sun and a torch differ in exactly two things —
which way the light comes from and how much of it arrives — so the arithmetic moved into
`direct_light` and both call it. Two copies would drift, and the way they would drift is that a
material looks right under the sun and wrong under a lamp.

## Consequences

**They cast no shadows.** Everything a lamp lights, it lights through walls. That is the honest state
of it, it is why the flashlight is not finished, and it was Justin's explicit call to ship the lights
first and the shadows after — so that the component shape, which is the expensive-to-change part
because it lives in scene files, gets settled and used before shadows complicate it.

A spot light's shadow is a second shadow map with a perspective projection; a point light's is six
faces of a cube. Both want a shadow atlas, which `TargetFormat::ShadowMap32`'s layer count (ADR 0055)
already has the shape for.

**Byte-identical for every existing scene**, pinned as bytes: a world with no punctual light, and a
world with one at zero intensity, produce the same pixels as before this existed.

**`games/atrium` gained a warm lamp**, because a feature nothing uses is a feature nobody has looked
at — the same argument that put a character in the Atrium and a player on the Scarp. It shows the
falloff on a pillar and a pool of light on the floor, and it shows the missing shadow just as
plainly.

**The uniform grew by 528 bytes per view.** Eight lights at 48 bytes each plus a count. Uniform
buffers are guaranteed to 64 KB, so this is not close to a limit, but it is the largest single thing
in `GpuMeshView` now and is the first place to look if a future view uniform starts costing.

## Alternatives rejected

**One component with a kind field.** Half its fields meaningless whichever kind was chosen, and a
scene file that can spell `inner_angle` on a bulb. `ShadowMode` makes that trade for a *mode* of one
light; a light's kind is not a mode.

**Lights on the frame rather than on the view.** They sit on `View` beside everything else a backend
needs to draw one pass, which is what ADR 0031 established when the camera became an entity — a
camera rendering to its own texture may one day want its own lighting, and reaching up to the frame
for lights would be the one exception to a rule that currently has none.

**Storing cone angles rather than their cosines.** The shader compares a cosine against a dot
product, so it would convert per pixel per light — a transcendental in the innermost loop in the
engine, and one ADR 0053 would have to make deterministic for no reason.
