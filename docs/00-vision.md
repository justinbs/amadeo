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
| Multiplayer / netcode | Enormous scope multiplier; touches every subsystem. | **Yes, meaningfully** — determinism + snapshot/rollback are exactly the substrate rollback netcode needs. Deferred, not designed out. |
| Console platforms | NDAs, dev kits, certification. | wgpu abstracts the backend; not blocked structurally. |
| Mobile / touch | Different input, perf, and packaging model. | Input action layer is device-agnostic by design. |
| Visual scripting graphs | Large UI investment; a text-first engine serves both authors better. | Node graph could be authored as text and rendered by the editor later. |
| Terrain, open-world streaming, large-scale vegetation | Specialist subsystems, only needed by a narrow genre. | Could arrive as a module. |
| Localization / i18n | Real, but not architecture-shaping this early. | Text assets go through the asset system, so retrofit is contained. |
| Asset store / plugin marketplace | Requires users. We have two. | Module system already provides the extension shape. |
| Writing our own physics engine | rapier is deterministic, maintained, and 2D+3D. Building this ourselves would consume an entire milestone for a worse result. | Wrapped behind engine traits, so it's replaceable. |
| Supporting languages other than Rust for engine code | Fragmentation cost. | Game logic language is open question Q1; WASM would open this up. |

## What "done enough to make games" looks like

The engine is not a research project — it is infrastructure for making games with. The bar for M3
(see `05-roadmap.md`) is a small but genuinely *complete* game: title screen, playable loop,
win/lose states, save/load, audio, and a build you can hand to someone. Everything before that is
scaffolding toward that bar.

## Guiding principle when in doubt

> Prefer the choice that keeps both authors equally capable, and that keeps the loop from
> *change* to *observed result* short. Almost every design argument in this project resolves
> correctly under those two tests.
