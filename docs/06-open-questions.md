# 06 — Open Questions

Check this before assuming any undecided thing. Resolve with an ADR, then move the entry to the
resolved section at the bottom.

Priority: **P0** blocks work now · **P1** needed for the current milestone · **P2** can wait.

---

## Q3 · P1 · How do 2D and 3D coexist in the renderer?

Detailed in `04-subsystems.md` §4. Unified orthographic pipeline with a specialized sprite batcher,
two separate pipelines sharing the render graph, or 2D as a compositing layer over 3D.

Expensive to reverse. Needs an ADR before M1's 2D work, not before M2's 3D work — otherwise M1's
sprite renderer gets built on an assumption M2 has to undo.

---

## Q4 · P1 · Asset identity: stable paths or GUIDs?

Paths are readable and diff-friendly (serves I1) but break on move/rename. GUIDs survive moves but are
opaque and are precisely what makes Unity's scene files unreadable to humans and agents alike.

Prior: **stable paths as primary identity, plus a rename-tracking tool** (`amadeo mv` that fixes
references). Prioritizes legibility, and the refactoring pain is tooling-solvable.

Needs an ADR in M1.

---

## Q14 · P0 · Where does `amadeo describe` actually run?

**ADR 0011 has a consequence the roadmap did not anticipate.** Game logic is compiled into the game
binary, so a standalone `amadeo` CLI **cannot know a game's components**. It can only ever describe
the engine's own.

`docs/05-roadmap.md` lists `amadeo-cli` with `describe` and `inspect` as though it were a standalone
tool. For a real project it cannot be. The commands split cleanly:

| Command | Needs the game's registry? |
|---|---|
| `new`, `fmt` | no — scaffolding and pure syntax |
| `check`, `describe`, `inspect`, `run`, `replay` | **yes** |

Three ways out:

1. **The game binary hosts the agent.** `cargo run -p mygame -- describe`. Simplest and always
   correct, but every game must wire it up, and `amadeo describe` stops being one command.
2. **The CLI launches the game and talks to it over RPC.** `amadeo describe` spawns the binary with
   a flag, connects, asks, prints. Keeps one entry point and is the shape the editor needs at M4
   anyway (I5 says the editor is an RPC client). Costs a transport before anything else works.
3. **A generated manifest.** The game writes its schema to a file at build time; the CLI reads it.
   Fast and offline, but it is a cache, and a stale cache describing a component that no longer
   exists is exactly the "plausible but wrong" failure Pillar 2 is meant to eliminate.

Prior: **(2), with (1) available underneath it.** The transport has to exist for M4 regardless, I5
demands CLI/RPC parity, and building it now means the parity is real from the first command rather
than retrofitted. (1) falls out for free as the thing the CLI launches.

**P0 because it blocks the shape of `amadeo-cli`**, which is most of what remains in M1's exit gate.
Decide before writing the CLI, not during.

---

## Q13 · P1 · Should `ComponentId` come from the code location or the canonical name?

Found while moving `Transform2d` for ADR 0015.

`ComponentId` is the FNV-1a hash of `std::any::type_name::<T>()`, which is the **fully-qualified
path**. ADR 0008 chose the name over `TypeId` because `TypeId` is not build-stable — that reasoning
holds. What it did not consider is that the *path* couples a component's identity to where its code
lives: moving `amadeo_render::components::Transform2d` to `amadeo_transform::Transform2d` changed its
id, and would have invalidated every state hash containing it.

It happened to be free this time — nothing committed asserted a hash containing `Transform2d`. It
will not always be. As written, a pure refactor (moving a type, renaming a module) is a
replay-invalidating change, and nothing warns you.

**The alternative:** hash `TypeInfo::name` instead — the canonical name that already exists because
`Component: Reflect` (ADR 0013), and which is *already* what a scene file writes. Then the ECS's
identity and the file's identity are literally the same string, moving code is free, and
`#[reflect(name = "...")]` lets a Rust type be renamed without changing identity.

**The cost:** two components with the same short name in different modules would collide. Today the
full path makes that impossible. `TypeRegistry::register` already rejects a name collision with a
clear message — but registration is not *enforced*, so an unregistered pair could silently share an
id and corrupt archetype lookup. Closing that means either enforcing registration or accepting the
risk.

Prior: **switch to the canonical name**, because one name for one component across the ECS, the
registry, and the file is worth more than a collision case the registry already detects. Wants a
decision before many replays exist.

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
| `ComponentId` derivation | Hash of the type *name*, never `TypeId` — `TypeId` is not stable across builds | `adr/0008` |
| System ordering tie-break | Alphabetical by label, never registration order | `amadeo-app` schedule docs |
| Spawning from a command buffer | `spawn_with(closure)` rather than a reserved-id handle; new entity not referenceable by other commands in the same batch | `amadeo-ecs` commands docs |
| **Q1** — game logic authoring and hot reload | **Rust, compiled in. No scripting layer.** WASM reserved as the escape hatch behind a measured threshold; snapshots promoted as the real iteration-loop fix | `adr/0011` |
| Reflection shape | Value tree plus two derives, not dynamic field access; metadata vocabulary fixed | `adr/0012` |
| Is reflection optional? | No — `Component: Reflect` makes I8 a compiler-enforced bound | `adr/0013` |
| **Q2** — scene file syntax | **A custom, indentation-based, line-oriented format.** Chosen by Justin from four hand-written candidates; TOML is the fallback, *not* KDL | `adr/0014` |
