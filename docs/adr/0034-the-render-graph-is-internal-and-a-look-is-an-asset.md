# ADR 0034 — The render graph is internal, and a look is an asset

**Status:** Accepted · **Date:** 2026-08-04 · **Decides:** whether the render graph is a public
extension surface · **Builds on:** ADR 0020, ADR 0029, ADR 0031, ADR 0032, ADR 0033

## Context

`docs/05-roadmap.md` makes the render graph M2's next build item — "declared passes, resource
dependencies, transient targets", running **once per camera** per ADR 0031. `STATUS.md` flagged a
decision hiding inside it and deliberately left it open, because nothing had been built against it:

> **Is the graph a public, extensible surface or an internal detail of the wgpu backend?** M2
> requires a *configurable* post-process stack, which implies games or modules can add passes — and
> that makes the graph an API rather than an implementation.

**The framing needed correcting again, and this is the fourth time in this subsystem.** ADR 0018,
ADR 0031 and ADR 0033 each found `docs/04` §4 asking about the pipeline while the expensive decision
was the data beside it. Here the confusion is in the *vocabulary* rather than the emphasis: "render
graph" names two independent things.

**A frame scheduler.** Each drawing step (a **pass**) declares which images it reads and writes; the
engine derives the order, allocates the temporary images (**transient targets** — images that exist
only within one frame), reuses their memory between passes, and inserts the GPU synchronisation.
This is Frostbite's 2017 FrameGraph, which is where the industry got the idea.

**An extension point.** A place where a game or module inserts a pass of its own.

The roadmap line describes the first. The worry `STATUS.md` recorded is about the second. They are
independent, and only the second is an API decision.

**Most of the first is already done for us.** Frostbite's two headline wins were automatic
synchronisation and reusing GPU memory between passes. wgpu already tracks resource state and inserts
barriers itself — that is one of its core reasons to exist over raw Vulkan or DX12. The scheduler
this engine writes is bookkeeping: which pass writes what, and how big each temporary image is.

### The requirement does not say what it looks like it says

"Configurable post-process stack" can mean two very different things, and `docs/00-vision.md` asks
for only one of them. Its requirement is that "the renderer cannot bake in a look" across five art
directions, and M3's exit gate 5 is a dark corridor with a flashlight that "reads as genuinely
atmospheric". Every part of that is an engine-provided effect with knobs on it — fog, bloom,
tonemapping, colour grading, vignette. **Tunable**, not **extensible**.

## What the research found

**Every reference engine ships the tunable stack as the primary answer and the extension point as an
advanced, later, harder escape hatch.**

| Engine | Tunable (primary) | Extensible (advanced) |
|---|---|---|
| **Godot** | `Environment` resource — sky, ambient, fog, glow, tonemap, adjustments | `CompositorEffect`, arriving in 4.3. The docs call it "an advanced feature that requires a high level of understanding of the rendering pipeline", it runs on the render thread with raw `RenderingDevice` access, and it works on only two of the three renderers |
| **Unity** | Volume framework — profiles blended by proximity | `ScriptableRendererFeature` on the Render Graph API |
| **Unreal** | Post Process Volume settings, plus post-process *materials* | `SceneViewExtension` feeding RDG passes; community write-ups describe getting one working as a months-long effort |
| **Bevy** | thin — this is its weak spot | `ViewNode` in a public render graph |

**Bevy is the one engine that made the graph public, and it is evidence against doing so.**

It *walked back from resource dependencies*. Bevy's graph originally passed data between nodes
through "slots" — exactly the "resource dependencies" this roadmap line asks for. They found slots
boilerplate-heavy and hard to follow, and moved to keeping textures in ECS components (`ViewTarget`)
with the graph doing **ordering only**. The engine that built the full version deleted the expensive
half of it.

And making it public turned it into a permanent migration surface. Bevy's rendering API breaks in
most releases — 0.13 added lifetime constraints to graph node types, and 0.19 shipped
"render-graph-as-systems", a rewrite of the graph itself. Each one breaks every game with a custom
node.

## Decision

### 1. The graph is built in full, and it is internal

Declared passes, resource dependencies, transient targets, running once per camera — all of it, as
the roadmap asks. What it is *not* is a public promise: its types live inside `amadeo-render`, and
nothing outside can name a pass, order one, or add one.

This is what keeps `RenderBackend` a total isolation boundary. That property is the most valuable
thing in this subsystem and it has been load-bearing three times: it is why ADR 0018, ADR 0023 and
ADR 0031 could each say the pipeline was cheap to revisit, and it is what let ADR 0031 prove, in a
three-row table, that an entire render restructuring contributed **nothing** to simulation state.
A public graph gives that up permanently.

### 2. Post-processing and atmosphere are configured by reflected data

The engine owns the effects. Content sets their parameters. Because that configuration is ordinary
reflected data, everything the project already has applies to it with nothing new built: it writes in
a `.scene` file with ADR 0032's nested values, `describe` reports it, `describe --example` can spell
it, `amadeo check` validates it, `amadeo fmt` formats it, snapshots capture it, and it is visible
headless.

**That last point is the invariant argument, and it is the one that decides this.** Invariant I5
says anything the editor can do, the CLI and RPC can do; invariant I7 says every subsystem is
headless-capable. A configuration made of data satisfies both for free. A pass supplied as code
satisfies neither — `NullBackend` cannot report it, `describe` cannot see it, and `amadeo check`
cannot validate it. This is the same shape of argument that settled ADR 0030: the protocol's job is
decided by what the invariants ask of it.

### 3. That data is an asset: an `Environment`, named by an id

A camera holds an environment id, exactly as it already holds a render target id and a `Mesh` will
hold a material id:

```text
  Camera
    projection Orthographic
      height 8.0
    environment "corridor_dark"
```

The file is a scene document with a single root carrying one `Environment` component — the same
shape as a prefab (ADR 0029) and a material (ADR 0033). No new format.

**Consistency with ADR 0033 is the argument, and it is worth being honest that 0033's *decisive*
argument does not apply here.** A material is shared by forty-four walls, so inlining it would put
forty-four copies in every state hash. A world has one to three cameras, so that saving is
negligible. What carries the decision instead:

- **A look is the thing that gets tuned and swapped.** M3's exit gate is an atmosphere exam. Going
  from corridor to safe room is changing one string; blending two named looks later is well-defined,
  which is exactly why Godot, Unity and Unreal all made this a named, reusable object.
- **One rule rather than two.** ADR 0033 established that shared, tunable look data is an asset. An
  environment is the same category, and a second answer would have to be explained every time.
- **The whole asset toolchain applies for nothing** — validation with "did you mean", the ADR 0021
  load barrier, `amadeo assets`, `amadeo import`, and moving the file breaking nothing.

An empty id means the engine default look, matching `Camera::target`, where empty means the window.
It sits on the camera rather than on the world because ADR 0031 already made the camera the thing
that decides what is drawn and how — so a security monitor rendering in night vision is the same
mechanism rather than a special case. A world-wide default can be added later as a resource without
changing any of this.

### 4. An `Environment` holds named effect blocks in an engine-defined order

**Decided here because it is the shape of the type**, which reaches the scene format and the state
hash; the *field list* is deferred exactly as ADR 0033 deferred a material's, because adding a field
to a reflected type is the cheap change the schema exists for.

A fixed set of named, individually-configurable blocks — `fog`, `bloom`, `tonemap`, `grade` and
whatever follows — **not** a user-ordered list of effects. Godot, Unity and Unreal are unanimous on
this and the reason is arithmetic rather than taste: bloom operates on high-dynamic-range values,
tonemapping collapses that range to what a monitor can show, and grading and vignette follow it. The
order is a property of the maths, and a format that lets content reorder them is a format whose main
product is wrong pictures. Order is the engine's job; presence and parameters are content's.

### 5. If an extension point is ever needed, it is Rust, not a file format

Naming the hatch now so it is not re-derived later, in the same spirit as ADR 0011 reserving WASM.

Should a real case appear that engine effects cannot cover, the answer is a Rust trait behind the
`gpu` feature — a game implements a pass, as in Bevy and Unreal. It is **not** a text format that
declares passes. That option was considered and rejected on its own merits: its vocabulary would be
GPU state (blend modes, texture formats, depth settings) chasing wgpu's API forever, it would need a
second namespace of resource names with its own validation and canonical writer, and it would break
I7's symmetry, because a declared pass means nothing to `NullBackend` and headless is how the agent
and CI see everything.

**The trigger is a target game wanting an effect that is genuinely not expressible as parameters on
an engine effect** — a multi-pass or history-aware effect, which is precisely the line Unreal's
documentation draws between a post-process material and a `SceneViewExtension`. Adding an effect to
`amadeo-render` is the answer until then, and for a one-person project it is the same act.

## Consequences

**Good:**

- `RenderBackend` stays the total isolation boundary that made three previous renderer decisions
  cheap, and the graph behind it can be rewritten without breaking a single game.
- A look is authorable, checkable, formattable and diffable before any renderer reads it — the same
  property that let `vault.scene` be written before the sprite path worked.
- The agent can read and set a look through the protocol with no new method, which is I5 satisfied
  by construction rather than by effort.
- The roadmap's "post-process stack **and atmosphere**" becomes one asset rather than two systems.

**Bad, and accepted:**

- **A game cannot add an effect kind the engine has not got.** For this project that means editing
  `amadeo-render`, which is the same act; for a future module or mod author it is a real wall, and
  it is related to Q15's unresolved question about what a mod may do.
- **Expressiveness is bounded by imagination now rather than later.** If the chosen effect set is
  wrong we find out by writing M3's horror slice, which is the right place to find out but not a
  cheap one.
- **Every game gets an environment file**, which is ADR 0033's recorded papercut recurring exactly
  as predicted — and an environment shares one id namespace with textures, prefabs and materials, so
  `corridor` the texture and `corridor` the environment collide.
- **One more indirection between reading a level and knowing what it looks like.** The
  counter-argument for inline data was legibility, and legibility is a stated hard requirement; this
  spends a little of it for reuse.

## What was rejected

- **A public graph with passes declared in a text file.** The maximal I1/I5 answer, and no reference
  engine offers it — an agent could author a whole render pipeline from text. Rejected on three
  counts: the format's vocabulary is GPU state and would chase wgpu's API indefinitely; it needs a
  second resource-name namespace with its own validation and canonical writer; and it breaks I7,
  since a declared pass is meaningless to `NullBackend`. Bevy built this shape as graph slots and
  removed it for being boilerplate-heavy.
- **A public graph as a Rust trait, now.** Cheapest of the three to build, since it is mostly making
  internals `pub`, and genuinely unbounded. Rejected because a pass must be written in wgpu types, so
  wgpu would leak into `amadeo-render`'s public API and every wgpu upgrade would become a breaking
  change for games — which is what Bevy's history demonstrates rather than predicts. Reserved as the
  escape hatch in 5 above rather than dismissed.
- **Post-process configuration inline on the camera.** The strongest legibility answer, no extra file
  for one look, no id collision — and ADR 0033's three arguments genuinely do fail here. Rejected
  narrowly, on the tuning-and-swapping argument and on having one rule for look data rather than two.
- **A user-ordered list of effects.** See 4.
