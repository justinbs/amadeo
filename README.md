# Amadeo

A general-purpose, genre-agnostic game engine designed to be driven equally well by a human in a
graphical editor and by an AI agent through text and RPC.

**Status: planning.** No engine code yet. See [STATUS.md](STATUS.md).

---

## The idea

Existing engines were built for humans and had AI bolted on afterward. Amadeo assumes from the first
line of code that two kinds of author will use it, and that neither is a guest:

- A **human** works through a graphical editor, code, or both.
- An **AI agent** works through text files, a CLI, and a live introspection protocol.

Neither can do something the other cannot. That symmetry is the product.

In practice that means four things, and they're structural rather than cosmetic:

1. **Text files are the only source of truth.** Scenes and prefabs are hand-writable, canonically
   formatted, byte-stable text. The editor is a client that reads and writes those files — it never
   holds private state.
2. **The simulation is deterministic.** Fixed timestep, seeded RNG, ordered iteration. Any run can be
   recorded, replayed, hashed, and snapshotted, which is what makes game *behavior* testable.
3. **A running game is queryable.** A JSON-RPC layer exposes entities, components, events, timings,
   and deterministic screenshots. The agent can see what it built without a human describing it.
4. **The API describes itself.** Every component registers a machine-readable schema, so an agent can
   ask what exists instead of guessing.

## Stack

Rust · wgpu (Vulkan/DX12/Metal, and WebGPU later) · winit · glam · rapier · egui

Native desktop first, Windows primary. Unified 2D and 3D. Web export planned for M5.

## Documentation

| Doc | Contents |
|---|---|
| [CLAUDE.md](CLAUDE.md) | **How to work in this repo.** Invariants, layout, conventions. Start here. |
| [STATUS.md](STATUS.md) | Where the project actually is right now. |
| [docs/00-vision.md](docs/00-vision.md) | Goals and — importantly — non-goals. |
| [docs/01-architecture.md](docs/01-architecture.md) | Layers, crate graph, the frame, scene-tree-vs-ECS. |
| [docs/02-tech-stack.md](docs/02-tech-stack.md) | Stack detail and rejected alternatives. |
| [docs/03-ai-native-design.md](docs/03-ai-native-design.md) | The differentiator. Determinism, reflection, introspection. |
| [docs/04-subsystems.md](docs/04-subsystems.md) | Every subsystem: job, open decisions, milestone. |
| [docs/05-roadmap.md](docs/05-roadmap.md) | Milestones M0–M6 with concrete exit gates. |
| [docs/06-open-questions.md](docs/06-open-questions.md) | What's still undecided, and priorities. |
| [docs/adr/](docs/adr/) | Architecture decision records. |

## Getting started

Nothing to build yet. Before M0 begins:

```bash
rustup toolchain install stable
```

Then read [docs/05-roadmap.md](docs/05-roadmap.md) § M0.
