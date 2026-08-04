# Amadeo — Current Status

**Last updated:** 2026-08-04 (end of session 9)
**Current phase:** **M0 complete. M1 closed. M2 under way, with all four of its expensive decisions
made before their code** — ADR 0035 settles what a mesh asset is (a procedural shape or vertex data),
and its data half is built. **ADR 0031** settles 2D/3D coexistence and makes the camera an entity —
decided, then built. **ADR 0033** settles the material and shader model — decided, not yet built,
because the code it calls for needs mesh rendering. **ADR 0034** settles whether the render graph is
a public API — it is not — decided, then built the same session.

**The agent can see.** `amadeo capture shot.png` launches a game headless, renders it on an offscreen
GPU and writes a PNG. That closes ADR 0021's "agent's eyes" and gave the GPU path its first automated
coverage, which `STATUS.md` had carried as a known gap through three milestones.

**M1 closed — all five exit gates tested, four met and one refuted.**
Reflection, the scene format, the agent's read layer, the agent protocol and a working `amadeo` CLI,
the whole asset layer, the sprite batcher, textured sprites on the GPU, invariant I8, snapshots,
prefabs, and **`games/vault` — a complete small 2D game** have all landed.

**Gate 4 was tested and found false** — `describe` is a schema, not a manual — which is a result
rather than an omission. **ADR 0030 settles what the protocol is for** and fixes the three parts of
that finding that were genuine holes; the API half stays in `docs/07` by invariant I5. **ADR 0029
closes Q7** with prefabs, and **ADR 0032 closes Q21** by letting a scene file nest values at all.
Q3, Q4, Q7, Q10, Q13, Q14, Q16, Q17, Q19, Q21 and Q22 are all closed, and **every question this
session opened has been closed in it**. Nothing is blocked and nothing is undecided.
**Remote:** `origin → https://github.com/justinbs/amadeo.git` (private). Green on every job.

**Commits are waiting to be pushed** at the end of session 9 — check with
`git log --oneline origin/main..HEAD`, and see the correction below for why that is the instruction
rather than a list.

> **A correction to what this file said.** The previous version claimed two commits were waiting,
> naming `3ea5794` and `c017854`. Both were already on `origin/main`; only `4cc03a1` was unpushed.
> The claim was written before those pushes and never revised. **Check with
> `git log --oneline origin/main..HEAD` rather than trusting this line** — that is why the previous
> version told you to, and it was right.

> ### ⚠️ Two working rules that changed in session 7 — read before doing anything
>
> 1. **Do not `git push`. Justin pushes.** Commit as much as you like; leave it on the local branch
>    and tell him what is waiting. Checking CI with `gh` after *he* pushes is still right.
> 2. **Consult him on anything hard to reverse** — the test is cost-to-undo, not visibility. An
>    internal mechanism nobody would read still warrants asking if ripping it out later means
>    rewriting a lot. Both rules are in `CLAUDE.md` §5.

---

## Where we are

Sessions 1–2 established scope, stack, and architecture. Session 3 built M0. Session 4 closed it by
resolving Q1. Session 5 built most of M1's foundations. Session 6 resolved six open questions and
built the whole agent transport and CLI. Session 7 finished `amadeo-assets`, audited the earlier
work, took the target list from three games to eight, built the sprite batcher, and then chased its
cost down through two layers of the ECS. **Session 8 put sprites on the screen** — a decoder crate, a
texture cache, and the wgpu texture path — **closed invariant I8**, making `Reflect` a
compiler-enforced bound on resources and events and shipping `world.resources`, **shipped snapshots**,
**built `games/vault` and closed M1**, and then **settled Q7 with prefabs**. ADRs 0022–0029.

**Fifteen crates plus two games**, all tested: `amadeo-derive`, `amadeo-image`, `amadeo-core`,
`amadeo-reflect`, `amadeo-ecs`, `amadeo-transform`, `amadeo-events`, `amadeo-assets`, `amadeo-input`,
`amadeo-render`, `amadeo-scene`, `amadeo-snapshot`, `amadeo-agent`, `amadeo-app`, `amadeo-cli`, plus `games/quad-demo` and
`games/vault`.
**932 tests passing** (plus 9 GPU capture tests behind `--features gpu`); fmt, clippy
`-D warnings`, and rustdoc all clean. CI runs on Windows and Linux with a dedicated determinism job.

Twenty-three things work end to end today:

- **The engine runs.** `cargo run -p quad-demo` opens a window with a quad you steer with WASD.
  Deterministic at a fixed 60 Hz, records to a hand-editable `.replay` file, and replays against
  checkpoint state hashes in CI.
- **A text file builds a world.** A `.scene` file (ADR 0014) parses, formats byte-stably, and
  instantiates into a `World` using the engine's real components, hierarchy included.
- **The engine describes itself.** `amadeo_agent::describe` emits the full component schema as
  JSON — names, types, docs, units, ranges, replication — generated from the code, so never stale.
- **The CLI talks to a running game.** `amadeo describe Velocity` describes a type defined in
  `games/quad-demo`, answered over JSON-RPC by the game binary the CLI launched. Also `query`,
  `entity`, `schedule`, `status`, `call`, `check`, `replay`, and a standalone `fmt`.
- **A replay reproduces in a fresh process.** `amadeo replay games/quad-demo/replays/wander.replay`
  launches the game, plays a hand-written recording, and asserts four checkpoint hashes. This is the
  stronger half of the golden-replay claim — the in-process test proves a recording survives a
  rebuild, this proves it survives a new process — and it runs in CI.
- **Assets are named, found, and loaded.** `amadeo assets` lists every declared id with the file
  behind it and whether its bytes are resident; `amadeo import` gives a new file a sidecar with an id
  from its filename. A scene declares what it needs in an `assets` block, and `amadeo check` refuses
  one naming an id that does not exist — with "did you mean" when it is close.
- **Loading cannot move a replay.** quad-demo loads a real file at startup and `wander.replay` still
  matches all four checkpoints, because ADR 0009's `Service` split keeps asset state out of the hash
  structurally rather than by convention.
- **Sprites batch into draw calls.** 20,000 fully interleaved sprites collapse to 32 batches in
  2.58 ms (15.5% of a 60 Hz frame); 50,000 tiles on one sheet are a single draw call.
- **A scene file nests.** An indented block is a list if its lines start with `- ` and named fields
  otherwise (ADR 0032), so `Material` with a `base_colour` inside it, a map, and
  `projection Orthographic` with `height 8.0` beneath it all write and read back byte-identically.
- **The view is part of the level.** `entity eye "Camera"` in a scene file decides what is drawn and
  from where. A world may hold any number (ADR 0031) -- each with a projection, a target that is the
  window or a texture, a viewport rectangle, and an order -- and a camera parented to a character
  *is* a follow camera, with no special case.
- **A frame is a declared plan, not a hardcoded sequence.** The render graph (ADR 0034) names each
  pass and what it reads and writes, and derives the order from that: `view 0` and `view 1` write the
  `scene` transient, `present` reads it and writes the destination. It knows nothing about wgpu, so
  `NullBackend` compiles the same graph and reports the resolved order — a pass-ordering bug is
  catchable with no GPU. Composing the frame off-screen is also **what gave the windowed backend
  `capture`**, which this file had listed as waiting on post-processing.
- **A shape written as three numbers becomes geometry.** `games/vault/assets/meshes/wall_panel.mesh`
  is six lines of text carrying `BoxMesh { size 1.0 2.5 0.2 }`, and it comes out as a tessellated box
  of exactly that size — no toolchain, no binary, no import step (ADR 0035). `amadeo check` validates
  it and `amadeo fmt --check` finds it already canonical. **Nothing draws it yet**; the mesh pass is
  the next thing to build, and the data it will read is done.
- **A camera has a look, and the look is a file.** `environment "corridor_dark"` on a camera, and the
  file behind it is a scene document with one `Environment` — exposure, tonemap, grade, vignette, in
  an order the engine fixes because the order is arithmetic rather than taste (ADR 0034). The
  cameras draw into an **HDR** target so tonemapping has something to compress. `amadeo fmt --check`
  found the new file already canonical, which is "no new format" being true rather than asserted.
  **The default look is a byte-identical no-op** — the Vault's capture is the same PNG, byte for
  byte, as before post-processing existed.
- **The agent can see.** `amadeo capture --package vault --ticks 200 shot.png` launches the game with
  no window, renders it offscreen on the GPU, and writes a PNG — walls, sigils, wardens, the player,
  the score readout. ADR 0021 called capture the agent.s eyes; this is them. `WgpuBackend::offscreen`
  is the mechanism, and it also gives the GPU path its first automated tests: a red quad reaches the
  middle of the target and does *not* fill the corners.
- **Sprites are on the screen.** `cargo run -p quad-demo` shows a strip of textured floor tiles, each
  reading a different cell of one 2×2 texture through its `region` — one texture, one draw call — plus
  one sprite deliberately asking for an id that does not exist, which draws the magenta placeholder.
  A file becomes an id becomes bytes becomes pixels becomes a GPU texture, with a decoder crate
  (`amadeo-image`), a `TextureCache`, and the wgpu texture path in between. ADR 0026.
- **A missing texture is visible and explainable, not fatal.** Three-step fallback ending in an image
  built in code, so the last resort cannot itself be a missing file, and `TextureCache::failures()`
  says which ids fell back and why.
- **The engine describes its whole state, not just half of it.** `amadeo call world.resources`
  reports `Camera2d`, `InputState` and `SimRng` with live values from a running game. Entities carry
  components and everything else is a resource; before ADR 0027 the second half was invisible.
- **And its whole schema, with nothing dangling.** `amadeo describe` has four sections —
  `components`, `resources`, `types`, and a `manual` pointer. `types` is the transitive closure, so a
  field reported as a `Phase` can actually be looked up and its variants read. Before ADR 0030 the
  schema named types it could not describe, and resources were absent entirely.
- **The engine shows you how to spell something.** `amadeo describe Run --package vault --example`
  emits a minimal valid instance in both the scene and JSON spellings, generated from one value so
  they cannot disagree. It teaches that `phase` takes a **bare word** — `phase "Playing"` parses and
  then fails to load, and no schema could ever have said so, because bare-versus-quoted is grammar
  rather than type information. Tested by pasting the output into a scene and loading it, for every
  component in the engine.
- **A moment can be saved and returned to.** `amadeo snapshot --ticks 600 mid.snapshot` captures a
  whole world to a readable text file; `amadeo status --from mid.snapshot` gets back to tick 600 by
  *reading the file* rather than simulating 600 ticks. Verified across separate processes, hashes
  matching exactly. This is the answer ADR 0011 named to the one problem its spike actually found —
  re-simulation, not compilation, is what degrades the agent's loop.
- **There is a game.** `cargo run -p vault` — collect six sigils in a walled arena without touching
  a patrolling warden. Player movement, wall collision, enemy patrols, a sprite-digit score, and a
  win and a lose state. The level is a `.scene` file; the sprites come from hand-written `.pix` text.
  **It was built and debugged without ever being looked at**, which is the whole point: 22 tests
  drive it headlessly, and `render.describe` caught a layout bug — the score readout overlapping the
  top wall — that no simulation test could have seen.
- **Repeated content is written once.** `entity s1 "Sigil" from sigil_pickup` plus a two-line
  `override Transform` is a whole sigil (ADR 0029). The prefab is an asset like any other, so
  `amadeo check` validates the reference and offers "did you mean" on a typo. A prefab may instance
  another; a cycle is reported with its chain rather than expanded forever.
- **A prefab that changed cannot silently lose an override.** If an override names a component the
  prefab no longer has, loading refuses and says which entity, which component, which prefab. That is
  the deliberate opposite of Unity, where the value quietly reverts and you find out months later.

**M0 exit gate: 4 of 4, nothing carried.** Gate item 2's "separate process" half — open since M0
because it needed `amadeo-cli` — closed in session 6: `amadeo replay` plays
`games/quad-demo/replays/wander.replay` through the real game binary in a fresh process, four
checkpoints asserted, and CI runs it in the determinism job.

**M1 exit gate: all five tested.** Gate 1 (a complete small 2D game) is `games/vault`. Gate 2
(verify it through the CLI and RPC without looking) is `tests/verified_without_eyes.rs`, and it found
a real layout bug. Gate 3 (scene round-trip byte-identical) has held since session 5. Gate 5 (golden
replays still pass) holds, including through the prefab conversion. **Gate 4 was tested and found
false**, which is a result rather than an omission — see below.

**No blockers of any kind.** Q14, Q13, Q4, and two thirds of Q3 all closed in session 6 — every one
of them except Q4 built the same session it was decided.

## The single most important thing to do next

**The mesh pass — the GPU half of 3D.** ADR 0035 is decided and its *data* half is built: `BoxMesh`
and `PlaneMesh` tessellate into `MeshData`, `Material` carries the metallic-roughness fields,
`MeshCache` and `MaterialCache` hold them, and `App::load_meshes`/`load_materials` read them off
disk. `games/vault/assets/meshes/wall_panel.mesh` is six lines of text that becomes a tessellated box
with the right dimensions, proved end to end.

**Nothing is on screen in 3D yet**, and that was deliberate — the hard-to-undo half went first.
Since then the *whole CPU side* has landed too, so what is left is only the wgpu work:

- ✅ **The maths.** `Mat4::perspective` (WebGPU's 0..1 depth range, not OpenGL's −1..1),
  `Mat4::inverse_rigid` — which is what a view matrix is — and `project_point`, which refuses a point
  behind the camera rather than folding it back onto the screen mirrored.
- ✅ **Collection.** A `Mesh` with loaded geometry becomes a `MeshInstance` carrying its model matrix
  and its **resolved** material; a `DirectionalLight` becomes a direction and a pre-multiplied
  colour. `View` carries both, plus `eye_matrix`.
- ✅ **A perspective camera is no longer skipped**, and a camera's projection now selects which pass
  it feeds — orthographic gets quads and sprites, perspective gets meshes, neither built on the
  other (ADR 0031).

- ✅ **The depth buffer.** `TargetFormat::Depth32`, declared **only when a frame holds a perspective
  camera** — a full-screen depth texture for every 2D game would be a real cost paid for nothing.
  Depth is its own field on a `Pass` rather than an entry in `writes`, because it is state a pass
  tests against rather than an image any later pass reads. Verified on a real device, not just in
  the plan.

**What remains, all behind `RenderBackend`:**

1. **The mesh pipeline**: vertex and index buffers, which this backend has never had — every pass so
   far generates its geometry from the vertex index alone. Also GPU-side upload of `MeshData`,
   following the texture-upload pattern (`has_texture` / `upload_texture`).
3. **A shader.** Start with diffuse `N·L` against the material's base colour to prove the path, then
   PBR — which is a shader change and therefore cheap, per four ADRs.
4. **The projection**, built in the backend from `eye_matrix` and the target's aspect ratio, because
   only the backend knows the target size. `View` deliberately carries the camera's transform rather
   than a finished view-projection for that reason.

#### One wrinkle already found by reading, before writing any of it

**A depth texture cannot use the transient pool's existing bind group.** `create_transient` builds
one against `texture_layout`, which declares `TextureSampleType::Float` — correct for every transient
so far, all of which are colour images that a later pass samples. A depth texture's sample type is
`Depth`, so creating that bind group against it is a wgpu validation error at *creation*, not at
draw, which makes it look like an allocation bug rather than a layout one.

Two ways out, and the second is better: give `PooledTexture::bind_group` an `Option` and leave it
`None` for depth (nothing samples the depth buffer until shadow maps or fog need it), or give depth
its own layout. **Prefer the `Option`** — it is honest about the fact that nothing reads it yet, and
the day something does, the compiler asks about every place that assumed it could.

Related: `assign_transients` matches on `(width, height, format)`, so a depth transient will never be
handed a colour texture by accident. That was luck rather than design, and is worth keeping.

#### And a decision to make when the pass is written

**Where does depth fit into the graph's vocabulary?** A `Pass` currently declares `reads` and
`writes`, both colour. A depth attachment is neither exactly — it is written, but it is also *state*
the pass tests against. The simplest honest answer is a `Pass::depth: Option<String>` naming a
transient, cleared by the first view pass and loaded by later ones, which is the same rule colour
already follows. Not decided; it is cheap either way, and the graph is internal (ADR 0034) so nothing
outside the crate can observe the choice.

Then fog and volumetrics, which need the depth buffer from (2), and which M3's exit gate 5 depends
on. Then shadow maps, culling and glTF import — the last of which ADR 0035 made additive.

**Also still open in the renderer, in rough order after the mesh pass:**

- **Bloom's blur passes.** Its fields exist in the schema and are inert (`intensity` defaults to
  zero). This is the first effect needing more than one pass, so it is what will finally give
  `assign_transients` two same-shaped transients to reuse a texture between — `Lifetime::overlaps` is
  tested and has never had a real decision to make.
- **Render targets on a camera**, which ADR 0031 shipped as a field and nothing implements. Q23 says
  per-camera post-processing is the same work, so do them together.

### A gap found by running a claim instead of repeating it

ADRs 0033, 0034 and 0035 all say the same thing: a material, an environment and a mesh are scene
files, so `amadeo check` validates them **for nothing**. Pointed at the environment file the Vault
already ships, it refused — *no component named `Environment` is registered*.

The loader reads the type directly through `Reflect::from_value` and never consults the
`ComponentRegistry`, so **loading worked while validation did not**, and the two disagreed about what
counts as valid. Fixed where it belongs — a game that ships an asset registers the type it holds —
and both files are now checked in CI, which is what would have caught it.

**Worth generalising**: "the existing toolchain applies for nothing" is a claim about *other* code,
which makes it exactly the kind that gets written into an ADR and never executed. Run it.

### Physics is the other half of M2, and it has a P0 question — Q24

Raised by Justin asking whether the engine needs a physics engine. It does, and in **this milestone**:
two of M2's four exit gates depend on `amadeo-physics`, which is not started.

**Rapier can give exactly what gate 3 asks for** — bit-level cross-platform determinism, same bytes
on different CPUs and operating systems — through its `enhanced-determinism` feature. **But that
feature cannot be enabled alongside `parallel` or `simd-*`.** Determinism and fast physics are
mutually exclusive, and there is no "take both and decide later". It also pins the rapier version,
because an upgrade may legitimately change results and invalidate every replay containing physics.

The prior is that determinism wins — I3 is non-negotiable and gate 3 is written against it — but that
means the physics layer is single-threaded and scalar *by design*, which is a different layer from
one written assuming it might parallelise later. `CLAUDE.md`'s trap list puts retrofitting
determinism first, so this gets decided before the crate exists.

**Nothing is blocked and nothing else is undecided.** All four of M2's expensive rendering decisions are made
before their code — ADR 0031 for 2D/3D coexistence and the camera, ADR 0033 for the material and
shader model, ADR 0034 for the render graph's visibility. What is left is build work.

### The render-graph decision is closed — ADR 0034

The question this file carried into session 9 — *is the graph a public, extensible surface or an
internal detail?* — was researched and put to Justin, who took the recommendation: **internal**.

**The framing needed correcting for the fourth time in this subsystem**, and this time the confusion
was in the vocabulary rather than the emphasis. "Render graph" names two independent things: a frame
scheduler that derives pass order and allocates transients, and an extension point where a game
inserts a pass. The roadmap line asks for the first; the worry recorded here was about the second.
Only the second is an API decision — and most of the first is already done by wgpu, which tracks
resource state and inserts barriers itself.

**"Configurable post-process stack" also does not say what it looks like it says.** Tunable and
extensible are different things, and `docs/00-vision.md` asks only that the renderer not bake in a
look. Godot, Unity and Unreal all ship the tunable stack as the primary answer and put the extension
point behind an advanced, much later, much harder door. Bevy is the one engine that made its graph
public, and it is evidence *against*: it walked back from resource dependencies (graph slots removed
as boilerplate-heavy) and its public graph has been rewritten repeatedly, most recently as
render-graph-as-systems in 0.19.

The deciding argument for data over code was **I5 and I7**, not anything about rendering:
configuration made of data is authorable, describable, checkable and visible headless for nothing,
and a pass supplied as code is none of those. Same shape of argument that settled ADR 0030.

### The rest of M2

- **Mesh rendering, and with it the `Material` field list.** ADR 0033 settled *where* a material
  lives — an asset with an id, its file a scene file with one root — and deliberately left what it
  *holds* to arrive with PBR, since adding a field to a reflected type is the cheap change the schema
  exists for.

### Gate 4's result, and what closed it — ADR 0030

`describe` is a **schema, not a manual**, and the gate asked it to be both. Justin was given three
options and chose the most complete one. The decision splits along a line the gate had blurred:

**The API half stays out of the protocol, and `describe` says where it lives.** How to declare a
component, register one, write a system, query the world — that is API knowledge, and **invariant I5
is what settles it**: anything the editor can do, the CLI and RPC can do, and the editor will *never*
declare a new Rust component type, because that means editing the game crate and recompiling. So the
gate was asking the protocol for something the project's own invariants do not ask of it. `describe`
now carries a `manual` key naming `docs/07-working-with-the-code.md` — a pointer rather than the
prose, because prose copied into a protocol reply is documentation nothing recompiles.

**The schema half was a genuine hole, and fixing it properly found two more.** Resources were
missing — `Run`, which holds the Vault's entire outcome, appeared nowhere. Beyond that: the schema
was **not closed** (`Run.phase` reported `"type": "Phase"` and nothing could look `Phase` up, so
nothing could know the legal values were `Playing`, `Won`, `Lost`), and a fixed array's **length
existed only inside its name**, so anything needing the count had to parse `"array<f32, 2>"` apart.
Both are editor blockers that would not otherwise have surfaced until M4.

**And `describe <Type> --example` now emits something that loads** — a minimal valid instance in the
scene spelling and the JSON spelling, generated from one value so they cannot disagree. The clearest
justification for it: `phase Playing` is a **bare word**, and `phase "Playing"` parses and then fails
to load. Bare-versus-quoted is scene-format grammar rather than type information, so no amount of
schema would ever have said so.

`games/vault/tests/gate_four.rs` pins all of it, in the game that found the gap. The write-up
`docs/09-gate-4-describe-is-not-enough.md` keeps its honest caveat: the experiment was run by an
agent that had already read the engine source, so the gaps are ones it *noticed* rather than ones it
was stopped by, and the stronger test — hand the JSON to a reader with no prior exposure — has still
not been run.

### What building a real game found

The roadmap's bet was that a game would find what the engine is awkward at faster than reasoning
would. It did, and these are the findings rather than a list of chores:

- **The scene format is impractical for repeated content, and prefabs are what fix it.** A sigil cost
  fourteen lines of scene text and there are six of them. **This is what got Q7 settled** — ADR 0029,
  same session, decided from use rather than from theory. A sigil is three lines now and the scene
  went from 223 lines to 142 with `collect-three.replay` matching all four checkpoints unchanged.
  **But prefabs did not fix the walls, and should not.** Forty-four tiles as instances is 176 lines
  against a seven-line picture of the level, so they stay in `MAP`. Prefabs fix repeated *designed*
  content; a grid wants a tilemap (M7). "Prefabs will fix the walls" was the obvious expectation and
  it was wrong — which is itself the finding.
- **No game had ever loaded a scene file.** `markers.scene` had existed since session 5 and nothing
  read it. The reason was a papercut with teeth: `instantiate` needs the world mutably and the
  registry shared, `App` owns both, and the borrow checker refuses the obvious spelling — so every
  game would have had to rediscover the workaround. `App::load_scene` fixes it.
- **A game with two binaries breaks every CLI command against it** unless it sets `default-run`.
  `amadeo` launches games with `cargo run -p <package>` (ADR 0016), which is ambiguous the moment a
  package has a tool binary alongside the game. The failure is a cargo error with nothing to do with
  the engine, which makes it slow to diagnose.
- **PPM cannot express a sprite.** It has no alpha, so anything drawn over the floor would be an
  opaque rectangle. The Vault's art is therefore PNG, generated from hand-written `.pix` text files
  by a small tool in the game's own directory — which is a miniature of the import pipeline ADR 0026
  defers, with the same shape: hand-authorable input, machine-readable output, one command between.

### Then, in rough order

- **`snapshot.diff`** — comparing two snapshots. The format is text and diffable already, so this is
  polish rather than capability.

**The sprite path has been confirmed on screen** — Justin ran `cargo run -p quad-demo` at the end of
session 8 and the screenshot checks out against the world coordinates: nine floor tiles alternating
light/dark (so each is reading a *different texel* of one shared texture through its `region`, which
is the tilesheet mechanism), the 4×4 magenta placeholder where the deliberately-missing sprite is,
markers and player where their transforms put them, and texture colours matching the literal values
in the `.ppm` (so the sRGB texture format and sRGB surface agree rather than double-converting).

**One thing that is still unexercised: the vertical flip in `sprite.wgsl`.** The UV calculation does
`1.0 - corner.y` because world space has +Y up and texture space has v = 0 on the top row. With a
2-row test texture and `region.height = 0.5`, every sample lands in the top row whichever way v runs
— so a flipped image would look identical. **The first time a real photograph or a tall sprite sheet
goes in, check it is not upside down**, and if it is, that one line is the suspect.

**One more thing waiting on a trigger rather than on a decision:** the **import pipeline**, for when
a target game wants compressed textures or mip levels. ADR 0026 sets out exactly what changes and
what does not; the short version is that nothing above `TextureCache` is affected.

### Also worth knowing

**Q15** — modding versus ADR 0011, raised by the target list growing — blocks nothing today but
should not be discovered late. The other question raised alongside it, the **ADR 0014 / ADR 0020
disagreement about `from`**, is closed: ADR 0029 says an asset id and supersedes 0014's grammar.

### `amadeo-assets` and the sprite batcher — done, session 7

### `amadeo-assets` — done, in the order STATUS.md previously listed

1. ✅ **A directory scan** producing a catalogue. Sorted walk into a `BTreeMap` (I3), duplicate ids
   refused naming both files, and every problem reported at once rather than the first.
2. ✅ **A missing sidecar generated on import**, id defaulting to the filename stem. Prepare-then-apply,
   so a dry run is the same code path as a real one and nothing is written if anything would fail.
3. ✅ **`assets.list` and `amadeo assets`** — the ADR 0020 requirement, in place before ids became the
   reference syntax. Also `amadeo import`, and `--check` on it so it can gate a commit.
4. ✅ **Loading**, to ADR 0021's rule, plus the barrier and the `assets` block a scene declares in.
5. ✅ **`amadeo check` verifies asset ids**, with `similar_to` giving "did you mean".
6. ✅ **The sprite batcher and ADR 0023**, settling Q3's last third against a measurement.

**One decision came up that STATUS.md had said would not** — see ADR 0022 below.

The list that followed it here — `Resource: Reflect`, then snapshots, then Q7 — is kept current at
**The single most important thing to do next** near the top of this file. The first item is done as
of session 8 (ADR 0027); snapshots are now next.

### ADR 0022, and a correction to what this file said

The previous version of this section claimed the loading half had **no open decisions left in it**.
That was wrong on one point, found immediately on starting the work: a game names its asset directory
with a *relative* path, and the working directory differs in all four ways a game gets started — the
CLI sets it to the project root, `cargo run` from a subdirectory does not, and a packaged binary
could be anywhere.

Researched rather than guessed, per the standing instruction. Bevy answers with an environment-variable
chain (`BEVY_ASSET_ROOT` → `CARGO_MANIFEST_DIR` → executable directory); Godot anchors on a marker
file, defining `res://` as the directory holding `project.godot`. **ADR 0022 takes Godot's approach**,
because this project already has a marker file and `amadeo-cli` already walks up for it — resolving
the game side by a different rule would invent a disagreement about which project we are in. It also
needs no shared code, which matters because `amadeo-cli` deliberately does not depend on `amadeo-app`.

Worth knowing for next time: "no open decisions left" is a claim that should be checked, not trusted.

**Two things are undecided rather than unbuilt** — Q12 and Q15. All four entries are in
`docs/06-open-questions.md`; the struck-through two are kept here because their reasoning is still
worth reading:

- ~~**Q3 (the last third) — which render pipeline shape.**~~ **Resolved in session 7 — ADR 0023.**
  Sprites batch by `(sort order, texture)`. Decided against measurements, as the question demanded:
  20,000 interleaved sprites collapse to exactly 32 batches, and a whole tilesheet is one draw call.
  The measurement also found that the pipeline shape is *not* currently the limiting factor — Q16 is
  — which is the opposite of what the question expected.
- ~~**Q7 — prefab override semantics.**~~ **Resolved in session 8 — ADR 0029**, and built the same
  session. `from` holds an asset id (superseding ADR 0014's grammar); an override is a top-level
  patch on the instance **root** and can reach nothing inside it, which is what makes nesting
  structurally safe rather than carefully handled; a dangling override refuses to load. The Unity and
  Godot failure modes the question told us to study are what decided the middle one — both come from
  overrides reaching *inward*, so here they cannot.
- **Q12 — `Service: Send + Sync`.** Not moot: a `kira` audio manager, an asset loader holding a file
  watcher, and a `wgpu` surface all hit it in M3. Decide when the first real offender lands.
- **Q15 — modding, and whether ADR 0011 still holds.** New in session 7, raised by the target list
  growing. ADR 0011 decided game logic is plain Rust, by measurement — but it measured *iteration
  speed for the developer*, and a mod author cannot rebuild the engine at any speed. The reserved
  WASM hatch is probably the right answer (the Q1 spike measured it bit-identical to native at 1.24×,
  and sandboxed by construction), but the trigger ADR 0011 recorded does not cover this reason.
  **Decide before the module system hardens in M2–M3**, since "what can a mod do" is the same
  question as "what is the module boundary". Nothing today depends on it.

Prefab instancing, which this paragraph used to describe as unbuilt-rather-than-undecided, is now
both decided and built: `App::prefab_library` resolves each `from` id through the asset catalogue and
hands the parsed documents to `instantiate_with`.

## Q1 is resolved — ADR 0011

**Game logic is Rust systems in the game crate.** No scripting layer, no dynamic reload,
no `amadeo-script`.

Four candidates were prototyped and measured against one shared benchmark (a three-state enemy AI
over 64 entities, 1800 ticks). Everything is in `spikes/q1-game-logic/`, re-runnable via
`measure.ps1`.

| | edit → observe | state survives | hash vs native Rust | µs/tick |
|---|---|---|---|---|
| **A** pure Rust | 0.95 s (2.1 s in the real game) | no | reference | 4.6 |
| **B** cdylib | 0.69 s | yes | ✅ identical | 4.6 |
| **C** Luau | 0.4 ms | yes | ❌ **differs** | 109.7 |
| **D** WASM | 0.63 s | yes | ✅ identical | 5.7 |

**The recorded Luau prior was refuted, and it is worth knowing why.** Luau is not nondeterministic —
it reproduces perfectly across processes. But its numbers are `f64` and components are `f32`, so it
computes something *different* from the Rust reference: the two agree at tick 1 and diverge at tick 2.
That kills the prior's central mechanism specifically, because "graduate hot logic from Luau into
Rust" changes behaviour and invalidates every golden replay taken before the move.

**The premise behind the whole question was also wrong at this scale.** Q1 was written to avoid a
feared 30-second rebuild. Measured: **0.9 s** for a gameplay edit, **2.0 s** for `quad-demo` (which
links wgpu and winit), **3.2 s** for an engine-crate edit rebuilding everything downstream. There was
no crisis to solve, so the decision is to not pay a permanent architectural cost for it.

**WASM is reserved, not rejected.** It is bit-identical to native Rust (verified across two
optimisation levels) at 1.24× runtime cost, and it is the same artefact M5's web export needs. ADR
0011 names it as the escape hatch behind a measured threshold — a gameplay rebuild sustaining above
5 s. Check by re-running the spike, not by impression.

### Decided
- Name: **Amadeo**.
- Unified 2D **and** 3D from the start (not 2D-first, and not 3D-only). Restated in session 6: the
  three 3D target games order the work, they do not narrow the engine. `CLAUDE.md` §7 trap 9.
- Native desktop first, Windows as the primary target. Web export deferred to M5.
- Graphical editor **and** full text/code/headless parity are both first-class requirements.
- Stack: Rust + wgpu + winit + glam + rapier + egui. See `docs/adr/0002`.
- Scene tree is the authoring model; ECS is the runtime model. See `docs/adr/0004`.
- Text files are the only source of truth. See `docs/adr/0003`.
- Determinism is a hard invariant, designed in from tick zero. See `docs/adr/0005`.
- **Code must stay legible to a Rust-learning human.** Justin intends to read, debug, and fix the
  codebase himself. Boring Rust over clever Rust; accepted cost in verbosity. `CLAUDE.md` §6.
- **Target games: eight of them, extended from three in session 7.** Palworld, Schedule I, Inside the
  Backrooms, **Minecraft, Terraria, Project Zomboid, RimWorld, Stellaris**. Deliberately different
  genres, dimensions, scales, and art directions — used as a prioritisation signal. The intersection
  defines the core; the divergence defines what must stay pluggable. See `docs/00-vision.md`
  § Target games for what the five additions changed; the short version is that 2D became a
  requirement rather than a principle, destructible chunked worlds became a real subsystem, ECS
  throughput and dense UI both moved up sharply, and **modding put ADR 0011 under real pressure
  (Q15)**.
- **Renderer must not bake in an art style.** Configurable post-process stack, flexible dynamic
  lighting, fog/volumetrics. The three targets span stylised-realistic outdoors, low-poly, and dark
  atmospheric interiors.
- **Camera rig is separate from the character controller** — the targets are a mix of first- and
  third-person.
- **Multiplayer is no longer a non-goal.** All three targets are co-op. Client-server with server
  authority and client prediction (*not* deterministic lockstep). Hooks reserved during M0–M2, netcode
  built at M6. See `docs/adr/0006`.
- **First game to finish: single-player first-person atmospheric horror slice** at M3 — smallest
  genuinely finishable complete game, and the hardest test of the renderer.
- **Game logic is plain Rust in the game crate.** No scripting layer, no hot reload. WASM reserved as
  a pre-selected escape hatch behind a measured threshold. See `docs/adr/0011`.
- **`spikes/` exists** for prototypes that answer a question with a measurement. Separate cargo
  workspaces, frozen once their ADR is written. See `spikes/README.md`.

- **Q4 resolved — an asset is named by a declared `id` in its sidecar**, not its path and not a GUID.
  Defaults to the filename stem on import, so it reads like a path and survives a move. ADR 0020.
- **Q13 resolved — `ComponentId` is the hash of a component's canonical name**, not its Rust path.
  Moving a component between crates is free; renaming one is a deliberate, visible change. ADR 0017.
- **Q3 resolved, two thirds of it — one 3D `Transform`, and an explicit `SortOrder`.** 2D is the
  degenerate case rather than a separate type; rotation is Euler degrees so it stays hand-writable.
  The pipeline shape is deliberately still open. ADR 0018.
- **Q14 resolved — the game binary hosts the agent; the CLI launches it.** One-shot JSON-RPC over
  stdio, hand-written parser, `App` owns the `ComponentRegistry`. See `docs/adr/0016`.

### How Justin wants to work — stated in session 6, and load-bearing

These are not preferences to weigh; they are instructions. Full versions in `CLAUDE.md` §5 and §6.

- **Research before asking, not instead of asking.** He has no game-engine-development background
  and says he tends to take whichever option is recommended. So a menu of options I have not
  researched is not sharing a decision — it looks like collaboration and is not. When the codebase
  alone cannot settle a trade-off, go read how real engines solve it. He explicitly endorsed the
  time. ADR 0021 is the worked example: the research changed the answer.
- **Pros *and* cons for every option**, including the recommended one.
- **Plain language**, with the vocabulary defined at the point it affects a choice he has to make.
- **Prefer the more complete option over the faster one.** His words: he would rather have a
  complete engine than one that accumulates problems, and does not mind more steps or more time.
  Do not quietly narrow scope to save effort — that is not the trade he is asking for.
- **No `Co-Authored-By: Claude` trailer on commits.** Personal project; he knows. End the message at
  the last line of the body.

### Not yet decided (blocking)

Nothing is blocking. Q14, the last P0, closed in session 6.

## Environment

Verified on this machine (2026-07-30):

| | |
|---|---|
| OS | Windows 11 Pro 26200 |
| CPU | AMD Ryzen 7 5700X3D (8C/16T) |
| GPU | NVIDIA RTX 4060 Ti — Vulkan and DX12 capable, fine for wgpu |
| RAM | 40 GB |
| Installed | Node 24.16, npm 11.13, git 2.53, Java 25 |
| **Rust** | ✅ rustup + rustc 1.97.1 + cargo 1.97.1, target `stable-x86_64-pc-windows-msvc`, in `%USERPROFILE%\.cargo\bin` |
| **MSVC build tools** | ✅ VS Build Tools 2022 17.14.37, MSVC 14.44.35207. Verified 2026-07-30: `cargo build` compiles **and links**, and the binary runs. |
| Editor | ✅ VS Code + rust-analyzer v0.3.2989 |
| **Toolchain status** | ✅ **No blockers.** Compiles, links, runs, tests. |
| Also missing | Python, cmake. Neither is needed. |
| Gotcha — PATH | Installers update the persistent PATH but not running processes. VS Code's integrated terminal needs **VS Code itself** restarted, not just a new tab. |
| **Gotcha — `gh`** | The GitHub CLI is installed but **not on PATH** for tool invocations, the same as `cargo`. It lives at `C:\Program Files\GitHub CLI\gh.exe`; prefix with `$env:PATH = "C:\Program Files\GitHub CLI;$env:PATH"`. Worth knowing because checking CI yourself after a push is faster than waiting to be told it is red. |
| Smart App Control | **Resolved.** It was blocking every binary this project builds — confirmed via event log (3118, policy `{0283ac0f-…}`). Justin disabled it (one-way change on Win11). If a future machine hits `os error 4551`, this is why; see `docs/07-working-with-the-code.md` §5. |
| Gotcha — winget | `winget install` on an already-installed package attempts an *upgrade* and silently ignores `--override`, so it cannot add a workload. Use the VS Installer to modify an existing install. |
| Gotcha — wgpu | This project is on **wgpu 30**, which differs from most material online. Read the crate source under `~/.cargo/registry/src/*/wgpu-30.0.0/src/api/` rather than trusting search results. `docs/07` records the three changes that cost the most time. |
| **Gotcha — line endings** | `core.autocrlf` is **true** by default on Windows and on GitHub's windows-latest runners. It rewrites committed LF into CRLF on checkout, breaking byte comparisons of `.replay` and `.scene` fixtures — invariant I2. `.gitattributes` pins `eol=lf`; **do not remove it**. This machine has `core.autocrlf=false` set locally, which is why it reproduced nowhere here. Tell: only the *Windows* CI jobs fail, because Linux checkout does no conversion. |

## CI

Green as of session 6. Five jobs: `check` (fmt + clippy), `test` on windows-latest and
ubuntu-latest, `determinism` (the suite three times serially, then release, then a separate-process
replay), and `docs`.

**The first push, in session 6, went red 3/5 and stayed red for four commits.** Not a determinism
failure despite looking exactly like one — see the line-endings gotcha above. Worth knowing that the
run before the fix failed *with identical state hashes on both sides of the assertion*; the
simulation was never wrong.

Older commits still show red on GitHub. That is correct and needs no action: CI ran against trees
that had no `.gitattributes`, so re-running them would fail identically. The code in them is fine —
in every red run, `golden_file_replays_to_its_recorded_hashes` (the test that actually asserts state
hashes) passed.

## Next actions

**M0 is under way and unblocked.** Done so far, in the order it was built:
- ✅ Cargo workspace, workspace lints (`unsafe_code = "forbid"`), toolchain pinned
- ✅ Q5 resolved: 60 Hz fixed timestep (ADR 0007)
- ✅ ECS storage strategy decided: safe archetype columns, no unsafe (ADR 0008)
- ✅ `amadeo-core`: `Tick`, `FIXED_DT`, hand-written PCG32 `Rng` with stream forking, hand-written
  FNV-1a `StableHasher` (cross-checked against an independent implementation), `StableId` / `NetId` /
  `Authority` (the ADR 0006 hooks).
- ✅ `amadeo-ecs`: generational `Entity` handles, `ComponentId` derived from type *name* (not
  `TypeId`, which is not build-stable), type-erased-but-safe archetype columns, archetype migration
  on component add/remove, `iter` / `for_each_mut` / `for_each_pair_mut` queries, per-row change
  ticks, and `World::state_hash`.
- ✅ CI: fmt, clippy `-D warnings`, tests on Windows + Linux, a **determinism job** that runs the
  suite three times in separate processes plus a release build, and a rustdoc job.

- ✅ `Resource` (simulation state, hashed) and `Service` (engine machinery, **not** hashed) as two
  separate stores on `World`, with the distinction enforced by trait bounds — ADR 0009. Found by a
  failing determinism test rather than by design foresight.
- ✅ `amadeo-events`: typed double-buffered queues, a shared `EventClock` giving a total order across
  event types, and a `WorldEvents` extension trait. Events written on tick N are readable on N+1.
- ✅ `amadeo-app`: `Stage`, `Schedule` with `before`/`after` constraints resolved by topological sort
  with **alphabetical tie-breaking** (so registration order cannot influence results), the
  fixed-timestep loop with both `run_ticks` (deterministic, ignores wall time) and
  `advance_real_time` (accumulator, capped at 8 ticks/frame to prevent a catch-up spiral), and
  `SimRng`.
- ✅ Determinism integration suite (`crates/amadeo-app/tests/determinism.rs`) — 14 tests covering
  repeat-run agreement, per-checkpoint agreement, seed divergence, headless-vs-windowed equivalence,
  real-time-vs-exact-tick equivalence, stall recovery, and event ordering.

- ✅ `amadeo-input`: `ActionId` (gameplay reads named actions, never keys), `InputState` with
  `just_pressed`/`just_released` edge detection, `InputSource` implementations (null, scripted,
  replay), and a `Recorder` that writes change-only recordings.
- ✅ **The replay file format** — the project's first authored text format, built to the rules every
  later format must follow (I1/I2): hand-writable, line-oriented, canonically ordered, byte-stable
  round-trip, LF endings, and parse errors carrying line numbers. Rejects a tick-rate mismatch rather
  than replaying it wrong (ADR 0007).
- ✅ **Golden replay harness** with a committed fixture at
  `crates/amadeo-app/tests/golden/walk_and_jump.replay`. A recording made once is replayed by every
  later build and asserted against checkpoint state hashes. Regenerate deliberately with
  `UPDATE_GOLDEN=1 cargo test -p amadeo-app --test golden_replay`.

- ✅ **Deferred commands** — `Commands` service with `despawn`, `insert`, `remove`, `spawn_with`, and
  a `queue` escape hatch. Systems can now change structure from inside a query. The app flushes after
  every stage, so a change requested in `PreSimulation` is visible in `Simulation`. Commands queued
  *during* a flush wait for the next one, deliberately — an unbounded loop inside one flush would
  hang, which is far worse to diagnose than a one-stage delay.

- ✅ `amadeo-render` **abstraction and null backend** — `Transform`, `Quad`, `Camera2d`, the
  `RenderBackend` trait, `NullBackend` (records what *would* have been drawn, so draw calls are
  assertable with no GPU), and the `render_quads` collection pass. Draw order is by explicit
  `Quad::layer` with a stable sort, never by iteration order.
- ✅ `World::iter_pair` — a read-only two-component query, added because the renderer needed one:
  the mutable version would mark every drawn entity as changed each frame and make change detection
  worthless.

- ✅ **The Q1 spike** (session 4) — four candidates for game-logic authoring and hot reload,
  prototyped against one shared benchmark and measured. Resolved by ADR 0011: **plain Rust**.
  Prototypes and numbers in `spikes/q1-game-logic/`; re-run with `measure.ps1`. Established the
  `spikes/` convention (separate workspaces, frozen after their ADR).

**M0 is complete.** Nothing remains.

### M1 so far (session 5)

- ✅ **Three-component ECS queries** — `iter_triple` and `for_each_triple_mut` (writes two, reads
  one). Added because the Q1 benchmark needed exactly that shape and had to work around it.
- ✅ **`amadeo-reflect`** — the `Value` tree (struct fields sorted by construction, so I2 does not
  depend on anyone remembering), `TypeInfo` schema, `TypeRegistry` (BTreeMap, so anything generated
  from it is diffable), and the metadata vocabulary including ADR 0006's replication annotations.
  ADR 0012.
- ✅ **`amadeo-derive`** — `#[derive(Reflect)]` and `#[derive(StableHash)]`. The second matters more
  than it looks: a hand-written `stable_hash` that forgets a field still compiles and still produces
  a plausible number, while silently excluding part of the simulation from every replay assertion.
- ✅ Two gaps closed in `amadeo-core` found while building the above: `stable_hash_of` was `pub` but
  never re-exported, and `[T; N]` had no `StableHash` impl.
- ✅ **`Component: Reflect`** (ADR 0013) — invariant I8 is now enforced by the compiler rather than by
  remembering. An unreflectable type cannot be a component. Every existing component converted to
  `#[derive(StableHash, Reflect)]`, hand-written hash impls deleted, and `Transform`/`Quad`/
  `Camera2d` annotated with units, ranges, and ADR 0006 replication policies.
- ✅ **Q2 resolved and `amadeo-scene` layer 1 built** (ADR 0014). Justin chose a custom,
  indentation-based format from four hand-written candidates in `spikes/q2-scene-format/`. Parser
  with line-numbered actionable errors, canonical byte-stable writer, and the round-trip test that
  satisfies **M1 exit gate 3**. The ADR's worked example is asserted byte-identical to the
  formatter's output, so the spec cannot drift from the implementation.

- ✅ **Scene layer 2** — `ComponentRegistry` in `amadeo-ecs` builds a component from a *name* and a
  `Value`, using monomorphised function pointers rather than a trait object (ADR 0012 chose a
  non-object-safe `Reflect` deliberately, and this is the way back). It owns the `TypeRegistry`, so
  one `register::<T>()` call satisfies I8 with no way to register the constructor and forget the
  schema. Then `amadeo_scene::instantiate` turns a document into entities **atomically** — any
  failure despawns everything it created, because a half-loaded scene looks like it worked.
- ✅ Numeric leniency in `Reflect`: a scene's `intensity 3` arrives as an integer because the parser
  has no schema, and must still fill an `f32` field. Floats accept any numeric value; integers stay
  strict, since an out-of-range integer is a mistake rather than an approximation.

- ✅ **`amadeo-transform`** (ADR 0015) — a new crate holding `Transform` (moved out of
  `amadeo-render`) and `Parent`. Resolves a straight contradiction between `CLAUDE.md` §4 and
  `docs/04` §3 about where hierarchy lives; the `CLAUDE.md` note was a dependency-direction error,
  since render, physics, and animation all sit *below* `amadeo-scene` and all need transforms.
  Scenes now materialise their nesting as real `Parent` components instead of just recording it.

- ✅ **`amadeo-agent`, read half** — `describe` renders the registry as JSON (Pillar 2: "what can I
  do?"), `entity` and `query` render the live world (Pillar 3: "what did I just do?"), on a
  hand-written JSON writer whose objects are sorted so a dump is diffable. `ComponentRegistry` gained
  a type-erased *reader* to match its inserter, and `World::entities()` lists live entities in a
  stable order so introspection does not show churn that did not happen. All read-only, so looking at
  a world cannot perturb it.

### M1 continued (session 6)

- ✅ **Q14 resolved — ADR 0016**, then built the same session. See the session log below for what
  reading the code changed about the question.
- ✅ **A JSON reader** in `amadeo-agent`, beside the writer that was already there, with a round-trip
  test pinning the two together. Strict — no trailing commas, comments, or `NaN` — plus two
  strictnesses past the spec, each because the alternative hides a bug: **duplicate object keys are
  an error** rather than a silent last-one-wins overwrite into a `BTreeMap`, and **nesting is capped**
  so a few thousand `[` arriving from a pipe is a message rather than a stack overflow.
- ✅ **`App` owns a `ComponentRegistry`**, with `App::register_component::<T>()`. This was the gap
  ADR 0016 found by reading code rather than docs: the registry was built ad hoc in tests and nowhere
  else, and `quad-demo` registered nothing, so `describe` against a real game would have reported an
  empty schema for the game's own types the first time anyone tried it.
- ✅ **The protocol** (`amadeo-agent`) and **the host** (`amadeo-app`), split where I6 forces it —
  `amadeo-agent` sits above `amadeo-app`, so it cannot reach down for `App`. It owns the JSON-RPC
  envelope and the methods needing only a world; `amadeo-app` owns the stdin loop and the methods
  needing the schedule or the tick count. A client never sees the seam. Spec in `docs/protocol/v1.md`.
- ✅ **`quad-demo` hands over in one line**, sharing `build_simulation()` with the windowed path so an
  answer about the inspected world is an answer about the game that actually runs (I7).
- ✅ **`amadeo-cli`** — `describe`, `query`, `entity`, `schedule`, `status`, `call`, `check`,
  `replay`, and `fmt`. The ADR 0016 split is visible in `--help`: `fmt` runs in the CLI and never
  builds anything; everything else launches the game through `cargo run`, so a stale binary is
  rebuilt rather than answering for code that no longer exists.
- ✅ **`amadeo replay`** — the separate-process half of the golden-replay mechanism, and the last
  thing carried over from M0's exit gate. `--replay` and `--seed` are *launch* arguments rather than
  methods, because a recording must be installed before the first tick and `App::with_seed` fixes the
  seed at construction — before the handover is even reached. So a game reads
  `amadeo_app::requested_seed()` before building; one that does not gets a clear seed-mismatch error
  instead of a divergence that looks like a regression. Reports every failing checkpoint, not the
  first. Fixture at `games/quad-demo/replays/wander.replay`, hand-written and then filled in from the
  mismatch report — which is the intended way to author one.
- ✅ **`GlobalTransform` and `propagate_transforms`** (ADR 0019) — waiting since ADR 0015, unblocked
  by ADR 0018 settling what a transform is. Walks up the parent chain per entity rather than keeping
  a depth-sorted work list, because that list is a cache with an invalidation story and hierarchies
  are shallow. A `Parent` cycle falls back to the local transform rather than hanging.

  **`GlobalTransform` is `DERIVED`, so it is excluded from the state hash** — Justin decided this
  directly, and it is the reason matrix arithmetic cannot move a replay. Proven rather than asserted:
  `quad-demo` now carries a `GlobalTransform` on every entity and **both replay fixtures are
  byte-unchanged**. Two tests guard each other — one that propagation does not move the hash, one
  that a real change still does, so neither can pass because hashing quietly broke.

  Also a scalar `Mat4` in `amadeo-transform` rather than creating `amadeo-math` or taking glam:
  propagation needs compose-and-multiply and nothing else, and designing a maths surface backwards
  from its first caller is how a wrong abstraction gets locked in.
- ✅ **`amadeo check`** — validates scene files against the game's *real* schema, which is precisely
  what a standalone tool cannot do. Reports **every** problem in one pass rather than the first:
  `instantiate` stops at the first error because that is right for loading and wrong for checking, so
  `amadeo_scene::validate` collects instead, on a new `ComponentRegistry::validate` that answers
  "would this build?" with no `World` to build into. Diagnostics come back naming an entity id; the
  CLI turns that into `file:line` because it is the side that still has the text. One launch covers
  every file named, since a build per scene would make checking a directory unusable.
- ✅ **Q13 resolved — ADR 0017.** `ComponentId` now hashes a component's canonical name rather than
  its Rust path, so moving a type between crates stopped being a silent replay-invalidating change.
  Cost: two components sharing a canonical name now collide. The registry already refuses that;
  `World::insert` gained a **debug-build guard** for anything unregistered.
- ✅ **Q3 resolved, two thirds — ADR 0018.** One 3D `Transform` (2D is its degenerate case,
  `Transform2d` retired), rotation as **Euler degrees** so it stays hand-writable, and `SortOrder`
  replacing `Quad::layer`. The pipeline shape is deliberately still open and dropped to P2.
- ✅ **`GlobalTransform` and `propagate_transforms`** (ADR 0019) — waiting since ADR 0015, unblocked
  by ADR 0018 settling what a transform is. Walks up the parent chain per entity rather than keeping
  a depth-sorted work list, because that list is a cache with an invalidation story and hierarchies
  are shallow. A `Parent` cycle falls back to the local transform rather than hanging.

  **`GlobalTransform` is `DERIVED`, so it is excluded from the state hash** — Justin decided this
  directly, and it is the reason matrix arithmetic cannot move a replay. Proven rather than asserted:
  `quad-demo` carries a `GlobalTransform` on every entity and **both replay fixtures are
  byte-unchanged**. Two tests guard each other — one that propagation does not move the hash, one
  that a real change still does — so neither can pass because hashing quietly broke.

  Also a scalar `Mat4` in `amadeo-transform` rather than creating `amadeo-math` or taking glam:
  propagation needs compose-and-multiply and nothing else, and designing a maths surface backwards
  from its first caller is how a wrong abstraction gets locked in.
- ✅ **The renderer reads `GlobalTransform`**, so hierarchy reaches the screen. Scale and rotation
  come back out of the **composed matrix**, not the local transform — a matrix's columns are its
  scaled axes, so a column's length is that axis's total scale and its angle the total rotation.
  Without that a parent's turn would move a child but not rotate it.
- ✅ **`.gitattributes`** — the fix for the CI failure, see the CI section above.
- ✅ **Q4 resolved — ADR 0020**, and **ADR 0021** on top of it. Asset identity is a declared `id` in
  a sidecar; the simulation never observes asset *state*.
- 🟡 **`amadeo-assets`, first slice** — the `.ama-meta` sidecar format and the `AssetCatalogue`
  mapping id to file, with duplicate ids refused naming both files. Loading, handles, the import
  pipeline and hot-reload are still to come, to ADR 0021's rule.

### M1 continued (session 7)

- ✅ **`amadeo-assets`, the loading half** — all five steps listed above. The scan reports what it
  could *not* catalogue (unimported files, orphaned sidecars), because ADR 0020 predicted that exact
  confusion by name: asking for `wall` is refused while `wall.png` sits right there in the tree.
  Stored paths are normalised to forward slashes, since they go over the protocol and
  `textures\wall.png` against `textures/wall.png` would need a special case in every cross-platform
  assertion. Dotfiles are not assets, which is the *only* rule about what counts as one — an
  extension allowlist would be genre knowledge and I4 forbids it.
- ✅ **ADR 0022** — the asset root is found by walking up for `amadeo.toml`. See the correction above.
- ✅ **The load barrier**, and the `assets` block a scene declares its requirements in. A missing
  asset is recorded and survivable rather than fatal, per ADR 0021. **Proven, not asserted:**
  quad-demo now loads a real 700-byte file at startup and `wander.replay` still matches all four
  checkpoints, because `Assets` is a `Service` and ADR 0009 excludes those by trait bound.
- ✅ **`amadeo check` validates asset ids**, with near-miss suggestions.
- ✅ **A PCG32 reference cross-check** — see the audit below.
- ✅ **The sprite batcher — ADR 0023, resolving Q3's last third.** A `Sprite` component holding a
  texture *id* (ADR 0020) plus a `region`, so a tilesheet is one texture and one batch. Batches are
  `(sort order, texture)` pairs: layering is never violated, and within one order the relative order
  of *different* textures is explicitly not guaranteed — that is the trade, and `SortOrder` is the
  mechanism for controlling it.

  Decided against numbers, as the question demanded. 20,000 fully interleaved sprites collapse to
  exactly **32** batches — the theoretical minimum that preserves layering — and 50,000 tiles on one
  sheet are **one** draw call. Batch counts are asserted (a pure function of the world, no clock);
  times are printed, with only an algorithmic-collapse ceiling asserted.

  Two things the measurement changed. The first version sorted by `(order, &str)` and was 55% slower;
  keying on an index into a sorted texture table made the sort integer-only. And `SpriteInstance`
  carries the transform's **axes** rather than a size and an angle, which removes a round trip
  through trigonometry on both the CPU and the shader — and is strictly more expressive, since a
  size-and-angle pair cannot represent a sheared or non-uniformly-scaled-then-rotated sprite.

- ✅ **Component ids are compile-time constants now — ADR 0024, resolving Q16.** `Reflect` gained
  `STATIC_NAME` (filled in by the derive) and `STATIC_NAME_HASH` (a `const fn` FNV-1a over it), so
  `ComponentId::of::<T>()` is a constant load rather than a `String` allocation plus a hash on
  **every** component access.

  This is an engine-wide win, not a rendering one — `World::get`, `World::insert`, and every query
  pay it. Sprite collection went **5.13 ms → 3.32 ms** at 20,000 sprites (31% → 20% of a frame), and
  the 50,000-tile case **11.55 ms → 6.77 ms**. Ids are byte-identical: both golden replays and the
  separate-process `amadeo replay` pass unchanged, which is the assertion that matters, since a
  different hash would have invalidated every committed replay at once.

- ✅ **Queries are tuples of terms, and a term may be optional — ADR 0025, resolving Q17.**
  `world.query::<(&Transform, &Sprite, Option<&SortOrder>, Option<&GlobalTransform>)>()`. Each column
  is resolved **once per archetype** instead of once per entity, which is the structural reason
  archetype ECSs are fast and the thing Amadeo's hand-written query methods could not express.

  **Justin chose this**, over hand-writing every shape or a lower-level per-archetype accessor, after
  the trade was put to him with the legibility cost stated. It is the one deliberate piece of clever
  Rust in the ECS — a trait with an associated type plus a macro writing the tuple impls — and the
  module docs explain each part of the machinery next to the code rather than only in the ADR.

  Read-only on purpose: a generic *mutable* query cannot prove two type parameters are different
  columns, so Bevy uses `unsafe` for it, this crate forbids `unsafe`, and the measured problem was
  entirely on the read side. `for_each_pair_mut` and friends are untouched, and a test asserts the
  old and new paths see the same world.

  Sprite collection: **3.32 ms → 2.58 ms** at 20,000 sprites, and **5.13 → 2.58 ms** across ADRs 0024
  and 0025 together — 15.5% of a 60 Hz frame, from 31%.

### M1 continued (session 8) — sprites reach the screen

- ✅ **`amadeo-image`, a new crate at the bottom of the graph** — ADR 0026. Decodes PNG and PPM into
  `TextureData { width, height, format, pixels }`. Depends on **no engine crate at all**, so it sits
  beside `amadeo-derive` below even `amadeo-core`.

  **The format tag is the load-bearing part.** `PixelFormat` has exactly one variant today, and it is
  there so that adding GPU-compressed textures later is a new variant plus a new producer rather than
  a change to the loader, the cache, the backend, and every test that asserts on pixels. That is the
  one genuinely expensive-to-retrofit piece of this design, and it costs nothing now.

  Format is chosen by **sniffing the leading bytes, not the extension** — which matters more here
  than in most engines, because ADR 0020 makes the path bookkeeping an author may freely change.

- ✅ **`TextureCache` in `amadeo-render`** — id → bytes → pixels, held. `get` **never fails**: a
  three-step fallback ends in a magenta check built in code, because a placeholder that is itself a
  file cannot cover the case where files are the problem. Every fallback is reported, since a frame
  that silently draws magenta is a frame an agent cannot diagnose.

- ✅ **The wgpu backend draws sprites.** Texture upload, one nearest-neighbour sampler, one bind group
  per texture built once at upload rather than per frame, and a second pipeline sharing the camera
  bind group with the quad one. Every batch's instances go into **one** buffer and each batch draws
  its own slice, so there is one buffer write per frame regardless of batch count — the batches only
  decide how often the texture binding changes, which is the cost ADR 0023 is actually about.

- ✅ **quad-demo shows it.** A nine-tile floor strip, each tile reading a different `region` of one 2×2
  texture, plus one sprite deliberately naming an id that does not exist so the placeholder path is
  visible in the running game rather than only in a test.

**`wander.replay` was regenerated, and the diagnosis was verified rather than assumed.** All four
checkpoints moved. With *only the ten new sprite entities* removed — but `TextureCache` installed,
`Sprite` registered, and a second asset loaded — every checkpoint matched its committed value
exactly. So none of the new machinery touches the state hash; the divergence is authored content
changing, which is what a replay should catch. The diff is four checkpoint lines and a byte-identical
input stream.

**One real error found by writing the shader.** `SpriteInstance::axes` documented itself as carrying
*half*-extent axes and gave a corner formula multiplying by two, while `instance_for` has always
produced full-extent axes and `SpriteInstance::size()` has always read them back as full extents.
The code was consistent; the contract was wrong, and the shader would have been written to it. Fixed,
and the doc now names `QuadInstance` as the convention it shares.

### Invariant I8 closed — ADR 0027, session 8

The other half of I8, deferred by ADR 0013 because it was **not yet possible**: two of the four
resources could not reflect at all. `SimRng` wraps an `Rng` whose state is private to `amadeo-core`,
which sits *below* `amadeo-reflect` and so cannot implement the trait (I6); and `InputState` is two
maps, which the value tree could not represent.

- ✅ **`Resource: Reflect` and `Event: Reflect`**, both compiler-enforced. Events were not in the
  original scope and were added once the work started — `Events<T>` is a `Resource`, so it hit the
  bound transitively, and the argument turns out to be *stronger* for events: the event log is how an
  agent answers "what did I just do?".
- ✅ **`Value::Map`, with string keys.** Kept distinct from `Value::Struct` even though they hold the
  same shape, because a struct's fields are fixed and a map's keys are data — which is what lets
  `from_value` be strict about one and permissive about the other. **Justin chose string keys** over
  Bevy's and Godot's arbitrary-key maps, after the trade was researched: `Value` holds floats and so
  has no total order to sort arbitrary keys by, and a struct-as-a-key has no hand-writable syntax.
- ✅ **`Rng::state()` / `from_state()`**, serving three things that all need to *observe* a generator
  rather than draw from it: reflection, hashing, and snapshots. And **`Reflect for Tick` written
  inside `amadeo-reflect`** — the simpler answer when the state is already public, since the impl can
  go where the trait lives rather than where the type does.
- ✅ **`world.resources`**, the concrete payoff. `amadeo call world.resources` reports a real game's
  `Camera2d`, `InputState` and `SimRng` with live values. Blocked in `docs/protocol/v1.md` on exactly
  this bound; a resource behind a trait object had thrown away everything about its type but a hash.

**Both replays were regenerated, and the diagnosis was verified rather than assumed.** `SimRng` used
to hash `format!("{:?}", rng)` — which made every committed replay depend on the exact text of a
`Debug` impl, so renaming a private field would have invalidated all of them for a reason nobody
would connect to the failure. Justin chose to pay the regeneration now rather than leave it armed.
Reverting *only* that hash — with `Resource: Reflect` in force and five types newly reflected —
restored both replays exactly, proving the reflection work is invisible to the state hash.

**One gap created rather than found, and recorded as Q18.** `InputState` reflects faithfully and
unreadably: its keys are `ActionId`s, which are hashes whose names are not kept, so the protocol
reports `"8831028638596390904"` instead of `"move_x"`. Only visible once `world.resources` existed
and could be pointed at a running game. Nothing is blocked, and the fix belongs at the presentation
layer rather than in the type.

**Verified green: 698 tests passing; clippy, fmt, and rustdoc all clean under `-D warnings`.**

### The sprite work, session 8

**Verified green: 669 tests at that point; all four commands clean.**

**And verified on screen.** Justin ran the demo and the screenshot matches the world coordinates
exactly — tile positions, marker positions, sprite widths at the window's aspect ratio, the
alternating tile colours proving `region` picks a different texel per tile, and texture colours
coming back as the literal values in the file. The one thing it does *not* exercise is the vertical
flip; see "The single most important thing to do next".

### Session 7's work

**Verified green at the end of session 7: 610 tests passing; clippy, fmt, and rustdoc all clean.**

### The audit Justin asked for, session 7

He asked for the earlier work to be re-checked, since everything before the last two additions was
built on whichever option was recommended. What was checked, and what it found:

**The invariants hold, and two of them hold better than the docs claim.**

- **I3 (determinism).** There is **no `HashMap` or `HashSet` anywhere in the engine** — the only
  occurrences are comments explaining why a `BTreeMap` is used instead. No `Instant::now` or
  `SystemTime` in any engine crate. Transcendental functions (`sin_cos`, `atan2`, `hypot`) appear in
  exactly **two** places, and both are outside the hashed path: `amadeo-transform`'s matrix build,
  which feeds `GlobalTransform` (`DERIVED`, excluded by ADR 0019), and `amadeo-render`'s matrix
  decomposition, which is render-side. That matters more than it looks — IEEE 754 does not specify
  transcendental functions, so `sin` can differ in the last bit between platforms. **ADR 0019's
  decision is load-bearing for cross-platform determinism in a way the ADR does not state.**
- **The safety net is real.** The `test` CI job runs on **both** Windows and Linux and includes the
  golden-replay test, which asserts *committed* hashes. So a hashed path growing a `sin` call would
  fail CI on one platform. That is a genuine cross-platform determinism check and the docs undersell it.
- **I6 (dependency DAG).** Verified crate by crate. Every edge points the right way; no cycles.
- **`World::state_hash` is sound.** Entities sorted by index and generation, components in sorted id
  order, resources in `BTreeMap` order, tick included, services excluded. `DERIVED` components skip
  **their id as well as their value**, which is the subtle half — writing the id would mean adding a
  `GlobalTransform` still moved the hash. The sorted-`component_ids` invariant it relies on is
  enforced by `debug_assert`.
- **The golden replay is not vacuous.** Four distinct checkpoint hashes, with a paired `assert_ne`
  guarding against the hash being constant.

**One real gap, now closed. `Rng` had no known-answer test.** Every existing test was a
*self-consistency* property — same seed gives same sequence, different seeds diverge, outputs in
range. All of them would still pass if the algorithm were subtly wrong (shift by 17 instead of 18),
because a wrong generator is still a perfectly deterministic one: I3 would hold and the statistical
quality PCG was chosen for would be silently gone. `StableHasher` *was* cross-checked against an
independent FNV-1a when written; the generator was going on the claim in its own doc comment.

Closed by `crates/amadeo-core/tests/pcg_reference_vector.rs`. **The result: `Rng` reproduces the
official PCG32 demo output exactly** — seeded `(42, 54)` it emits `a15c02b7, 7b47f409, ba1d3330,
83d2f293, bfa4784b, cbed606e`. So the implementation is genuinely PCG32 XSH-RR 64/32, confirmed
against a published vector rather than against a transcription that could share a mistake with it.
FNV-1a's constants and its xor-then-multiply ordering were checked too, and are correct.

**Smaller things found and fixed in passing:**

- `amadeo-agent`'s lib docs still said there was no JSON-RPC server and no JSON parser. Session 6
  built both.
- `quad-demo`'s `build_simulation` doc comment had been detached from it by a `const` inserted
  between them, so the function was undocumented and `DEFAULT_SEED` was documented as a colour palette.
- `docs/protocol/v1.md` listed `assets.list` as not implemented. Now specified.

**Smaller things found and left alone, deliberately:**

- Three `expect()` calls in `amadeo-app/src/schedule.rs` technically breach the "no `unwrap`/`expect`
  in engine crates" convention. All three are provably unreachable local invariants established a few
  lines above, each with an explanatory message; rewriting them would add unreachable error paths.
  Every other occurrence in the engine is inside a doc-comment example, which is fine.
- `amadeo-app` lists `amadeo-input` in both `[dependencies]` and `[dev-dependencies]`. Harmless.

Two things found by running it rather than by thinking about it:

- **PowerShell's pipe prepends a UTF-8 BOM**, and rejecting it produced an error pointing at an
  invisible character — the least actionable message that parser could produce. A leading U+FEFF is
  now skipped, and only a leading one.
- **`state_hash` goes over the wire as a hex string**, not a number. It is a `u64`, JSON numbers are
  `f64`, and above 2^53 a client silently reads a different value — which would break replay
  assertions in the least visible way available.

### Session 5 detail

**The golden replay did not need regenerating**, which was not guaranteed. The derive sorts fields by
name, so any component whose fields were not already alphabetical changes fingerprint. The committed
fixture happens to use only `Position { x, y }` and `Velocity { x, y }` — alphabetical, scalar, no
arrays — so its hashes are byte-identical. `Transform`, `Quad`, and `Camera2d` *did* change, and
nothing asserts on them. Reasoning in ADR 0013 so nobody re-derives it from scratch.

Carried into M1 rather than counted as done — **now closed, in session 6:**
- A **separate-process** replay check. The golden test replays in-process against a committed
  fixture, which covers "separate build" but not "separate process". `amadeo replay` closes it:
  `games/quad-demo/replays/wander.replay` is played by the real game binary in a fresh process, with
  four checkpoints asserted, and CI runs it in the determinism job. **M0's exit gate is now 4 of 4
  with nothing carried.**

Known gaps deliberately left for later:
- No bundle/spawn-with-components API, so building an entity with N components costs N archetype
  migrations. Correct but wasteful; optimise when it shows up in a profile.
- Query shapes reach three components (`iter_triple`, `for_each_triple_mut` — writes two, reads one),
  added in session 5 because the Q1 benchmark needed exactly that and had to work around it. Four or
  more, or a different mutability split, still needs collect-and-write-back. Extend on demand.
- **`Service` requires `Send + Sync`**, which excludes any non-`Sync` runtime from living in the
  world — found when neither script VM in the Q1 spike could be stored there. Harmless today, will
  bite the audio mixer and asset loader in M3. Filed as **Q12**.
- Events cannot be sent from inside a query closure (the world is already borrowed). Workaround is to
  collect then send, as `bounce` does in the determinism tests. Deferred commands solve the same
  problem for structural changes; an equivalent for events has not been built.
- No parallel system execution. ADR 0005 permits it only where access is provably disjoint, and the
  scheduler does not yet track access patterns.
- `SimRng`'s `StableHash` goes through its `Debug` output, which works but is inelegant. Revisit when
  the reflection registry lands in M1 and can expose the state fields directly.

## Open risks

| Risk | Mitigation |
|---|---|
| Scope is genuinely very large (unified 2D/3D + editor + AI layer ≈ rebuilding Godot). | Vertical slices with hard exit gates. Reuse proven crates for solved problems instead of writing them. Ruthless non-goals list in `docs/00-vision.md`. |
| Rust compile times degrade the agent iteration loop. | **Measured, session 4:** 0.9 s for a gameplay edit, 3.2 s for a full downstream rebuild — not currently a problem (ADR 0011). Now depends on keeping the crate graph small and shallow, which has become load-bearing rather than hygiene. Re-run `spikes/q1-game-logic/measure.ps1` when the engine has grown; WASM is the pre-selected answer if the threshold is crossed. |
| **Re-simulation cost, not compile time, degrades the loop.** Getting back to the moment of interest grows linearly with session length (~21 µs/tick; 382 ms to reach 5 simulated minutes). | Snapshot/restore, promoted to an M1 priority by ADR 0011. |
| Determinism erodes silently as features land. | Golden-replay tests in CI from M0. Every subsystem PR adds one. |
| Editor drifts into being the source of truth. | I1/I5 enforced by making the editor an RPC client with no privileged path. Round-trip byte-stability test in CI. |

## Reading order for a fresh session

If you are starting cold, this is the shortest path to being useful:

1. `CLAUDE.md` — invariants (§2), what exists (§4), how to verify (§4b), **how to put a choice to
   Justin (§5)**, and the traps (§7).
2. This file: **How Justin wants to work**, **The single most important thing to do next**, and
   **CI**. Those three are the whole handoff; everything else here is background.
3. `docs/07-working-with-the-code.md` — the Rust patterns this engine uses and why, the everyday
   `amadeo` commands, and the golden-replay mechanism. Skip if you already know the codebase.
4. `docs/adr/` — 28 of them now, so read by need rather than in order:
   - **0023** and **0026** before touching the renderer, **0024** and **0025** before touching
     `amadeo-ecs`. 0026 in particular if you are about to add an asset kind or wonder why the engine
     has a dependency that is not `thiserror`.
     0025 in particular: `world.query` is the API every read path should use, and its module docs
     explain the one piece of deliberately non-boring Rust in the engine.
   - **0013** and **0027** before adding a component, resource, or event — all three require
     `Reflect` by trait bound, and 0027 covers the one awkward case (a type whose state is private to
     a crate below `amadeo-reflect`) plus how maps work.
   - **0005** (determinism), **0008** (ECS storage), **0009** (resource vs service) and **0019**
     (derived components) before touching `amadeo-ecs` or anything that reaches `state_hash`.
   - **0003** and **0004** before touching scenes or the editor; **0014** for the scene format.
   - **0011** before proposing a scripting language or hot reload — decided by *measurement*, so
     reopening it needs numbers, not arguments.
   - **0016** plus `docs/protocol/v1.md` before touching the CLI, the agent, or process boundaries.
   - **0028** before touching snapshots — and before assuming a state-hash comparison proves a
     restore is correct, because it does not.
   - **0028** before touching snapshots — and before assuming a state-hash comparison proves a
     restore is correct, because it does not.
   - **0017** before moving or renaming a component (moving is free now; renaming is not).
   - **0018** before touching transforms or draw order; **0020** and **0021** before assets.
5. `docs/06-open-questions.md` — before assuming anything undecided. Ten remain, none blocking.
   **Q15** (modding vs ADR 0011) and the **`from` conflict inside Q7** are the two that were raised
   in session 7 and deliberately left for Justin. **Q18** is new in session 8 and is the smallest of
   the three: a reflected `ActionId` is a hash nobody can read.

Then `git log --oneline -25`. Commit messages explain *why*, deliberately, and session 6's are long
on purpose — several record a diagnosis that took a while to reach.

**Things that will bite a cold session specifically:**

- **`cargo` is not on PATH for tool invocations.** Prefix with
  `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"`.
- **`gh` is not on PATH either.** It is at `C:\Program Files\GitHub CLI\gh.exe`.
- **Windows PowerShell 5.1 reads UTF-8 as ANSI and writes back a BOM.** If you script a file edit,
  use .NET APIs with `UTF8Encoding($false)`, or every em-dash in the repo is silently corrupted.
  Console *display* of em-dashes as mojibake is harmless; a `git diff --stat` showing the whole file
  changed is not.
- **PowerShell here-strings break `git commit -m`** when the message contains quotes — the message
  gets split into pathspecs and the commit fails confusingly. Write the message to a file with the
  Write tool (which emits UTF-8 with no BOM) and use `git commit -F <file>`.
- **Do not push.** See the box at the top of this file.

## Session log

- **S1 (2026-07-30):** Scope, stack, and architecture decided. Planning docs and ADRs 0001–0005
  written. Repo initialized. No code.
- **S2 (2026-07-30):** Target games captured (Palworld / Schedule I / Inside the Backrooms), module
  priorities reordered toward 3D, and the renderer required to stay art-direction-agnostic.
  **Multiplayer promoted from non-goal to planned M6 with hooks reserved in M0–M2 (ADR 0006)** — the
  largest plan change so far. M3's exit gate set to a horror slice with concrete criteria.
  Human-legibility requirement added to `CLAUDE.md` §6 and `docs/07-working-with-the-code.md`
  created. GitHub remote added (personal account; the *global* git identity on this machine is a
  work account, so this repo carries a local override — do not remove it). Rust verified installed,
  MSVC build tools confirmed missing and blocking, rust-analyzer installed; Smart App Control found
  blocking and disabled by Justin. No engine code.
- **S3 (2026-07-30):** M0 implementation, essentially complete. In order: workspace + CI + `amadeo-core` (ADR 0007 fixed
  timestep, ADR 0008 ECS storage); `amadeo-ecs` archetype storage; `amadeo-events` +
  `amadeo-app` schedules and loop + the resource/service split (ADR 0009, found by a failing test);
  `amadeo-input` + the `.replay` text format + golden replay harness; deferred commands;
  `amadeo-render` abstraction and null backend; the wgpu backend behind an opt-in `gpu` feature; and
  `games/quad-demo`, whose window Justin confirmed working. 228 tests. ADRs 0007-0010 written.
  Visual-design preference recorded in `CLAUDE.md` §6. **Remaining in M0: the Q1 spike only.**
- **S4 (2026-07-31):** **M0 closed.** The Q1 spike, run as a measurement rather than an argument:
  four candidates (pure Rust, hot-reloaded cdylib, embedded Luau, WASM) implementing one shared
  benchmark — a three-state enemy AI over 64 entities — with agreement between them tested by state
  hash rather than by inspection. **ADR 0011: game logic is plain Rust in the game crate**, WASM
  reserved as an escape hatch behind a measured threshold.

  The recorded Luau prior was refuted, and specifically: Luau is perfectly deterministic but its
  `f64` arithmetic computes something *different* from `f32` components, diverging at tick 2. That
  breaks the prior's own central mechanism — graduating a system from Luau to Rust would change its
  behaviour and invalidate every golden replay taken before the move. Luau was also 24× slower, of
  which ~78% turned out to be the marshalling binding rather than the language.

  The question's premise was also wrong at this scale: the feared 30-second rebuild measured at
  0.9–3.2 s. Two engine gaps surfaced along the way — `Service: Send + Sync` excludes any non-`Sync`
  runtime (filed as Q12), and the two-component query limit is now confirmed as a real constraint
  rather than a speculative one. Established the `spikes/` convention. No engine code changed;
  still 228 tests.
- **S5 (2026-07-31):** **M1 begins.** Three-component ECS queries first, closing the gap the Q1 spike
  had exposed. Then the M1 keystone: `amadeo-reflect` and `amadeo-derive`, settling the four
  decisions `docs/04-subsystems.md` §8 flagged as needing to be made before writing any of it — a
  value tree rather than dynamic field access, struct fields sorted by construction so I2 is
  structural rather than remembered, the metadata vocabulary (including ADR 0006's replication
  annotations), and a derived `StableHash` so a forgotten field cannot silently drop simulation state
  out of every replay assertion. ADR 0012. Two latent `amadeo-core` gaps closed on the way. Then
  **ADR 0013: `Component: Reflect`**, turning invariant I8 from a convention into a compiler-enforced
  bound and converting every existing component — the same move ADR 0009 made for
  `Resource: StableHash`, and cheapest at eight components. The golden replay survived, for a reason
  worth reading in ADR 0013 rather than assuming. Finally **Q2**: four scene syntaxes hand-written
  and diffed (`spikes/q2-scene-format/`), where the prescribed criterion turned out not to
  discriminate — diffs are identical in all four — so the spike narrowed it to two and Justin chose
  the custom format. `amadeo-scene` built to it (**ADR 0014**) — parser, canonical writer, and then
  layer 2: `ComponentRegistry` and `instantiate`, so a scene file now loads into a `World` using the
  engine's real components. That surfaced a contradiction between two docs about where hierarchy
  components live, resolved by **ADR 0015** with a new `amadeo-transform` crate — and a second trap
  found on the way, filed as Q13: a component's id is the hash of its *fully-qualified path*, so
  moving a type between crates silently invalidates every state hash containing it. Finished with
  the read half of `amadeo-agent` — `describe`, `entity`, `query`, and a deterministic JSON writer —
  which made Pillar 2 real and surfaced **Q14**: under ADR 0011 a standalone CLI cannot know a
  game's components, so the roadmap's `amadeo-cli` shape needs rethinking before it is written.
  392 tests.
- **S6 (2026-07-31):** **Q14 resolved — ADR 0016 — and then built.** The decision came first and
  alone, deliberately: it fixes the shape of `amadeo-cli` and most of what remained in M1, and was
  worth settling before writing the CLI rather than during.

  Reading the code rather than the roadmap changed the framing twice. First, **option 1 was never a
  competing option** — the game binary is the only process holding the registry, the world, and the
  systems at once, which is the same argument ADR 0010 used to put the event loop there, so hosting
  the agent in the game is the substrate all three options are built on and the only live question is
  what wraps it. Second, **the registry has no home**: `ComponentRegistry` is built ad hoc in tests
  and nowhere else, and `quad-demo` registers nothing, so `describe` would today report an empty
  schema for a real game's own components. ADR 0016 puts the registry on `App` for the same reason
  ADR 0013 made `Component: Reflect` a compiler-enforced bound — registering in one place and
  spawning in another is how a component ends up invisible to the agent.

  Two sub-decisions the question had not asked. **One-shot batch before a live session**: each CLI
  invocation is a fresh deterministic run that exits, which is *more* reproducible than attaching and
  covers M1's exit gate; `sim.step` and the mutating calls wait for M4's editor to actually need a
  connection that outlives one question. And **the JSON parser is hand-written**, joining the writer
  already in `amadeo-agent` — `serde_json` was considered and rejected as the first real dependency
  beyond `thiserror` in a workspace that has hand-rolled PCG32, FNV-1a, and two text formats on
  legibility grounds.

  Then **built all of it**: the JSON reader, the registry on `App`, the protocol in `amadeo-agent`,
  the host in `amadeo-app`, the one-line handover in `quad-demo`, and `amadeo-cli` itself. The thing
  that now works is the point of the whole milestone — `amadeo describe Velocity` describes a type
  defined in `games/quad-demo`, answered over JSON-RPC by a game binary that a CLI which has never
  linked it went and launched. Two bugs were found by running it rather than by reasoning about it: a
  UTF-8 BOM from PowerShell's own pipe producing an error that pointed at an invisible character, and
  `state_hash` needing to be a hex string because a `u64` above 2^53 does not survive JSON's `f64`
  numbers.

  Then **`amadeo check`** on top of it — the first command that could not exist in a standalone CLI
  at all, since validating a component name means knowing which names exist. It needed
  `amadeo_scene::validate`, which collects *every* problem rather than stopping at the first the way
  `instantiate` does: stopping is right for loading and wrong for checking, because an agent fixing a
  file cannot ask a follow-up question and one error per round trip is a functional defect.

  Finally **`amadeo replay`**, which closes the separate-process replay gate carried since M0 —
  the last outstanding item from that milestone. The seed problem it raised turned out to have a
  boring answer: the game asks `requested_seed()` *before* building, rather than the host re-seeding
  afterwards, because a world whose construction consumed randomness would then differ from the one
  recorded and the divergence would look like a real regression.

  Then two decisions that had been waiting, each built the same session it was made. **Q13**
  (ADR 0017): `ComponentId` now hashes a component's canonical name rather than its Rust path, so
  moving a type between crates stopped being a silent replay-invalidating change. Both replays were
  regenerated, and confirmed the diagnosis rather than merely obeying it — only the checkpoint lines
  moved, with byte-identical input streams.

  Then **Q3**, which turned out to be three decisions wearing one question. Reading the code showed
  the framing everyone uses — one pipeline or two — is the *cheapest* of the three to reverse, since
  `RenderBackend` isolates it entirely, while the two expensive ones are about data: what a transform
  is, and what decides draw order. So a three-pipeline spike would have measured the wrong thing.
  **ADR 0018** settles the data half: one 3D `Transform` with 2D as its degenerate case, rotation as
  Euler degrees so it stays hand-writable, and `SortOrder` replacing `Quad::layer`. The pipeline
  choice is deliberately deferred to when the sprite batcher exists and can be measured.

  Then **`GlobalTransform` and `propagate_transforms`** (ADR 0019), waiting since ADR 0015. Justin
  decided directly that a derived component stays **out of the state hash**, which needed a mechanism
  the ECS did not have: `Component::DERIVED`, carried through the type erasure by `Column`, honoured
  by `state_hash`. Named `DERIVED` rather than `HASHED` on purpose — the first states what must be
  *true* so the rule follows from the name, the second describes what it does and invites anyone
  wanting a quieter diff to reach for it. Proven, not asserted: `quad-demo` now carries a
  `GlobalTransform` on every entity and both replay fixtures are byte-unchanged.

  **Then CI, which had been red since the first push and was not what it looked like.** The failing
  assertion had *identical checkpoint hashes on both sides* and differed only in `\n` versus `\r\n`.
  `core.autocrlf` is true by default on GitHub's Windows runners; with no `.gitattributes` it
  rewrote every committed LF on checkout. This machine has it set to `false` locally, which is why
  it reproduced nowhere here across seven different reproductions of CI's exact commands. Fixed with
  `.gitattributes`, verified by two fresh clones under `autocrlf=true` (17 CR bytes before, 0
  after). Worth recording that the toolchain-pin commit immediately before it was a real fix for a
  real defect — `channel = "stable"` was not pinning anything despite its comment promising exactly
  the reproducibility I3 needs — but it was **not** this bug, and I presented it with more adjacency
  to the failure than it earned.

  Finally **Q4 (ADR 0020)** and **ADR 0021**, plus the first slice of `amadeo-assets`. Q4 asked what
  names an asset; the answer follows Q13 one layer up — a path is a *location*, so identity is a
  declared `id` in a sidecar, defaulting to the filename stem so it reads like a path but survives a
  move. ADR 0021 then settled how loading avoids breaking I3, and this one was **researched rather
  than reasoned about** (Justin's standing instruction): the industry pattern is a loading barrier,
  but Bevy chooses it for user experience and tolerates mid-game loads, so adopting it for
  determinism would give the right shape for the wrong reason and would not hold the first time
  someone streams a chunk. The invariant is stronger: gameplay holds an id and never observes asset
  *state*, so there is nothing to branch on.

  519 tests, all five CI jobs green, seventeen commits.
- **S7 (2026-08-02):** **`amadeo-assets` finished** — all five steps the previous handoff listed, in
  its order. The scan, sidecar generation on import, `assets.list` plus `amadeo assets` and
  `amadeo import`, the load barrier with a scene-declared `assets` block, and asset ids validated by
  `amadeo check`.

  **The handoff's claim that the loading half had "no open decisions left in it" was wrong**, and it
  surfaced in the first hour: a relative asset path has to resolve against *something*, and the
  working directory differs in all four ways a game starts. Researched rather than guessed — Bevy
  uses an environment-variable chain, Godot anchors on a marker file — and **ADR 0022** took Godot's
  approach, because this project already has `amadeo.toml` and the CLI already walks up for it.

  Then **the audit Justin asked for**, which is written up in its own section above. Headline: the
  invariants hold, and I3 holds better than the docs claim — there is no `HashMap` anywhere in the
  engine, and all transcendental maths is confined to non-hashed paths, which makes ADR 0019's
  derived-component decision quietly load-bearing for cross-platform determinism. One real gap:
  `Rng` had only self-consistency tests, which would all pass on a subtly wrong generator. Now
  cross-checked, and it **reproduces the official PCG32 demo vector exactly**.

  One unresolved conflict found and deliberately *not* decided alone: ADR 0014 and ADR 0020 disagree
  about whether `from` holds a path or an asset id. Filed under Q7, since it has to be settled before
  prefab instancing.

  578 tests, all four verification commands green, `wander.replay` unchanged.

  **Then the target list grew from three games to eight** — Minecraft, Terraria, Project Zomboid,
  RimWorld, Stellaris added to Palworld, Schedule I, and Inside the Backrooms. Written up in
  `docs/00-vision.md`, and it is a larger change than a list edit: the original three were all 3D,
  all action-paced, all co-op, all rendering-led, and the five additions break every one of those.
  Six consequences, of which two matter most. **2D stopped being a principle being defended and
  became a requirement** — three of the eight are 2D or isometric, which lands the same week the
  sprite batcher does. And **modding became a target-driven requirement**, which puts ADR 0011 under
  a kind of pressure Q1 never evaluated: it decided by measuring the developer's iteration speed, and
  a mod author cannot rebuild the engine at any speed. Filed as **Q15**, deliberately not decided.

  **Then the sprite batcher (ADR 0023), which closed Q3 and then kept going.** The batching rule is
  `(sort order, texture)`: layering exact across orders, grouped by texture within one. 20,000
  interleaved sprites collapse to exactly 32 batches, and a whole tilesheet is one draw call.

  What made the rest of the session was that **the measurement did not agree with the theory.**
  Collecting 20,000 sprites took 5.1 ms, and removing the batcher's own trigonometry moved it by 4% —
  which ruled out the obvious suspect and pointed into the ECS twice over:

  - **ADR 0024** — `ComponentId::of` was allocating a `String` and hashing it *on every call*, on the
    hot path of every component access. Now a compile-time constant via two new `Reflect` consts.
    5.13 → 3.32 ms, and it made the whole engine faster, not just rendering.
  - **ADR 0025** — the ECS could not express an *optional* component in a query, so the renderer fell
    back to `world.get` per entity: 40,000 lookups a frame, which is exactly what archetype storage
    exists to avoid. `world.query::<(&A, &B, Option<&C>)>()` now resolves each column once per
    archetype. 3.32 → 2.58 ms. **Justin chose this design** from three options after research.

  Two near-misses worth keeping. A `static` cache inside a generic function is shared across
  monomorphisations, not per-type — it collapsed every component onto one id and the archetype tests
  caught it instantly. And the throughput fixture gave no entity a `GlobalTransform`, so it was
  measuring a fallback path no shipped game takes; fixing it changed the final number materially.

  **610 tests, all four verification commands green, both replays unchanged throughout** — which
  mattered most for ADR 0024, where a wrong hash would have invalidated every committed replay at
  once.
- **S8 (2026-08-03):** **Sprites reach the screen**, which is what STATUS.md had named the single
  most important thing to do next.

  **The decision turned out to be bigger than the handoff framed it.** The handoff asked where the
  decoder should live. Reading `docs/04-subsystems.md` §5 found something else: an import pipeline
  was already recorded there as **decided** — "the runtime never parses source formats" — with a ✅
  beside it, no ADR behind it, and code doing the opposite. A decision that exists on paper,
  contradicts reality, and has never met real work is worth re-deriving rather than obeying.

  Researched rather than reasoned about. **The reason an import pipeline is eventually mandatory is
  concrete**: GPU-compressed formats like BC7 are deliberately asymmetric, cheap to decode and
  minutes-to-hours per texture to *encode*, so compression can only ever happen offline. Godot, Unity
  and Unreal all import for that reason; Bevy is the outlier, and this project has already declined
  Bevy's answer twice on its merits.

  **What dissolved the tension was noticing that the expensive part is the type, not the pipeline.**
  Give the runtime an explicit `PixelFormat` on day one and the import step becomes a later
  *addition* rather than a later rewrite, because everything above the decoder already speaks
  `TextureData` and cannot tell where one came from. Building the pipeline now would mean a compiled
  file format, a cache, and cache invalidation — which §5 still lists as unsolved — carrying nothing
  but the same RGBA the decoder produces anyway.

  **The dependency question was measured, not argued**, since it breaks a pattern the project has
  held to deliberately: `png` costs 9 crates and a 3.2 s clean release build, `image` costs 15 and
  14.5 s, and both are one-time. What justifies the break is that PNG data is DEFLATE-compressed, so
  hand-writing it means hand-writing inflate — ~800 lines whose failure mode is *slightly corrupt
  pixels* rather than a wrong known answer. PCG32 and FNV-1a were worth hand-rolling; this is not the
  same kind of thing. Justin chose all three recommendations after the trade was put to him.

  Then built it: `amadeo-image`, `TextureCache`, the wgpu texture path, and a demo that shows it.
  Two things worth keeping. **The contract for `SpriteInstance::axes` was wrong** — it claimed
  half-extents where the code has always produced full extents — and it was only caught because the
  shader had to be written from it, which is a good argument for writing the consumer of a doc
  comment rather than trusting it. And **`wander.replay`'s regeneration was diagnosed rather than
  obeyed**: removing only the ten new entities restored all four committed hashes exactly, proving
  the new machinery is invisible to the simulation and the content change is the whole cause.

  669 tests, all four verification commands green, and **confirmed on screen** — Justin ran the demo
  and the screenshot checks out against the world coordinates, including the alternating tile colours
  that prove `region` is picking a different texel per tile.

  **Then invariant I8 was closed — ADR 0027.** `Resource: Reflect` had been the oldest unpaid debt in
  the engine, deferred by ADR 0013 because it was genuinely *not yet possible*: `SimRng` wraps an
  `Rng` whose state is private to a crate sitting below `amadeo-reflect`, and `InputState` is two
  maps, which the value tree could not represent. Both had to be solved before the bound could exist.

  **Two decisions went to Justin and he took both recommendations.** Maps in the value tree get
  **string keys** rather than Bevy's and Godot's arbitrary ones — researched, and the deciding facts
  were that `Value` holds floats and so has no total order to sort arbitrary keys by, and that a
  struct-as-a-key has no hand-writable syntax in an indentation-based format. And **`SimRng`'s
  `Debug`-based hash was retired now** rather than left, at the cost of regenerating both replays.

  The scope grew twice in ways worth recording. **Events joined the bound** — `Events<T>` is a
  `Resource`, so it hit it transitively, and the argument turns out to be stronger for events than
  for resources, since the event log is how an agent answers "what did I just do?". And
  **`world.resources` was built on top**, because the bound alone is invisible: it exists so that
  something can be *shown*, and the protocol doc had listed that method as blocked on exactly this.

  Two things worth keeping. **A type below `amadeo-reflect` has two different answers** depending on
  whether its state is public: `Tick`'s impl was written *inside* `amadeo-reflect` (legal, since the
  impl can live where the trait does), while `Rng` had to expose `state()` and be reflected a layer
  up. And **the replay regeneration was diagnosed, not obeyed** — reverting only the `SimRng` hash,
  with five types newly reflected and the bound in force, restored both replays exactly, proving the
  reflection work never touched the state hash.

  One gap was **created** rather than found, and only became visible once `world.resources` could be
  pointed at a running game: `InputState`'s keys are `ActionId`s, which are hashes whose names are
  not kept, so it reports `"8831028638596390904"` instead of `"move_x"`. Filed as **Q18** with the
  recommendation deliberately withheld until a second instance shows what the general shape should be.

  698 tests, all four verification commands green, both replays regenerated and passing.

  **Then snapshots — ADR 0028 — which ADR 0027 had just unblocked.** Two parts of this were forced
  by earlier decisions rather than chosen, and neither was obvious until traced back. A snapshot must
  be a **file**, because ADR 0016 makes every CLI invocation a fresh process that exits, so an
  in-memory one would die with the process that took it. And a snapshot must capture the **entity
  allocator**, because `state_hash` excludes the free list — so a snapshot of only the live entities
  would restore a world that hashed identically and then handed out different entity handles on the
  next `spawn`. That second one is the whole subject of the ADR: it means **hash equality after a
  restore is necessary and not sufficient**, so correctness is tested by running the world *on*
  afterwards. Delete `free_slots` from the format and exactly one test fails.

  **Justin chose text over binary, and a separate crate over a module.** The speed objection to text
  does not survive the numbers already in STATUS — re-simulation is ~21 µs/tick and writing a few
  dozen entities is well under a millisecond. Reusing the `.scene` format was rejected outright as
  trap 4. The consequence of the separate crate was handled rather than accepted: `amadeo-snapshot`
  *borrows* `amadeo-scene`'s scalar encoding, because `format_float` is subtle in three different
  ways and two copies would drift.

  **Running it against the real game found a defect no unit test had.** `InputState` is two maps, and
  the format had no nesting — so its value fell out in `Display` form, which no parser reads back.
  Since `InputState` is a resource in *every* game, a snapshot of anything real would capture and
  then refuse to restore: broken in the way that looks like it worked until you need it. The format
  gained proper nesting, which it should have had from the start.

  747 tests, all four verification commands green, `wander.replay` unchanged.

  **Then M1's exit gate: `games/vault`, a complete small 2D game.** Collect six sigils in a walled
  arena without touching a patrolling warden. All five things the gate asks for — player moves,
  enemies patrol, collision, a score, a win state.

  **Three engine gaps had to close first**, and finding them was worth as much as the game. Gate 2
  names `render.describe` as the verification channel and **it did not exist**. Gate 1 says the game
  is authored via text files, but **no game had ever loaded a scene file** — `markers.scene` had sat
  unread since session 5, because `instantiate` needs the world mutably and the registry shared and
  `App` owns both, so every game would have had to rediscover the workaround. And the roadmap's
  snapshot acceptance test had never been run: measured at **22× faster than re-simulating** in
  debug, which is the profile the agent's loop actually uses.

  **The game was built and debugged without ever being looked at.** That is the milestone's whole
  thesis and it held: the win circuit was authored blind, by reasoning about distances and speeds,
  and passed first time. `render.describe` then caught a real layout bug — the score readout
  overlapping the top wall by 0.15 units — which no simulation test could have seen and which was
  fixed before anyone opened a window.

  **What the game found about the engine is written up above**, and the headline is that **the scene
  format is impractical for repeated content**: forty-four wall tiles would be four hundred lines of
  near-identical text. That is what prefabs are for, and prefabs were blocked on Q7 — which is what
  got Q7 settled later the same session. The argument arrived from use rather than from theory.

  795 tests, all four verification commands green, and a new replay fixture asserted by CI in a
  separate process.

  **Then exit gate 4, tested — and its claim is false.** The gate says `describe` output should be
  sufficient to write a new component and system without reading engine source. Tested by doing it:
  `Trap` and `spring_traps`, shipped in the Vault. `describe` turned out to be **sufficient to author
  content and silent about the API** — every field carries type, unit, range and meaning, which is
  what made `vault.scene` writable, and nothing in it says how to declare a component, register one,
  write a system, or query a world. **Resources are absent from it entirely**, so `Run` — the very
  resource `spring_traps` exists to change — appears nowhere. Written up in
  `docs/09-gate-4-describe-is-not-enough.md`, with an honest caveat about the confound: the
  experiment was run by an agent that had already read the engine source, so the gaps are ones it
  *noticed*, not ones it was stopped by. Three options for closing it are in that document and the
  choice is Justin's, because it decides whether the protocol is a schema or a manual.

  **Then Q7 — prefabs — ADR 0029**, chosen over the roadmap's next item because the Vault had just
  run straight into it and nothing would ever be better informed. Both halves settled:

  - **`from` holds an asset id**, superseding ADR 0014's path grammar. The whole asset toolchain then
    applies to a prefab for nothing — `amadeo check` validates the reference and offers "did you
    mean", ADR 0021's barrier makes it resident before the first tick, `amadeo assets` lists it.
  - **An override is a top-level patch on the instance root and reaches nothing inside it.** This is
    the half the research decided. Unity's overrides evaporate under nesting because an override
    names something *inside* a prefab and then has to track it across every future edit of that
    prefab; Godot's editable children can write back to the source scene and to every other instance.
    Both failures come from overrides reaching inward. Here there is no syntax that can, so there is
    nothing to lose track of — nesting is **structurally** safe rather than carefully handled, and
    `nesting_is_safe_because_overrides_cannot_reach_inside` is a passing test rather than a hope.
  - **A dangling override refuses to load**, naming the entity, the component and the prefab. The
    direct counter to Unity's worst behaviour: the failure arrives when the prefab changed, not
    months later as a value that mysteriously reverted. `override Foo` on a component the prefab
    lacks is an error, and a bare `Foo` on one it *has* is an error too, because the author meant
    `override` and silently picking one would hide it.

  **Proof it is behaviour-preserving:** the Vault's six sigils and two traps became prefab instances,
  the scene went from **223 lines to 142**, each sigil from fourteen lines to three — and
  `collect-three.replay` matched **all four checkpoints unchanged**. The same world, authored
  differently.

  Two costs, both recorded rather than swept up. A prefab shares one id namespace with every other
  asset, and the Vault hit that immediately: `sigil.scene` collided with the `sigil` texture, fixed
  by renaming to `sigil_pickup`. And **`amadeo import` cannot import a prefab** — a bootstrapping
  deadlock, since `import` launches the game and the game refuses to start while a prefab it needs
  has no sidecar. The Vault's two sidecars were written by hand; the deadlock is filed as **Q19**.

  **What prefabs deliberately do not fix: the wall grid.** As instances the forty-four tiles would be
  176 lines of scene text against a seven-line picture of the level, so they stay in `MAP`. Prefabs
  fix repeated *designed* content; a grid wants a tilemap, which is `mod-tilemap` in M7. Worth
  stating because "prefabs will fix the walls" was the obvious expectation and it is wrong.

  **And a bonus find while writing the prefabs up:** `amadeo fmt --check` had never been pointed at
  a scene file, and **all four in the repo were non-canonical** — components out of sorted order,
  written by hand and never run through the formatter. Invariant I2 applies to hand-written scene
  files exactly as `cargo fmt` applies to code and nothing was enforcing it. Reformatted, and CI now
  checks all four; `collect-three.replay` still matched all four checkpoints afterwards, which is
  also a small proof that component order within an entity does not reach the state hash.

  817 tests, all four verification commands green, `amadeo check` passing on the scene and on both
  prefabs.

  **Then gate 4's decision, which had been left for Justin — ADR 0030.** Three options were put to
  him, from "leave it failed and say so" to extending the protocol; he took the most complete one.
  The reframe that made it tractable: the five gaps gate 4 found are **two different kinds of
  thing**, and treating them as one question is what made it look hard.

  **Four of them are API knowledge and stay out of the protocol.** The argument is **I5**: anything
  the editor can do, the CLI and RPC can do — and the editor will never declare a new Rust component
  type, since that means editing the game crate and recompiling. So the gate was asking the protocol
  for something the project's own invariants do not ask of it. `describe` gained a `manual` key
  naming the file instead. Rejected outright: putting the recipe *in* the reply, because prose inside
  a protocol reply is documentation nothing recompiles. MCP has exactly that field — servers may
  return an `instructions` string at handshake — and the spec calls it a hint, and most servers do
  not set it.

  **The fifth was a real hole, and looking at it properly found two more the gate had missed.**
  Resources were absent from `describe` entirely. The schema was also **not closed** — `Run.phase`
  reported `"type": "Phase"` and nothing could look `Phase` up, so nothing could know its legal
  values. And a fixed array's **length lived only inside its name**, so anything needing the count
  had to parse `"array<f32, 2>"` back apart. Both of those are editor blockers that would not have
  surfaced until M4.

  Bevy's remote protocol is the closest analogue and it went the same way twice: resources were added
  to BRP after the fact, and a third-party crate added `discover_format` because the schema alone
  "doesn't show the actual JSON format needed" — leaving people reverse-engineering shapes out of
  error messages. That is what `describe.example` is, built in rather than bolted on.

  **`describe <Type> --example` emits a minimal valid instance** in both the scene and JSON
  spellings, from one value so they cannot disagree. Its clearest justification is a single line:
  `phase Playing` is a bare word, and `phase "Playing"` parses and *then* fails to load — grammar
  rather than type information, so no schema would ever have said it. The testable property is that
  the emitted example **loads**, and that is the test, for every component the engine has.

  Two things went in underneath to make it possible, both defensible on their own: `Reflect` gained a
  derive-generated `register_dependencies`, so registering a type registers everything it names
  (inserted before recursing, so a self-referential type terminates — that is a test, not a hope);
  and `TypeKind::List` gained a `length`.

  827 tests, all four verification commands green, both replays matching all eight checkpoints
  unchanged.

  **Then M2 opened with its ADR, which the roadmap requires before any code — ADR 0031.** The
  interesting part is that the question was pointed at the wrong thing, and it was the *second* time.
  `docs/04` §4 calls the pipeline shape "the real decision of this subsystem"; ADR 0018 had already
  corrected that framing once, noting that Q3 emphasised the pipeline while the expensive decisions
  were data. ADR 0023 then recorded outright that the pipeline is cheap, because `RenderBackend`
  isolates it so completely that no file and no hash can observe it.

  So the pipeline was a consequence rather than a choice: **two passes in one render graph**, neither
  built on the other. Option (a), one unified orthographic pipeline, was not actually available —
  ADR 0023 had already rejected depth-buffering sprites because transparent sprites erase what is
  behind them, so "one pipeline" would have meant a 3D pipeline with depth switched off for sprites.
  Two pipelines with the honesty removed. Option (c), compositing 2D over 3D, forecloses a 3D object
  drawn in *front* of a 2D layer, and is the arrangement Godot needs a plane-mesh-and-SubViewport
  workaround to escape. Bevy runs separate `Core2d` and `Core3d` subgraphs in one graph, which is
  where this lands too.

  **The expensive decision hiding inside it was the camera model, and nothing had framed it as a
  question.** A camera is reflected data — it lives in the schema, it can live in a scene file, and
  today it lives in the state hash. `Camera2d` is a *resource*, so a world can hold exactly one,
  forever. Justin chose to make it an entity now rather than later, taking the full version with
  render targets and viewport rectangles.

  Three things forced it. **M4's editor needs a camera the game does not own**, and invariant I1 puts
  it in the world rather than in private editor state — so deferring would have made M4 a migration
  moving the scene format, the schema, the state hash and a new GUI at once. **Render-to-texture** is
  a target setting and impossible with one camera; Backrooms and Schedule I want security monitors,
  RimWorld and Zomboid want minimaps. And **Project Zomboid is isometric**, which is neither cleanly
  2D nor cleanly 3D — an orthographic projection feeding sprite drawing with Y-sorting, which only
  works if the projection belongs to the camera rather than to a pipeline. Bevy migrated to
  camera-driven rendering the same way, which is evidence both that the shape is right and that
  retrofitting it is expensive.

  **And designing the component found a real hole in the scene format.** Probing it directly rather
  than assuming: a nested struct emits `{height: 8}`, a Rust `Debug` form nothing parses; an enum with
  a payload does the same; and `Option::None` writes a bare field name that the parser refuses
  outright. Never hit before, because every component in the engine is scalars and flat lists. It is
  why ADR 0031's camera is flat — a fieldless `projection` enum beside plain `height`, `fov`, `near`
  and `far` — when `Projection::Orthographic { height }` is the obvious design and the better type.
  Accepted rather than solved, and filed as **Q21 at P1**, because fixing it is a change to ADR 0014's
  grammar and deserves its own decision. It has to be settled before M2's material model, where the
  same problem arrives at a type nobody would want to flatten.

  **No code yet.** The ADR is committed on its own, which is what "decided before code" means.

  **Then ADR 0031, built.** A `Camera` component replaces the `Camera2d` resource; `FrameData`
  becomes a list of `View`s, one per active camera, already in draw order; the wgpu backend runs one
  pass per view with a dynamic-offset uniform buffer, only the first clearing, so a HUD camera
  composes over a world camera rather than erasing it; and `render.describe` answers for the camera
  that draws first to the window, with `describe_frame_through` for any other. **Both games now
  author their camera in their scene file**, which is invariant I1 reaching a subsystem it had not
  reached — the view is part of the level.

  **A world with no camera draws nothing**, where it used to fall back to a default. That was the
  right answer when there could only ever be one camera and is the wrong one now: inventing a view
  nobody authored would draw a picture nobody asked for. The screen is still cleared, so "no camera"
  looks empty rather than frozen.

  **Both replays moved, and the isolation was unusually clean.** `docs/07` says find out why before
  regenerating, and the answer here is a three-row table: HEAD reproduces; HEAD with *only* the
  camera's data placement changed gives `950455d547a4adf9` at tick 300; the whole refactor with that
  same change gives **the identical value**. So the entire render restructuring contributes nothing
  to simulation state, and every bit of the movement is the deliberate data move. Regenerated on that
  basis.

  **Building the control turned up Q22.** The stand-in resource had the same canonical name and the
  same fields and hashed *differently* — because `ResourceId` hashes the **Rust path** while
  `ComponentId` hashes the canonical name (ADR 0017). Opposite rules for the two, which means moving
  a resource between crates silently invalidates every golden replay. Nothing is broken today; the
  crate graph is still moving, so it is worth deciding.

  839 tests, all four verification commands green, both replays passing on their new hashes, and
  `amadeo check` and `amadeo fmt --check` clean on all four scene files.

  **Then Q21 — ADR 0032 — because the camera's flat fields were a symptom.** The grammar already had
  the slot: a field with no inline value already opened an indented block, it just only accepted
  `- ` items. So the whole extension is one rule, and it is **YAML's** rule — a block is a list if
  its lines start with `- ` and named fields otherwise. No schema is consulted, which matters,
  because layer 1 deliberately has none. Nested structs, maps and enum payloads all fall out of it,
  and **maps became scene-expressible as a side effect**, closing ADR 0027's recorded gap.

  Purely additive, so every scene file valid before is valid after.

  `Option::None` was left unsolved on purpose. `none` collides with an enum variant of that name; a
  sigil would be this format's first punctuation, having chosen indentation over punctuation
  throughout; and omitting the field destroys ADR 0014's distinction between "explicitly nothing" and
  "whoever wrote this forgot". Nothing has an `Option` field, so it waits for a real case.

  **`Projection` was un-flattened immediately** — `Orthographic { height }` and
  `Perspective { fov, near, far }`, each carrying only what it needs, with `Projection::height()`
  returning `None` for a perspective camera rather than a fallback. Done now rather than later
  because the replays had just been regenerated, so it was the cheapest moment it will ever be.

  **Three things fell out of doing it, all found by use rather than reasoning:**

  The derive was **silently dropping `min`, `max` and `unit` on enum variant fields** — so a field
  lost its declared range simply by moving into a variant, which is precisely what this ADR
  encourages. The struct and variant paths now share one function.

  `amadeo-snapshot` **could not write a payload enum**: it came out as `Orthographic({height: 8})`,
  Rust's `Debug`, which nothing reads back. That is the *second* time that exact defect has been
  found in that crate — the first was maps, earlier this session — and both times by snapshotting a
  real game and reading the file. It now has a test that builds a world holding every awkward shape
  and asserts the restored state hash matches.

  And **quad-demo had been drawing nothing since the previous commit.** ADR 0031's camera went into
  `scenes/markers.scene`, and quad-demo *does not load its scene file* — it never has. Nothing caught
  it: its replay still passed, because a camera the world never had cannot move the state hash, and
  quad-demo has no `render.describe` test the way the Vault does. It now spawns its camera in code
  beside everything else, and has two tests — one that it has a camera, one that something is
  actually on screen.

  Both replays regenerated again, and the vault's cause was proven by snapshot diff first: the only
  difference between the two worlds was the camera's four flat fields collapsing into the
  projection's payload. Nothing else moved.

  853 tests, all four verification commands green.

  **Then Q19, which prefabs had opened.** `amadeo import` writes the `.ama-meta` sidecar an asset
  needs before a game will start — and it learned the asset directory by *launching the game*, which
  refuses to start while a sidecar is missing. The tool that fixes the problem could not run.

  `amadeo import --assets <dir>` names the directory instead. Asking the game stays the default,
  because the path is a constant in the game's own source and so nothing can disagree with it; the
  flag is the escape hatch for exactly the case where the game will not start.

  **The first attempt was wrong and worth recording.** It put `assets = "..."` in `amadeo.toml`, and
  a manifest is per-*project* while an asset directory is per-*game* — in this repo, with two games,
  the key could only describe one. The Vault, the case that motivated the question, runs under
  `--package vault` and would have fallen straight back to launching the game. Caught by asking
  whether the fix reached the motivating case, which it did not.

  **Verified by reproducing the deadlock**: deleted both prefab sidecars, watched
  `amadeo import --package vault` fail exactly as before, then `amadeo import --assets
  games/vault/assets` wrote them with nothing launched — **byte-identical** to the hand-written ones.

  855 tests, all four verification commands green.

  **Then Q22, which turned out not to be a question.** A resource's identity in the state hash was
  `std::any::type_name` — the Rust path — where a component's is its canonical name, so moving a
  resource between crates silently invalidated every golden replay.

  **ADR 0017 had already decided it and deferred only the timing:** *"resources get this treatment
  when `Resource: Reflect` lands"*. ADR 0027 landed that bound earlier the same session, so the
  trigger had fired and been missed. Worth noticing as a *class* of mistake rather than a one-off —
  a deferred obligation inside an accepted ADR has nothing watching it, and only surfaces when
  something trips over the inconsistency. ADR 0017 even argued the timing: it rejected deferring
  because "the cost of this decision grows with every recorded replay".

  Services keep the Rust path, permanently, for the reason that ADR gave: not reflected, not in any
  hash, named by no file.

  **Three replays regenerated** — including `walk_and_jump.replay`, the in-process one, which had not
  moved all session. The signature was exactly what ADR 0017 recorded for an identity change: input
  streams byte-identical, only checkpoint lines moved. Confirmed independently by snapshot diff, the
  world before and after being byte-identical apart from the `state-hash` line.

  857 tests, all four verification commands green.

  **Every open question this session raised was closed in it** — Q7, Q19, Q21, Q22 — along with Q3,
  Q10 and M1's gate 4. What is left is build work rather than decisions.

  **Then the GPU path got its first automated coverage, ever.** `STATUS.md` carried "no automated
  coverage at all" as a known gap through three milestones: `render.describe` checks what *should* be
  drawn, computed from the world, and nothing checked what the GPU actually produced. Every claim
  about the wgpu backend rested on somebody opening a window and looking.

  `WgpuBackend::offscreen(width, height)` renders into a texture it owns rather than a window's
  swapchain, and `RenderBackend::capture` reads it back. **The two backends differ in where the frame
  lands and in nothing else** — same shaders, same pipelines, same passes — which is what makes a
  captured image evidence about the renderer that ships rather than about a second one written to be
  testable. It is also the path agent mode needs, since ADR 0016 launches a game with no window.

  Four tests: the clear colour is the dark non-black it is supposed to be, a red `Quad` reaches the
  middle pixels *and does not fill the corners*, two cameras over one world produce different images,
  and a backend that cannot capture says so while naming what answers the same question instead.
  That third one is the interesting one — it catches a projection wired up wrongly, which
  `render.describe` structurally cannot, because `describe` *computes* the same projection rather
  than observing it.

  They skip and pass on a machine with no adapter, which is honest rather than convenient: a missing
  GPU is a fact about the machine. CI runs them as their own step, since `cargo test --workspace`
  does not enable the `gpu` feature.

  861 tests plus the 4 GPU ones, all four verification commands green.

  **Then `render.capture` over the protocol — the agent has eyes.** `amadeo capture shot.png`
  launches a game headless, opens an offscreen GPU, renders the world, encodes a PNG and writes it.
  Run against the Vault it produces the arena: walls, six sigils, two wardens, two traps, the player,
  and the score readout — and captured at two different ticks the wardens have moved along their
  patrol routes, so it is live simulation state rather than a static picture.

  **Justin chose PNG** over the PPM this engine already reads. The deciding argument is that the
  point of a capture is that a *human opens it*, and nothing opens a PPM — not a file browser, not a
  chat client, not a pull request. The `png` crate was already a dependency for decoding, so encoding
  cost no new one, and the same reasoning that kept DEFLATE out of the hand-rolled column applies
  identically to writing it.

  **The image goes to a file rather than into the reply**, and the *game* writes it: a screenshot is
  hundreds of kilobytes, and base64 in a JSON-RPC line would make a transcript unreadable for no
  gain. The reply carries the path, the size, the tick, and the number of drawable entities — that
  last one because "the file is small and the world is empty" and "the file is small and something is
  wrong" look identical otherwise.

  Capture creates an offscreen device, uses it, and drops it. That costs a device creation per call,
  which is the right trade for an introspection method nobody calls in a loop — the alternative is
  holding a GPU open for every headless run, including the thousands that never capture anything. It
  is behind a `gpu` feature on `amadeo-app`, off by default, so a dedicated server does not link a
  graphics stack it will never use (I7). Without it the refusal names `render.describe`.

  CI now runs both halves: the unit tests, and the whole path end to end through a real game binary.

  864 tests plus the 4 GPU ones, all four verification commands green.

  **Then ADR 0033, the material and shader model — decided before its code, like ADR 0031.** And
  `docs/04` §4 had the emphasis wrong for the **third** time: it asks about shaders, which
  `RenderBackend` isolates completely, while the hard-to-reverse decision was where a material's
  *data* lives. That is now a pattern worth naming rather than three coincidences.

  **A material is an asset with an id**, Justin's call, on three arguments. It is shared by
  construction — the Vault's forty-four walls use one, so inline data would be forty-four copies in
  every state hash and every snapshot. ADR 0023's batching rule extends to `(sort order, material)`
  and comparing an id is a string compare where comparing a struct is a deep one, on the path the
  batcher exists to keep cheap. And the whole ADR 0020/0029 toolchain — validation, "did you mean",
  the load barrier, `amadeo assets`, `amadeo import` — applies for nothing.

  Its file *is* a scene file with a single root, exactly as a prefab is, so the parser, the canonical
  writer, `amadeo fmt` and ADR 0032's nested values all work on it the day it exists.

  **This was blocked until earlier the same session.** The inline alternative was unrepresentable
  before ADR 0032 gave the format nested values, and deciding against a format that could not hold
  the alternative would have prejudged the answer.

  Shaders: hand-written WGSL with `#include`, `#ifdef` and a pipeline cache keyed by the defines —
  Bevy's shape, reached after they hit the variant problem for real. **No material graph**: that is
  an editor-sized project before the first triangle, and if ever wanted it is additive, since a graph
  emits WGSL rather than replacing it. Decided alone and flagged, since `RenderBackend` isolates it.

  What a `Material` *holds* is deliberately not decided — that depends on the PBR model and arrives
  with meshes, because adding a field to a reflected type is the cheap change the schema exists for.
- **S9 (2026-08-04):** **The render graph — decided, then built**, which is what this file had named
  the single most important thing to do next.

  **The framing was wrong for the fourth time in this subsystem, and this time it was the
  vocabulary.** ADR 0018, 0031 and 0033 each found `docs/04` §4 asking about the pipeline while the
  expensive decision was the data beside it. Here the trouble was that "render graph" names two
  independent things — a frame scheduler that derives pass order and allocates transients, and an
  extension point where a game inserts a pass. The roadmap line asks for the first and the worry
  recorded here was about the second. Separating them is what made the question answerable, and it
  also revealed that **most of the first is already done**: wgpu tracks resource state and inserts
  barriers itself, which was half of what Frostbite's FrameGraph existed for.

  **The requirement also does not say what it looks like it says.** "Configurable post-process stack"
  can mean tunable or extensible, and `docs/00-vision.md` asks only that the renderer not bake in a
  look. Godot, Unity and Unreal all ship the tunable stack as the primary answer and put the
  extension point behind an advanced, later, harder door — Godot's `CompositorEffect` arrived in 4.3
  and its own docs call it an advanced feature working on only two of three renderers.

  **Bevy is the one engine that made its graph public, and it is evidence against.** It walked back
  from resource dependencies — graph slots removed as boilerplate-heavy, data moved into ECS
  components with the graph doing ordering only — and making it public turned it into a permanent
  migration surface, rewritten as render-graph-as-systems as recently as 0.19.

  **Justin took both recommendations.** The graph is internal; a look is an `Environment` asset held
  by the camera, its file a scene file with one root exactly as a material's is. The deciding
  argument for data over code was **I5 and I7** rather than anything about rendering — configuration
  made of data is authorable, describable, checkable and visible headless for nothing, and a pass
  supplied as code is none of those. Same shape as ADR 0030. Recorded honestly in the ADR: 0033's
  *decisive* argument does not apply here, since a world has one to three cameras rather than
  forty-four walls, so the asset form rests on a look being the thing that gets tuned and swapped.

  Then built it. The graph is a plan that knows nothing about wgpu, so `NullBackend` compiles it too
  and reports the resolved pass order — a pass-ordering bug is now catchable on a machine with no
  GPU, which would have been impossible had the graph lived inside the wgpu backend. Ordering is
  write-before-read, then declaration order between two writers of one image, then declaration order
  for anything unordered — deliberately the opposite of `Schedule`'s alphabetical tie-break, because
  a schedule's registration order is accidental while a graph's is the order the frame is composed
  in.

  **Two findings, both from writing the tests rather than the code.**

  The `scene` transient is always RGBA and deliberately does **not** inherit the destination's
  format. A window surface is commonly BGRA — the adapter picks, not the engine — so a transient that
  copied it would hold the finished picture with red and blue swapped on the windowed path and not
  the offscreen one, and every capture would have to know which.

  And **the first version of `capture` was wrong**, caught only because the new orientation test was
  checked against a deliberately broken shader and *passed*. It read the transient on both paths,
  which meant a capture no longer observed the present pass at all — so the one shader that now runs
  on every frame had no coverage, and the screen could have been upside down with every test green.
  That is the exact gap session 8 closed and it had been quietly reopened. Fixed by having an
  offscreen backend read its **destination**, after the present pass, so the path CI and agent mode
  both use covers the whole pipeline; a windowed one reads the transient and is everything except the
  final copy. Broke the shader again afterwards to confirm the test fails. **Worth generalising: a
  new test is not evidence until it has been seen to fail.**

  **And the windowed backend can capture**, which this file had listed as waiting on post-processing.
  It was never really waiting on the effects — only on the off-screen target they need, which the
  graph brought with it.

  Verified end to end: the Vault captures through the new graph with walls, sigils, wardens, traps,
  player and score readout, right way up and unchanged in colour.

  879 tests plus 5 GPU capture tests, all four verification commands green.

  **Then post-processing, which is what the graph was built for.** An `Environment` asset the camera
  names by id, holding exposure, tonemap, grade and vignette; the cameras draw into an **HDR** target
  and a post pass brings it down, because on an 8-bit target bloom has nothing above the display
  range to isolate and tonemapping has nothing to compress. `TargetFormat` gained `Hdr16` and it cost
  a match arm — ADR 0026's format-tag argument coming true a second time.

  **The dependency direction decided where the loading lives, not preference.** An environment's file
  is a *scene* file and `amadeo-scene` sits **above** `amadeo-render`, so by I6 the renderer cannot
  parse its own asset. It owns the type and the cache; `App::load_environments` reads. The same split
  `TextureCache` already had, arrived at the same way.

  **A real defect in the state hash, found by accident and worth the detour.** Adding `environment`
  to `Camera` should have moved every golden replay and moved *none* of them. `StableHash for str`
  wrote the bytes with **no length prefix**, so an empty string contributed nothing — and worse, two
  adjacent string fields hashed as their concatenation. `Camera { target: "", environment: "x" }` and
  `Camera { target: "x", environment: "" }` are different worlds that hashed **identically**.
  Reachable from content, in shipped code, in the one mechanism the whole determinism story rests on.

  The fix goes in the `StableHash` impl rather than in `write_str` — which is exactly where the `[T]`
  impl writes its own length, and which leaves the name-hashing path and every `ComponentId` alone.
  **Diagnosed rather than obeyed**, and the control was already in hand: with the `Camera` field
  added and *before* the fix, both replays matched exactly, so all of the movement is the fix.
  `walk_and_jump.replay`, whose world holds no string fields, did not move at all — which is what a
  string-hashing change predicts and nothing else does. Both regenerated files changed four lines
  each, the checkpoints, with input streams byte-identical: ADR 0017's recorded signature for an
  identity change.

  **Two findings worth keeping beyond the fix.** *Adding a field to a reflected component breaks
  every existing scene file that authors it* — `vault.scene` had to gain `environment ""` by hand,
  because the format is strict about missing fields by design (ADR 0014's "explicitly nothing" versus
  "whoever wrote this forgot"). That cost is small now and will recur with every `Material` field;
  worth knowing before it is a surprise. And *a test is not evidence until it has been seen to fail* —
  the same lesson as the capture bug earlier in the session, learned twice in one day.

  **The Vault ships `corridor_dark.environment` and deliberately does not use it.** Its appearance is
  what M1's exit gate was judged against, and pointing the camera at a look would move
  `collect-three.replay` for a cosmetic reason — a content decision that is Justin's rather than
  something to slip in. It is a worked example and the fixture for `a_look_is_a_file.rs`, which
  drives the whole chain against real files: sidecar, catalogue, load barrier, parser, reflection,
  cache.

  Recorded as **Q23**: one environment per frame, from the camera that draws first. ADR 0031 has
  every camera compose into one image, so per-camera post needs per-camera targets — the same work as
  `Camera::target`, so the two belong together.

  900 tests plus 7 GPU capture tests, all four verification commands green, both golden replays
  regenerated and passing in separate processes.
