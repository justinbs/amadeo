# 06 — Open Questions

Check this before assuming any undecided thing. Resolve with an ADR, then move the entry to the
resolved section at the bottom.

Priority: **P0** blocks work now · **P1** needed for the current milestone · **P2** can wait.

---

## Q3 · P2 · Which render pipeline shape?

**Two thirds of this question are resolved — see `adr/0018`.** The original Q3 bundled three
decisions with very different reversal costs. The two expensive ones, both *data*, are settled: one
3D `Transform` with 2D as its degenerate case, and an explicit `SortOrder` that dominates depth.

What remains is the pipeline itself: a unified orthographic pipeline with a specialised sprite
batcher, two pipelines sharing the render graph, or 2D composited over 3D (`04-subsystems.md` §4).

**Dropped to P2**, which is the point of having split it. `RenderBackend` isolates this completely —
no scene file, no component schema, and no state hash can observe which was chosen — so it is the
cheapest of the three to change and does not block `GlobalTransform` propagation or the sprite
batcher's *interface*.

Decide it **while writing the sprite batcher**, against a real throughput target (§4 suggests 20k
sprites at 60 fps). A spike of three prototypes would measure less than the real thing does.

---

## Q17 · P1 · The ECS cannot express an optional component in a query

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

## Q7 · P2 · Prefab override semantics

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

## Q10 · P2 · One dimension per project, or both simultaneously?

Whether a project selects 2D or 3D at build time (simpler, smaller binaries, cleaner physics choice)
or can freely mix both (2D UI over a 3D world is common; 2D minigames inside 3D games exist).

Note that 2D UI over 3D is a `amadeo-ui` concern, not necessarily a 2D-renderer concern — so these may
be less coupled than they look. Decide in M2.

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
