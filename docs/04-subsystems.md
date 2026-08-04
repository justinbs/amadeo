# 04 — Subsystems: Key Areas to Settle Before Building

Each entry states the subsystem's job, the decisions that must be made before writing it, the
decisions already made, and the milestone it lands in.

**Legend:** ✅ decided · ⚠️ needs a decision · 🔬 needs a spike (measure, don't argue)

The first six sections correspond to the areas in the original brief. The sections after that are
areas the brief didn't list but that shape the architecture just as much — several of them, especially
**Determinism & Time**, **Reflection**, and **Scene Format**, must be settled *before* the ones in the
original list, because the others depend on them.

---

## 1. Core Application Layer — `amadeo-core`, `amadeo-app` · M0

**Job:** process lifecycle, the main loop, plugin/module registration, configuration, logging, IDs and
handles, arena allocation, the error model.

✅ Fixed-timestep loop with decoupled render, spec'd in `01-architecture.md`.
✅ Plugin model: a module registers components, systems, node types, and assets. No privileged access.
✅ Headless is a first-class mode, not a flag bolted on (I7). Null backends for render/audio/input.
✅ Error model: `thiserror` in libraries, no panics in engine crates, context-rich messages.

⚠️ **Module load order and determinism.** Registration order must not affect simulation order —
otherwise determinism depends on module list order. Resolve with explicit system ordering labels
rather than registration sequence.
⚠️ **Config layering.** Engine defaults → project config → user config → CLI overrides. Needs a
defined precedence and a way to dump the effective config (the agent needs to know what's actually
in force).
⚠️ **Threading model.** How many pools, what runs off the sim thread (asset loading, audio mixing,
render submission), and how results re-enter the deterministic zone in a fixed order. This is where
determinism is most often lost; decide it before adding the first background task.

---

## 2. Event System — `amadeo-events` · M0

**Job:** decoupled typed communication between systems, and the semantic activity log.

✅ Typed, double-buffered queues. No immediate-dispatch callbacks in simulation — they make ordering
implicit and allow reentrancy.
✅ Write during tick N, read during tick N+1 by default; same-tick reads allowed only across declared
stage boundaries.
✅ Events are exposed to the agent as a per-tick log (`events.since`). A cheap semantic diff of what
happened, much cheaper than comparing full state.

⚠️ **Retention policy.** How many ticks of event history to keep in memory, and whether that's
configurable per event type. Needed for time-travel debugging and for `events.since` to be useful.
⚠️ **Ordering guarantee within a tick.** Insertion order per type is easy; a total order across types
needs a sequence number. Decide whether the agent-facing log needs total order — it probably does, for
causality reading.
⚠️ **Command buffers vs events.** Structural changes (spawn/despawn/add component) are deferred
commands, not events. Keep the two concepts strictly separate; conflating them is a common source of
ordering bugs.

---

## 3. Entity Component System — `amadeo-ecs` · M0

**Job:** the runtime data model. Everything else is downstream of it.

✅ Archetype-based storage, structure-of-arrays. Chosen for cache behavior *and* for uniformity —
uniform layout with known schemas is what makes automated reasoning about state possible.
✅ Components are plain data. No side-effecting methods, no `Rc`/`RefCell`.
✅ Hierarchy is data — but it lives in **`amadeo-transform`**, not here (ADR 0015). `amadeo-ecs`
defines what a component *is*; it does not own concrete components. Only `Parent` exists so far;
`GlobalTransform` and the propagation system wait on Q3, alongside the 2D renderer.
✅ Structural changes go through deferred command buffers, merged in deterministic order.

⚠️ **Entity ID scheme.** Generational indices are the obvious choice. But the scene format needs
*stable, human-meaningful* IDs that survive reordering and merges (I2) — so there are two ID spaces
(authoring identity vs runtime handle) and a mapping between them. Design both together; retrofitting
stable authoring IDs is painful. **Also design the network identity space here** (ADR 0006) — a third
space, shared across processes. Nearly free while designing the other two, invasive later.
⚠️ **Authority.** An explicit notion of who owns an entity, per ADR 0006, even though it is always
`Local` until M6. Systems written against it from the start stay correct; systems that assume universal
write access all need revisiting.
✅ **Query API ergonomics.** Settled by use rather than by prototyping: reads return iterators
(`iter`, `iter_pair`, `iter_triple`), writes take closures (`for_each_mut`, `for_each_pair_mut`,
`for_each_triple_mut`). Shapes are added when a real system needs one — the three-component version
exists because the Q1 benchmark needed it. Four or more still needs collect-and-write-back.
✅ **Change detection.** Per-component tick stamps, marked on *mutable access* rather than on actual
modification — there is no way to tell whether a caller holding `&mut T` really wrote. Conservative
in the safe direction, which is why read-only query shapes exist alongside the mutable ones.
⚠️ **Relations beyond parent/child.** Godot has one tree; real games want many relationships
(equipped-by, targeting, owned-by). Decide whether to support general entity relations now or model
them as plain components. Leaning: plain components first, revisit if it hurts.
🔬 **Archetype fragmentation.** Many small archetypes destroys iteration performance. Measure with a
realistic component mix before committing to the storage strategy.

---

## 4. Rendering Pipeline — `amadeo-render` · M0 (window) → M1 (2D) → M2 (3D)

**Job:** get pixels on screen for both 2D and 3D from one coherent architecture.

✅ wgpu backend. ✅ **Render graph architecture** (declared passes with resource dependencies) rather
than a hardcoded pipeline — this is what makes unified 2D/3D and later additions tractable. The ✅
here was premature until session 9: it described an intention, while the wgpu backend ran a hardcoded
per-view loop. **Built now, and ADR 0034 decides its visibility: internal.** Declared passes with
reads and writes, order derived from the dependencies, transient targets pooled across frames, one
pass per camera — but its types are not a public extension surface, because that is what keeps
`RenderBackend` the total isolation boundary that made ADR 0018, 0023 and 0031 cheap.

The graph knows nothing about wgpu, so `NullBackend` compiles it too and reports the resolved pass
order — a pass-ordering bug is catchable with no GPU, which is what I7 asks. And because every camera
now draws into a transient that a present pass copies onward, **the windowed backend can capture**,
which `STATUS.md` had listed as waiting on post-processing.
✅ Rendering reads simulation state, never writes it.
✅ Must be fully disableable (null backend) for headless runs.

✅ **How 2D and 3D coexist — ADR 0031.** Option (b): two passes sharing one render graph, one device,
one frame, neither built on the other. The lean toward (a) did not survive contact with ADR 0023 —
depth-buffering sprites was already rejected because transparent sprites erase what is behind them,
so "one pipeline" would have meant a 3D pipeline with depth switched off for sprites, which is two
pipelines with the honesty removed. (c) was rejected for foreclosing a 3D object drawn in front of a
2D layer, which is the arrangement Godot needs a plane-mesh workaround to escape.

**And this section had the emphasis wrong, twice.** Calling the pipeline "the real decision" is what
ADR 0018 corrected once and ADR 0031 corrected again: `RenderBackend` isolates the pipeline so
completely that no file and no hash can observe it. The expensive decision was the **camera model** —
see below.
⚠️ **Sorting and layering.** 2D needs painter's-order layers; 3D needs depth plus separate transparent
sorting. A unified scheme that serves both without surprising either.
✅ **Material and shader model — ADR 0033.** A material is an **asset** named by an id, not inline
component data: it is shared by construction, an id keeps ADR 0023's batching rule cheap, and the
whole asset toolchain applies to it for nothing. Its file *is* a scene file with one root, exactly as
a prefab is. Shaders are hand-written WGSL with `#include`, `#ifdef` and a pipeline cache keyed by the
defines — Bevy's shape. **No material graph**; that is an editor-sized project, and if ever wanted it
is additive, since a graph emits WGSL rather than replacing it.

**And this section asked about the cheap half again** — the third time, after ADR 0018 and ADR 0031.
`RenderBackend` isolates the shader strategy; the expensive decision was where the material's *data*
lives. Worth internalising: in this subsystem, ask what data a rendering choice implies before asking
about the pipeline.
✅ **Camera model — ADR 0031, and this was the expensive decision of the subsystem.** A camera is an
**entity** carrying a `Camera` beside a `Transform`, not a resource. A world holds any number, each
with a projection, a target (the window or a texture), a viewport rectangle, and an order. Position
and orientation come from `Transform`, so parenting a camera to a character *is* a follow camera,
with no special case. Decided now rather than when meshes land because M4's editor needs a camera the
game does not own, and invariant I1 puts it in the world — deferring would have made M4 a migration
across the scene format, the schema, the state hash, and a new GUI at once.

The camera's fields are **flat** — a fieldless `projection` enum beside plain `height`, `fov`, `near`
and `far` — because the scene format cannot express an enum with a payload. That is a worse type than
it should be and it is recorded as **Q21** rather than hidden.
✅ **Post-process and atmosphere model — ADR 0034.** The engine owns the effects; content configures
them. That configuration is an **`Environment` asset** named by an id, its file a scene file with one
root exactly as a material's is (ADR 0033), held by the camera the way a render target already is. It
carries **named effect blocks in an engine-defined order** — `fog`, `bloom`, `tonemap`, `grade` — not
a user-ordered list, because the order is a property of the maths rather than a preference. What an
`Environment` *holds* arrives with the effects, as a material's field list arrives with meshes.

**The deciding argument is I5 and I7 rather than rendering.** Configuration made of data is
authorable, describable, checkable and visible headless for nothing; a pass supplied as code is none
of those. A Rust extension trait is reserved as the escape hatch and named in ADR 0034 §5, triggered
by an effect that genuinely is not parameters on an engine effect. A text format declaring passes is
rejected outright.
✅ **`render.describe` support.** Built, and it earned its place immediately — it caught a layout bug
in the Vault (a score readout overlapping a wall) that no simulation test could have seen. Screen-space
bounds come from the world rather than from the last frame, so it costs nothing when nobody asks and
works with no GPU. ADR 0031 makes it answer for **one camera at a time**, since "what is on screen"
stops having a single answer once a world can hold several.
✅ **Sprite batching throughput** — measured, ADR 0023. 20,000 fully interleaved sprites collapse to
32 batches in 2.58 ms (15.5% of a 60 Hz frame); 50,000 tiles on one sheet are one draw call. The
measurement is re-runnable: `cargo test -p amadeo-render --test sprite_throughput -- --nocapture`.
✅ **Sprites reach the GPU** — ADR 0026. `TextureCache` turns an asset id into pixels;
`WgpuBackend::upload_texture` puts them on the device with one bind group each; the sprite pass binds
once per batch and draws every batch's instances out of one shared buffer.
⚠️ **Texture filtering is one global nearest-neighbour sampler.** Right for the three pixel-art target
games and wrong for a photographic one. The `.ama-meta` sidecar already carries a `filter` setting;
wire it through when the first asset needs linear.
⚠️ **Mip levels.** None are generated, so a sprite drawn much smaller than its texture will shimmer.
Belongs to the import pipeline rather than the runtime — see §5 and ADR 0026.

---

## 5. Asset Management — `amadeo-assets` · M1

**Job:** get data from disk into memory, correctly, asynchronously, and hot-reloadably.

🟡 Import pipeline: source assets (`.png`, `.gltf`, `.wav`) are compiled once into internal formats.
The runtime never parses source formats. **Still the destination, but not yet true — ADR 0026.** The
runtime *does* parse PNG and PPM today, via `amadeo-image`, because the only thing an import step
could emit right now is the same RGBA the decoder produces anyway. What makes the eventual move
cheap is that the runtime already carries an explicit `PixelFormat`, so a compiled BC7 texture is a
new variant and a new producer rather than a redesign of everything downstream. The trigger is the
first target game that wants GPU-compressed textures or mip levels — compression is minutes per
texture and can *only* happen offline, which is why this is a "when", not an "if".
✅ Asset metadata is text and hand-editable (I1) — import settings live next to the asset, in a
`.ama-meta` sidecar or similar.
✅ Handle-based access with async loading. Hot-reload via filesystem watch.

✅ **Load timing cannot affect simulation** — ADR 0021. Two rules: gameplay holds an asset *id* and
never observes an asset's *state*, so there is nothing to branch on; and a scene declares what it
needs, with no tick running until it is resident. Anything gameplay needs (hitbox, collision shape)
is authored, never derived from the loaded file. Streaming is therefore safe to add later without a
redesign.
✅ **Identity: a declared `id` in the sidecar** — ADR 0020. Not the path (a location, so moving a file
would break every reference) and not a GUID (opaque, which is what makes Unity's files unreadable).
Defaults to the filename stem on import, so it reads like a path and survives a move.
⚠️ **Cache invalidation and build determinism.** Same source in, same compiled bytes out — otherwise
asset builds churn and can't be cached or verified.
⚠️ **Dependency graph.** A material references textures; a prefab references meshes. Needs cycle
detection and a defined unload policy.
⚠️ **Missing/failed assets.** Placeholder assets and a structured error rather than a crash — the
agent must be able to keep working and *see* what's broken.

---

## 6. Physics & Math — `amadeo-math` · M0, `amadeo-physics` · M2

**Job:** spatial math, collision, dynamics.

✅ `glam` wrapped by `amadeo-math`, so we own the public surface.
✅ `rapier` for both 2D and 3D, behind engine-owned traits so it's replaceable.
✅ Physics steps inside the deterministic zone at fixed dt.

⚠️ **Determinism configuration.** rapier has a deterministic mode with real constraints (enable
`enhanced-determinism`, fixed iteration counts, ordered body insertion). Verify cross-run
reproducibility with a test *before* building anything on top of it — this is a load-bearing
assumption.
⚠️ **f32 vs f64 vs fixed-point in simulation.** f32 is fastest and standard; strict cross-*platform*
determinism eventually wants fixed-point. Since we're Windows-first and single-platform for now, f32
is fine — but note it in the ADR so the future decision is informed rather than surprising.
⚠️ **How much physics is engine vs module.** Character controllers, one-way platforms, coyote time,
and jump buffering are *genre* concerns (I4) and belong in modules, not in `amadeo-physics`. Draw this
line explicitly; it's where engines usually leak genre logic downward.
⚠️ **2D and 3D in one project.** Whether a project picks one dimension at build time (feature flag) or
can use both simultaneously. Simpler: pick one per project. Decide in M2.

---

# Areas the original brief didn't list

These are not lower priority. Several outrank the list above in ordering.

## 7. Determinism & Time — cross-cutting · M0 · **build this first**

**Job:** the property that everything in `03-ai-native-design.md` depends on.

✅ Fixed timestep, `tick` as the only clock in simulation, seeded RNG as a world resource,
ordered collections in simulation paths, deterministic command merge. `adr/0005`.

⚠️ **Where the seed lives and how it forks.** Systems needing randomness must draw from a
deterministic per-system or per-entity stream, not a shared mutable global — otherwise system
execution order leaks into results.
⚠️ **State hashing.** What's included, what's excluded (render caches, timings), and how stable it is
across builds. This hash is the assertion in every golden replay test, so its definition is critical.
⚠️ **Snapshot format and cost.** Full-world serialization per snapshot is simple but expensive;
delta snapshots are cheap but complex. Start full, measure, optimize if needed.
⚠️ **Fixed dt value.** 60Hz is conventional. 120Hz gives better physics fidelity at 2x cost. Pick and
document; changing it later invalidates every recorded replay.

## 8. Reflection & Schema — `amadeo-reflect` · **built** (M1)

**Job:** the single registry that powers serialization, the editor inspector, and agent introspection.

Everything above depends on it, which is why it's early. See `03-ai-native-design.md` Pillar 2.

✅ **A value tree, not dynamic field access** (ADR 0012). `Reflect` converts to and from a `Value`;
there is no cursor into a live value. All three consumers want a whole tree at once, and the
`dyn Reflect` alternative costs object-safe accessors and a downcast per level for a capability
nothing here needs.
✅ **Derive macros**, in `amadeo-derive`: `#[derive(Reflect)]` and `#[derive(StableHash)]`. The second
matters more than it looks — a hand-written `stable_hash` that forgets a field still compiles and
still produces a plausible number, while silently excluding part of the simulation from every replay
assertion.
✅ **The metadata vocabulary is fixed:** `name`, `version`, `min`/`max`, `unit`, `sync`,
`interpolate`, `skip`. Ranges are advisory and deliberately not enforced on load.
✅ **Replication annotations** (ADR 0006) are in that vocabulary and carried through to
`amadeo describe`. Authority is *not* a field annotation — it belongs to an entity and already exists
as `amadeo_core::Authority`.
✅ **Reflection is not optional** (ADR 0013): `Component: Reflect`, so I8 is a compiler-enforced bound
rather than a convention. `Resource: Reflect` is the outstanding half.
✅ **A version tag per type**, ready for migration.

⚠️ **Migration itself is not built.** The version number is recorded; nothing yet reads an old scene
and upgrades it. Needed before any project has saved data worth keeping.
⚠️ **`Resource: Reflect`** — deferred by ADR 0013. Needs `Rng`'s state exposed so `SimRng` can
reflect, and map support in `Reflect` for `InputState`.

## 9. Scene & Prefab Format — `amadeo-scene` · M1 · **build this third**

**Job:** the shared authoring surface. The literal thing Justin and Claude both write to.

✅ Text, hand-writable, canonical, byte-stable, stable IDs. `adr/0003`, `adr/0004`.

✅ **The concrete syntax is decided: a custom, indentation-based, line-oriented format** (ADR 0014),
chosen by Justin from four hand-written candidates in `spikes/q2-scene-format/`. The spike's main
empirical finding was a negative one — diff behaviour, the criterion everyone expected to decide it,
is identical across all four. What decided it was compactness (roughly half of RON), keeping the tree
visible in the file (which ruled TOML out), and owning the error messages, which Pillar 5 makes a
functional requirement. **If ever revisited, the fallback is TOML, not KDL.**

✅ **Parser and canonical writer exist** with line-numbered errors and a byte-stable round-trip test.
✅ **Layer 2 is built:** `ComponentRegistry` binds parsed values to real component types, narrows
numbers to declared widths, and `instantiate` turns a document into entities atomically.
✅ **Prefab override semantics — ADR 0029.** This was called the hardest problem in the subsystem and
the answer turned out to be a *restriction* rather than a mechanism. An override is a top-level patch
on the instance **root**, and there is no syntax that can name anything inside a prefab. Unity's
overrides evaporate under nesting because they name something inside and then have to track it across
every edit of that prefab; Godot's editable children can write back to the source scene. Both
failures need overrides to reach inward, so here they cannot. Nesting is allowed and cycles are
refused; a dangling override refuses to load rather than reverting quietly. The price is that you
cannot nudge one child of an instance — you make a variant prefab, which is more files.
⚠️ **Merge friendliness.** Two authors editing one scene. Line-oriented, stable-ordered, one-property-
per-line formatting makes git merges tractable. This is a formatting constraint driven by
collaboration, and it's why "whatever the serializer emits" is not acceptable.
⚠️ **Scene composition.** Sub-scene instancing, additive loading, streaming.

## 10. Game Logic Authoring & Hot Reload — **no crate** · resolved in M0

**Job:** how gameplay code gets written and how fast a change becomes visible.

✅ **Resolved by measured spike — ADR 0011.** Game logic is **Rust systems in the game crate**. There
is no `amadeo-script`, no scripting VM, and no dynamic reload. Four candidates were prototyped and
measured in `spikes/q1-game-logic/`; the headline is that a one-line gameplay edit rebuilds in
0.9–2.0 s, so the compile-time crisis this subsystem was invented to solve does not exist at this
scale.

✅ **The escape hatch is chosen in advance.** If a gameplay rebuild sustains above 5 s, or getting
back to the state of interest sustains above 2 s once snapshots exist, the answer is **WASM via
wasmtime** — measured bit-identical to native Rust at 1.24× runtime cost. Re-run
`spikes/q1-game-logic/measure.ps1` before invoking it; the trigger is a number, not a feeling.

⚠️ **The real iteration-loop investment is snapshot/restore**, not reload. Re-simulating to the point
of interest is the only cost that grows with session length (linear, ~21 µs/tick). Promoted to an M1
priority in `05-roadmap.md`.

⚠️ **Luau remains available outside the deterministic zone.** Its `f64` arithmetic does not agree
with `f32` components, which rules it out for simulation — but menus, quest triggers, and dialogue
are not simulation, and its 0.4 ms reload is real. A separate ADR at M3 if it is wanted.

⚠️ **Keeping the crate graph small and shallow is now load-bearing.** ADR 0011 rests on the measured
rebuild times, and those degrade if the graph grows wide or deep.

## 11. Input — `amadeo-input` · M0

**Job:** devices in, deterministic actions out.

✅ Action-based abstraction (`Jump`, `MoveX`), never raw key checks in gameplay. Required for replay
determinism *and* for remapping and controller support.
✅ Input is sampled once per simulation tick from either a live device or a replay stream — the
simulation cannot tell the difference. This is what makes replays work.

⚠️ **Action map format.** Text, reflected, hand-editable, editor-editable. Same requirements as scenes.
⚠️ **Analog, deadzones, and buffering.** Deadzone shaping and input buffering (fighting-game style)
are game-feel concerns — decide what's engine and what's module.
⚠️ **Recording fidelity.** Record actions (compact, robust) rather than raw device events. Actions are
the deterministic boundary.

## 12. Audio — `amadeo-audio` · M3

**Job:** sound. Consistently underestimated, and a huge share of perceived game quality.

⚠️ **Library choice.** `kira` (game-oriented, good mixing/tweening) vs `rodio` (simpler) vs direct
`cpal`. Leaning kira.
⚠️ **Audio must not affect simulation.** Mixing runs on its own thread at its own rate; simulation
fires events, audio consumes them. If simulation ever waits on audio, determinism is gone.
⚠️ Buses, ducking, 2D/3D spatialization, and a null backend for headless (I7).

## 13. Game UI — `amadeo-ui` · M3

**Job:** menus, HUD, dialogue boxes, inventory screens.

**Explicitly not egui.** egui is the *editor's* UI. Game UI must be authored in scene files, themed,
animated, and navigable by controller. Different requirements, different system. Conflating them is a
mistake worth naming twice.

⚠️ **Layout model.** Flexbox-like, constraint-based, or anchor-based. Flex is familiar to both authors
and well-understood.
⚠️ **Retained vs immediate mode.** Retained, to be authorable in scene files and inspectable by the
agent. Immediate-mode UI is invisible to introspection, which breaks the whole observability story.
⚠️ **Focus navigation.** Controller/keyboard focus traversal. Always an afterthought, always painful
to add later.
⚠️ **Text rendering and font handling.** Deceptively large: shaping, atlasing, SDF vs raster.

## 14. Animation — `amadeo-anim` · M3

⚠️ Sprite animation, skeletal animation (glTF skins), blend trees, state machines, and tweens are four
fairly separate systems. Decide which are engine and which are modules.
⚠️ Animation state machines and gameplay state machines want to be the same abstraction. Deciding this
early avoids two parallel half-systems.
⚠️ Must be tick-deterministic — animation driving gameplay (hitboxes on frames) is common and must
reproduce exactly.

## 15. Save/Load & Serialization of Live State — M3

✅ Falls out of determinism: a save is a snapshot, optionally plus the replay log.
⚠️ Save format versioning and migration across engine and game changes.
⚠️ What's excluded from saves (caches, handles, derived state) — needs a reflection-level annotation.

## 16. Editor — `amadeo-editor` · M4

**Job:** make the engine pleasant for a human, without ever becoming the source of truth.

✅ Built as an RPC client of `amadeo-agent` (I5). No privileged access. Structurally enforced by the
crate graph.
✅ Inspector widgets generated from the reflection registry, not hand-written per component.

⚠️ **In-process or separate process?** Separate is architecturally purer (proves the protocol is
sufficient) and better for crash isolation. In-process is simpler and faster. Leaning separate,
precisely because it forces the protocol to be complete — if the editor needs something the protocol
lacks, that's a bug in the protocol, and we find out immediately.
⚠️ **Undo/redo.** Must be expressed as protocol operations so both authors' changes are undoable, and
so undo history never becomes hidden editor state.
⚠️ **Play-in-editor.** Runs the real loop with real determinism; not a special mode with different
semantics.
⚠️ **Gizmos**, multi-select, snapping, and tilemap painting — where a GUI genuinely beats text, and
worth real investment.

## 17. Build, Packaging & Export — M5

⚠️ Asset bundling and compression; a reproducible content build.
⚠️ Windows native export first; web (wasm) export second — cheap because wgpu targets WebGPU, but not
free (threading, filesystem, and audio all differ).
⚠️ Project templates via `amadeo new`.

## 18. Observability & Profiling — M0 (hooks) → M5 (tooling)

✅ Per-system timings exposed via `profile.frame`.
⚠️ **Declared frame budgets per system**, so a regression is an automatic failure rather than a slow
realization. This is how an agent catches performance problems it cannot feel.
⚠️ `tracing` spans throughout, plus a chrome-trace export.
⚠️ Memory tracking per subsystem.

## 19. Documentation as Data — cross-cutting, continuous

⚠️ `amadeo describe` emits the machine-readable API surface (Pillar 2). Should run in CI so it can't
rot.
⚠️ Doc comments are the agent's primary API surface — treat them as load-bearing.
⚠️ A cookbook of worked examples ("how do I make a thing patrol") is disproportionately valuable to an
agent, because a working example beats a signature every time.

---

## 20. Networking — `amadeo-net` · hooks M0–M2, built M6

**Job:** co-op multiplayer. Six of the eight target games require it (`00-vision.md`) — and two of
those, Minecraft and Project Zomboid, are large-world streamed games, so interest management (only
replicating what a client can see) is a requirement rather than an optimisation.

✅ **Client-server with server authority and client prediction** — explicitly not deterministic
lockstep. ADR 0006.
✅ Hooks reserved during M0–M2; no transport code before M6. The cheap-now vs expensive-later split is
tabulated in ADR 0006.
✅ Dedicated server comes almost free from invariant I7 (everything headless-capable).

⚠️ Transport choice (QUIC vs a reliable-UDP library) — M6.
⚠️ Interest management for open worlds — matters at Palworld scale, not for bounded interiors.
⚠️ **How the agent interface layer behaves across a client/server split.** Does `world.query` target the
server's authoritative state or a client's predicted state? Both are legitimately useful. Worth deciding
before M6, since it determines how debuggable networked gameplay is — and networked gameplay is the
hardest thing in this project to debug without good introspection.

---

## 21. Agent Interface Layer — `amadeo-agent`, `amadeo-cli` · M1

**Job:** the engine's only control surface, and the thing invariant I5 makes the editor a client of.
Full design in `03-ai-native-design.md` Pillar 3.

✅ **The read half exists.** `describe` renders the component registry as JSON — names, types, docs,
units, ranges, replication — and `entity`/`query` render a live world. Read-only, so inspecting
cannot perturb what it measures.
✅ **A deterministic JSON writer**, hand-written: sorted object keys so a dump is diffable, and
numbers that stay visibly typed so a float field is not mistaken for an integer one.

🔬 **Q14 — where does `describe` actually run?** **P0, and it blocks the shape of `amadeo-cli`.**
ADR 0011 compiles game logic into the game binary, so a standalone CLI cannot know a game's
components. `fmt` and `new` work standalone; `check`, `describe`, `inspect`, `run`, and `replay` all
need the game's registry. Options and a prior are in `06-open-questions.md`. **Decide before writing
the CLI, not during.**

⚠️ **The mutating calls** — `world.spawn`, `world.set_component`, `sim.step`, `sim.pause` — and
`events.since`, `scene.load`/`save`, `replay.*`, `snapshot.*`. Keeping the read side independent has
been worth it so far; that separation is worth preserving.
⚠️ **JSON parsing.** The writer exists; nothing reads JSON. The RPC server needs a parser, and it is
a larger piece than the writer was.
⚠️ **`render.capture` / `render.describe`** — the agent's eyes. Need the 2D renderer, so they need Q3.
⚠️ **The protocol spec** in `docs/protocol/`, versioned. Not written; the editor, the CLI, and the
agent all depend on its stability, so it should exist before the second client does.

## Recommended build order

The order matters more than the list. Dependencies run downward:

```
1. math, core, ECS, determinism & time     ← ✅ the spine, M0. Nothing works without it.
2. reflection & schema                     ← ✅ ADR 0012/0013. Unlocked serialization and the agent.
3. events, input, app loop                 ← ✅ M0
4. Q1 spike: game logic & hot reload       ← ✅ M0. Resolved by measurement (ADR 0011): no crate.
5. scene format + agent interface layer    ← 🟡 M1. Format done (ADR 0014); agent read half done;
                                              CLI/RPC blocked on Q14. The collaboration surface.
6. assets, 2D rendering                    ← M1. 2D wants Q3 settled first.
7. 3D rendering, physics                   ← M2
8. audio, animation, UI, save/load         ← M3
9. editor                                  ← M4
10. build/export, profiling tools          ← M5
11. genre modules, then actual games       ← M6+
```

Items 1, 2, and 4 are the ones that are ruinously expensive to retrofit. Everything else can be built,
thrown away, and rebuilt without much loss — which is exactly why the early milestones should move
slowly and carefully and the later ones can move fast.
