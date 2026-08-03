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

## Q19 · P2 · `amadeo import` cannot import a prefab

**Found in session 8, by hitting it.** A bootstrapping deadlock created by ADR 0029 making prefabs
assets:

1. a prefab needs a `.ama-meta` sidecar before anything can reference it;
2. `amadeo import` writes sidecars;
3. but `import` launches the game (ADR 0016) to ask where its asset directory is;
4. and the game refuses to start while a prefab its scene names has no sidecar.

So the first prefab in a project has to have its sidecar written by hand, which is what the Vault's
two have. The error message is good — it says exactly what is missing — but the tool that fixes it
cannot run.

**The likely fix is that `import` should not need the game at all.** Importing is a filesystem
operation over a directory; the only thing the game supplies is the directory's name, and
`amadeo.toml` could carry that instead. That would also make `amadeo import` work on a project that
does not currently compile, which is a good property for a repair tool.

Worth checking whether the same deadlock reaches `amadeo check` and `amadeo assets`, which launch the
game for the same reason.

---

## Q8 · P2 · General entity relations, or just parent/child?

Games want many relationships: equipped-by, targeting, owned-by, docked-to. Parent/child covers
transforms only.

Prior: plain components first (`Targeting(Entity)`), revisit if it becomes painful. General relations
are a significant ECS complexity increase and it's not yet clear we need them.

---

## Q9 · P2 · Threading model, precisely

Which pools exist, what runs off the simulation thread (asset loading, audio mixing, render
submission), and exactly how results re-enter the deterministic zone in a fixed order.

This is where determinism is most commonly lost in real engines. Decide before adding the first
background task, not after.

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

## Q22 · P2 · A resource's identity in the state hash is its Rust path, not its canonical name

**Found in session 8**, while building the control experiment for ADR 0031's replay regeneration —
by writing a stand-in type with the same canonical name and the same fields, and finding it hashed
differently.

`ResourceId::of::<T>()` is `hash_type_name::<T>()`, which hashes `std::any::type_name::<T>()` — the
**Rust path**. `ComponentId` does the opposite: ADR 0017 makes it the hash of the *canonical* name,
precisely so that moving a component between crates is free and renaming it is the deliberate
breaking change.

So resources and components follow opposite rules, and the comment in `type_hash.rs` acknowledges it
("See that ADR for why components differ from resources and services here"). The consequence is real:
**moving a resource from one crate to another changes every state hash containing it**, silently
invalidating every golden replay, with nothing in the type's own definition having changed.

Nothing is broken today. But `amadeo-render`, `amadeo-input` and `amadeo-app` all own resources, and
the crate graph is still moving — `Camera2d` moving out of `amadeo-render` would have been exactly
this, had it not been deleted instead.

Worth deciding whether resources should follow ADR 0017 too. Against: a resource is never named in a
scene file, so the canonical name buys less than it does for a component. For: `world.resources` and
`describe` both report resources *by canonical name* already, so the identity used for hashing is
already not the identity the outside world sees.

---

## Q21 · **P1** · A scene file cannot express a nested struct, a payload enum, or `None`

**Found in session 8** by probing the format directly, while designing the camera component. Never
hit before because no component has ever had such a field — every one so far is scalars, flat lists,
and marker types.

What a component field can be today:

| Shape | |
|---|---|
| scalar (`f32`, `bool`, `string`, ints) | ✅ |
| flat list (`[f32; 3]`, `Vec<f32>`) | ✅ |
| fieldless enum variant (`phase Playing`) | ✅ |
| **nested struct** | ❌ emits `{height: 8}`, a Rust `Debug` form nothing parses |
| **enum with a payload** | ❌ emits `Ortho({height: 8})`, same problem |
| **`Option::None`** | ❌ writes a bare field name, and the parser refuses it |
| map | ❌ known, recorded by ADR 0027 |

The writer does not *lie* about it — `inline_value` returns `None` and the value falls through to a
debug rendering, which fails loudly at parse time rather than silently changing shape. That was a
deliberate choice in ADR 0014's implementation and it is why this is a gap rather than a corruption
bug. But it means a whole class of natural component design is unavailable.

**It shaped ADR 0031's camera**, which is flat — a fieldless `Projection` enum beside plain `height`,
`fov`, `near` and `far` fields — where the obvious design is `Projection::Orthographic { height }`.
That is a worse type: it has representable states that mean nothing, which is exactly what the
Vault's `Phase` comment argues against.

**It will bite again at materials** (M2), where `Material { base_colour, metallic, texture }` nested
under a mesh is the natural shape, and at anything with an optional field.

The likely answer is an indented block under a field name, which the *grammar* has room for — a field
with no inline value already introduces a block, it just only accepts `- ` list items today. Wants
its own ADR against ADR 0014, and wants deciding before M2's material model rather than after.

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
