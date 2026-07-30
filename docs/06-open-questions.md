# 06 — Open Questions

Check this before assuming any undecided thing. Resolve with an ADR, then move the entry to the
resolved section at the bottom.

Priority: **P0** blocks work now · **P1** needed for the current milestone · **P2** can wait.

---

## Q1 · P0 · How is game logic authored and hot-reloaded?

**The highest-priority unresolved question in the project.** It determines the edit→observe loop,
which determines how effective I can be as a collaborator. Everything else can be adjusted later;
this one shapes the whole engine's ergonomics.

The problem: Rust's compile times are the chosen stack's real weakness. If changing an enemy's speed
requires a 30-second engine rebuild, iteration dies — tolerable for a human, corrosive for an agent
doing twenty iterations to tune one behavior.

**Option A — Pure Rust, no scripting layer.**
Game logic is Rust systems in the game crate. Rely on small crates, `cargo check`, and `lld` to keep
rebuilds tolerable.
*For:* one language, full type safety, no API duplication, maximum performance, simplest engine.
*Against:* every gameplay tweak is a rebuild. Even 10s is painful at high iteration counts. No live
tweaking of a running game.

**Option B — Rust game logic as a hot-reloaded dynamic library.**
Game crate compiles to a `cdylib`; the engine reloads it and preserves world state across reloads.
*For:* keeps one language and full type safety; reload is seconds not tens of seconds.
*Against:* genuinely fiddly — state must survive the swap, no `#[repr(Rust)]` types across the
boundary, statics reset, debugging gets harder, and Windows DLL locking is its own adventure. Known to
be brittle in practice.

**Option C — Embedded scripting language.**
Gameplay in a scripting language, engine in Rust. Candidates: **Luau** (typed Lua, fast, sandboxed,
enormous training corpus via Roblox), Lua via `mlua`, or `rhai` (Rust-native, easy binding, slower and
much less well-known to me).
*For:* instant reload, live tweaking, sandboxing, easy to inspect. Typed Luau is genuinely pleasant
for gameplay and its ubiquity means I write it accurately.
*Against:* two languages. Every engine API must be bound and kept in sync — a classic engine tarpit.
Performance ceiling for hot logic. Type safety across the boundary is weaker.

**Option D — WASM as the game-logic boundary.**
Game logic compiles to WASM (from Rust now, other languages later); engine hosts it via `wasmtime`.
*For:* hot-swap a module cleanly, deterministic by spec, sandboxed, language-agnostic later, and the
same logic runs on native and web.
*Against:* boundary serialization cost, most complex to implement well, and ECS access across the
boundary needs careful design to not be slow.

**Recommendation:** run the spike in M0 and decide on measurements. My prior is **B or C, with a
leaning toward C (Luau) for gameplay plus the ability to "graduate" hot logic into Rust systems** —
that combination gives instant iteration where it matters and full performance where it matters, and
the graduation path means the scripting layer never becomes a performance ceiling.

**The spike must measure, for each option:** edit→observe latency, whether world state survives
reload, ergonomics of writing a non-trivial system, and how well the agent-facing schema story works.

---

## Q2 · P1 · Which concrete syntax for scene files?

Arguably the most user-visible decision in the project — it's the file both authors literally type
into.

- **RON** — Rust-native, expressive, handles enums well. Less familiar generally, and can get noisy.
- **TOML** — very familiar, excellent diffs, but nests poorly, and scenes are deeply nested.
- **KDL** — designed for exactly this shape (nodes with properties and children). Less tooling.
- **Custom** — perfect fit, full control over canonical formatting and error messages; costs us the
  parser, formatter, and editor support.

Constraints from I1/I2: hand-writable, deeply nestable, line-oriented enough for clean git merges,
stable ordering, canonical formatting.

**Resolve by:** writing the *same* moderately complex nested scene in all four, by hand, and judging
readability and diff behavior. Cheap experiment, high-value answer. Do this at the start of M1.
My prior: KDL or custom.

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

## Q5 · P1 · Fixed timestep rate?

60Hz is conventional and cheap. 120Hz gives better physics fidelity and input latency at 2x
simulation cost. Some engines decouple physics substeps from the logic tick.

**Changing this later invalidates every recorded replay**, so decide in M0 and write it down.
Prior: 60Hz logic tick, with configurable physics substeps.

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
