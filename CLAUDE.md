# CLAUDE.md — Amadeo Engine

> Read this file, then `STATUS.md`, before doing anything else in this repo.
> `STATUS.md` says where the project actually is right now. This file says how to work in it.

---

## 1. What Amadeo is

Amadeo is a **general-purpose, genre-agnostic game engine designed to be driven equally well by a
human in a graphical editor and by an AI agent through text and RPC.**

Two audiences, one engine, no second-class citizen:

- **Justin** (the human) works through a graphical editor, code, or both.
- **Claude** (the agent) works through text files, a CLI, and a live introspection protocol.

Neither can do something the other cannot. That symmetry is the product, not a feature.

**It is not** a framework for one genre, a rendering demo, or a wrapper over an existing engine.
It is a data-oriented engine core plus optional genre modules.

## 2. Non-negotiable invariants

Breaking any of these is a bug, no matter how convenient. If a task seems to require breaking one,
stop and raise it instead of working around it.

| # | Invariant | Why |
|---|---|---|
| **I1** | **Text files are the only source of truth.** Scenes, prefabs, assets metadata, and config live in human-readable, hand-editable text. The editor is a *client* that reads and writes those files. It never holds private state. | Without this, the agent is locked out of authoring. See `docs/adr/0003`. |
| **I2** | **Serialization is canonical and byte-stable.** Saving an unchanged file produces a byte-identical file. Sorted keys, stable IDs, fixed formatting. `amadeo fmt` is the single authority. | Editor saves and hand-edits must produce clean, reviewable diffs. |
| **I3** | **Simulation is deterministic.** Fixed timestep, seeded RNG, no wall-clock or unordered iteration in gameplay logic. Same inputs + same seed = same state hash, on any machine. | This is the keystone. It buys replay-as-test, headless verification, snapshots, save/load, and time-travel debugging. See `docs/adr/0005`. |
| **I4** | **The engine core contains zero game logic.** No concept of health, jumping, inventory, or damage below the module layer. Genre knowledge lives only in `modules/` and in games. | This is what "genre-agnostic" actually means operationally. |
| **I5** | **Anything the editor can do, the CLI and RPC can do.** The editor is built strictly on top of the same protocol the agent uses. No editor-only capabilities, ever. | Guarantees the agent never falls behind the human. |
| **I6** | **Dependencies flow one way.** The crate graph is a strict DAG (see §4). A lower layer never references a higher one. No cyclic crates, no "just this once." | Keeps the engine comprehensible and testable in isolation. |
| **I7** | **Every subsystem is headless-capable.** Rendering, audio, and input all have null backends. The whole engine must run with no window and no GPU. | Headless is how the agent runs and verifies games, and how CI works. |
| **I8** | **Reflection is not optional.** Every component, resource, and event registers a machine-readable schema. If it can't be reflected, it can't be serialized, inspected, or edited. **Enforced by trait bound** — ADR 0013 for components, ADR 0027 for the other two. | One registry powers serialization, the editor, and agent introspection. |

## 3. Tech stack (decided)

- **Language:** Rust (2024 edition), `#![forbid(unsafe_code)]` outside explicitly audited modules.
- **Graphics:** `wgpu` — one API over Vulkan/DX12/Metal, and it targets WebGPU, so a browser export
  path stays open for free.
- **Windowing:** `winit`. **Math:** `glam`. **Physics:** `rapier` (2D+3D) behind engine-owned traits.
- **Editor UI:** `egui` (immediate-mode, in-process, cheap to build). Game UI is a separate,
  retained-mode system — do not confuse the two.
- **Primary target:** native desktop, Windows first. Web export is a later milestone, not a
  parallel obligation.
- **Game logic authoring:** **Rust systems in the game crate.** No scripting layer, no dynamic
  reload. Settled by measured spike — ADR 0011, evidence in `spikes/q1-game-logic/`. WASM is the
  pre-selected escape hatch if a gameplay rebuild ever sustains above 5 s; check by re-running
  `spikes/q1-game-logic/measure.ps1`, not by impression.

Rationale and rejected alternatives: `docs/02-tech-stack.md` and `docs/adr/0002`.

## 4. Repository layout & dependency order

Crates are listed in dependency order. **A crate may only depend on crates above it.**
`✅` exists and is tested. `—` planned, not yet written.

```
crates/
✅ amadeo-derive      proc macros: #[derive(Reflect)], #[derive(StableHash)]. No engine deps, so it
                     sits below even amadeo-core. Re-exported next to each trait; never used directly.
✅ amadeo-image       decodes PNG (via the `png` crate) and PPM (hand-written) into TextureData —
                     width, height, an explicit PixelFormat, and flat pixels. Also no engine deps.
                     ADR 0026: decoding happens at load time *for now*, and the format tag is what
                     makes the eventual import pipeline an addition rather than a rewrite. Holds the
                     only non-`thiserror` dependency in the engine; that is why it is its own crate.
— amadeo-math        vectors, matrices, quaternions, rects, curves. No engine deps.
✅ amadeo-core        Tick, FIXED_DT, Rng (PCG32), StableHasher (FNV-1a), StableId/NetId/Authority
✅ amadeo-reflect     Value tree, TypeInfo schema, TypeRegistry. ADR 0012. Values include maps with
                     string keys (ADR 0027) — a key type implements ReflectKey, and `to_key` must be
                     injective. Also holds `Reflect for Tick`: a type below this crate cannot
                     implement the trait (I6), so the impl goes where the *trait* lives instead.
✅ amadeo-ecs         archetype SoA storage, resources, services, deferred commands,
                     ComponentRegistry (builds a component from a name + a Value), and queries:
                     `world.query::<(&A, Option<&B>)>()` resolves each column once per archetype
                     (ADR 0025). Read-only; mutation stays with for_each_*_mut.
✅ amadeo-transform   Transform (3D; 2D is its degenerate case, ADR 0018), Parent, GlobalTransform +
                     propagate_transforms, and a scalar Mat4. GlobalTransform is computed, never
                     authored, and DERIVED so it stays out of the state hash (ADR 0019).
✅ amadeo-events      typed double-buffered queues, EventClock total ordering
🟡 amadeo-assets      AssetCatalogue (declared id -> file, ADR 0020), the .ama-meta sidecar format,
                     a sorted directory scan, sidecar generation on import, and byte loading behind
                     ADR 0021's barrier. Asset-root resolution is by marker file (ADR 0022).
                     Typed handles, the import/decode pipeline, and hot-reload still to come.
✅ amadeo-input       action mapping, InputState, recording/replay, the .replay text format
🟡 amadeo-render      RenderBackend trait, NullBackend, Quad/Sprite/SortOrder/Camera2d, the sprite
                     batcher (ADR 0023: batches are (sort order, texture) pairs), and TextureCache —
                     id -> bytes -> pixels, with a three-step placeholder fallback ending in an
                     image built in code so it cannot itself be missing. wgpu behind `gpu` draws
                     **quads and sprites**: texture upload, a nearest sampler, one bind group per
                     texture, one draw call per batch. Still to come: render targets, `render.capture`.
— amadeo-audio       mixer, buses, spatialization (null backend required)
— amadeo-physics     rapier integration behind engine traits
— amadeo-anim        sprite anim, skeletal, state machines, tweens
— amadeo-ui          retained-mode game UI: layout, theming, focus navigation
✅ amadeo-snapshot    the .snapshot text format (ADR 0028): capture a whole world to a file and put
                     it back. Sits above amadeo-scene because it borrows that crate's scalar
                     encoding — format_float is subtle and two copies would drift. **It captures the
                     entity allocator's free list**, which state_hash excludes: without it a restored
                     world hashes identically and then spawns different handles.
🟡 amadeo-scene       the .scene text format (ADR 0014): parser, canonical writer, instantiate into
                     a World, and the `assets` block a scene declares its requirements in (ADR 0021).
                     Prefab instancing still pending, and blocked on Q7's sub-question — ADR 0014 and
                     ADR 0020 disagree about whether `from` holds a path or an asset id.
✖ amadeo-script      NOT BUILT. ADR 0011: game logic is plain Rust in the game crate.
🟡 amadeo-agent       the protocol: JSON reader and writer, JSON-RPC envelope, and the methods that
                     need only a world + registry (describe, world.query/entity/list/resources).
                     Read-only. Mutation, snapshots, and capture pending. ADR 0016, spec in
                     docs/protocol/v1.md.
✅ amadeo-app         Stage/Schedule, fixed-timestep loop, SimRng, ComponentRegistry, and the agent
                     *host* — serve_if_requested reads stdin and answers. The host lives here rather
                     than in amadeo-agent because it needs App and I6 forbids reaching down.
— amadeo-editor      graphical editor. A CLIENT of amadeo-agent. No privileged access.
🟡 amadeo-cli         the `amadeo` binary. Built: describe/query/entity/schedule/status/call/check/
                     replay/fmt/assets/import/snapshot, plus `--from <file>` on any of them to
                     restore a snapshot before answering.
                     Pending: new/run/test/build/export. ADR 0016: `fmt` is standalone;
                     everything else spawns the game binary in agent mode and talks to it over stdio,
                     because only that process knows the game's components.
modules/             optional, genre-flavored. Core NEVER depends on these.
games/               actual games built with the engine
  quad-demo          M0's exit gate: a steerable quad, plus the replay fixture CI asserts on.
  vault              M1's exit gate: a complete small 2D game. The level is scenes/vault.scene;
                     the sprites are generated from hand-written .pix text by
                     `cargo run -p vault --bin pix`. Its tests are the milestone's proof —
                     plays_itself.rs drives the game with scripted input, and
                     verified_without_eyes.rs checks the screen through render.describe.
                     NOTE it has two binaries, so it sets `default-run` — without that
                     `cargo run -p vault` is ambiguous and every CLI command against it fails.
docs/                design docs and ADRs
spikes/              separate cargo workspaces holding the evidence behind an ADR. Frozen once
                     written; excluded from the engine workspace. See spikes/README.md.
```

**Note:** an earlier version of this section said `Transform` would move to `amadeo-scene` with the
hierarchy components. **That was wrong** and ADR 0015 corrects it: `amadeo-render`, `amadeo-physics`,
and `amadeo-anim` all sit *below* `amadeo-scene` and all need transforms, so I6 makes that placement
impossible. They live in `amadeo-transform`.

**Careful:** `ComponentId` is the hash of a component's **canonical name** (`Reflect::type_name`),
not its Rust path — ADR 0017. So *moving* a component between crates is free, and **renaming one
changes its id and every state hash containing it**. `#[reflect(name = "...")]` renames the Rust type
without changing identity. Two components may not share a canonical name; the registry refuses it.

## 4b. Verifying the build

Everything must be green before a commit. These four are what CI runs:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Golden replays live in `crates/amadeo-app/tests/golden/`. If one fails, read
`docs/07-working-with-the-code.md` § Golden replays **before** regenerating it.

**Where does new code go?** If it needs to know what a game *is about*, it belongs in `modules/` or
`games/`. If it's a mechanism with no opinion about genre, it belongs in a crate. When in doubt,
put it higher up the stack — pushing things down later is easy; pulling them out is not.

## 5. Working agreement for sessions

**At the start of a session:**
1. Read `CLAUDE.md`, then `STATUS.md`, then the current milestone section of `docs/05-roadmap.md`.
2. Run `git log --oneline -15` to see what actually happened last.
3. Check `docs/06-open-questions.md` — if the task depends on an open question, resolve it with
   Justin *before* writing code that assumes an answer.

**During a session:**
- Any decision that constrains future work gets an ADR in `docs/adr/`. Cheap to write, saves entire
  sessions of re-litigation. Number sequentially, never edit a decided ADR — supersede it.

- **When to put a choice to Justin: anything hard to reverse.** Stated by him in session 7, choosing
  this deliberately over the narrower "only things I'd read or write often". **The test is cost to
  undo, not visibility** — an internal mechanism nobody would look at still warrants asking if
  ripping it out later would mean rewriting a lot. Genuinely cheap-to-change internals can still be
  decided alone and flagged in the summary, as ADR 0022 was.

- **How to put a choice to Justin.** He has no game-engine-development background and has said he
  tends to take whichever option is recommended. That means offering a menu is not sharing the
  decision — the burden is on the recommendation to have been *earned*. So:
  - **Research before asking, not instead of asking.** When the codebase alone cannot settle a
    trade-off, go read how real engines solve it. He explicitly endorsed spending the time.
  - **Pros *and* cons for every option**, including the recommended one. A list of upsides with one
    "(Recommended)" tag is not a decision aid.
  - **Plain language.** Define the vocabulary — "sprite batcher", "render graph", "gimbal lock" —
    at the point where it affects a choice he has to make.
  - **Prefer the more complete option over the faster one.** Stated directly in session 6: he would
    rather have a complete engine than one that accumulates problems, and does not mind more steps.
    Do not quietly narrow scope to save effort; that is not the trade he wants.
- Prefer a working vertical slice over a complete horizontal layer. Every milestone must end with
  something runnable.
- Write the determinism test alongside the feature, not after. Retrofitting determinism is the
  single most expensive mistake available in this project.

**At the end of a session:**
1. Update `STATUS.md`: what landed, what broke, what's next, any new sharp edges.
2. Update `docs/06-open-questions.md` — remove resolved, add discovered.
3. Commit. Message body should explain *why*, not restate the diff.
   **No `Co-Authored-By: Claude` trailer.** Justin asked for this in session 6: it is a personal
   project, he knows Claude worked on it, and he does not want the paired authorship on GitHub. This
   overrides the default Claude Code convention of appending one. End the message at the last line
   of the body.
4. **Do not `git push`. Justin pushes.** Stated in session 7. Commit as much as you like and leave it
   on the local branch, then say how many commits are waiting and what is in them. Do not offer to
   push "just this once" — pushing is what makes the work public and starts CI, and he holds that
   gate. If you need CI to settle something, say the push is needed rather than pushing to find out.
   Checking CI with `gh` after *he* pushes is still the right habit.

## 6. Conventions

- **Errors:** `thiserror` for library crates, `anyhow` only in `amadeo-cli` and `games/`. No
  `unwrap()` or `expect()` in engine crates outside tests — return typed errors. Error messages must
  include actionable context (entity id, system name, asset path). Both a human and an agent read
  these; a bad error message is a real defect.
- **Naming:** components are nouns (`Transform`, `Velocity`). Systems are verb phrases
  (`integrate_velocity`, `resolve_collisions`). Events are past tense (`EntitySpawned`, `DamageDealt`).
- **Data layout:** structure-of-arrays over arrays-of-structs in ECS storage. Components are plain
  data — no methods with side effects, no `Rc`/`RefCell` in components.
- **Tests:** unit tests inline. Determinism and golden-replay tests in `tests/`. Every subsystem
  needs a headless test. No test may depend on frame timing or wall-clock.
- **Docs:** every public item gets a doc comment. Doc comments are the agent's API surface — treat
  them as load-bearing, not decoration.

### Legibility for a Rust-learning human — a hard requirement

Justin wants to be able to **read, debug, and fix this codebase himself**, including in sessions where
Claude isn't involved or has gotten stuck. He is not yet a Rust expert. This is a stated project
requirement, not a preference, and it constrains how code gets written:

- **Boring Rust beats clever Rust.** Prefer explicit types, plain functions, and obvious control flow.
  Avoid deep generic nesting, trait gymnastics, complex lifetime puzzles, and macro magic unless there
  is a real, stated reason. Where an exotic construct is genuinely necessary, comment *why* — not what.
- **Comment the non-obvious Rust, not the obvious code.** `// the Arc is here because the asset loader
  touches this from a worker thread` is useful. `// increment the counter` is noise.
- **No unexplained idioms.** If a construct would make someone with three months of Rust stop and
  squint (`impl Trait` in odd positions, `PhantomData`, interior mutability, `unsafe`), it needs a
  one-line explanation next to it.
- **Explain in prose when introducing a pattern.** When a session introduces a new architectural
  pattern, add it to `docs/07-working-with-the-code.md` with a short worked example. That file is
  Justin's map into the codebase and must stay current.
- **Commit messages explain why.** He will read git history to understand how things came to be.
- **Errors must be actionable by a human too**, not just structured for an agent. Same standard, both
  audiences.

The trade: some code will be slightly more verbose or slightly less optimal than peak-idiomatic Rust.
That is an accepted and deliberate cost. A codebase only one author can maintain has already failed
this project's core goal.

### Visual design: do not ship the default "AI app" look

Applies to the editor (M4), the game UI system and its default theme (M3), any tooling UI, and any
document or page produced for this project. Justin raised this explicitly and disliked the house style
that LLM-generated interfaces converge on.

**Avoid the tell-tale defaults:**
- `Inter` / `system-ui` / `-apple-system` as the typeface, and font stacks chosen by not choosing
- purple-to-blue gradients, and gradient text
- uniform large border radii on everything; glassmorphism; frosted translucent panels
- centred hero layouts with vast empty margins
- emoji as section markers or button icons
- soft grey drop shadows on floating white cards
- the generic "clean minimal SaaS" arrangement applied regardless of what the thing is

**Aim for instead:**
- A typeface picked deliberately, with some character. For a game engine, the right references are
  professional creative tools — Blender, Houdini, Ableton, Reaper, Nuke — not landing pages.
- **Information density over whitespace.** This is a tool for people doing sustained detailed work.
  Dense, legible, and quick to scan beats airy and sparse. Pro tools look busy because they are.
- Deliberate, slightly idiosyncratic colour. Committed choices, not hedged neutrals.
- Sharp or mixed corner treatments; visible structure; real dividers rather than implied ones.
- Personality. It is allowed to look like *something* rather than like nothing.

If a design decision could be described as "what an AI would produce by default", that is the signal
to choose differently. When in doubt, look at how a mature creative tool solves the same problem.

## 7. Traps specific to this project

Things that will quietly destroy the design if allowed:

1. **Editor convenience creep.** "Just store this one thing in editor state." No — see I1. Every
   piece of editor state that isn't in a file is a capability the agent loses.
2. **Nondeterminism leaks.** `HashMap` iteration, `Instant::now()` in gameplay, unsorted parallel
   writes, uninitialized float garbage. Each one silently voids replay testing. Use ordered maps in
   simulation paths.
3. **Genre logic drifting downward.** A `Health` component in `amadeo-ecs` breaks I4 and starts the
   slide toward a single-genre engine.
4. **The scene format becoming a serializer dump.** If the format is whatever the serializer happens
   to emit, humans stop being able to write it. The format is a designed artifact with its own spec.
5. **Skipping reflection registration.** Ships fine, then the editor and the agent can't see the
   type, and you find out three milestones later.
6. **Building breadth before the spine works.** Ten half-subsystems can't run a game. One thin
   working slice can.
7. **Forgetting the reserved multiplayer hooks.** Six of the eight target games are co-op or
   multiplayer. ADR 0006 reserves
   network identity, replication metadata, and authority during M0–M2 — while those systems are being
   written for the first time. Skipping them means a sweep across every component later. Equally: do
   **not** build transport or prediction code before M6; that's scope creep in the other direction.
8. **Baking an art style into the renderer.** The target games span stylised-realistic outdoors,
   low-poly, and dark atmospheric interiors. A pipeline tuned for one is a pipeline that can't do the
   others. Post-process and lighting stay configurable.
9. **Letting 2D become second-class.** Amadeo supports 2D and 3D equally — a 2D game is a genre, and
   I4 says genres are not privileged. Doing 3D earlier is fine; shipping a 2D feature that is worse
   than its 3D equivalent, or foreclosing 2D with a design choice, is not. Raised by Justin in
   session 6 when the target list was all-3D; **session 7 settled it by adding Terraria, RimWorld,
   and Project Zomboid**, so 2D and isometric are now target requirements rather than a principle
   being defended. See `docs/00-vision.md` § Target games.
10. **Assuming a game has a character, a camera behind it, and a 3D world.** Of the eight targets,
   Stellaris has no character at all, three are 2D or isometric, and three have fully destructible
   chunked worlds. A character controller belongs in `modules/`, not in the core, and the camera rig
   must not assume a character exists.
11. **Designing the module boundary without thinking about mods.** Four of the eight targets are
   defined by their modding ecosystems, which is in real tension with ADR 0011's "game logic is plain
   Rust". Nothing needs deciding yet, but "what can a mod do" is the same question as "what is the
   module boundary" — see **Q15**. Retrofitting a sandbox boundary is much worse than designing to one.

## 8. Reading order for the design docs

| Doc | Read it when |
|---|---|
| `docs/00-vision.md` | You need to know what we're building and what we're deliberately not. |
| `docs/01-architecture.md` | You're placing new code or changing structure. |
| `docs/02-tech-stack.md` | You're questioning a stack choice. |
| `docs/03-ai-native-design.md` | You're touching agent tooling, determinism, or introspection. **Highest-value doc in the repo.** |
| `docs/04-subsystems.md` | You're about to build a subsystem. Per-system requirements and decisions. |
| `docs/05-roadmap.md` | Start of every session. Milestones and their exit gates. |
| `docs/06-open-questions.md` | Before assuming any undecided thing. |
| `docs/07-working-with-the-code.md` | Setup, commands, and the Rust patterns this engine uses. **Justin's map into the codebase — keep it current.** |
| `docs/08-assets.md` | You're adding an asset, or wondering why it isn't showing up. |
| `docs/adr/` | You want to know why something is the way it is. |
