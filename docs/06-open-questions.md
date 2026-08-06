# 06 — Open Questions

Check this before assuming any undecided thing. Resolve with an ADR, then move the entry to the
resolved section at the bottom.

Priority: **P0** blocks work now · **P1** needed for the current milestone · **P2** can wait.

---

## ~~Q3~~ · **Resolved — ADR 0018, 0023, 0031.** Two passes in one graph, and the camera is an entity

Split into three across three sessions, which is what made it tractable. **ADR 0018** settled the two
expensive parts, both *data*: one 3D `Transform` with 2D as its degenerate case, and an explicit
`SortOrder` dominating depth. **ADR 0023** settled the batching rule against a measurement. **ADR
0031** closed the last third in session 8 — the sprite pass and the mesh pass are separate pipelines
sharing one render graph, neither built on the other.

**The last third turned out not to be the expensive part, for the second time.** Q3's framing
emphasised the pipeline; ADR 0018 pointed out the expensive decisions were the data around it, and
the same held again — the pipeline was a consequence, and the genuinely hard-to-reverse decision
hiding inside it was **the camera model**, which nothing had framed as a question. That is the half
Justin decided: a camera is an entity, not a resource.

Worth remembering as a pattern. "Which pipeline" is almost always the cheap question, because
`RenderBackend` isolates it and no file or hash can observe it. Ask what *data* the choice implies.

---

## ~~Q17~~ · **Resolved — ADR 0025.** Queries are tuples of terms, and a term may be optional

Justin chose the trait-plus-macro approach over hand-writing each shape or a lower-level accessor.
`world.query::<(&Transform, &Sprite, Option<&SortOrder>, Option<&GlobalTransform>)>()` now resolves
each column once per archetype. Sprite collection at 20,000 sprites went 3.32 ms → 2.58 ms, and
5.13 ms → 2.58 ms across ADR 0024 and 0025 together. Kept read-only: generic mutable queries would
need `unsafe`, which this crate forbids, and the measured problem was all on the read side.

The original text is below for the reasoning that led there.

<details>
<summary>Q17 as filed</summary>

### The ECS cannot express an optional component in a query

**Found in session 7**, as the successor to Q16 — fixing that one removed the id cost and left this
as the dominant remaining expense.

Archetype ECSs are fast for one structural reason: **a query matches whole archetypes, not individual
entities.** The column is located once per archetype and then iterated contiguously, which is why
Flecs describes archetype matching as the main source of query speed and why Bevy caches that
matching in `QueryState` — "gathering archetypes is a costly operation, so it is cached".

Amadeo's queries do this for their *required* components. They cannot do it for optional ones,
because there is no way to say "and this component if it is present".

**The consequence, measured.** The sprite batcher needs `SortOrder` and `GlobalTransform`, and both
are deliberately optional — an entity without them still draws, at order zero and at its local
transform. So it falls back to `world.get::<T>(entity)` per entity, which is exactly the per-entity
lookup archetype storage exists to avoid. At 20,000 sprites that is 40,000 individual lookups, each
one a location fetch plus a binary search plus a downcast, and after ADR 0024 it is what the
remaining ~3.3 ms is spent on.

The same shape appears in `render_quads`, and will appear in every system that wants to treat a
component as optional — which is most of them, since requiring a component means an entity silently
disappearing from a query when someone forgets to add it.

### What to build

An `Option<&T>` query term: resolve the column once per archetype, yield `Some(&value)` for
archetypes that have it and `None` for those that do not. No per-entity lookup at all.

Sub-questions worth settling first:

- **How far does it generalise?** Query shapes currently stop at three components (`iter_triple`) and
  each new shape is a hand-written method. Adding optional terms multiplies that combinatorially, so
  this may be the moment the query API needs a real abstraction rather than another method — which
  is a bigger design question, and one where `CLAUDE.md`'s "boring Rust over clever Rust" pulls hard
  against the generic machinery Bevy uses for it.
- **Does it change iteration order?** It must not: draw order and state hashes both depend on
  iteration being reproducible (I3).
- **Is the archetype match cached, as Bevy does, or recomputed per call?** Caching needs invalidation
  when a new archetype appears, which is a correctness risk to weigh against the saving.

### Why this matters now

The target list grew to eight games in session 7, and Stellaris, Terraria, RimWorld, and Project
Zomboid are all large-entity-count simulations. ECS throughput stopped being an aesthetic concern and
became a target requirement (`docs/00-vision.md` § Divergent).

**Worth doing before the sprite batcher reaches the GPU**, and worth doing on its own merits.

</details>

---

## Q25 · P1 · Level of detail across chunks at different resolutions

**New in session 11, and deliberately deferred by ADR 0042 rather than overlooked.**

Surface nets avoids marching cubes' seam problem *between chunks at the same resolution*: a chunk
that samples one cell into its neighbour already agrees with it about where the surface is. **Chunks
at different resolutions still crack**, because a coarse chunk's samples do not line up with a fine
one's.

This has to be answered before terrain covers real distances, because every open-world target needs
distant chunks to be cheaper than near ones.

### Two of the four options in the original version were not real — corrected in session 12

The research done for ADR 0043 removed two candidates this question used to list, so they are struck
here rather than left to be re-investigated:

- ~~**Transition cells**, "the surface-nets equivalent of TransVoxel"~~. **Transvoxel does not
  apply.** It is for marching cubes, a *primal* method; surface nets is *dual*. There is no
  equivalent to port.
- ~~**Seam meshes** (Nick Gildea's approach for dual contouring)~~. **Needs an adaptive octree with
  variable-size leaf nodes.** `amadeo-voxel` is a uniform grid, so this would be a rewrite of the
  mesher rather than an addition to it.

What is left, and every one of them would be derived here rather than ported:

- **Skirts.** Each chunk grows a downward-facing lip at its border, hiding the crack rather than
  fixing it. Cheapest by a long way, universally used, and visibly wrong at glancing angles — and one
  reported failure mode is worth knowing about: on flat ground the skirt can overlap geometry the
  mesher already produced there.
- **Derived transition geometry** — a chunk is told its neighbours' resolutions and generates matching
  boundary cells. Correct, and it makes a chunk's mesh depend on its neighbours' *choices*, not just
  their samples, so one chunk changing level dirties up to six others.
- **Sample the coarse level everywhere near a boundary**, so both sides agree. Simple and correct, and
  it gives up the saving exactly where chunk counts are highest.
- **Clipmaps** — concentric rings at fixed resolutions centred on the camera. Very well understood for
  heightfields, much less so for a full 3D field. Note that ADR 0043 already adopted concentric
  *integer boxes* for residency, which is the same idea one level up.

### It is now better posed, and deliberately still open

**ADR 0043 pinned colliders to the finest detail level**, which changes this question's character
entirely: a collider never changes resolution, so **the seam is purely a rendering problem** and sits
outside the state hash. It was previously entangled with determinism and no longer is.

So the question is no longer "which of four options". It is: **may a chunk's mesh depend on its
neighbours' resolutions?** Everything else follows from that answer.

Still not decided, for the reason this question always gave: the honest comparison needs a running
streaming system to look at, and ADR 0043 built the residency and meshing layers rather than the whole
pipeline. `ChunkKey` carries `lod` and `ChunkShape` does the arithmetic, so whichever answer wins is
an addition rather than a change to the key type everything is built on.

Nothing is blocked: terrain at one resolution works today.

---

## Q26 · P2 · `render.describe` cannot see meshes

**Found in session 10 by reaching for it and getting nothing.** Asked what `games/atrium` was
drawing, it reported a default orthographic camera and zero entities — because it only knows the 2D
path: quads and sprites.

That made it useless for the one debugging job it was actually reached for, and the workaround was
writing a throwaway test that printed `FrameData` by hand.

**It matters beyond that afternoon.** `render.describe` is the agent's main way of seeing a frame
*without* a GPU, and `docs/03` makes "what is on screen" a pillar. An editor will need it in M4, and
by then 3D is most of what there is to see.

The shape is probably obvious — a `DrawnKind::Mesh` alongside the existing kinds, carrying the mesh
id, the resolved material and the model matrix. What needs thought is what "on screen" means for a
perspective camera, since the existing `visible` / `off_screen` split is computed against a 2D
viewport rectangle.

---

## Q27 · P2 · A third-person camera clips through walls

**Found in session 10 by `games/atrium`**, whose follow camera is a child entity at a fixed offset.
Backing the character into a wall pushes the camera outside the room, and the world turns inside out.

This is a solved problem everywhere — a spring arm that shortens the offset when something is in the
way — and **`PhysicsBackend::move_shape` is already the right tool**: ADR 0037 explicitly names "a
camera that must not clip through a wall" as something the query describes.

The decision is not *how*, it is **where**. A camera rig is not geometry and is not gameplay; it sits
in the same awkward place `CharacterController` did. `docs/00-vision.md` says the camera rig must be
separate from the character controller because the targets are a mix of first- and third-person, so
this is probably a second module — `modules/amadeo-camera` — rather than anything in `crates/`.

Worth deciding when a game needs it. The Atrium tolerates it because you can see the whole room.

---

## Q28 · P2 · Ambient light is a hardcoded constant, and shadows made that visible

**Found in session 10 the moment shadows landed.** Before shadow maps the only ambient-only pixels
were faces turned away from the light — small, and fine as near-black. With shadows, whole areas of
*floor* are ambient-only, and at 0.03 they came out as holes in the world rather than as shade.

Raised to 0.12, which is a stopgap and is marked as one in `mesh.wgsl`. **The real answer is an
authored sky colour on `Environment`**, because ambient is standing in for the sky being a light —
and `Environment` is already the place a game says what its look is (ADR 0034).

Two things to decide with it: whether ambient is a flat colour or a simple sky/ground gradient (the
cheap version of image-based lighting, and the one that makes an outdoor scene stop looking flat);
and whether it belongs to the `Environment` asset or to a light entity.

Also found alongside it, and worth remembering: **`Grade::contrast` above 1.0 crushes shadows to pure
black**, because the operation is `(colour - 0.5) * contrast + 0.5` and near-black values go negative
and clamp. Inherent to a pivot with no toe. Nothing in a scene had ever been that dark before shadows
existed.

---

## Q15 · P1 · Modding, and whether ADR 0011 still holds

**Raised session 7, when the target list went from three games to eight.** Four of the five
additions — Minecraft, RimWorld, Terraria, Stellaris — are games substantially *defined* by their
modding ecosystems. That was not true of any of the original three.

**ADR 0011 decided game logic is plain Rust in the game crate**, no scripting layer, no dynamic
reload. It was a good decision and it was made properly: four candidates prototyped and measured
against one benchmark, with the recorded Luau prior refuted on evidence (`spikes/q1-game-logic/`).

**But it answered a different question than this one.** Q1 asked how *the developer* authors game
logic, and the deciding evidence was iteration speed — the feared 30-second rebuild measured at
0.9–3.2 s, so no architectural cost was worth paying for it. A **mod author is not the developer**.
They do not have the source, do not have a Rust toolchain, and cannot rebuild the engine. "Recompile
the game to add a mod" is not a modding story at any speed.

So the trigger ADR 0011 recorded does not cover this. It reserved WASM as a pre-selected escape hatch
behind a measured threshold — *a gameplay rebuild sustaining above 5 s* — which is an
iteration-speed trigger. Modding would open that hatch for an entirely unrelated reason.

**What is genuinely encouraging:** the escape hatch that was reserved happens to be the right tool.
The Q1 spike measured WASM as **bit-identical to native Rust** across two optimisation levels at
1.24× runtime cost, which is precisely what a deterministic engine needs from a mod sandbox — plus
it is sandboxed by construction, which matters far more for third-party code than for first-party.
And it is the same artefact M5's web export needs. So this is likely to be a *confirmation* of a
reserved option rather than a reversal of a decision.

### What to decide, and when

Not now. Nothing built so far is invalidated, and nothing in M1 or M2 depends on the answer. But it
should be settled before the module system hardens (M2–M3), because "what can a mod do" is really
"what is the module boundary", and retrofitting a sandbox boundary is far worse than designing to one.

Specific sub-questions when it is time:

- **Do mods get code, or only data?** RimWorld and Stellaris are heavily data-modded (XML-ish
  definitions) with code as the escalation. Amadeo's reflection registry plus the `.scene` and
  sidecar formats already give a strong data-modding story almost for free — that may cover most of
  the ground at very little cost, and it is worth measuring how far it reaches before assuming a VM.
- **If code: WASM, per the reserved hatch?** Re-run `spikes/q1-game-logic/measure.ps1` rather than
  arguing from the old numbers; the engine has grown since.
- **How does a mod stay inside I3?** A mod running simulation logic is simulation logic. Determinism
  is not negotiable, which rules out anything with the `f64`-versus-`f32` divergence that killed Luau.
- **How does a mod register components?** I8 says everything reflectable; a mod adding a component
  has to reach the same registry, and `ComponentId` is a name hash (ADR 0017), so mod-defined names
  need a collision story.

**Do not pre-emptively build a scripting layer for this.** ADR 0011's reasoning against paying a
permanent architectural cost up front still stands; what has changed is that a second, independent
reason to open the hatch now exists, and it should be recorded rather than rediscovered late.

**ADR 0036 adds a constraint worth knowing here:** physics is now bit-deterministic on any IEEE
754-2008 target, **including WASM**. So the reserved hatch has no physics exception — a mod running
in WASM sees the same simulation as native, which removes one of the harder objections to WASM
modding before it was ever raised.

### A second instance arrived in session 9 — ADR 0034

**ADR 0034 drew part of the module boundary, and it drew it closed.** The render graph is internal,
so a game or module cannot add a render pass; it configures a look through reflected data instead.
For a solo project that is the same act — adding an effect means editing `amadeo-render` — but for a
mod author it is a wall, and it is the *first* place this project has said "no" to third-party code
rather than simply not having got to it yet.

**That makes the question sharper rather than more urgent.** ADR 0034 reserves a Rust extension trait
as the escape hatch and names its trigger (an effect that genuinely is not parameters on an engine
effect), so nothing is foreclosed. But it is now the second subsystem where "what can a mod do" has a
concrete, already-decided answer, and the answers should agree with each other rather than be reached
one at a time. Worth reading ADR 0034 §5 alongside this question when it is time to settle it.

---

## Q18 · P2 · A reflected `ActionId` is a number nobody can read

New in session 8, created by ADR 0027 rather than found by it — the gap only became visible once
`world.resources` existed and could be pointed at a running game.

`InputState` is two maps keyed by `ActionId`, and an `ActionId` is the FNV-1a hash of an action's
name **with the name not kept** — deliberately, since that is what makes it fixed-size, `Copy`, and
cheap enough to look up every tick. So the protocol reports:

```json
"InputState": { "axes": { "8831028638596390904": { "previous": 0.0, "value": 0.0 } } }
```

Faithful, and useless. Pillar 3 is "what did I just do?", and this is the one resource whose whole
purpose is answering that.

**The names do exist.** The input driver holds a table of them — that is how a `.replay` file writes
`axis move_x 1.0` rather than a number. They sit outside `InputState` on purpose: a resource is part
of `state_hash`, and two runs that registered different *names* for the same actions must not
diverge.

### What to decide

Where the join happens, and how general it is:

- **At the presentation layer**, in `world.resources`: look up the driver's table when rendering. The
  narrow fix. Costs a crate edge from `amadeo-agent` to `amadeo-input`, which couples a generic
  protocol layer to one specific subsystem — the part worth thinking about.
- **A general "key alias" mechanism** in reflection, so any hash-derived key can carry a display
  form. More honest about the fact that this will recur — a chunk coordinate, a texture id, an
  entity relation could all want it — and more machinery.
- **Leave it.** The raw id is sufficient for a machine that also has `describe`, and an agent could
  hash the names itself to match them up.

Recommendation is deliberately not stated yet; this needs the second real instance before the general
shape is knowable. Nothing is blocked — `amadeo describe` and the `.replay` format both read fine.

**Session 10 added the first real consumer, and it went the other way than expected.**
`modules/amadeo-character` reads four named actions (`move_forward`, `move_right`, `turn`, `jump`) and
publishes them as `pub const` strings rather than putting `ActionId`s on the `CharacterController`
component. That was the right call for the module — a scene file carrying unreadable integers would
be worse than one carrying none — but it means the question is now **more** likely to bite: a game
inspecting a running character sees four opaque numbers in `InputState` and has to know the names to
match them up. Still not blocking. Worth revisiting when the second module wants input.

---

## Q23 · P1 · One environment per frame, when a world may hold several cameras

**New in session 9**, created by building ADR 0034's post-processing rather than found by reasoning
about it.

ADR 0034 puts the look on the **camera** — `environment "corridor_dark"` — which is what Godot,
Unity and Unreal all do, and what makes a render-to-texture security monitor showing night vision the
same mechanism as everything else.

**But ADR 0031 has every camera compose into *one* image.** A HUD camera loads what the world camera
left rather than clearing, which is what makes composition work without extra machinery. By the time
the post pass runs there is one picture and the cameras are no longer separable, so there is nothing
to apply two different looks to.

The current rule is therefore: **the post pass uses the environment of the camera that draws first**,
which is the same "which camera when there are several" rule ADR 0031 gave `render.describe`.
`FrameData::look` is where it happens, and every `View` still carries its own camera's environment —
the information is resolved and then deliberately not used, so nothing has to be re-plumbed later.

### What this actually costs today

A HUD camera cannot have a different grade from the world beneath it. That is the whole of it, and
nothing in either game wants it. `the_frames_look_comes_from_the_camera_that_draws_first` in
`crates/amadeo-render/tests/environment.rs` pins the behaviour so it is a known state rather than a
surprise.

### Why it should be decided with `Camera::target` rather than on its own

Per-camera post-processing means each camera drawing into **its own** transient, being post-processed
separately, and the results being composited. That is the same machinery `Camera::target` needs —
ADR 0031 shipped the field and nothing implements it yet, so a camera cannot actually render to a
texture. Both are "a camera owns its own image" and solving them twice would be building the same
thing twice.

Sub-questions when it is time:

- **Does compositing replace ADR 0031's load-rather-than-clear rule, or sit beside it?** The current
  rule is cheap and correct for a HUD; per-camera targets are correct for a minimap. They may both
  need to exist, selected by whether the camera names a target.
- **What does `render.describe` say** once cameras genuinely have separate images?
- **Does the frame get one graph or one per camera?** The graph already runs a pass per camera; this
  would make it a *subgraph* per camera, which is where Bevy landed.

Nothing is blocked. Decide before M2's exit gate, since gate 1 wants a 3D scene and gate 2 wants the
M1 2D scene still rendering unchanged — neither needs two looks, but render-to-texture is on M2's
list and that is the trigger.

---

## ~~Q24~~ · **Resolved — ADR 0036.** Physics is deterministic before it is fast

**Justin chose determinism, permanently.** `enhanced-determinism` is on, physics is single-threaded
and scalar, physics state is in the state hash, and the rapier version is pinned exactly so that an
upgrade is a deliberate replay regeneration rather than a mystery.

The deciding argument was the *failure mode* of the alternative rather than the merits of either: a
replay that passes on one machine and fails on another does not look like a physics configuration
problem, it looks like a bug in the game, and it is close to unattributable.

Two things the research settled that the question had not anticipated. **A per-game switch is not
available** — Cargo unifies features across a build, so two games in one workspace cannot disagree
about a feature rapier forbids combining. And **excluding physics from the state hash is not really
possible** either: a physics-driven character's position *is* gameplay state, so a replay that
skipped it would prove almost nothing about any game using physics.

The original text is below for the reasoning that led there.

<details>
<summary>Q24 as filed</summary>

### Rapier's determinism costs parallelism, and pins the version

**Raised in session 9**, prompted by Justin asking whether the engine needs a physics engine. It
does, and sooner than the question assumed: `amadeo-physics` is an **M2** item, and two of M2's four
exit gates depend on it — gate 1 wants "a physics-driven character controller you can walk around
with", gate 3 wants "a physics-heavy replay (200+ bodies) reproduces bit-identically across runs and
processes".

Gate 3 is the problem. **Invariant I3 is the keystone of this project**, and physics is the single
largest body of floating-point arithmetic the engine will ever run.

### What rapier actually guarantees

Better than feared, and with a price tag. Rapier offers an **`enhanced-determinism`** feature giving
bit-level cross-platform determinism — serialise the state after N steps on two different machines
with different CPUs and operating systems, and the bytes match. That is exactly what gate 3 asks for.

The conditions are the decision:

- **`enhanced-determinism` cannot be enabled alongside `parallel`, `simd-stable` or `simd-nightly`.**
  Determinism and multi-threaded or vectorised physics are **mutually exclusive**. So the choice is
  bit-identical replays or fast physics, and it cannot be deferred by taking both.
- **The target must comply strictly with IEEE 754-2008.** Fine for desktop and for WASM, which
  matters because WASM is both M5's web export and ADR 0011's reserved modding hatch.
- **Determinism holds for one rapier version.** An upgrade may legitimately change results, which
  invalidates every committed replay that contains physics — the same class of event ADR 0017
  described for an identity change, but triggered by a dependency rather than by us.

### What to decide, before `amadeo-physics` exists

1. **Is `enhanced-determinism` on, permanently?** The prior is **yes**, and strongly: I3 is
   non-negotiable and gate 3 is written against it. Then physics is single-threaded and scalar, and
   the throughput budget for gate 4 has to be set with that known rather than discovered.
2. **What does that do to Q9?** Q9 asks what runs off the simulation thread. This removes the most
   obvious candidate, which makes Q9 easier rather than harder — worth resolving the two together.
3. **How is a rapier upgrade handled?** Pinning an exact version is the minimum. Beyond that, the
   honest options are to treat a physics upgrade like a deliberate replay regeneration (with the
   `docs/07` procedure) or to keep a physics-free replay set that an upgrade cannot move. Probably
   both.
4. **Does the engine-owned trait boundary hide the version?** ADR 0002 already says rapier sits
   behind engine-owned traits. Worth checking that a rapier type never reaches a scene file, a
   snapshot or the state hash — because if one does, the wrapper is not actually a boundary.

### Why this is P0 rather than P2

Not because anything is blocked today, but because **the answer changes what gets built**. A physics
layer written assuming it may parallelise later is a different layer from one that knows it never
will, and `CLAUDE.md`'s trap list puts retrofitting determinism first: it is "the single most
expensive mistake available in this project". Decide before the crate exists, which is the same
rhythm ADRs 0031, 0033, 0034 and 0035 used.

---

## Q12 · P1 · `Service: Send + Sync` excludes every non-`Sync` runtime

Found by the Q1 spike (ADR 0011), which could not put a script VM in the world.

`Service` requires `Send + Sync`, added speculatively so the scheduler could run systems in parallel
later. Neither `mlua::Lua` nor `wasmtime::Store` is `Sync`, so **neither candidate could store its
runtime in the `World` at all** — both had to hide it in an `Rc<RefCell<..>>` captured by the system
closure, where `world.resources` and the rest of the introspection layer cannot see it.

Q1 was resolved in a direction that makes this moot for scripting. It is **not** moot generally:
`kira`'s audio manager, an asset loader holding a file watcher, and a `wgpu` surface are all likely
to hit the same wall in M3.

Options: relax `Service` to `Send` only and gate parallelism on a narrower bound; keep `Sync` and add
a `LocalService` store for main-thread-only machinery; or wrap offenders in a `Mutex` and pay for a
lock the single-threaded simulation does not need.

Prior: **a separate `LocalService` store.** It keeps the parallel-execution promise honest for things
that can keep it, and stays visible to introspection — which is the actual loss today.

Decide when the first real offender lands, which is M3 at the latest. Do not decide speculatively.

**Session 11 update: ADR 0041 changed the argument without resolving the question.** `Send + Sync` on
`Service` was added speculatively so the *scheduler* could run systems in parallel later — and
ADR 0041 decided it never will: system execution stays sequential and ordered, because the whole
simulation tick is 8.3 µs and parallelising it would optimise nothing.

So the original justification for the bound is gone. What replaced it is a better one: a `Service` is
where a background job's results land (ADR 0041), and `JobPool` and `Inbox` both genuinely cross
threads. The bound is now **earned rather than speculative**, which makes the `LocalService` prior
stronger rather than weaker — the things that need to be `Sync` really do, and the things that cannot
be have no reason to pretend.

---

## Q6 · P2 · Editor in-process or separate process?

Separate process is architecturally purer — it *forces* the RPC protocol to be complete, so a gap
becomes an immediate visible bug rather than a slow drift toward editor privilege. Also gives crash
isolation. Costs latency and complexity.

Prior: **separate**, specifically because the discipline it imposes protects invariant I5, which is
the hardest invariant to keep honest.

Decide before M4.

---

## ~~Q7~~ · **Resolved — ADR 0029.** Prefabs are assets; an override reaches only the instance root

**Resolved in session 8**, after `games/vault` ran straight into it — forty-four wall tiles would be
four hundred lines of near-identical scene text.

- **`from` holds an asset id**, superseding ADR 0014's path grammar. A prefab is an asset, so the
  whole asset toolchain applies to it for nothing.
- **An override is a patch on the instance root, and reaches nothing inside.**
- **A dangling override refuses to load**, naming the entity, the component, and the prefab.

**The research is what decided the middle one.** Unity's overrides evaporate with nesting because an
override names something *inside* a prefab and has to track it across every future edit of that
prefab; Godot's editable children can write back to the source scene. Both failures come from
overrides reaching inward — so here they do not, which makes nesting **structurally** safe rather
than merely carefully handled.

Proof: the Vault's scene went from 223 lines to 142, and `collect-three.replay` matched all four
checkpoints **unchanged** — the same world, authored differently. Full reasoning in `docs/adr/0029`,
including what it deliberately does *not* fix (the wall grid, which wants a tilemap rather than
prefabs).

<details><summary>Q7 as filed</summary>

### Prefab override semantics

The hardest problem in the scene subsystem. Instance-level field overrides, nested prefabs, and
propagation of prefab changes to non-overridden fields on instances.

Unity gets this genuinely wrong (hidden override state, confusing propagation). Godot is better but
still surprising. Requirement here: **all override state is visible in the text file.** No hidden
state, ever.

Needs design work in M1, and it's worth studying both engines' failure modes first.

### A smaller question inside it, found in session 7 and needing an answer first

**What does `from` actually hold — a path or an asset id?** Two accepted ADRs disagree:

- **ADR 0014** specifies the grammar as `entity <id> "<name>" from <path>` and its worked example,
  which a test pins byte-for-byte, uses `from prefabs/door_metal`.
- **ADR 0020** decided an asset is named by a declared id rather than a path, and its worked example
  uses `entity a1 "Wall" from wall_concrete`.

These are not reconcilable as written: `prefabs/door_metal` is not a usable asset id, because
`is_usable_id` rejects `/` — an id appears bare in a scene line and a slash would be ambiguous.

The likely answer is that ADR 0020 supersedes 0014 here, since under 0020 a prefab is simply an
asset with a declared id, which makes `from` an ordinary asset reference and unifies the two ideas.
But that has consequences this question owns: a prefab reference would then count toward ADR 0021's
load barrier, and `amadeo check` would validate it against the catalogue.

Nothing is broken today because prefab instancing is refused outright (`PrefabNotSupported`), and
`SceneDocument::required_assets` deliberately covers only the declared `assets` block and says why.
**Decide this before building prefab instancing**, and supersede whichever ADR loses.

</details>

---

## ~~Q19~~ · **Resolved — `amadeo import --assets <dir>`**

**Found in session 8 by hitting it, fixed the same session.** A bootstrapping deadlock created by
ADR 0029 making prefabs assets:

1. a prefab needs a `.ama-meta` sidecar before anything can reference it;
2. `amadeo import` writes sidecars;
3. but `import` launched the game (ADR 0016) to ask where its asset directory is;
4. and the game refuses to start while a prefab its scene names has no sidecar.

So the tool that fixes the problem could not run, and the Vault's two sidecars were written by hand.

**`--assets <dir>` names the directory directly**, which breaks the cycle and makes `import` work on
a project that does not compile at all — the right property for a repair tool. Asking the game stays
the default, because it is authoritative: the path is a constant in the game's own source, so nothing
can disagree with it.

**Verified by reproducing the deadlock**: the two sidecars were deleted, `amadeo import --package
vault` failed exactly as before, `amadeo import --assets games/vault/assets` wrote them without
launching anything, and the files came back **byte-identical** to the hand-written ones.

### Why not a manifest key

The first attempt put `assets = "..."` in `amadeo.toml`, and it was wrong: a manifest is per-*project*
and an asset directory is per-*game*, so in this repo — which has two games — the key could only ever
describe one of them. The Vault, which is the case that motivated the question, runs under
`--package vault` and would have fallen straight back to launching the game. A flag has no such
blind spot, and it avoids growing the hand-written TOML subset toward tables.

### The rest of the question, checked

`amadeo check` and `amadeo assets` launch the game for the same reason and are **not** deadlocked by
it, because neither is the tool that repairs the problem — they report it. `check` in particular gives
the error that sends you to `import`, which now works.

---

## Q8 · P2 · General entity relations, or just parent/child?

Games want many relationships: equipped-by, targeting, owned-by, docked-to. Parent/child covers
transforms only.

Prior: plain components first (`Targeting(Entity)`), revisit if it becomes painful. General relations
are a significant ECS complexity increase and it's not yet clear we need them.

---

## ~~Q9~~ · **Resolved — ADR 0041.** Parallelism is deterministic by construction, or it does not exist

Raised in session 2, resolved in session 11 — and resolved the way it asked to be. It said *"decide
before adding the first background task, not after"*, and Justin asking for multithreaded asset
loading and parallel ECS queries **was** the first background task.

**The research found three specific ways parallelism destroys determinism**, all demonstrated in
shipping engines: Bevy's own `ParallelCommands` documents that command order depends on thread count;
floating-point addition is not associative, so parallel reduction changes results (the same wall
ADR 0036 hit); and whether a job finished by tick N depends on the wall clock, which diverges a
replay even when every computation was correct.

**But ECS is a viable deterministic concurrency model on a precise condition**: each parallel unit
must write only its own entity's components, with no shared accumulator and no cross-entity reads.
That is enforceable by API shape, which is what ADR 0041 does — the unsafe shapes are made
*unspellable* rather than discouraged.

**And one measurement set the priority.** Gate 4 says the whole simulation tick is 8.3 µs. Parallel
*system execution* would optimise something that costs nothing; asset loading and chunk meshing are
where the work is, and neither is a gameplay system.

What was built: `amadeo-jobs` (barrier or `Service`, `Inbox` draining in key order),
`par_for_each_mut` (`Fn + Sync` closure), background asset loading. System execution stays sequential
and ordered.

**Q12 is not moot** — see below. But `Service: Send + Sync` is now an earned bound rather than a
speculative one.

---

## Q11 · P2 · Agent introspection across a client/server split

Once M6 lands, does `world.query` report the server's authoritative state or a client's predicted
state? Both are useful for different debugging tasks, so this may need to be an explicit target
parameter on the RPC rather than a single choice.

Worth deciding before M6 rather than during it. Networked gameplay is the hardest thing in this project
to debug, which makes it the place where good introspection pays off most — and the place where bad
introspection hurts most.

---

## ~~Q10~~ · **Resolved — ADR 0031.** Both simultaneously, and the camera is what selects

Asked whether a project picks 2D or 3D at build time or can freely mix them. **Both, always** — there
is no build-time switch and there never was a good case for one, because the thing that decides what
gets drawn is a *camera*, and a camera is now an entity. A world can hold an orthographic camera
drawing sprites and a perspective camera drawing meshes at the same time, each with its own target.

The question anticipated "2D UI over a 3D world" as the hard case and guessed it might be an
`amadeo-ui` concern rather than a renderer one. That guess was right and ADR 0018 had already made it
moot: UI over the world is a higher `SortOrder`, not a separate hierarchy or a separate projection.

The case it did *not* anticipate is the one that mattered: **isometric**. Project Zomboid is neither
2D nor 3D — an orthographic projection feeding sprite drawing with Y-sorting. A build-time dimension
switch would have had no answer for it. A camera does: it is a projection setting.

---

## ~~Q22~~ · **Resolved — ADR 0017's second instalment**, which had been waiting for ADR 0027

**Found in session 8** while building the control experiment for ADR 0031's replay regeneration — by
writing a stand-in type with the same canonical name and the same fields, and finding it hashed
differently. `ResourceId::of` hashed `std::any::type_name`, the **Rust path**, where `ComponentId`
hashes the *canonical* name. Opposite rules, so **moving a resource between crates silently
invalidated every golden replay** with nothing in the type's own definition having changed.

**It turned out not to be a new question at all.** ADR 0017 had already decided it and deferred only
the timing:

> `ResourceId` and `ServiceId` keep the path. Neither is reflected, so neither has a canonical name
> to use. **Resources get this treatment when `Resource: Reflect` lands**; services never will, since
> they are engine machinery that no file names.

ADR 0027 landed that bound earlier in the same session. The trigger had fired and been missed —
which is worth noticing as a *class* of mistake: a deferred obligation inside an accepted ADR has
nothing watching it, and only surfaces when something trips over the inconsistency.

ADR 0017 also anticipated the cost and argued for exactly this timing. It rejected "defer and change
both together" because **"the cost of this decision grows with every recorded replay"** — so the
right moment to act on the trigger is as soon as it fires.

Services keep the Rust path, permanently and for the reason ADR 0017 gave: a service is not reflected,
is not in any state hash (ADR 0009), and is named by no file.

**Three replays regenerated**, and the signature was exactly the one ADR 0017 recorded for an
identity change: the input streams are byte-identical and only the checkpoint lines moved. Confirmed
independently by snapshot diff — the world before and after is byte-identical apart from the
`state-hash` line, so nothing about *behaviour* moved.

---

## ~~Q21~~ · **Resolved — ADR 0032.** A block of named fields is a struct

**Found in session 8 by probing the format**, resolved the same session. A block of `name value`
lines is a struct; a block of `- ` lines is a list; a bare variant name with a block beneath it is an
enum carrying data. That is YAML's rule, it needs no schema — which matters, because layer 1 has
none — and the grammar already had the slot, since a field with no inline value already opened a
block.

What a component field can be now:

| Shape | |
|---|---|
| scalar (`f32`, `bool`, `string`, ints) | ✅ |
| flat list (`[f32; 3]`, `Vec<f32>`) | ✅ |
| fieldless enum variant (`phase Playing`) | ✅ |
| nested struct, to any depth | ✅ ADR 0032 |
| enum with a payload | ✅ ADR 0032 |
| map | ✅ ADR 0032 — same syntax as a struct, which closes ADR 0027's recorded gap |
| **`Option::None`** | ❌ **still**, deliberately |
| **anything empty** as a field value | ❌ an empty block is a parse error |

**`None` was left unsolved on purpose.** `none` collides with an enum variant of that name; a sigil
would be this format's first punctuation, having chosen indentation over punctuation throughout; and
omitting the field destroys ADR 0014's distinction between "explicitly nothing" and "whoever wrote
this forgot". Nothing in the engine has an `Option` field, so it waits for a real case to argue from.

**`Projection` was un-flattened immediately**, which was the point: `Orthographic { height }` and
`Perspective { fov, near, far }` each carry only what they need, and `Projection::height()` returns
`None` for a perspective camera rather than a fallback number.

Two defects fell out of doing it, both found by use rather than reasoning: the derive was silently
dropping `min`/`max`/`unit` on **enum variant fields**, so a field lost its range simply by moving
into a variant; and `amadeo-snapshot` could not write a payload enum, so a snapshot of any world
holding one would capture and then refuse to restore. Both fixed, both now tested.

---

## Q20 · P2 · Gate 4's stronger test has never been run

**Left over from ADR 0030**, and stated as a caveat in `docs/09-gate-4-describe-is-not-enough.md`
rather than hidden.

The gate-4 experiment was run by an agent that had **already read the engine source in the same
session**. So the five gaps it reported are ones that agent *noticed* while reaching for knowledge
`describe` did not supply — not ones it was actually stopped by. The gaps are structural and can be
verified by reading the output, which is why acting on them was safe. But the question the gate was
really asking is unanswered.

**The honest test:** hand `describe` output to a reader with no prior exposure to this engine and see
what they produce. That is now more interesting than it was, because ADR 0030 changed the answer —
resources are visible, the schema is closed, and `describe.example` shows the spellings.

Cheap to run and worth doing once M2 has added enough that the schema is not trivially small. It
would also be the first real evidence for or against `docs/03-ai-native-design.md`'s central premise,
which has so far been argued rather than measured.

---

## Resolved

| Q | Decision | ADR |
|---|---|---|
| Engine name | Amadeo | — (session 1) |
| Language and graphics stack | Rust + wgpu + winit + glam + rapier + egui | `adr/0002` |
| Editor vs code parity | Text files are the sole source of truth; editor is an RPC client | `adr/0003` |
| Node tree vs ECS | Scene tree for authoring, ECS for runtime; hierarchy persists as components | `adr/0004` |
| Determinism | Hard invariant. Fixed timestep, seeded RNG, ordered iteration | `adr/0005` |
| 2D vs 3D scope | Unified, both from the start | — (session 1) |
| Target platform | Native desktop, Windows first; web export at M5 | `adr/0002` |
| Physics engine | rapier, wrapped behind engine traits | `adr/0002` |
| Writing our own physics | No | `00-vision.md` non-goals |
| Building on Bevy | No — reference material, not a dependency | `adr/0002` |
| Human-legibility of the code | Hard requirement — boring Rust over clever Rust | `CLAUDE.md` §6 |
| Target games | Palworld, Schedule I, Inside the Backrooms — used as a prioritisation signal | `00-vision.md` |
| First game to finish | Single-player first-person atmospheric horror slice, at M3 | `00-vision.md` |
| Multiplayer | No longer a non-goal. Client-server with server authority; hooks reserved M0–M2, netcode at M6 | `adr/0006` |
| **Q5** — fixed timestep rate | 60 Hz logic tick, configurable physics substeps | `adr/0007` |
| ECS storage strategy | Archetype tables, columns as concrete `Vec<T>` behind a safe trait object, downcast once per archetype per query. No `unsafe` | `adr/0008` |
| Where non-simulation globals live | Two stores: `Resource` (hashed) and `Service` (not hashed), enforced by trait bounds | `adr/0009` |
| `ComponentId` derivation | Hash of a *name*, never `TypeId` — `TypeId` is not stable across builds. **Which** name is `adr/0017` | `adr/0008` |
| System ordering tie-break | Alphabetical by label, never registration order | `amadeo-app` schedule docs |
| Spawning from a command buffer | `spawn_with(closure)` rather than a reserved-id handle; new entity not referenceable by other commands in the same batch | `amadeo-ecs` commands docs |
| **Q1** — game logic authoring and hot reload | **Rust, compiled in. No scripting layer.** WASM reserved as the escape hatch behind a measured threshold; snapshots promoted as the real iteration-loop fix | `adr/0011` |
| Reflection shape | Value tree plus two derives, not dynamic field access; metadata vocabulary fixed | `adr/0012` |
| Is reflection optional? | No — `Component: Reflect` makes I8 a compiler-enforced bound | `adr/0013` |
| **Q13** — `ComponentId` derivation | **The canonical name** (`Reflect::type_name`), not the Rust path. Moving a component between crates is free; renaming one is a deliberate, visible change | `adr/0017` |
| Asset load timing vs determinism | **The simulation never observes asset state** — gameplay holds an id and never asks whether it is loaded, and anything gameplay needs is authored rather than derived. Plus a load barrier at scene entry. Makes streaming safe to add later without a redesign | `adr/0021` |
| **Q4** — asset identity | **A declared `id` in the asset's sidecar**, not its path and not a GUID. Defaults to the filename stem on import, so it reads like a path but survives a move. Duplicate ids refused at scan time, naming both files | `adr/0020` |
| **Q3** (two thirds) — transform and sort order | **One 3D `Transform`**, 2D is its degenerate case; rotation is Euler degrees so it stays hand-writable. **`SortOrder`** replaces `Quad::layer` and dominates depth. Pipeline shape still open | `adr/0018` |
| **Q2** — scene file syntax | **A custom, indentation-based, line-oriented format.** Chosen by Justin from four hand-written candidates; TOML is the fallback, *not* KDL | `adr/0014` |
| Where hierarchy components live | `amadeo-transform`, below `amadeo-scene` — render, physics, and anim all need transforms | `adr/0015` |
| **Q14** — where `describe` runs | **The game binary hosts the agent; `amadeo-cli` launches it and speaks JSON-RPC over stdio.** First transport is one-shot batch, not a live session; `App` owns the `ComponentRegistry`; the JSON parser is hand-written | `adr/0016` |
