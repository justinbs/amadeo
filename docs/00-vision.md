# 00 — Vision, Goals, and Non-Goals

## The premise

Existing engines were designed for humans and later had AI bolted on the side. Amadeo assumes from
the first line of code that **two kinds of author** will use it, and that neither is a guest.

That single assumption produces an engine that is measurably different in structure — not in mood
or marketing. The differences are concrete and testable, and they are listed in
`03-ai-native-design.md`.

## Why current engines resist AI agents

Worth being precise about the problem, because each item below maps to a design decision.

1. **The editor owns the truth.** In Unity, scenes and prefabs are editor serialization output —
   GUID-laden, order-sensitive, hostile to hand editing. Godot's `.tscn` is far better but still
   reorders and rewrites in ways that produce noisy diffs. An agent that edits these files is
   fighting the tool. → *Invariant I1, I2.*

2. **There is no feedback loop.** An agent writes gameplay code and then has no way to know if it
   worked. Did the enemy spawn? Is the jump height right? Compile errors are the only signal
   available, and compile success says nothing about behavior. → *Determinism + headless simulation
   + introspection (I3, I7).*

3. **Running state is opaque.** "What entities exist right now, and what is the player's velocity?"
   is unanswerable without attaching a debugger built for humans. → *Agent Interface Layer.*

4. **The API surface is huge and undiscoverable.** Thousands of classes and methods, documented for
   human browsing, not machine query. An agent guesses, and guessing produces plausible code that
   doesn't compile or silently misbehaves. → *Reflection registry emitting machine-readable schemas
   (I8).*

5. **Nondeterminism makes verification impossible.** Without reproducibility there are no
   regression tests for behavior, only for pure functions. Game bugs live in behavior. → *I3.*

## Goals

**G1 — Genre-agnostic.** The engine core has no opinion about what kind of game you're making. It
provides mechanisms: entities, components, systems, schedules, events, assets, rendering, physics,
audio, input, UI. Genre vocabulary (platformer controllers, tilemaps, dialogue trees, turn order)
lives in optional modules that the core never depends on.

**G2 — Dual authorship with true parity.** A human can build a complete game entirely in the
graphical editor. An agent can build a complete game entirely in text and RPC. Both produce the same
artifacts, and those artifacts diff cleanly in git so the two can work on one project together.

**G3 — Deterministic and observable.** Any run can be recorded, replayed, hashed, snapshotted, and
inspected. This is the foundation for testing, debugging, save/load, and eventually netcode.

**G4 — Data-oriented.** Archetype-based ECS with structure-of-arrays storage. Chosen for cache
behavior, but equally for *uniformity*: everything is data in a known layout with a known schema,
which is exactly what makes automated reasoning about a game state tractable.

**G5 — Fast iteration.** Changing gameplay and seeing the result must take seconds, not minutes.
This constrains the architecture (see open question Q1) and is treated as a functional requirement,
not developer comfort.

**G6 — Legible.** Small crates, one-way dependencies, doc comments on every public item, errors that
say what to do next. Both authors read this codebase; comprehension is a feature.

## Non-goals

Explicit non-goals are what make the scope survivable. Each of these is *deliberately excluded*,
with the door left open where cheap.

| Not doing | Reasoning | Door left open? |
|---|---|---|
| Competing with UE5 on visual fidelity | Ray tracing, virtualized geometry, and global illumination are multi-year specialist efforts and irrelevant to the games we want to make. | Render graph abstraction allows adding passes later. |
| ~~Multiplayer / netcode~~ | **No longer a non-goal.** Reclassified in session 2 because all three target games were co-op; the session-7 additions made it six of eight. Promoted to a planned milestone (M6) with architectural hooks reserved during M0–M2. See ADR 0006 and § Multiplayer below. |
| Console platforms | NDAs, dev kits, certification. | wgpu abstracts the backend; not blocked structurally. |
| Mobile / touch | Different input, perf, and packaging model. | Input action layer is device-agnostic by design. |
| Visual scripting graphs | Large UI investment; a text-first engine serves both authors better. | Node graph could be authored as text and rendered by the editor later. |
| Terrain, open-world streaming, large-scale vegetation | Specialist subsystems, expensive, and not needed to prove the engine works. | **Yes, and now expected** — reclassified in session 2 once the target game direction was known (see §"Target game direction"). Out of scope through M5, but the renderer's culling architecture, world-coordinate precision, and chunked asset loading must not *preclude* it. |
| Localization / i18n | Real, but not architecture-shaping this early. | Text assets go through the asset system, so retrofit is contained. |
| Asset store / plugin marketplace | Requires users. We have two. | Module system already provides the extension shape. |
| Writing our own physics engine | rapier is deterministic, maintained, and 2D+3D. Building this ourselves would consume an entire milestone for a worse result. | Wrapped behind engine traits, so it's replaceable. |
| Supporting languages other than Rust for engine code | Fragmentation cost. | Game logic language is open question Q1; WASM would open this up. |

## Target games

Established session 2 with three games, **extended to eight in session 7**. Deliberately spanning
different genres, dimensions, scales, and art directions:

| Game | Shape |
|---|---|
| **Palworld** | 3D third-person, open world, creature collection and companionship, survival and crafting, base building, co-op. Stylised-realistic outdoors. |
| **Schedule I** | 3D first-person business/dealing simulation, NPC daily schedules, economy and production chains, property management, co-op. Low-poly stylised. |
| **Inside the Backrooms** | 3D first-person co-op horror, bounded procedural interiors, pursuing entities, inventory and puzzles. Dark atmospheric realism. |
| **Minecraft** | 3D first-person voxel sandbox. Infinite streamed, fully destructible world; chunk meshing; crafting; multiplayer. Defined as much by its mod ecosystem as by the base game. |
| **Terraria** | **2D** side-on sandbox. Tile world, destructible and buildable, enormous item and entity counts, boss fights, co-op. |
| **Project Zomboid** | Isometric survival simulation. Very large streamed map, deep per-character simulation, hundreds to thousands of zombies, co-op. |
| **RimWorld** | **2D** top-down colony simulation. Deep autonomous agent AI, long uninterrupted simulation runs, dense UI, and one of the strongest modding cultures in games. |
| **Stellaris** | Grand strategy. Tens of thousands of simulated entities, minimal action rendering, extremely dense UI, long games with large save files, heavily modded. |

**Eight different games are a much better specification than three.** The intersection tells us what
belongs in the core; the divergence tells us what must stay pluggable. That distinction *is* the
genre-agnostic design (G1), and it is now grounded in real targets rather than speculation.

### What the session-7 additions changed

The original three were all 3D, all action-paced, all co-op, and all rendering-led. The five added in
session 7 break every one of those patterns, which is exactly what makes them useful. Six specific
consequences:

**1. 2D stopped being hypothetical.** Terraria and RimWorld are 2D games, and Project Zomboid is
isometric. The engine already promised 2D as a first-class capability (below), but that promise was
being kept on principle against an all-3D target list. It is now a *requirement of the target list*.
The sprite renderer is no longer "the 2D path we must not neglect" — it is the renderer three of the
eight targets ship on.

**2. Destructible, streamed, chunked worlds are now a category.** Minecraft, Terraria, and Zomboid
all have worlds that are generated, streamed, and modified at runtime. None of the original three
needed that — Palworld is open-world but not voxel-destructible. This is a genuinely new subsystem
(chunk storage, meshing, persistence of diffs) and it is the largest single addition to scope.

**3. Simulation scale now outranks rendering fidelity for several targets.** Stellaris and RimWorld
render very little and simulate enormously; Zomboid simulates thousands of agents. The original three
pushed hardest on the renderer. These push hardest on the **ECS**, and specifically on the gaps
already recorded as known: no bundle/spawn-with-components API (N archetype migrations per entity),
no parallel system execution, and query shapes that stop at three components.

**4. Dense UI is now a first-class requirement, not an M3 checkbox.** Stellaris and RimWorld are
mostly UI. `amadeo-ui` — the retained-mode game UI system — moves up sharply in priority, and its
requirements change: it needs to handle information-dense, scrollable, sortable, tabbed layouts, not
just a health bar and a menu.

**5. Long-run persistence gets harder.** Colony sims and grand strategy run for tens or hundreds of
hours and produce large, versioned saves. This raises the value of the snapshot work already flagged
as an M1 priority, and it means save-format versioning is a real problem rather than a later one.

**6. Modding is now a target-driven requirement, and it puts real pressure on ADR 0011.** Four of
the five additions — Minecraft, RimWorld, Terraria, Stellaris — are defined in large part by their
modding ecosystems. ADR 0011 decided game logic is plain Rust with no scripting layer, and it decided
that **by measurement against an iteration-speed premise**. Modding is a different argument
entirely: a third-party mod author cannot rebuild the engine, and "recompile the game to add a mod"
is not a modding story. ADR 0011 reserved WASM as an escape hatch, but behind a trigger that does not
cover this reason. **Filed as Q15** — see `docs/06-open-questions.md`. Nothing needs deciding today,
and nothing built so far is invalidated, but this should not be discovered late.

### The targets are a priority signal, not a scope limit

Five of the eight are 3D and three are 2D or isometric. **That orders the work; it does not narrow
the engine.** Amadeo supports 2D and 3D as equal first-class capabilities — decided in session 1,
restated by Justin in session 6 after ADR 0018 leaned too hard on "all three targets are 3D" while
justifying a 3D-first transform, and now settled by the target list itself.

The distinction that matters:

- **Legitimate:** doing 3D work *earlier*, because that is what the reference games need first.
- **Not legitimate:** treating 2D as a stepping stone to be discarded, shipping a 2D feature that is
  worse than its 3D equivalent, or making a design choice that forecloses 2D.

This is a genre-agnostic engine (G1). A 2D game is a genre. If a decision would leave 2D as a
second-class citizen, that is the signal to choose differently — the same way the divergence table
below forbids baking in one art direction.

Read this alongside ADR 0018, which retired `Transform2d` in favour of one 3D `Transform`. That
decision is **not** a deprioritisation of 2D: two transform types would have meant two hierarchies in
any world mixing them, which would have made 2D-over-3D harder, not easier. One transform serves both
better. The reasoning was right and some of its wording was not.

### Common to all eight → core, not modules

- **Autonomous entity AI with behaviour states** — pals, customers and police, pursuing entities,
  zombies, colonists, empires. Present in every single target, at wildly different scales. This is
  the strongest signal in the whole list.
- Inventory and item systems — all eight.
- Persistence, including long-run saves.
- A world clock driving day/night, schedules, or turns.
- **Large entity counts.** The original three implied hundreds; Stellaris and Terraria imply tens of
  thousands. ECS throughput is now a target requirement rather than an aesthetic preference.

Note what dropped off this list when the targets widened: **"3D real-time with a character
controller" is no longer common to all.** Stellaris has no character at all, RimWorld and Terraria
are 2D, and Zomboid is isometric. It belongs in a module, which is what I4 would have said anyway.

### Most, but not all → priority modules

Survival/crafting resource loops (five of eight), base or colony building (six of eight), a
character controller (five of eight), co-op multiplayer (five of eight).

### Divergent → must never be baked in

| Axis | Spread | Consequence |
|---|---|---|
| Dimension | 3D (Palworld, Schedule I, Backrooms, Minecraft) vs 2D (Terraria, RimWorld) vs isometric (Zomboid) vs mostly-UI (Stellaris) | **2D and 3D are equal first-class paths.** One `Transform` serves both (ADR 0018); the renderer must not privilege either. |
| Camera | First-person vs third-person vs top-down vs isometric vs free strategy camera | The camera rig must be **separate** from the character controller, and must not assume a character exists at all. |
| Art direction | Stylised-realistic outdoor / low-poly / dark atmospheric interior / voxel / pixel-art sprite | **The renderer cannot bake in a look.** Configurable post-process, flexible lighting, fog/volumetrics — and a sprite path good enough for a pixel-art game, not a fallback. |
| World model | Authored levels vs open world vs **fully destructible chunked voxel/tile** | Chunk streaming and runtime world mutation are now a real subsystem. Culling and persistence must not preclude them. |
| Simulation load | A handful of NPCs vs thousands of zombies vs tens of thousands of Stellaris entities | ECS throughput, and eventually parallel scheduling within ADR 0005's determinism rules (Q9). |
| UI weight | A health bar and a menu vs Stellaris' entire game being UI | `amadeo-ui` needs dense, scrollable, sortable, tabbed layouts — not a widget toy. |
| Extensibility | Closed games vs **mod-defined** games (Minecraft, RimWorld, Terraria, Stellaris) | Directly in tension with ADR 0011. See Q15. |
| Pace | Relaxed simulation vs tense horror vs turn-adjacent strategy | Audio and lighting carry disproportionate weight in some and almost none in others. |

### Multiplayer: reclassified

Six of the eight targets are co-op or multiplayer — all three of the originals, plus Minecraft,
Terraria, and Project Zomboid — so treating multiplayer as a non-goal was wrong once the targets were
known, and the session-7 additions only strengthened that. Networking is the most painful retrofit in
engine development: it touches entity identity, authority, state replication, and every gameplay
system simultaneously.

Decision (ADR 0006): **reserve the architectural hooks during M0–M2, build the netcode at M6.** The
cheap structural half gets decided while the relevant systems are being written; the expensive half
waits. Note that invariant I7 (everything headless-capable) already gives us a dedicated server as
close to a side effect.

Correcting an earlier framing: determinism does not by itself supply the networking model here. Co-op
survival games use **client-server with server authority and client prediction**, not deterministic
lockstep. Determinism remains valuable — a reproducible server simulation is far easier to debug — but
it is not the architecture.

### The first game to actually finish

**A single-player first-person atmospheric horror slice**, in the vein of Inside the Backrooms. This is
M3's exit gate.

Chosen because it is the smallest genuinely *finishable* complete game — bounded interiors, a handful
of entities, short runtime — while being the **hardest test of the renderer**: if it can produce a
convincing dark corridor with a flashlight and real atmosphere, the other two art directions are
easier. It exercises entity AI, inventory, interaction, procedural level assembly, and audio, which is
where horror lives or dies. Short horror games are a respected format, so small does not mean
unfinished.

Palworld-scale remains the long-term direction, not an early deliverable — it is a studio product built
by a team over years, and that fidelity is as much an art-asset problem as an engine one. Stating
otherwise would make this roadmap fiction.

## What "done enough to make games" looks like

The engine is not a research project — it is infrastructure for making games with. The bar for M3
(see `05-roadmap.md`) is a small but genuinely *complete* game: title screen, playable loop,
win/lose states, save/load, audio, and a build you can hand to someone. Everything before that is
scaffolding toward that bar.

## Guiding principle when in doubt

> Prefer the choice that keeps both authors equally capable, and that keeps the loop from
> *change* to *observed result* short. Almost every design argument in this project resolves
> correctly under those two tests.
