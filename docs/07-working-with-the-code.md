# 07 — Working With the Code

> Justin's map into the codebase. Written for someone comfortable programming but still learning Rust.
> **Claude: keep this current.** Every new architectural pattern gets a short entry with a worked
> example. It's a stated project requirement (`CLAUDE.md` §6), not documentation garnish.

Right now there's no code yet, so this file is mostly setup and orientation. It grows with the engine.

---

## Environment setup

### 1. Rust toolchain

Already installed (verified 2026-07-30): rustup, rustc 1.97.1, cargo 1.97.1, target
`stable-x86_64-pc-windows-msvc`. Lives in `%USERPROFILE%\.cargo\bin`.

### 2. MSVC build tools — required

Rust's `msvc` target uses Microsoft's linker, which does not ship with Rust:

```
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Without it, compilation succeeds and *linking* fails with `linking with link.exe failed`.

*Why the msvc target and not `gnu`?* The `x86_64-pc-windows-gnu` target bundles its own linker and
skips this download, but msvc has better support for Windows graphics APIs — which matters because
wgpu uses DX12 here. Not worth trading away for one install.

### 3. VS Code setup

Install the **rust-analyzer** extension. It gives inline types, jump-to-definition, inline errors, and
hover docs — the single biggest quality-of-life difference when reading unfamiliar Rust.

```
code --install-extension rust-analyzer
```

Also useful: **Even Better TOML** (for `Cargo.toml`), **CodeLLDB** (for debugging).

### 4. Known gotcha: stale PATH

Installers update the persistent PATH, but already-running processes keep the environment they started
with. VS Code's integrated terminal inherits from the VS Code process — so after installing a tool,
**restart VS Code itself**, not just the terminal tab. A `command not recognized` error right after a
successful install is almost always this.

### 5. Blocker: Smart App Control must be off to run anything we build

**Smart App Control blocks every binary this project produces.** It is reputation-based, and a
freshly compiled debug binary is unsigned and has no reputation, so it is blocked — regardless of
where it lives on disk.

```
error: An Application Control policy has blocked this file. (os error 4551)
```

Confirmed 2026-07-30 from the Windows event log
(`Microsoft-Windows-CodeIntegrity/Operational`): event **3118 "Smart App Control Block"** plus 3077,
policy ID `{0283ac0f-fff1-49ae-ada1-8a933130cad6}`. It blocked both a test binary in the project's
own `target/debug/deps` and `clippy-driver.exe` inside the rustup toolchain.

> **An earlier version of this section claimed the block was specific to temp directories. That was
> wrong.** A single hello-world binary happened to pass, which suggested a location rule that does
> not exist. SAC decides per-binary on reputation. Recorded here so nobody re-derives the wrong
> conclusion from the same partial evidence.

**What works and what doesn't under SAC:**

| Command | Status |
|---|---|
| `cargo check` | works — compiles without executing |
| `cargo build` | works — compiles and links |
| `cargo test` | **blocked** — must execute the test binary |
| `cargo clippy` | **blocked** — `clippy-driver.exe` is blocked |
| running the engine | **blocked** |

**There is no workaround.** SAC has no exclusion list (unlike Defender). Code signing does not help
without a reputable certificate *and* accumulated reputation, which a per-build debug binary can never
have. SAC is designed for machines that only run mainstream reputable software; it is fundamentally
incompatible with compiling your own.

**The fix is to turn Smart App Control off**, which only Justin can do:
Windows Security → App & browser control → Smart App Control → **Off**.

⚠️ **This is a one-way change.** Once off, re-enabling SAC requires reinstalling Windows. Defender
antivirus, SmartScreen, and UAC are unaffected and remain active — this removes one extra layer, and
leaves the configuration that the large majority of developer machines run.

**Claude must never change this setting**, or any other security setting. Surface it and let Justin
decide.

---

## Everyday commands

None of these work until M0 scaffolds the workspace. Recorded here so they're in one place.

| Command | What it does |
|---|---|
| `cargo check` | Type-checks without producing a binary. **Fastest feedback — use this while iterating.** |
| `cargo build` | Compiles a debug binary. Slower. |
| `cargo build --release` | Optimized. Much slower to compile, much faster to run. Use for perf testing. |
| `cargo run -p <crate>` | Builds and runs a specific crate in the workspace. |
| `cargo test` | Runs all tests, including determinism and golden-replay tests. |
| `cargo clippy` | Lints. Catches real bugs, not just style. Worth running before committing. |
| `cargo fmt` | Auto-formats. Never argue about formatting. |
| `cargo doc --open` | Builds and opens the API docs generated from doc comments. |
| `cargo run -p quad-demo` | Runs the demo game in a window. WASD or arrows to move, Escape to quit. |

**Asking the engine about itself.** `amadeo` is built with `cargo build -p amadeo-cli` and lands at
`target/debug/amadeo`. Run it from anywhere inside the project — it finds `amadeo.toml` by walking
up, the way `cargo` and `git` do.

| Command | What it does |
|---|---|
| `amadeo status --ticks 600` | tick, state hash, and what is registered, after 600 ticks |
| `amadeo describe` | the whole component schema, as JSON |
| `amadeo describe Velocity` | one type: fields, units, ranges, docs |
| `amadeo query Transform Player` | entities carrying **all** of those components |
| `amadeo entity 5` | one entity's components and values |
| `amadeo schedule Simulation` | systems in resolved execution order |
| `amadeo call <method> --params '{...}'` | any protocol method, so the CLI never lags the protocol |
| `amadeo check <file>...` | validate scene files against the game's real schema |
| `amadeo replay <file>` | replay a recording in a fresh process and verify its checkpoint hashes |
| `amadeo fmt <file>...` | rewrite scene files canonically; `--check` reports instead of fixing |

**`fmt` and `check` are different questions.** `fmt` asks "is this file written canonically" and is
pure syntax, so it runs in the CLI. `check` asks "would this scene load" — which means knowing that
`Transform` exists and has a `translation` field — so it launches the game. A file can be perfectly
formatted and still name a component nobody registered.

Everything except `fmt` compiles and launches the game, because a game's components are Rust types in
the game binary — see the pattern write-up below and `docs/protocol/v1.md`. `--compact` prints one
line of JSON instead of indented.

**Seeing something on screen.** `quad-demo` is the only thing that opens a window today. If you want
to check that a rendering change works, that is where to look — and note the first build after
touching anything GPU-related takes minutes, because it compiles wgpu.

**Reading a Rust error message.** rustc errors are unusually good — the useful part is usually below
the first line. Look for `help:` and `note:`; they frequently contain the exact fix. `cargo build`
also often ends with a suggested command like `rustc --explain E0502`, which gives a full explanation
of that error class.

---

## How to read this codebase

**Start with the crate graph** in `CLAUDE.md` §4. Crates are listed in dependency order, and a crate
may only depend on ones above it. That ordering is also a reading order: `amadeo-math` and
`amadeo-core` are small and self-contained; `amadeo-ecs` is where the interesting parts begin.

**Find behavior by looking for systems, not methods.** This engine has no `Player` class with an
`update()` method. Instead, there are components (plain data — `Transform`, `Velocity`) and systems
(functions that query and mutate them — `integrate_velocity`). To answer "what makes the player move,"
search for systems that read the relevant components. This is the biggest mental shift if you're
coming from Unity or Godot, and it's explained in `adr/0004`.

**Naming tells you what a thing is:**
- Components are nouns: `Transform`, `Velocity`, `Sprite`
- Systems are verb phrases: `integrate_velocity`, `resolve_collisions`
- Events are past tense: `EntitySpawned`, `DamageDealt`

---

## Rust patterns used in this engine

*Grows as the engine is built. Each entry: what it is, why it's here, minimal example.*

### `Result` and `?` — how errors travel
Rust has no exceptions. A function that can fail returns `Result<T, E>`, and `?` means "if this
failed, return that error to my caller." Engine crates return typed errors (`thiserror`); the CLI and
games use `anyhow`, which is more convenient but less specific.

```rust
fn load_config(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path)?;   // ? propagates a read failure upward
    let config = parse(&text)?;
    Ok(config)
}
```

You will not see `unwrap()` or `expect()` in engine crates outside tests — those crash on failure, and
`CLAUDE.md` §6 forbids them here. If you see one in engine code, it's a bug.

### Type erasure with `Any` — how one table holds columns of different types

An archetype needs to store a `Vec<Position>` next to a `Vec<Velocity>` in the same list. Rust has no
"list of different types", so the columns are stored as `Box<dyn Column>` (a trait object) and
recovered with a **downcast**:

```rust
// Stored erased...
let column: &Box<dyn Column> = &self.columns[index];
// ...and recovered as the concrete type.
let typed: &TypedColumn<Position> = column.as_any().downcast_ref::<TypedColumn<Position>>()?;
let values: &[Position] = typed.values();   // a plain contiguous slice
```

`downcast_ref` returns `Option` because the cast is checked at runtime — it fails if the type is wrong
rather than misinterpreting memory. That check is why this approach needs no `unsafe`.

The `as_any()` step exists because a trait object cannot be downcast directly; it has to be turned
into `&dyn Any` first. It looks like boilerplate and is genuinely required.

**Why this is fast anyway:** the downcast happens once per archetype per query, not once per entity.
A query over 10,000 entities in 5 archetypes does 5 downcasts, then iterates plain slices. See
ADR 0008.

### `get_disjoint_mut` — two mutable borrows from one slice

A system like "move each position by its velocity" needs `&mut` to one column and `&` to another, at
the same time, from the same `Vec`. The borrow checker cannot prove two dynamic indices differ, so it
refuses the obvious code.

`slice::get_disjoint_mut` does that check at runtime:

```rust
// Returns Err if the indices overlap, so aliasing is impossible.
let [slot_a, slot_b] = self.columns.get_disjoint_mut([index_a, index_b]).ok()?;
```

This is the single trick that makes multi-component mutable queries work without `unsafe`. If you see
it, that is what it is for.

### Generational indices — catching use-after-free without a crash

`Entity` is an index plus a generation counter. Despawning bumps the generation, so an old handle no
longer matches:

```rust
let entity = world.spawn();   // index 0, generation 0
world.despawn(entity);        // slot 0 is now generation 1
let reused = world.spawn();   // index 0, generation 1 -- same slot, new identity

world.get::<Position>(entity)  // None: generation 0 != 1
```

Without the generation, the stale handle would silently address whoever occupies the slot next —
producing wrong behaviour rather than an error. This pattern shows up throughout the engine for any
handle into a reusable slot.

### `BTreeMap` instead of `HashMap` — determinism, not preference

Anywhere iteration order can influence simulation results, the engine uses `BTreeMap`/`BTreeSet`.
`HashMap` iteration order varies between runs and builds, which would make state hashes disagree and
silently void every golden replay test (invariant I3).

`HashMap` is fine outside the deterministic zone — asset caches, editor state, tooling. Inside it, it
is a bug.

### Closures for mutable iteration, iterators for reads

```rust
// Read: a normal iterator.
for (entity, position) in world.iter::<Position>() { }

// Write: a closure.
world.for_each_pair_mut::<Position, Velocity>(|entity, position, velocity| {
    position.x += velocity.x;
});
```

Returning an *iterator* that yields `&mut` from multiple columns requires higher-ranked lifetime
bounds and a hand-written iterator type — a lot of hard-to-read Rust for a modest ergonomic gain. The
closure form does the same job in a way you can read. This is a deliberate call under `CLAUDE.md` §6.

### `world.query` — asking for several components, some of them optional

The general read query. A query is a **tuple of terms**, and each term says what you want:

```rust
// Every entity with a Transform and a Sprite, plus its SortOrder and GlobalTransform if it has them.
for (entity, (transform, sprite, order, global)) in
    world.query::<(&Transform, &Sprite, Option<&SortOrder>, Option<&GlobalTransform>)>()
{
    let layer = order.copied().unwrap_or_default().order;   // absent -> 0
}
```

| Term | Means |
|---|---|
| `&T` | **required** — an entity without it is not in the results |
| `Option<&T>` | **optional** — included when present, `None` when not, never a reason to exclude |

Up to eight terms. Use it for anything that reads; use `for_each_pair_mut` and friends to write.

**Why optional matters more than it sounds.** Requiring a component means an entity *silently
disappearing* from a query when someone forgets to add it — which is a horrible first failure,
because nothing errors, the entity just stops being processed. So most things a system reads should
be optional, with a sensible default when absent.

**Why this is fast, and why the old way was not.** Before this existed, a system wanting an optional
component asked for the required ones and then called `world.get::<T>(entity)` inside the loop. That
looks harmless and is not: an archetype ECS stores components in columns, and the whole point is to
find a column *once* and then walk it. A per-entity lookup throws that away. The renderer was doing
40,000 of them per frame; removing them took sprite collection from 3.32 ms to 2.58 ms, on top of a
separate fix that had already taken it from 5.13 ms.

**If you find yourself calling `world.get` inside a loop over a query, that is the smell.** Add the
component to the query as an `Option<&T>` instead.

The implementation (`crates/amadeo-ecs/src/query.rs`) is the one place in the ECS with a trait plus a
code-generating macro, which is against this project's usual preference for boring Rust. ADR 0025
records why, and the module's own docs explain each piece of the machinery next to the code. You
should not need to read any of it to *use* a query.

### `Resource` vs `Service` — the split that keeps replays honest

Both are single-instance globals living in the `World`, and both are reachable from a system. The
difference is whether they count as simulation state:

```rust
// Simulation state: hashed, so it participates in replay assertions. Requires StableHash.
impl Resource for SimRng {}

// Engine machinery: NOT hashed. Deliberately does not require StableHash.
impl Service for RenderCount {}
```

The rule: if two runs disagreeing about it means they have *diverged*, it is a `Resource`. If it is
machinery — a GPU device, an asset cache, a frame counter — it is a `Service`.

The compiler enforces the important direction. `Service` does not require `StableHash`, and `Resource`
does, so a `wgpu::Device` simply cannot be filed as a resource. Full reasoning in ADR 0009, including
how a failing test found this gap.

Watch the *other* direction, which the compiler cannot catch: filing genuine simulation state as a
`Service` would silently exclude it from replay assertions.

### `with_resource_taken` — using a resource and the world at once

A system often needs a resource *and* to iterate entities: "for each enemy, roll against the shared
RNG". Holding `&mut` to the resource borrows the whole world, so the query cannot run.

```rust
world.with_resource_taken::<SimRng, ()>(|world, rng| {
    world.for_each_mut::<Velocity>(|_entity, velocity| {
        velocity.x += rng.0.range_f32(-0.01, 0.01);
    });
});
```

It lifts the resource out for the duration and puts it back afterwards. Slightly unusual, and the
straightforward alternative to interior mutability or `unsafe`.

### System ordering is alphabetical, not registration order

```rust
app.add_system(Stage::Simulation, system("jitter", jitter).after("bounce"));
app.add_system(Stage::Simulation, system("integrate", integrate));
app.add_system(Stage::Simulation, system("bounce", bounce).after("integrate"));
// Runs: integrate, bounce, jitter
```

Constraints are resolved by topological sort. **Systems with no constraint between them run in
alphabetical label order**, never in the order they were registered — registration order depends on
how the app was assembled, and letting that decide execution order would make results depend on
plugin setup (invariant I3).

The consequence to know: adding an unconstrained system can shift the relative order of other
unconstrained systems. If order matters, say so with `before`/`after`.

### The `gpu` feature — why the GPU backend is opt-in

`amadeo-render` builds and tests **without wgpu by default**:

```bash
cargo test --workspace                                  # no GPU code compiled at all
cargo check -p amadeo-render --features gpu             # adds wgpu, ~200 crates
```

The abstraction and `NullBackend` have no GPU dependency, which is what invariant I7 actually needs —
headless is how tests run, how CI runs, and how a dedicated server will work. Making the real backend
opt-in keeps the everyday loop fast; a full `cargo test --workspace` stays a couple of seconds instead
of minutes.

CI compiles the `gpu` feature so it cannot silently rot, but the determinism job deliberately does
not: rendering is a `Service` and cannot affect the state hash, so building wgpu three times over
would add runtime and no coverage.

**A note on wgpu versions.** wgpu makes breaking changes across major versions and this project is on
**wgpu 30**. Much of what you find online targets older versions and will not compile. Three things
that moved recently and cost time to rediscover:

- `Instance::new` takes the descriptor **by value**, built via
  `InstanceDescriptor::new_without_display_handle_from_env()` — there is no `Default`.
- `Surface::get_current_texture` returns a `CurrentSurfaceTexture` **enum**, not a `Result`. Several
  variants (`Outdated`, `Lost`, `Timeout`, `Occluded`) mean "skip this frame", not "fail".
- Presentation is `Queue::present(texture)`, not a method on the texture.

If you need an API shape, read the source in
`~/.cargo/registry/src/*/wgpu-30.0.0/src/api/` rather than trusting a search result. It is faster and
it is definitive.

### Deferred commands — changing structure from inside a query

You cannot spawn or despawn while a query is running. The borrow checker rejects it, and it is right
to: removing an entity mid-iteration would reorder the rows being walked.

Queue the change instead:

```rust
world.with_service_taken::<Commands, ()>(|world, commands| {
    world.for_each_mut::<Health>(|entity, health| {
        if health.0 <= 0.0 {
            commands.despawn(entity);       // queued, not applied
        }
    });
});
// The app flushes after every stage; nothing to call by hand in a normal system.
```

Spawning takes a closure, because the new entity does not exist until the flush:

```rust
commands.spawn_with(|world, entity| {
    world.insert(entity, Position { x: 0.0, y: 0.0 });
});
```

**The limitation to know:** a spawned entity's handle is not available to other commands in the same
batch, so you cannot make two newly spawned entities reference each other until the next flush. Rare
in practice; if it stops being rare, the design gets revisited.

Commands apply in the order they were queued, which with a single-threaded schedule is fully
determined by system order — so no extra sorting is needed for determinism.

### Writing a gameplay system

Game logic is **plain Rust in the game crate**. There is no scripting language and no hot reload;
ADR 0011 settled that by measurement, and `spikes/q1-game-logic/README.md` has the numbers. In
practice a gameplay system is just a function, exactly like `integrate` in `quad-demo`.

A behaviour system usually wants three components at once — update some state, set a movement value,
and read a position to decide both. That is `for_each_triple_mut`, which **writes the first two and
reads the third**:

```rust
fn enemy_ai(world: &mut World) {
    // A resource and the entities, in the same pass. `with_resource_taken` lifts the RNG out
    // so the query can still borrow the world -- see the entry above.
    world.with_resource_taken::<SimRng, ()>(|world, rng| {
        world.for_each_triple_mut::<Enemy, Velocity, Transform>(
            |_entity, enemy, velocity, transform| {
                *velocity = decide(enemy, transform.position, rng);
            },
        );
    });
}
```

**Order the type parameters by how you use them, not by importance.** The two written components come
first, the read-only one last. Getting it wrong is a compile error only if the types differ in
mutability needs — otherwise it silently marks the wrong component as changed, which quietly degrades
change detection for every other system.

**Why `decide` is a separate function.** Keeping the branching logic out of the query closure makes it
a pure function of its inputs, so it can be tested without building a world. Worth doing for anything
more complicated than a single assignment. `spikes/q1-game-logic/a-rust/src/ai.rs` is a worked
example — though note it predates `for_each_triple_mut` and uses the older collect-and-write-back
workaround below, because a spike is frozen once its ADR is written.

**If you need four components, or a different mutability split**, the fallback is collect, decide,
write back:

```rust
let enemies: Vec<(Entity, Enemy, [f32; 2])> = world
    .iter_pair::<Enemy, Transform>()
    .map(|(entity, enemy, transform)| (entity, *enemy, transform.position))
    .collect();

for (entity, mut enemy, position) in enemies {
    // ...decide...
    if let Some(slot) = world.get_mut::<Enemy>(entity) { *slot = enemy; }
}
```

It costs an allocation and a location lookup per write. Prefer a real query; extend the query API when
a real system needs a shape it does not have, rather than speculatively.

### Defining a component: two derives, and why both

```rust
use amadeo_core::StableHash;
use amadeo_reflect::Reflect;

/// How much damage something can take.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Health {
    /// Current hit points.
    #[reflect(min = 0.0, max = 100.0, unit = "hp", sync = "on_change")]
    current: f32,
    /// Recomputed every tick, never saved.
    #[reflect(skip)]
    cached_ratio: f32,
}
```

**`StableHash` makes it participate in golden replays.** Write this by hand and forget a field and
the code still compiles, still runs, and still produces a plausible number — while quietly excluding
part of the simulation from every replay assertion. The tests keep passing and stop testing. That is
why it is derived.

**`Reflect` makes it visible to the editor and the agent** (invariant I8). Without it, the type
exists at runtime and nowhere else: it cannot be saved, inspected, or edited. Both are **required**
by the `Component` trait, so forgetting one is a compile error rather than a hole you find in M4
(ADR 0013).

**The `#[reflect(...)]` vocabulary**, all optional:

| | |
|---|---|
| `min` / `max` | advisory bounds — editor sliders, and a hint about what a sane value is. Not enforced on load. |
| `unit = "m/s"` | stops a whole class of "plausible but wrong", like passing degrees to a radians field |
| `sync` / `interpolate` | multiplayer annotations reserved by ADR 0006. Do nothing until M6. |
| `skip` | not authoritative state. Excluded from the schema, the saved value, **and** the hash. |

Doc comments are not decoration here — they are what an agent reads to understand a field without
the source. Full reasoning in ADR 0012.

**On a reflected type, `///` and `//` now mean different things.** A `///` comment is printed
verbatim by `amadeo describe`, so it is read by someone — or something — that has never seen the file
and wants to know what the type is *for*. Implementation history is noise there:

```rust
/// Where an entity is in 2D space.
///
/// Position is in world units, rotation in radians counter-clockwise.
// Moved here from `amadeo-render` by ADR 0015 — true, useful to a maintainer, and pure noise in a
// schema dump. So it is `//`, not `///`.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Transform { /* ... */ }
```

Rule of thumb: **`///` answers "how do I use this", `//` answers "why is it like this".** Both are
worth writing; only the first is worth shipping in the schema.

**A gotcha worth knowing:** the trait and its derive share a name, so `use amadeo_core::StableHash;`
imports both. That looks like it should be a conflict and is not — Rust keeps macros and types in
separate namespaces, and `Debug` works the same way.

### The scene format, in one screen

Decided in ADR 0014 after hand-writing the same scene in four syntaxes (`spikes/q2-scene-format/`).
Indentation is **two spaces per level**, and a line's first word says what it is:

```text
scene corridor_a
version 1

entity a1 "Corridor"           # entity <id> "<name>"
  Transform                  # a component
    position 0.0 0.0           # a field; several values on a line is a list
    rotation 0.0

  entity a2 "CeilingLight"     # indented under a1, so it is a1's child
    PointLight
      color 1.0 0.85 0.6

  entity a3 "Door" from door_metal
    override Door              # `from` names an ASSET ID, not a path (ADR 0029)
      locked true              # only the fields you name change; the rest come from the prefab

  entity a4 "Wanderer"
    Enemy
      state Patrol             # a bare word is an identifier / enum variant
      label "Patrol"           # a quoted word is a string. Different things.
      waypoints
        - 0.0 0.0              # a list whose elements are themselves lists
        - 4.0 0.0
      mood Chasing             # a variant CARRYING data: the fields go beneath it
        speed 4.0
      tuning                   # no value on the line, and no `- ` beneath: a nested struct
        aggression 0.8
        patience 0.2
```

**Three rules worth internalising:**

- **Bare words are identifiers, quoted words are strings.** `state Patrol` sets an enum variant;
  `label "Patrol"` sets text. The file says which, so no schema is needed to tell them apart.
- **`1` is an integer, `1.0` is a float.** The decimal point is what carries the type.
- **An indented block is a list if its lines start with `- `, and named fields otherwise** (ADR 0032).
  That one rule covers nested structs, maps, and enum payloads, and it is YAML's rule — deliberately,
  since it is the one people already know. Nesting goes as deep as the indentation does.

**What still has no spelling:** `Option::None`, and anything *empty* used as a field value — an empty
block is a parse error, so an empty struct, map or list cannot be written. Both are recorded in
ADR 0032 rather than being oversights. Nothing in the engine has an `Option` field yet.

**Canonical form** (`amadeo fmt`): fields sorted by name, components sorted by name, **children in
declaration order** — siblings are a sequence and their order is meaningful, so sorting them would
destroy information.

**Indentation is structure, so `amadeo fmt` cannot repair it.** A mis-indented line is ambiguous
rather than untidy, and you get a line-numbered error instead of a guess. Tabs are rejected outright.

**Two layers.** `amadeo-scene` is two layers stacked, and they are kept apart on purpose. Layer 1 is
syntax only: it will happily parse a scene naming a component that does not exist, which is what lets
`amadeo fmt` work on a file whose module is not loaded. Layer 2 checks that `Transform` exists and
has a `translation` field, against the reflection registry — that is `validate` (which is what
`amadeo check` runs) and `instantiate`. A syntax error and a schema error are different things with
different messages rather than one confusing pile.

### Cameras: a camera is an entity, and nothing draws without one

New in session 8 (ADR 0031). A camera is a `Camera` component beside a `Transform`, and a world may
hold any number. **A world with no camera draws nothing** — the screen is cleared and that is all.
That trips people once: if you build a world by hand in a test and assert on what was drawn, spawn a
camera or every assertion passes vacuously.

```text
entity eye "Camera"
  Camera
    active true
    order 0
    projection Orthographic
      height 8.0
    target ""
    viewport 0.0 0.0 1.0 1.0
  Transform
    rotation 0.0 0.0 0.0
    scale 1.0 1.0 1.0
    translation 0.0 0.35 0.0
```

**The position is on the `Transform`, not on the `Camera`.** That is ADR 0018's one-transform rule,
and it pays off immediately: parenting a camera to a character *is* a follow camera, with no special
case and no code.

| Field | |
|---|---|
| `projection` | `Orthographic { height }` or `Perspective { fov, near, far }` — each variant carries only what it needs. Nothing draws through a perspective camera yet; the mesh pass is later in M2. |
| `target` | **empty means the window.** Anything else is a texture asset id. |
| `viewport` | `[x, y, width, height]` in `0..1` of the target. A left half is `0.0 0.0 0.5 1.0`. |
| `order` | low draws first. Only the first camera clears, so a higher-order camera composes *over* a lower one rather than erasing it. |
| `active` | `false` keeps a camera configured but idle. |

**The projection carries its own parameters**, so a perspective camera has no `height` at all rather
than a meaningless one — `Projection::height()` returns `None` for it. It was flat for exactly one
commit, because the scene format could not express an enum with a payload until ADR 0032.

**Asking what is on screen.** `render.describe` answers for the active orthographic camera with the
lowest `order` drawing to the window. For any other — a minimap, a security monitor, the editor's
viewport — use `describe_frame_through(&world, camera_entity)`, which returns `None` if that entity
is not a camera rather than quietly answering about a different one.

### Prefabs: write the thing once, place it many times

New in session 8 (ADR 0029). A prefab is **a scene file with exactly one root entity**, used as a
stamp. `games/vault/assets/prefabs/sigil_pickup.scene` is one:

```text
scene sigil_pickup
version 1

assets
  sigil

entity root "Sigil"
  Sigil
  Sprite
    color 1.0 1.0 1.0 1.0
    region 0.0 0.0 1.0 1.0
    size 0.8 0.8
    texture "sigil"
  SortOrder
    order 10
  Transform
    rotation 0.0 0.0 0.0
    scale 1.0 1.0 1.0
    translation 0.0 0.0 0.0
```

and the level places six of them, three lines each:

```text
entity sigil_nw "Sigil NW" from sigil_pickup
  override Transform
    translation -4.0 2.0 0.0
```

**Three things to know, and the third is the one people trip on.**

**`from` names an asset id, not a path.** `sigil_pickup`, never `assets/prefabs/sigil_pickup.scene`.
So a prefab needs a `.ama-meta` sidecar like any other asset, `amadeo check` will offer "did you
mean" on a typo, and moving the file breaks nothing. The `from` line is *itself* the declaration —
you do not also list the prefab in the `assets` block.

**An override is a patch, and only the top level merges.** `override Transform` with just
`translation` keeps the prefab's `rotation` and `scale`. But a field whose value is itself a struct
is replaced whole — recursive merging would make "how much did that line actually change?"
unanswerable by looking at it.

**An override can only touch the instance root.** There is deliberately no syntax for reaching a
prefab's children. If you want one child different, you make a variant prefab. This looks like a
missing feature and is the load-bearing decision in the whole design: Unity has that syntax, and
Unity's overrides silently evaporate under nesting because an override that names something *inside*
a prefab has to keep track of that thing across every future edit of the prefab. Here there is
nothing to keep track of, so nesting prefabs inside prefabs is safe by construction rather than by
care.

Two shapes on an instance, and mixing them up is an error rather than a surprise:

| You write | Meaning | If you get it wrong |
|---|---|---|
| `override Foo` | replace what the prefab put on the root | prefab has no `Foo` → refuses to load |
| `Foo` | add something the prefab lacks | prefab already has `Foo` → refuses to load |

That second row exists because if a bare `Foo` silently overrode, a typo'd `override` keyword would
change behaviour with nothing to see in the diff.

**A prefab edit can break every scene that uses it.** Delete a component from a prefab and every
instance overriding it refuses to load, naming the entity, the component, and the prefab. That is
bought deliberately: `amadeo check` lists them all before anything runs, which is much better than
Unity's version where the override quietly reverts and you find out months later.

**In code**, a scene with prefab instances needs a `PrefabLibrary` — a map from id to parsed
document. `App::load_scene` builds one for you out of the resident asset bytes, so a game just calls
that. `instantiate_with(&document, &registry, &prefabs, &mut world)` is the layer underneath, and it
does not touch the filesystem, because resolving an id to a file is asset work and `amadeo-scene`
sits below `amadeo-assets` (I6).

**Adding a new prefab needs `--assets`:**

```bash
amadeo import --assets games/vault/assets
```

A prefab is an asset, so it needs a `.ama-meta` sidecar before the game will start — and `amadeo
import` normally launches the game to find the asset directory, which it cannot do while a sidecar is
missing. Naming the directory breaks that cycle (Q19). Plain `amadeo import` is still right whenever
the game *does* start, and is authoritative because the path comes from the game's own source.

### Pattern: the game binary hosts the agent, and the CLI launches it

The one that explains why `amadeo describe` is not a normal CLI command.

**The problem.** ADR 0011 settled that game logic is plain Rust compiled into the game binary. So a
game's components are Rust types that live in `games/mygame/`. The `amadeo` binary is compiled
separately and has *never linked them* — it cannot construct one, describe its fields, or read its
values. A standalone CLI could only ever describe the engine's own types, which is the least
interesting half of the answer.

**The shape (ADR 0016).** Three pieces, in three crates, each where the dependency order forces it:

```text
amadeo-cli     `amadeo describe Player`
                 └─ spawns: cargo run -p quad-demo -- --amadeo-agent --ticks 300
                            │
games/quad-demo             ├─ main() calls serve_if_requested(&mut app)
                            │
amadeo-app                  ├─ serve(): reads stdin, routes, writes stdout
                            │           answers schedule.list and sim.status itself
amadeo-agent                └─ dispatch_world(): describe, world.query, world.entity
                                       plus Request/Json — the protocol itself
```

The split between the last two is **not** taste. `amadeo-agent` sits above `amadeo-app` in the crate
order (`CLAUDE.md` §4), so it may not reach down for `App`. It therefore owns the protocol, and
`amadeo-app` owns the hosting. Adding a method that needs only a world goes in `amadeo-agent`; one
that needs the schedule or the tick loop goes in `amadeo-app`. A client never sees the seam — an
unknown method lists both halves in one list.

**What a game has to do.** Two things, both small:

```rust
// 1. Register the components you use, so `describe` can see them and scenes can name them.
app.register_component::<Transform>()?;
app.register_component::<Velocity>()?;

// 2. Hand over, before any window exists.
if amadeo_app::serve_if_requested(&mut app)? {
    return Ok(());
}
```

Registration is the part that is easy to forget and silent when forgotten: an unregistered component
works perfectly at runtime and is simply invisible to `describe`. That is why the registry lives on
`App` rather than being built separately — one object, one place to register.

**Why one invocation is one fresh run.** How far to simulate is `--ticks` at launch, never a method.
So every question in a session sees the same world, and the same command twice gives the same answer
twice:

```bash
amadeo status --ticks 600
```

That reproducibility is the point. A question an agent asks is a question it can put in a test —
which is not true of poking at an attached, already-running process.

**Two rules that will bite if broken.**

1. **Never `println!` in a game outside agent mode's control.** stdout *is* the protocol. Diagnostics
   go to `eprintln!`. The CLI reports a game that prints to stdout as sending "something that is not
   JSON", which is the right error but an annoying one to chase.
2. **The CLI goes through `cargo run`, deliberately**, so a stale binary is rebuilt rather than
   answering with a schema for code that no longer exists. That costs a second or two and is the
   whole reason a cached schema file was rejected.

The full method list is `docs/protocol/v1.md`.

### How a texture gets from a file onto the screen

Worth following once end to end, because it crosses four crates and each one deliberately refuses to
do the next one's job.

```
assets/textures/wall_concrete.ppm      a file
  + wall_concrete.ppm.ama-meta         declares  id = "wall_concrete"        (ADR 0020)
        |
        |  amadeo-assets: scan + load. Reads BYTES. Does not look inside them.
        v
  Assets.store["wall_concrete"] = Vec<u8>
        |
        |  amadeo-image: decode(). Picks the format by sniffing the leading
        |  bytes, not the extension, and normalises everything to RGBA8.
        v
  TextureData { width, height, format: Rgba8UnormSrgb, pixels }
        |
        |  amadeo-render: TextureCache holds it, keyed by the same id.
        v
  Sprite { texture: "wall_concrete", .. }  ->  collect_sprites  ->  SpriteBatch
        |
        |  WgpuBackend::upload_texture: one wgpu::Texture + one bind group, once.
        v
  one draw call per batch
```

**The five things worth knowing about that chain:**

1. **The id is the same string the whole way.** A scene file, `amadeo assets`, a `world.entity` dump,
   and the GPU's texture table all use `wall_concrete`. There is no resolution step where two of
   them could disagree.
2. **`amadeo-assets` deliberately does not decode.** Image formats are per-kind knowledge, and that
   crate has none. This is why `amadeo-image` exists as a separate crate rather than a module.
3. **`TextureCache::get` never fails.** Missing, unloaded, or corrupt all fall back — first to an
   asset called `placeholder`, then to a magenta check built in code. The code fallback is the one
   that matters: a placeholder that is itself a file cannot cover "files are the problem".
4. **A fallback is always reported.** `TextureCache::failures()` says which ids fell back and why. If
   the screen is magenta, that list is the answer, and it is what `render.describe` will surface.
5. **None of it can move a replay.** `Assets` and `TextureCache` are both `Service`s, which
   `World::state_hash` excludes by trait bound (ADR 0009). This is not a convention anyone has to
   remember — it is enforced by the type system, and `sprite_textures.rs` asserts it by running the
   same world with the texture present and absent and getting the same hash.

**If a sprite is not showing up**, in the order worth checking:

```bash
amadeo assets          # is the id catalogued, and did its file load?
```

Then: is the id in the scene's `assets` block (nothing loads that is not declared)? Is
`TextureCache` installed as a service? Is `render_quads` registered in the `Render` stage? A sprite
whose texture failed still draws — as a magenta check — so a *completely* invisible sprite is a
transform, camera, or sort-order problem rather than a texture one.

### The render graph: how a frame decides what order to do things in

New in session 9. `crates/amadeo-render/src/graph.rs`, and ADR 0034 decides what it is allowed to be.

**The vocabulary first**, because three words do most of the work:

- A **pass** is one step of drawing a frame: "draw what camera 0 sees", "blur the bright parts",
  "put the finished picture on the screen".
- A **resource** is an image a pass reads or writes.
- A **transient** is a resource that only exists inside one frame. Scratch paper, not a saved file.

A **render graph** is the *declaration* of those passes and what each one touches. It works out the
order itself: if one pass writes an image another pass reads, the writer has to go first. Before
this, `WgpuBackend::render` was a long function where that order was spelled out by hand, and moving
two lines would have silently drawn the wrong picture.

What a frame looks like today:

```
  transient "scene"  (destination-sized, always RGBA8 sRGB)

  view 0     writes scene      clears first
  view 1     writes scene      loads what view 0 left    <- so a HUD composes over the world
  present    reads  scene      writes destination        <- one full-screen triangle
```

**Five things worth knowing:**

1. **The graph does no drawing and knows nothing about wgpu.** It is a plan. `WgpuBackend` executes
   it; `NullBackend` compiles it, throws it away, and reports the labels through
   `NullBackend::last_passes()`. So a pass-ordering bug is catchable on a machine with no GPU, which
   is what invariant I7 asks of every subsystem.
2. **Nothing outside the crate can add a pass.** ADR 0034. `RenderBackend` isolating rendering
   completely is what made three earlier renderer decisions cheap to revisit, and a public graph
   gives that up permanently — Bevy made theirs public and has rewritten it repeatedly. Games get to
   configure a *look* through reflected data instead.
3. **The cameras draw into `scene` rather than straight at the window**, for two reasons. It is where
   post-processing will be inserted, since a pass cannot read the image it is writing. And it is what
   lets a **windowed** run capture at all: a window's own image can never be read back, but a
   transient can.
4. **`scene` is always RGBA, never the destination's format.** A window surface is frequently BGRA.
   If the transient copied that, a capture would come back with red and blue swapped on the windowed
   path only. The present pass is the single place the destination's format is met, and the hardware
   converts while writing.
5. **Ties are broken by declaration order, not alphabetically** — the opposite of `Schedule`
   (see above). A schedule's registration order is accidental, so letting it decide would be a trap;
   a graph's declaration order is the order the frame is composed in, and two passes writing the same
   image *depend* on it. Both are deterministic, which is the property that actually matters.

**Adding a pass** means adding a `PassKind` variant, declaring it in `graph::frame_graph`, and
handling it in `WgpuBackend::render`'s match. The compiler finds the second and third for you once
you have done the first, which is why `PassKind` is an enum rather than a trait object.

### How a look gets from a file onto the screen

New in session 9, and worth following once because it crosses four crates in the *opposite*
direction from the texture chain above.

```
assets/looks/corridor_dark.environment      a scene file with one root
  + .ama-meta declaring  id = "corridor_dark"        (ADR 0020)
        |
        |  amadeo-assets: scan + load. Reads BYTES.
        v
  amadeo-app: parse, find the root's `Environment`, Reflect::from_value
        |
        v
  EnvironmentCache["corridor_dark"]              a Service, so never hashed
        |
        |  amadeo-render: render_quads resolves `Camera::environment`
        v
  View { environment, .. } -> FrameData::look() -> the post pass's uniform
        |
        v
  post.wgsl: exposure, tonemap, grade, vignette
```

**Four things worth knowing:**

1. **`amadeo-app` does the parsing, not the renderer**, and this is the interesting bit. An
   environment's file is a *scene* file, and `amadeo-scene` sits **above** `amadeo-render` — so by
   invariant I6 the renderer cannot read its own asset. It owns the type and the cache; the layer
   that can see both crates does the reading. `App::load_environments` is called automatically by
   `load_scene`, and is public for a game that spawns its camera in code.
2. **Nothing here can move a replay.** The camera's `environment` *id* is authored data and is in the
   state hash; the look behind it lives in a `Service`, which ADR 0009 excludes by trait bound. So
   whether the file loaded is invisible to the simulation (ADR 0021), and an id that never resolves
   renders with the default look rather than failing.
3. **The default look is a byte-identical no-op.** Verified rather than assumed: capturing the Vault
   through the HDR path produces the same PNG, byte for byte, as before post-processing existed.
   `the_default_environment_leaves_the_picture_alone` in `tests/capture.rs` is the standing version.
4. **The order of the effects is the engine's**, not content's — ADR 0034 §4. Exposure scales light,
   bloom needs values still above the display range, tonemapping collapses that range, grading and
   vignetting correct the result. A format that let a scene file reorder them would mostly produce
   wrong pictures and could not say so.

**Adding an effect** means a field on `Environment`, a branch in `post.wgsl`, and a line in
`GpuPost::from_environment`. If it needs more than one pass — bloom does, because a blur reads
neighbouring pixels — it also needs a `PassKind` and its own transients, which is what the graph is
for.

### Reflection: what a type has to be able to say about itself

Three traits in this engine require `Reflect`, and it is the same argument each time:

| Trait | Since | Why |
|---|---|---|
| `Component` | ADR 0013 | A component the agent cannot see still works at runtime, so the omission surfaces milestones later. |
| `Resource` | ADR 0027 | Same, and it is what makes `world.resources` possible at all. |
| `Event` | ADR 0027 | The event log is how an agent answers "what did I just do?". |

Usually this costs nothing — add `Reflect` to the derive list and give every field a doc comment:

```rust
/// How much damage something can take.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Health {
    /// Current hit points.
    #[reflect(min = 0.0, max = 100.0, unit = "hp")]
    current: f32,
}
```

**The one case where it does not just work: a type whose state is private to a lower crate.**
`Reflect` lives in `amadeo-reflect`, so nothing below it can implement the trait (invariant I6).
There are two answers, and which one applies depends on whether the state is public:

- **Public state** — write the impl in `amadeo-reflect` itself. It depends on `amadeo-core`, so
  `impl Reflect for amadeo_core::Tick` is legal there. The impl goes where the *trait* lives rather
  than where the type does.
- **Private state** — expose it, then reflect one layer up. `Rng::state()` and `Rng::from_state()`
  exist for exactly this, and `SimRng` in `amadeo-app` hand-writes the impl on top of them.

**Registering one type registers everything it names.** `registry.register::<Run>()` also puts
`Phase`, `u32` and anything *those* name into the registry (ADR 0030). So `registry.names()` is
longer than the list you registered, and `f32` and `array<f32, 3>` are in there. That is deliberate:
without it the schema could say a field is a `Phase` with nowhere to look `Phase` up. If you
hand-write a `Reflect` impl for a type with fields, write `register_dependencies` too — the derive
does it for you.

### Asking the engine what it knows: `describe`

`amadeo describe` returns four things, and it is worth knowing which is which:

| Key | What it is |
|---|---|
| `components` | what you can put on an entity — the list you pick from |
| `resources` | what the world holds exactly one of |
| `types` | every type the two above *name*, transitively — a lookup table, not a menu |
| `manual` | a path to this file, for the things `describe` deliberately does not carry |

`describe <Type>` resolves against all of them, so `amadeo describe Phase` works on a nested enum
that is neither a component nor a resource.

**`describe <Type> --example` is the one to reach for when you are writing a scene file by hand.** It
emits a minimal valid instance in two spellings — the scene block and the JSON — generated from one
value, so they cannot disagree:

```bash
amadeo describe Run --package vault --example
```

```text
  Run
    collected 0
    phase Playing
    total 0
```

That output is teaching you something no schema could: `phase` takes a **bare word**. Writing
`phase "Playing"` parses fine and then fails to load, because bare-versus-quoted is scene-format
grammar rather than type information. Numbers come back as zero, or a declared range's minimum where
zero would be outside it, so the example is minimal rather than realistic — it teaches spelling, not
values.

**What `describe` does not tell you** is how to *write* code against the engine: the derives a
component needs, `impl Component`, the registration call, a system's signature, `world.query`. That
is what this file is for, and it is a deliberate boundary rather than an omission — the reasoning is
in ADR 0030 and `docs/09-gate-4-describe-is-not-enough.md`.

### Maps in a reflected type

`BTreeMap<K, V>` reflects, as long as `K` implements `ReflectKey` — `String` and the fixed-width
integers already do. Two things worth knowing:

**Keys are strings on the way out.** So a `BTreeMap<u32, T>` round-trips through decimal text. The
contract is that `to_key` is **injective**; if two keys render the same, an entry is silently lost,
which is why the impl carries a `debug_assert` on the entry count.

**A key that is a hash reads badly.** `InputState` is keyed by `ActionId`, which is a hash whose name
is not kept, so `world.resources` shows `"8831028638596390904"` rather than `"move_x"`. That is a
known gap (Q18), not something you have done wrong — but it is worth avoiding in a *new* type. If
you are choosing a key, prefer one a human can read.

### Regenerating a replay: verify the diagnosis, don't just obey it

When a change moves a state hash, the golden-replay procedure below says to be sure the change is
correct before regenerating. The useful way to be sure is to **isolate the cause**: temporarily undo
only the part you believe is responsible, and check the committed hashes come back.

Two worked examples from session 8, both of which changed the answer from "probably fine" to "proven":

- Sprites were added to `quad-demo` and all four checkpoints moved. Removing *only* the ten new
  entities — while leaving the texture cache installed and a second asset loaded — restored every
  hash exactly. So the new machinery was invisible to the simulation, and the content change was the
  whole cause.
- `SimRng`'s hash changed at the same time as five types gained reflection. Reverting *only* the hash
  restored both replays, proving the reflection work touched nothing.

If the hashes do *not* come back, the change is bigger than you thought, and that is exactly what you
wanted to find out before committing.

### Snapshots: getting back to a moment without re-simulating

The loop this exists to fix: you want to look at what happens at minute five, and getting there costs
382 ms of simulation every single time you look (ADR 0011 measured it). Instead:

```bash
amadeo snapshot --ticks 18000 five-minutes.snapshot
```

Then every question afterwards starts from the file:

```bash
amadeo status --from five-minutes.snapshot
```

`--from` works on any game command, and composes with `--ticks` — restore to the recorded moment,
then run that many more:

```bash
amadeo query Transform --from five-minutes.snapshot --ticks 30
```

**A snapshot is text, and reading one is a supported thing to do.** So is editing one: change a value,
restore it, and see what happens. The file is `docs/adr/0028`'s format.

**Two things it does not capture**, both deliberately:

- **Services** — asset caches, the GPU device, the renderer. Those are machinery, not simulation
  state (ADR 0009). A restore puts the *simulation* back; the process around it carries on as it was.
- **Anything from a different build.** Rename a component and old snapshots stop loading, with an
  error saying so. A snapshot captures one moment of one run; there is no migration path and one
  would be the wrong thing to build.

**Why restoring is a launch flag rather than a method.** A snapshot says what the world *is*, so it
has to be installed before the first tick — by the time an RPC method could run, the pre-roll has
already happened. Same shape and same reason as `--replay`.

**The non-obvious part, if you ever touch the format.** A snapshot records the entity allocator's
*free list*, and `World::state_hash` deliberately does not. That means a snapshot missing the free
list would restore a world that hashes identically and then hands out different entity handles on the
next `spawn`. So comparing hashes after a restore cannot prove a restore is correct — the tests run
the world *on* afterwards instead, which is the only check that can see it.

### `modules/` — the layer above the engine, and what belongs in it

New in session 10 (ADR 0037), and the first thing that ever lived there is
`modules/amadeo-character`.

**The rule, in one line:** a module may depend on any engine crate and on other modules, and **no
engine crate may ever depend on a module**. That is invariant I6 stated one level up, and it is the
whole point of the layer.

**How to tell whether something is a crate or a module.** Ask what it would have to *know*:

| | Belongs in `crates/` | Belongs in `modules/` |
|---|---|---|
| Knows about | shapes, matrices, files, ticks | characters, health, inventories, weather |
| Question it answers | "move this capsule and slide" | "how fast does the player walk" |
| Could a game not want it? | no, it is machinery | yes, it is a genre choice |

The character controller is the worked example, and the split runs right through the middle of it.
The **geometry** — sweep a shape, slide along what it hits, report whether it landed on something —
is in `amadeo-physics` as `PhysicsBackend::move_shape`, and knows nothing about walking; it describes
a lift, a projectile and a camera equally well. The **character** — walk speed, acceleration, jump,
turning, which input actions drive them — is in `modules/amadeo-character`.

`CLAUDE.md` trap 10 is why this matters rather than being tidiness: of the eight target games one has
no character at all, so an engine that assumed one would have quietly picked a genre.

**A module registers its own components.** `amadeo_character::install(&mut app)?` registers
`CharacterController` and `CharacterMotion`, adds `step_physics`, and adds its own system *after* it.
It returns a `Result` because registration can clash. Do not skip the registration if you write
another module — trap 5 is that a component nobody registered works fine at runtime and is invisible
to `describe`, to `amadeo check`, and to the editor, and you find out three milestones later.

### Why the character system must run after physics

This is the one ordering in the engine that is wrong in a way you would not notice.

`move_shape` answers from a spatial index that `step_physics` builds each tick. Register the character
system *before* the step and it queries an **empty** index on tick one — so the character passes
through the level exactly once, at startup, and behaves perfectly forever after. `install` sets
`.after(STEP_PHYSICS)` so no game has to remember; if a game registers the character system by hand
and forgets, the schedule refuses to resolve and names the missing label.

### A worked bug: the character that sank through the floor

Worth reading because the *shape* of it recurs, and because it is what the tick-by-tick trace in
`modules/amadeo-character/tests/` is guarding.

The first version pressed the character gently downward while grounded — a common trick, meant to keep
it attached to the floor. It sank about 0.07 units per second and eventually fell through.

The cause: ground detection holds a character a **skin width** (0.01 units) above the surface, and the
downward bias moved it 0.0167 units in one tick. Moving further than the gap put the capsule exactly
touching the floor, which is the degenerate case for a shape cast — rapier's own penetration-fixing
routine is commented out in its source — so the next tick sank again. Slow enough to look like tuning,
fast enough to lose a level.

The fix was to stop pressing downward at all: vertical speed is exactly zero while grounded, and
staying attached is `snap_distance`'s job, which pulls the character down to the surface *after* the
move rather than aiming below it.

**The generalisable part:** when something moves by a small amount per tick and something else holds a
small tolerance, compare the two numbers. If the per-tick movement exceeds the tolerance, it will
tunnel, and it will do it slowly enough to be mistaken for a feel problem.

### A shape cast that starts inside something has no answer, and rapier's is *unstable*

**The bug:** *"any movement in any direction makes the camera flicker close or far"*, reported by
Justin in session 15 and present since the follow camera was written.

`keep_camera_clear` sweeps a sphere upward from the parent's origin to find its pivot. The parent's
origin is the middle of the parent's own **capsule collider**, and the sweep did not say to ignore it.
That is the degenerate case, and rapier does not resolve it the same way twice: it reads the
penetration as a surface too steep to stand on, reports `sliding_down_slope`, and cancels the motion
**depending on the exact contact normal**, which shifts as the parent walks.

Measured, at the Scarp's ordinary walking speed:

```
PROBE rose=3.0000 slide=false | higher=3.0000 | ignoring=3.0000
PROBE rose=0.0000 slide=true  | higher=3.0000 | ignoring=3.0000   <- about one tick in ten
```

`higher` is the same sweep started clear of the capsule and `ignoring` is the same sweep excluding it.
Both succeed on **every** tick, including all the ones the real sweep fails — which is what attributes
the failure to the start point rather than to the geometry overhead.

What that looks like in the game is not "a sweep occasionally returns zero". The pivot collapsed to
the player's feet, the arm snapped to its 1.2 m minimum, eased back out at 0.1 m per tick, and was
knocked down again long before covering the 5.8 m. So the camera **never once reached its authored
distance** and jittered near the player permanently.

**The rule: every shape query names what it starts inside.** `modules/amadeo-character` had always
called `.ignoring(entity)` — with a comment explaining exactly this — and the camera module, written
later and one crate away, did not. A missing filter is not a missing optimisation.

> **Why no test caught it.** The camera's own "is the sweep consulted at all?" test forced an
> obstruction with a four-metre probe sphere — which, centred on a pivot three metres up, **contains
> the player's capsule**. So it hit the player, reported a block, and asserted the right answer for
> the wrong reason. It was passing *because of* the bug, and it broke when the bug was fixed. If a
> test's forcing mechanism is "make the probe enormous", check what the probe is actually touching.

### Moving a shape and casting a shape are different questions — ask the right one

The engine has two ways to ask physics about a line through the world, and picking the wrong one is
not a performance mistake, it is a correctness one.

| | Asks | Answers by | Ends up |
|---|---|---|---|
| `move_shape` (ADR 0037) | *where does this body end up?* | **sliding** along what it hits | anywhere |
| `cast_shape` (ADR 0054) | *how far along this line before something blocks it?* | stopping | **on the line** |

If you want a distance, a clearance, a line of sight, or "can this fit here", you want `cast_shape`.
If you are moving a character, you want `move_shape`. Both must be called **after** `step_physics` in
the same tick, because both read an index the step builds and an empty index reports everything clear.

> **A correction layered on a borrowed operation has its own failure mode**, and this is the worked
> example. The follow camera asked the cast question with the move operation, and the slide made the
> answer wrong. Projecting the travel onto the query axis fixed the case where the slide went
> *sideways* — and could not fix the case where it went *along* the axis. Tilted up, the camera's arm
> points down and back; it hit the ground, slid backward, and backward was 0.87 of the arm, so the
> projection reported nearly a full arm of clearance for a shape that had gone nowhere. The camera
> ended up 0.06 m under the terrain, where its own 0.35 m probe should have held it clear.
>
> By the end that one call carried two corrections — project onto the axis, and exclude the parent
> body. **Two corrections on one call is the signal that the question is wrong**, not that a third is
> needed.

And the symptom is worth recognising, because it is the third time this session that a camera in the
wrong place read as a rendering fault: **a dark mass filling the frame over a band of sky is the
underside of the terrain.** Surfaces are two-sided since ADR 0052, and an underside faces away from
the sky so it picks up almost no light. "I am seeing the skybox" and "I am under the ground" look the
same from inside the frame.

### A third-person camera orbits its pivot; it does not sit at an offset and tilt

The other half of the same report: *"pointing the camera downwards I end up looking at the ground,
upwards I end up looking upwards from where the camera is"*.

That one was not a bug in the sense of a wrong line. The camera's position was the constant
`[0, height, distance]`, and pitch went into its **rotation** and reached its position nowhere — so
tilting spun the camera on the spot and whatever it followed slid out of frame.

A follow camera is an **arm**, and pitch is an angle *around the pivot* rather than a property of the
camera alone. Both of the references worth copying work this way — Unreal's `USpringArmComponent` and
Cinemachine's orbital rigs:

```
pivot  = parent + up × height          (swept for, so a ceiling shortens it)
arm    = rotate(back, pitch)           (the orbit — this is the bit that was missing)
camera = pivot + arm × distance
```

Tilt down and the camera rises and comes over the top, looking down *at* the thing it follows; tilt
up and it drops and looks up past it. The subject stays framed at every angle **for free**, because
the camera's forward is exactly the arm reversed — there is no "look at" step and nothing to keep in
sync.

**Two consequences worth knowing before tuning one:**

`height` changes meaning. It is no longer "how high the camera floats" but "**the point the camera
aims at**" — so a value that looked fine before now literally decides what is in the middle of the
screen. Both games' cameras had to come down (3.0 → 1.6 and 2.8 → 1.5) to stop aiming a metre above
the character's head, and those are eyeball numbers rather than derived ones.

And **the arm length becomes real state**. It is smoothed across ticks, so it must survive to the next
one, so it cannot be recovered from the transform — the transform's local `z` is `distance × cos
(pitch)` once the arm leans, which is close enough to a distance to pass a tolerance and wrong enough
to make a test mean something else. That is what `CameraArm` is, and it is `CharacterController` /
`CharacterMotion` again: authored settings in one component, live state in another.

### How a shadow gets onto the floor

Worth reading before touching lighting, because it is the first thing in the engine where one pass
*reads* what another pass measured.

A **shadow map** is the scene drawn from the light's point of view, storing only depth — how far the
light can see before something blocks it. Shading a pixel then asks "is anything closer to the light
than me?" If yes, it is in shadow. Four steps:

1. **`fit_shadow` works out what box the map covers** (`amadeo-render/src/lib.rs`). A directional
   light has no position, so the box is centred on the *camera* — that is where resolution is needed.
   It comes out as a `ShadowData` on the frame's `LightData`, so the backend is handed a finished
   matrix and never reaches back into the world.
2. **The graph declares a shadow pass** writing a `ShadowMap32` transient, and the view pass declares
   it in `reads`. The ordering is *derived from that*, not from writing them in order.
3. **`run_shadow_pass` draws the meshes** through a depth-only pipeline with no fragment stage and no
   colour attachment — the only pass in the engine shaped like that.
4. **`mesh.wgsl` samples it** with `textureSampleCompare`, which does the comparison in hardware
   across four neighbouring texels at once. That is a soft edge for the price of one sample.

**The three settings and what they trade.** `shadow_distance` is the half-extent of the box, and it
is the one that matters most: the map is a fixed number of pixels stretched over it, so doubling the
distance halves the detail. `shadow_resolution` costs memory as its *square* — 4096 is four times
2048, not twice. `shadow_bias` fixes acne (see below) and too much of it makes shadows detach from
what cast them.

**Why `ShadowMode` is an enum with two variants.** Cascades — splitting the camera's range into
slices with a map each — are what fixes blocky shadows over large outdoor scenes, and they arrive as
a third variant. The mode being *data* is why that is an addition rather than a rewrite; ADR 0038 has
the full argument, and it is the same one `PixelFormat` shipped with.

### A shader that compiles on your GPU can fail on CI — run the other compiler

**Before pushing any change to a `.wgsl` file:**

```bash
WGPU_BACKEND=dx12 WGPU_DX12_COMPILER=fxc cargo test -p amadeo-render --all-features --test capture
```

A real GPU compiles these shaders through DXC or Vulkan. **Windows CI has no GPU**, so it falls back
to WARP — Direct3D's software device — which compiles through **FXC**, an older and much stricter
compiler. The two disagree, and when they do, every GPU capture test fails at once with a wall of
identical warnings.

The one that cost a red build: a shadow sample inside the punctual-light loop.

```
warning X3570: gradient instruction used in a loop with varying iteration,
               attempting to unroll the loop
→ FXC D3DCompile error (Unspecified error (0x80004005))
```

`textureSample` and `textureSampleCompare` pick a mip level from the **implicit derivatives** of
their coordinates — they are *gradient instructions*, and HLSL forbids those in non-uniform control
flow. The light loop is bounded by a uniform, so FXC calls that "varying", tries to unroll, and gives
up. **The `Level` variants take no derivatives**: `textureSampleCompareLevel` and
`textureSampleLevel` name the mip explicitly and are legal anywhere.

> **The rule: never sample with an implicit mip level inside a loop whose bounds are not a compile-
> time constant.** A shadow map has one mip level anyway, so `…Level` costs nothing there. For a
> material texture the level genuinely matters — so sample it *outside* the loop and use the value
> inside.

**Ubuntu CI passes regardless**, because it has no software fallback at all: `WgpuBackend::offscreen`
fails, and these tests report that and skip. So a green Ubuntu job says nothing about shader
validity, and a red Windows job is the only signal.

### An absolute pixel threshold in a lit scene is measuring the ambient

Writing the first spot-light capture test, I asserted the floor **outside** the cone was darker than
40. It failed at 94.

94 is what a surface reads at with **no light in the world at all**: a camera naming no environment
still gets the neutral cube map, which is the `0.12` ambient constant ADR 0049 replaced with something
principled. So the assertion was unsatisfiable — and worse, the companion point-light assertion
(`under[0] > 90`) was **trivially true** and would have passed against a `PointLight` the renderer
ignored completely.

The fix is to capture the same scene with the light absent and compare:

```rust
let (Some(unlit), Some(image)) = (capture(&mut unlit, 64, 64), capture(&mut world, 64, 64)) else {
    return;
};
assert!(under[0] > pixel_at(&unlit, 32, 32)[0] + 40);
```

> **Generally: a capture test's threshold should come from another capture, not from a number you
> guessed.** Ambient, tonemapping, exposure and the clear colour all move absolute values around, and
> every one of them is a reason a hand-picked constant is either unsatisfiable or free.

### Two shaders reading one buffer must not each declare its layout

**The bug:** turning on shadow cascades drew a huge dark wedge across the horizon. Nothing failed to
compile, nothing failed wgpu validation, and every headless test passed.

`mesh.wgsl` and `sky.wgsl` read the **same uniform buffer at the same binding**, and each declared its
own copy of the struct. Cascades (ADR 0055) turned one `mat4x4` in it into an `array<mat4x4, 4>`,
which grew the struct by 192 bytes in one copy and not the other. The sky shader therefore read the
three vectors that turn a screen position into a world direction from 192 bytes too early, and drew
the sky pointing somewhere else.

The fix is `view.wgsl`: one declaration, prepended to both at pipeline creation.

```rust
source: wgpu::ShaderSource::Wgsl(
    concat!(include_str!("view.wgsl"), include_str!("sky.wgsl")).into(),
),
```

**One copy is left and cannot be removed this way**: `GpuMeshView` in `gpu.rs`. A `#[repr(C)]` struct
and a WGSL struct are two statements of one layout in two languages, and nothing checks them — only a
wrong picture does. If you add a field to one, add it to the other **in the same position**, and then
capture something and look at it.

> This is the same shape as four other findings in this file — normals versus winding, the two-sided
> apron, `format_float` shared between `amadeo-scene` and `amadeo-snapshot`, and the one below.
> **Two copies of one fact drift, and a comment saying "keep these in step" is not a mechanism.**
> Where the two copies can be made one, make them one.

### An introspection method must share the code it describes, not mirror it

The fifth instance, and the one where the *consequence* is worst. `audio.describe` reports what the
world sounds like; `collect_audio` decides what a backend is handed. Writing the query as a second
walk over the world would have been the obvious thing and is a trap: when the two drift, the method
reports a game playing sounds it is not playing, and **an agent acting on a confident wrong answer is
worse off than one told nothing.**

So `build_frame(world)` is one function and both call it, gains included.
`describing_agrees_with_what_was_actually_submitted` compares the described frame against the one the
null backend was actually given, which is what makes the sharing checked rather than merely intended.

**The general rule for any `*.describe` method: derive the answer from the same code the real path
uses, and pin the agreement with a test.** `render.describe` obeys it differently and for a stated
reason — it walks the world rather than reading `FrameData`, because a `SpriteInstance` deliberately
carries no entity id (ADR 0023). That is a deliberate divergence with its reasoning written down,
which is the only acceptable kind.

`anim.describe` obeys it in the smallest possible way, which is worth seeing because it shows how
little is needed. Almost all of it reads services directly, so there is nothing to mirror — except
"is this player doing anything", which `animate` also has to decide. That predicate is one method,
`AnimationPlayer::is_running`, and both call it. Two copies would have drifted into a report saying a
game is animating something it is not.

### Three shadow defects with names, and what this engine does about each

These have names because everyone hits them. If shadows ever look wrong, it is almost certainly one
of these three.

| Defect | What you see | What is done about it |
|---|---|---|
| **Acne** | A lit surface striped with thin shadows of itself | Front-face culling in the shadow pass, plus a slope-scaled bias |
| **Peter-panning** | A shadow detached from the object, which floats | Keep `shadow_bias` small; the culling above is what lets it be |
| **Shimmer / crawl** | Edges fizzing and swimming as you walk, with nothing moving | Snapping the box to a **world-anchored** texel grid |

The third is the one worth understanding, because the fix has a trap in it. Snapping the box to a
grid stops each shadow-map pixel covering a slightly different patch of world every frame. But the
grid must be anchored at the **world origin** — snapping relative to the camera is snapping to
something that moves, which is no snapping at all. That was got wrong once while deriving it, and
`a_shadow_box_moves_in_whole_texels` is what pins it.

### An asset that will not parse is skipped in silence — run `amadeo check` on all of them

The single most expensive hour of building `games/atrium` went here, so it is worth knowing before
you add an asset.

`load_materials`, `load_meshes` and `load_environments` all **skip** an asset that does not resolve,
does not parse, or does not hold the type they wanted. That is deliberate and ADR 0021 requires it —
a missing asset must be survivable, not a crash. The consequence is that a material with one field
missing produces no error, no warning, and a room where every surface is default white.

`amadeo check` catches it exactly, naming the file, the line, the component and the missing field:

```bash
amadeo check games/atrium/assets/materials/stone.material --package atrium
```

**Check the asset files, not just the level.** A material, a mesh and an environment are all scene
files with one root (ADRs 0033–0035), so `check` validates them the same way — but it only validates
what you point it at. CI now runs it over every one of them for both games.

Unlike the texture path, there is no `failures()` to ask at runtime. If a scene looks untextured or
uniformly white, that is the first thing to suspect.

### Getting a model from Blender into the engine

```bash
amadeo import-gltf games/atrium/assets/models/level.glb
```

Export from Blender as **`.glb`**, or as `.gltf` with buffers embedded. A plain `.gltf` that keeps
its buffers in sibling files is refused, and the message says so — the asset layer decides where
bytes come from (ADR 0021), not the parser.

That one command writes, next to the source:

| File | What it is |
|---|---|
| `level.scene` | the node hierarchy, as nested entities with `Transform` and `Mesh` |
| `level_<material>.material` | one per glTF material |
| `level_<name>.mesh` | one per glTF **primitive** — a pointer, not vertex data |
| `level.glb.ama-meta` | the sidecar giving the source file its asset id |

**The geometry stays in the `.glb`.** ADR 0039 has the full argument; the short version is that what
people and agents author is layout and materials, and nobody hand-edits vertex positions. A `.glb` is
source art exactly as a `.png` is.

Ids are prefixed with the file's stem, so importing two models cannot collide. Names from authoring
tools are lowercased and cleaned up — `Wall Segment` becomes `wall_segment`, `Cube.001` becomes
`cube_001` — and duplicates get a numeric suffix, because glTF does not require names to be unique
and two files claiming one id is something the asset scanner refuses outright.

**Re-importing overwrites.** Hand edits to generated files are lost, deliberately. Treat the
generated scene as a starting point to copy from or instance, not as a file to maintain.

**A glTF *mesh* is not an Amadeo mesh.** A glTF mesh holds one primitive per material, and an Amadeo
`Mesh` draws one thing with one material — so a primitive is the unit. A node whose mesh has several
primitives becomes one entity plus a child per extra primitive.

**What is not imported yet:** textures (a generated material carries colours only, so a textured
model imports untextured), animations, skins, and cameras.

### Where geometry comes from, and why nothing above the loader knows

There are three producers of `MeshData` and they all arrive at the same place:

| A `.mesh` file holding… | Produces geometry by… |
|---|---|
| `BoxMesh` | tessellating a box from three numbers |
| `PlaneMesh` | tessellating a quad from two |
| `GltfPart` | reading a primitive out of the `.glb` it names |

`App::load_meshes` tries each in turn, and `MeshCache` holds the result under the asset id. The
`Mesh` component, the render pass and everything downstream cannot tell which one a mesh came from.

That is ADR 0035 paying off: it was written before any of this existed specifically so the importer
would be an *addition* rather than a change to the mesh component, the cache, the batcher and every
test that asserts on a mesh. It was, three milestones later.

### Threads: the two shapes that are allowed, and why only two

ADR 0041 settles this. The rule is that **parallelism is allowed only where determinism is
structural** — the unsafe shapes are made unspellable rather than discouraged.

**Shape 1 — a job, for work that owns its inputs.** `amadeo-jobs`:

```rust
let pool = JobPool::for_this_machine();
let inbox: Inbox<ChunkCoord, MeshData> = Inbox::new();

for coord in chunks_to_build {
    let inbox = inbox.clone();
    pool.submit(move || inbox.deliver(coord, build_mesh(coord)));
}

pool.wait_for_idle();              // the barrier
for (coord, mesh) in inbox.drain() // sorted by coord, never by who finished
```

A job is `FnOnce + Send + 'static`, so it **cannot borrow the world**. There are exactly two ways an
answer may come back: wait at a barrier (which makes parallelism a pure speedup nothing downstream
can observe), or deliver into a `Service` that gameplay cannot see.

**`Inbox` drains in key order, never completion order.** That is the whole reason it is not a
channel.

**Shape 2 — `par_for_each_mut`, for heavy per-entity work.**

```rust
world.par_for_each_mut::<Height>(threads, |_entity, height| height.0 = sample_noise(height.0));
```

The closure is `Fn + Sync`, and that signature *is* the safety argument:

- **`Fn` forbids a captured accumulator** — summing into a captured variable needs `FnMut` and will
  not compile. Which matters, because float addition is not associative and a parallel sum genuinely
  gives a different number.
- **`Sync` forbids the escape hatches** — `Cell` and `RefCell` are not `Sync`.
- **No `&World`, no `Commands`** — so no cross-entity reads, no spawning, no despawning.

Every write goes to a row that thread owns exclusively, so the answer cannot depend on how the rows
were divided. `the_thread_count_cannot_reach_the_answer` runs the same work at 1, 2, 3, 5 and 8
threads and requires identical output — odd counts included, because an off-by-one in chunk slicing
hides completely when the rows divide evenly.

**When to reach for it: rarely.** Measured, 8 threads:

| rows | speedup |
|---:|---:|
| 2,048 | 1.29× |
| 16,384 | 3.35× |
| 131,072 | 5.42× |

Below `PARALLEL_THRESHOLD` (2,048) it runs sequentially and spawns nothing. The whole Atrium
simulation tick is 8.3 µs, so this is for a system doing real arithmetic over thousands of entities —
not for moving a hundred transforms.

### The threading rule most likely to be got wrong

**A streamed thing usually has two products, and they have different rules.**

A terrain chunk's **mesh** is drawn and nothing else — it goes in a `Service` and may arrive whenever
it arrives. Its **collider** is gameplay, because a character stands on it, so *when* it arrives
changes where the character ends up.

So: decide **which** chunks are active deterministically (from the player's position, which is
deterministic), do the **work** in parallel, and **block** on colliders you need. A slow machine gets
a frame hitch and keeps its replay.

ADR 0021 built half of this rule three milestones early by forbidding gameplay from asking "has this
asset finished loading?" The general form is that **gameplay may not observe any completion timing.**

### Big derived data reaches a backend by id, never through a component

**The pattern:** when something is *large* and *derived*, it does not become a component and it does
not travel through a per-tick function. It gets a name, is handed to the backend once, and stays
there until removed.

Terrain collision is the worked example. It looks like it wants `Shape::Trimesh`, and it cannot have
one, for two independent reasons:

```rust
// Why not a component: both of these are true of Shape, and a triangle mesh breaks both.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub enum Shape { /* Cuboid | Sphere | Capsule */ }
//               ^ Copy: a Vec is not.   ^ StableHash: ADR 0042 says an untouched world
//                                         must cost NOTHING to hash, and walking a world's
//                                         worth of vertices is the exact opposite.
```

And it cannot go through `step` either, because `BodyState` is handed over **in full every tick** —
that is what makes a step a pure function — and a chunk is thousands of triangles.

So it works the way a texture works:

```rust
// Handed over once, by id, and held between steps.
backend.insert_static_mesh(StaticMesh { id: StaticMeshId(key_as_number), .. })?;
// ... many ticks later, when the chunk streams out:
backend.remove_static_mesh(StaticMeshId(key_as_number));
```

**Why this is safe for determinism**, which is the part worth internalising: the geometry is
*derived* — regenerable from a seed and a sparse list of edits — so ADR 0019's rule applies and it
belongs outside the state hash. What *is* hashed is the seed and the edits that produced it. If you
ever find yourself wanting to hash the derived thing, that is the signal you have the data model
upside down.

**And the mechanism stays ignorant of the use case.** `StaticMesh` knows nothing about terrain,
chunks or ground — it is equally an imported level's collision geometry, a bridge, or scenery too
concave for a box. That is the same discipline `PhysicsBackend::move_shape` follows by knowing
nothing about characters (ADR 0037): the engine crate owns the *mechanism*, and the thing with an
opinion about genre lives above it.

### Never filter an output by "what does the caller already have"

**The pattern, and it is a trap rather than a technique.** When a system produces work in the
background and reports what changed, it is tempting to report only what the caller has not already
been given. Do not — that filter reads background completion state, so the output silently inherits
whatever the thread pool happened to finish.

Session 12 shipped this bug twice in one file:

```rust
// WRONG. `known` is filled by background deliveries, so which branch a chunk takes -- and therefore
// the ORDER of this list -- depends on how many worker threads there are.
for key in &required.collision {
    if !self.known.contains_key(key) { newly_meshed.push(...) } else { already_known.push(...) }
}
update.colliders = newly_meshed.into_iter().chain(already_known).collect();

// WRONG, same shape. "Was the caller ever told about this?" is a question about delivery timing.
if self.known.remove(&key).is_some() {
    update.removed.push(key);
}
```

```rust
// RIGHT. Both come from set differences over residency, which is a pure function of where the
// viewers are -- so contents AND order are identical on every machine at every thread count.
for key in required.collision.difference(&self.required.collision) { ... }
for key in self.required.visual.difference(&required.visual) { update.removed.push(*key); }
```

The second version reports removals for things the caller may never have had. **That is fine, and
making it fine is part of the design**: every consumer's removal is idempotent on purpose — a mesh
cache drops a missing key silently, and `PhysicsBackend::remove_static_mesh` documents that removing
something absent is not an error, precisely because most chunks are empty and never had a collider.

The general rule: **the deterministic outputs of a background system must be computed from
deterministic inputs only.** If you find yourself consulting a cache, a completion set, or an
"already sent" list to decide what to emit, the output now depends on machine speed. It will pass on
your machine and fail on a loaded CI runner, which is exactly how both of these were found.

A related corollary for tests: **anything asserting *which tick* background work landed on is
asserting on machine speed.** Count across ticks instead.

### And its sibling: gate on the set that has consumers, not the widest one available

Session 13 shipped a third variant of the same bug, and it is worth separating because the first two
were about *filtering* and this one is about *admitting*.

`TerrainStreamer` keeps three nested residency sets — `collision ⊆ visual ⊆ data`. Collection of
finished meshes was gated on `data`, which is `visual` grown by one ring. That ring is the **apron**:
it exists so meshing a drawn chunk can read its neighbours' samples, and nothing in it is ever
submitted, drawn or given an entity.

So a chunk that had left the drawn region could still be delivered while it sat in the apron — after
`removed` had already told the caller to drop it. The caller cached geometry for an entity that no
longer existed, and nothing would ever name that key again.

**Whether it happened depended on when a job finished.** Land on the same tick as the removal and the
caller's own insert-then-remove ordering hid it; land a tick later and the entry was orphaned
permanently. Green on a developer machine, red on a loaded CI runner.

> **The rule: a background result may only be admitted for something the caller still has a consumer
> for.** When several sets are in scope, the correct one is the narrowest that covers every consumer
> — not the widest that happens to contain the key.

And the way to test it is to **control when the work lands** rather than hope for a slow machine:
submit, change the state, and only *then* `wait_for_idle` and collect. That turns a coin flip into an
assertion.

### The fourth variant was in a test's setup, which is where nobody looks

Session 13 hit this shape a fourth time, after documenting the first three above, and the place it
landed is the lesson: **the guard clause of the test that proves an exit gate.**

The culling test ran 200 ticks and then checked `meshes > 20` before measuring — a sanity check that
the world was big enough to be worth measuring. On this machine 50 chunks had geometry by then; on a
CI runner, 17. The test failed on all three CI jobs.

**Every assertion that actually measured culling passed there** — 17 in the world, 8 in view, 8
submitted. Only the setup was wrong, and it was wrong in the same way as the other three: *how many
chunks have geometry after N ticks* is how fast the machine is.

> **The rule, generalised: a test may not assume background work has finished. Make it finish.**
> Advance until the pool is idle *and* the count has stopped moving, then measure. `run_ticks(n)`
> for a big `n` is not that — the main thread outruns the workers, and at one worker six hundred
> ticks were not enough.

`TerrainStreamer::wait_for_idle` exists for this. It is ADR 0041's **barrier** — shape one of the two
allowed ways for an answer to come back — so it cannot change what the streamer produces, only when.
Gameplay has no reason to call it, because ADR 0021 and ADR 0041 §2 together forbid gameplay from
observing delivery timing at all.

**And run the slow case locally.** `build_with_workers(1)` is the closest thing to a loaded CI runner
a fast machine can produce, and `once_the_pool_is_quiet_the_count_is_the_same_at_every_thread_count`
is what would have caught this before pushing.

### A terrain generator may not use `sin`, `cos` or `powf`

**The rule (ADR 0044):** anything that decides where the ground is may use only `+`, `-`, `*`, `/`,
`sqrt`, `floor`, comparison and integer arithmetic. No trigonometry, no `exp`, no `powf`, no `powi`.

**Why**, and it is not fussiness about the last decimal place. Rust's own documentation says of
`f32::sin`, `f32::cos` and `f32::powf`:

> The precision of this function is non-deterministic. This means it varies by platform, Rust
> version, and can even differ within the same execution from one invocation to the next.

`f32::sqrt` carries the opposite note and is guaranteed exact, because IEEE 754 *requires* correct
rounding for `+ - * / sqrt` and lists the transcendentals as merely recommended.

ADR 0043 made a chunk's **collider** gameplay state. `TerrainSource::sample` decides where that
collider is. So the obvious way to write rolling hills — a sum of sines — puts Windows and Linux on
different ground, and the bug report reads *"the replay does not reproduce on Linux"*, pointing at
physics, at the scheduler, at the job pool, at everything except a trigonometric function inside a
terrain generator.

**This is a fifth entry for trap 2**, alongside `HashMap` iteration, `Instant::now()` in gameplay,
unsorted parallel writes and uninitialised floats. It is the least visible of the five: the offending
code looks like ordinary mathematics and every test on one machine passes.

Use [`amadeo_noise`], which is built entirely from the permitted set:

```rust
let hills = Fbm { frequency: 0.012, octaves: 4, ..Fbm::new(seed) };
let height = 6.0 + hills.sample_2d(x, z) * 11.0;
```

`mul_add` is permitted and deliberately unused: it is correctly rounded and it is *a different
number* from `a * b + c`, because it rounds once instead of twice.

### …but you *can* place something at an angle, and there is one function for it

**Use [`amadeo_core::sin_cos_degrees`], never `f32::sin_cos`** (ADR 0053). The ban above is on the
standard library's transcendentals, which are unspecified — not on trigonometry, which is sometimes
the only way to say what you mean. A camera orbiting a pivot has no arithmetic dodge.

`amadeo_core::sin_cos_degrees` is built from `+ - * /` and `floor`, so it gives the same answer on
every machine, and `Mat4::from_euler_degrees` uses it — meaning **composing a rotation matrix is now
as reproducible as adding two numbers**, wherever it happens.

It is also *more* accurate than the obvious route, which is worth knowing because it makes the
obvious comparison misleading:

```rust
// Wrong, and wrong in a way that gets worse the larger the angle:
let (s, c) = angle.to_radians().sin_cos();
// Right:
let (s, c) = amadeo_core::sin_cos_degrees(angle);
```

`to_radians()` in `f32` loses the digits that decide the answer before `sin` ever runs — past a few
turns the engine's version is closer to the truth than the standard library's, so a test comparing
the two at `f32` fails against the *reference*, not against the implementation. Compare at `f64` if
you ever need to.

**And the reason this rule exists at all is a bug that had already happened.** `keep_camera_clear`
built a matrix from its parent's rotation, projected a distance onto one of its axes, and wrote the
answer into the camera's **hashed** `Transform`. `matrix.rs` had described that exact route in its own
header as the "side door" back into hashed state — and then guarded the lesser risk and left it open.
Nothing caught it: the Scarp's determinism test compares a machine against itself, and the pinned
cross-platform hashes live in physics and noise, neither of which rotates anything.

> **The general shape: a documented hazard is not a mitigated one.** A comment saying "careful, this
> could leak into the state hash" is a note for a reader who is already looking at the file. The thing
> that actually closes it is making the arithmetic specified, so no caller has to have read the note.

### Two things that look right about a mesh and are independent

A mesh carries **normals** and a **winding**, and they answer different questions:

| | Comes from | Decides |
|---|---|---|
| Normal | the field's gradient, or the shape's own maths | how brightly a surface is lit |
| Winding | the order a triangle's three corners are listed in | which side of it the GPU considers the front |

**Getting one right does not check the other.** Session 13 found that every mesh `surface_nets` had
ever produced was wound against its own normals — all three axes, uniformly — so every voxel surface
was inside-out. The mesher's own tests asserted that normals point away from the inside, and they
passed, because normals were never the broken part.

Two things kept it hidden for two sessions. Nothing had ever *drawn* a surface-nets mesh: the
collider path has no winding at all, so physics was correct throughout. And the symptom does not look
like geometry — a heightfield that is inside-out is **invisible from above** and faintly visible at
the horizon, which reads as chunks that failed to stream in.

The check that catches it needs no GPU, and every mesh producer should have one:

```rust
// The direction the GPU treats as "front" for this winding, against the normal we claim.
let facing = cross(b - a, c - a);
assert!(dot(facing, normal) > 0.0, "wound against its own normal");
```

`amadeo-render` has had `every_box_triangle_faces_outward` for `BoxMesh` since ADR 0035;
`amadeo-voxel` now has `triangles_are_wound_to_match_their_own_normals`. If you add a fifth producer
of `MeshData`, write this one first.

### Anything that averages colour must do it in linear light

**The rule:** before averaging, blending or interpolating sRGB pixel values, decode them to linear;
re-encode afterwards. Alpha is the exception — it is coverage, not light, and is never gamma-encoded.

`PixelFormat::Rgba8UnormSrgb` means the stored bytes sit on a **perceptual curve**. They are not a
measurement of light, so their arithmetic mean is not the mean brightness. Half black and half white
is `0.5` in light, which sRGB encodes as about **188** — averaging the bytes gives **128**, a
noticeably darker colour.

The symptom is indirect and easy to misattribute: textures that **dim as they recede**, because each
mip level is a little darker than the one above it. It reads as a lighting or fog problem.

`amadeo_image::mip_chain` does this correctly and
`black_and_white_average_to_the_perceptual_middle_not_the_byte_middle` pins it. The same rule will
apply to any future blur, bloom downsample, or texture blend.

> `powf` appears in the sRGB curve, and ADR 0044 **bans transcendentals** from anything deciding
> gameplay state. It is safe in this class of code and the distinction is worth internalising: this
> runs at **load**, its output is *pixels*, and nothing in a simulation depends on it. A mip level
> differing in its last bit between two machines changes a shade of green, not where the ground is.
> `games/scarp`'s `turf` generator carries the same note for the same reason.

### Moving a physics body by writing its `Transform` — mostly fixed, and the history is the lesson

**Since ADR 0072 this works for a root**, which is nearly everything you would want to move: a
character, a crate, a camera rig. `step_physics` now reads a root's own `Transform` rather than the
composed one, because for a root the two are equal whenever propagation is current and the local one
is a tick fresher.

**It is still one tick stale for a *child*.** A child's world pose exists only in the composed
matrix, and that matrix is written in `PostSimulation`, so physics necessarily reads last tick's.
Nothing needs this yet; if a *moving* parent with a physics child ever appears, the answer is to run
propagation before physics inside the tick, and ADR 0072 records that as the rejected-for-now option.

**The history is worth keeping**, because it is a good example of a fallback hiding a fault. It used
to read `GlobalTransform` in preference to `Transform` always, so a write between ticks was silently
undone on the next step: the entity did not move, nothing errored, and the only sign was that
whatever you expected at the new position did not happen. Session 17 spent a debug cycle on it in a
terrain-streaming test, and session 19 spent another when a test stood the player in front of a door
and found nothing there.

And the table that made it confusing is instructive on its own:

| Where the write happens | Then | Now |
|---|---|---|
| A system in `PreSimulation` or `Simulation` | Works | Works |
| A system after `propagate_transforms`, or in `Render` | Stale by one tick | Works |
| **Between ticks** — a test, an editor, a load | **Silently ignored** | Works |

**The first row working is exactly what made somebody believe the third one would.** A rule with an
exception that fires only in the case you are not currently looking at is worse than no rule.

**Q30 is narrower now**: a teleport works, and what is still missing is everything *around* one —
resetting velocity, clearing the solver's contacts, and deciding what happens to whatever the body
lands inside.

### A child's world pose lives in the matrix, and its `Transform` does not

The rule ADR 0072 settled, stated once so nothing else has to rediscover it:

- **Reading** a world pose: a **child** must come from the composed `GlobalTransform` — its own
  `Transform` is relative to its parent, so a door authored square inside a piece turned a quarter
  turn has a local rotation of zero and a world rotation of ninety degrees. A **root** should come
  from its own `Transform`, which is the same value one tick fresher.
- **Writing** a world pose back: it has to go through the parent's inverse
  (`Mat4::inverse_rigid`) first, or it is stored as if it were local and propagation applies the
  parent a *second* time. That compounds every tick.

The second half is what made every generated interior in `games/warren` wrong for a whole session,
and it was unreachable before ADR 0071's room pieces: a prefab has exactly one root, so a piece with
two colliders has to put them on children, and until then nothing had a reason to.

Two things about how it hid:

- **It looked fine at tick 1.** Nothing had propagated yet, so the fallback to the local transform
  was still in play and the first step was correct. The capture that recorded "it loads and draws"
  was taken at exactly that tick.
- **`amadeo check` could not see it**, and neither could a green test suite. Both are about text and
  rules. The cheapest check that would have caught it is a test that stands the player on the floor.

### Its general form: a rule inferred from the cases tried so far

Session 21 found the same shape a third time, so it is a pattern rather than three coincidences.
**A plausible value substituted where nobody made a decision**, which is right for every case anyone
has looked at and silently wrong for the next one:

| Where | The substitute | Where it stopped being true |
|---|---|---|
| `GlobalTransform` | falls back to the local transform when absent | right for a root, wrong for a child — and it hid three defects at once |
| `Layout`'s key placement | `max_by_key` returns the **last** maximum | fine until every room tied, when the tie-break silently *became* the rule |
| A capture test's camera | `if eye[2] < 0.0 { 180.0 } else { 0.0 }` | right for a camera behind the subject, wrong for one beside it — the shape left frame entirely |

The tell in all three is a **derivation standing in for a decision**: two lines of arithmetic that
happen to produce the right answer for the two or three inputs anyone has passed. The fix is the same
each time — **write the value down per case** — and it is worth more than it looks, because in each
instance the wrong answer was *plausible* rather than absurd, so nothing downstream complained.

When you find yourself computing something that a caller could simply state, ask which cases you have
actually tried. If the answer is "the ones in front of me", state it instead.

### And its sibling: a statement that was true when written and silently stopped being

The table above is about a value nobody decided. This one is about a claim nobody revisited, and by
the end of session 21 it had **five** documented instances — which makes it the most repeated defect
in this repository. In every case the statement was correct on the day it was written, something
later removed what made it true, and **nothing failed**:

| Where | What it asserted | What removed it |
|---|---|---|
| `append_translated`'s doc comment | rotation is refused *deliberately* | `CompoundMesh` needed rotation (ADR 0074 §2) |
| `describe --example`'s guard | an empty list field has no scene spelling | the format grew `[]` during ADR 0069's save work |
| `docs/12`, `STATUS`, `docs/11` | `docs/06` records skeletal animation as blocked | `docs/06` never contained it — three documents repeated one unchecked citation |
| `solid.rs`'s faceting comment | a cone's facet is non-planar, so `flat_shade` would crease it | it was never true; a frustum's lateral facet is a planar trapezoid |
| ADR 0034's fog note | fog needs a depth buffer | true of a *post-process* shape; ADR 0073 made it a forward term |

**The failure mode is that nothing breaks.** The guard kept refusing, the comment kept asserting, the
citation kept being repeated. No test goes red, because a test checks behaviour and these are claims
*about* behaviour. The fog one cost four milestones; the `--example` one cost three sessions and the
flagship type of ADR 0074 its scene form.

The mitigation is one sentence and it is cheap: **when you remove a limitation, grep for what asserted
it.** The assertion is almost always in a different crate from the fix — which is exactly why the
person making the fix does not see it.

### Mutate every capture assertion once, and say in the comment that you did

Session 21 wrote **two** vacuous tests, and neither was caught by care in writing them or by review.
Both were caught by breaking the thing they tested and watching them stay green.

They share a shape, and it is not "a bad assertion": **the assertion was true for a reason other than
the one it was written for.**

| The test | What it asserted | Why it passed anyway |
|---|---|---|
| The first sky-ordering test | a pane against a blue sky reads blue | the sky pass *painted over* the pane, so the pixel was the sky in either order |
| Session 20's pause test | the player's translation is unchanged | a player with no input does not move whether paused or not |
| Two capture framings | the shape covers the middle of the frame | it did — as a flat face square-on, or a slab, with the feature off screen |

No amount of care catches this, because the assertion is *correct*; what it cannot do is tell the
passing case from the failing one. **Only running the failing case shows that.** So:

- **Break the thing, run the test, watch it go red, put it back.** Once, when the test is written.
- **Say so in the comment**, because the next person cannot tell a mutated assertion from an
  unmutated one, and the whole problem is that both look identical when green.
- **Prefer a control inside the test where one can be built.** Rendering a `BoxMesh` of the same
  bounds beside the shape under test, or an opaque material beside a blended one, makes the
  discrimination *structural*: if a change stops the two differing, the comparison fails by itself.
  That is a permanent mutation rather than one somebody performed once, and it is strictly better.

The engine gate's shape tests all carry a control for this reason, and the pattern is worth copying to
anything that asserts a colour.

#### Undo the mutation with the inverse edit, never with `git checkout`

Session 21 lost work to this **twice**, and the second time it cost two rounds of wrong theorising.

`git checkout <file>` restores the file to **HEAD**, not to "before I broke it". So when the thing
being mutated is a feature that has not been committed yet — which is the normal case, because you
mutate a test the moment you write it — the restore silently reverts *the feature as well as the
mutation*. Nothing fails, because the code still compiles and the tests still pass; they just pass for
the old reasons.

The second instance is worth spelling out because the symptom was so misleading. `uv_scale` was
mutated out of `mesh.wgsl` to check the test could fail, then "restored" with `git checkout`, which
deleted the feature. The test then failed identically on a real GPU and on WARP — and two plausible,
carefully-argued theories followed, about mip selection and about vertex-attribute offsets. Dumping the
image showed two identical checkers and answered it in one look.

Two rules, and the first is enough on its own:

- **Undo a mutation with the inverse edit.** You know exactly what you changed; change it back.
- **Or commit first, then mutate.** Then `git checkout` means what you wanted it to mean.

There is also a **safe** way to use `git checkout`, and it is one command: `git checkout -- <file>`
restores from the **index**, not from HEAD. It reverted the feature because nothing was staged, so the
index happened to match HEAD. **`git add` the file before mutating it** and the index holds your work,
after which the restore does exactly what you expected.

#### A search-and-replace that matches nothing still succeeds

Session 21 reported a test assertion as changed when it had not been, and only found out because a
reviewer read the file. The mechanism is worth knowing because it is silent by construction:

```bash
perl -0pi -e 's/old text/new text/' some_file.rs   # exit 0 whether or not it matched
sed -i 's/old/new/' some_file.rs                   # the same
```

**Neither reports a miss.** The command succeeded; the substitution did not happen. And the usual
next step hides it further — `cargo fmt && cargo test` passes, because the file is still the file it
was, and a test that was green stays green.

The specific way it bit: the target text had been **reflowed by `cargo fmt`** since the pattern was
written, so a multi-line pattern that had matched an hour earlier no longer did.

The rule is one line: **grep for the result, not for the exit code.** After any scripted edit, check
the new text is present — `grep -c 'new text' file` — before believing it, and certainly before
reporting it. The dedicated edit tools fail loudly on a missed match, which is the reason to prefer
them for anything whose absence would be quiet.

#### Dump the artefact before arguing

Three instances now, and the third cost the most.

When a capture, a replay or a dump disagrees with your model of the code, **look at the artefact before
refining the model.** In every case so far the model was wrong in a way that reasoning could not reach,
and one look settled it:

- Session 19: two confident wrong theories about the geometry, against a capture of the *handcrafted*
  room — which located the fault in ten minutes.
- Session 21, transparency: two theories about a missing material bind group and a depth-test
  rejection, against one dump of what `split_by_alpha` had actually produced. Collection was perfect;
  the fault was in the backend.
- Session 21, texel density: two theories about mip selection and vertex-attribute offsets, against
  one image showing two identical checkers. The shader line was simply gone.

The tell is **two plausible theories that disagree**. That is the moment to stop arguing and print
something — a `Vec` of what a pass produced, a PNG of what a camera saw. It is almost always cheaper
than the next round of reasoning, and unlike reasoning it cannot be confidently wrong.

### A mesh now has three independent properties, not two

The entry above pairs **normals** and **winding** and says getting one right does not check the other.
Normal mapping adds a third, and it fails the same silent way both of those do.

| | Comes from | Decides |
|---|---|---|
| Normal | the field's gradient, or the shape's own maths | how brightly a surface is lit |
| Winding | the order a triangle's three corners are listed in | which side the GPU considers the front |
| **Tangent** | the UVs, or the file's own `TANGENT` attribute | **which way "left" points on the surface** |

A tangent frame can be wrong in three ways, and only the first is loud:

- **Zero length** → `normalize(0)` → `NaN` → the surface renders black, and the `NaN` spreads.
  This is the one you notice. `generate_tangents` cannot produce it: a vertex whose UVs carried no
  information gets an arbitrary axis in the surface instead.
- **Not perpendicular to the normal** → the frame is not a rotation, so it shears every direction the
  normal map stores. Looks like slightly wrong lighting.
- **Perpendicular but pointing the wrong way** → an orthonormal frame rotated 90°, which passes every
  "is it a valid frame" check and slides the normal map sideways across the surface.

The third is why `a_tangent_points_the_way_the_texture_grows` compares against a direction worked out
by hand — a plane's `u` axis runs along +x, so its tangent must too. A test that only checked
orthonormality would have passed a frame pointing anywhere.

> **If you add a fifth producer of `MeshData`, it needs all three checks**, not the two the earlier
> entry names. `every_box_tangent_is_a_usable_frame` and the one above are the templates.

### Colour, and things that are shaped like colour but are not

`PixelFormat` has two variants and the difference is not cosmetic:

- `Rgba8UnormSrgb` — the bytes are **colour**, on a perceptual curve. Everything an artist painted.
- `Rgba8Unorm` — the same four bytes read as **linear**. Normal maps, and soon roughness and masks.

**A `.png` cannot tell you which it is.** The bytes are identical; only intent differs. So the
declaration lives in the asset's `.ama-meta` sidecar:

```text
id = "brick_normal"
color_space = "linear"
```

`TextureCache::ensure` applies it with `TextureData::reinterpret`, which changes the tag and **not one
byte** — converting would be the bug, since a normal map's numbers are already the numbers wanted.

Getting it wrong is quiet: a normal map decoded as sRGB has every direction it stores bent, so the
surface is lit as though its bumps face somewhere they do not. Nothing errors. **Nothing warns yet
either** — that is **Q31**, and until it exists, checking the sidecar is a manual step when adding a
normal map.

This is the same rule as the mip-chain entry above, one layer earlier: that one is about *averaging*
colour correctly, this one is about knowing whether a thing is colour at all.

### Why a normal map needs a normal map placeholder

A material naming no normal map still binds one: a 1×1 pixel of `(128, 128, 255)`, which decodes to
`(0, 0, 1)` — "leaning nowhere" — and leaves the geometric normal exactly as it was.

That is the same trick as the white base-colour placeholder, and the same reason: **the shader's
bindings are declared by the pipeline, so leaving one empty is a validation error rather than a shader
that skips the lookup.** Binding an identity value means one pipeline serves both textured and
untextured materials, instead of two pipelines that can drift apart.

The engine now has three of these, and the pattern is worth recognising: the shadow map's 1×1
placeholder, the white base colour, and this. Each is the **identity of the operation it feeds** —
never the magenta "asset missing" check, which means something different and should stay meaning it.

### A service nobody installs is a feature that silently does nothing

**The fourth time this project has shipped something wired, tested, and inert**, and the first time
the cause was a missing line rather than a missing call.

Image-based lighting was built, unit-tested, and GPU-tested. Then `games/scarp` rendered a capture
**identical** to the one before it. Nothing errored, no test failed, and the asset was resident.

The cause: every game installs `TextureCache` by hand —

```rust
app.scan_assets(ASSET_DIRECTORY)?;
app.insert_service(TextureCache::new());
```

— so `SkyCache` followed that precedent, and nothing installed it. `prefilter_frame_skies` returns
early when the service is absent, `upload_frame_skies` likewise, and the backend falls back to a
neutral sky. Every one of those is individually correct, and together they are a feature that does
nothing quietly.

> **The rule: if a capability needs a setup line, remove the line rather than remember it.** A service
> that only some worlds need should install itself at the point where the need is *visible* — for
> `SkyCache` that is `load_environments`, because a look is the only thing that can name a sky.

The general shape, alongside its three siblings above: **the deterministic outputs of a background
system must come from deterministic inputs; a background result may only be admitted for something
with a consumer; a test may not assume background work has finished; and a capability may not depend
on a caller remembering to enable it.**

`TextureCache` is still installed by hand in four games, which is now the odd one out rather than the
pattern. Worth changing next time something touches it.

### A field added to a component invalidates every file that spells it out

Reflection requires **every** field to be present (`ReflectError::MissingField`), so adding one to a
component breaks every existing `.scene`, `.material` or `.environment` naming it.

That is deliberate and load-bearing: it is what catches a typo'd field name, and what makes a prefab
that lost a component refuse to load rather than silently reverting — ADR 0029's chosen opposite of
Unity's behaviour.

**The trap is not the churn, it is how the churn reports itself.** Adding `sky` to `Environment`
produced no parse error anywhere. `read_component_assets` skipped the unparseable file in silence,
`found` came back empty, the function returned early, and the failure surfaced three layers away as a
test asserting *"load_environments installs it on first use"*. Nothing in that message mentions a
field, a file, or a schema.

So, when adding a field to a reflected component:

1. **Find every file that spells the type out** — `find . -name "*.material"` and friends.
2. Add the field **in alphabetical order**, which is what the canonical writer emits.
3. Run `amadeo fmt --check` on them, which confirms the placement rather than trusting it.

This is **Q32**, and it is filed rather than solved. Five files is trivial; the reason it is worth a
question is that `Material` gained three fields in one session and image-based lighting wants more.

### Where an environment map is different from every other texture

A `TextureData` is bytes plus a format tag: decoded once, uploaded once, four bytes a pixel. An
environment map shares none of that shape, which is why [`HdrImage`] and [`Cubemap`] are separate
types rather than variants of it:

| | A texture | An environment map |
|---|---|---|
| Pixels | bytes, usually sRGB | **floats**, always linear, values well above 1.0 |
| Shape | one rectangle | **six square faces** of a cube |
| Preparation | decode | decode, project onto a cube, convolve **twice**, at seven resolutions |
| Cost | milliseconds | **seconds** |
| GPU format | `Rgba8UnormSrgb` | `Rgba16Float` |

The last row is the one that catches people. **`Rgba32Float` is not filterable in wgpu's base feature
set** — sampling one with anything but nearest-neighbour needs `FLOAT32_FILTERABLE`, which plenty of
adapters do not advertise. An environment is sampled with smooth interpolation across faces *and*
between roughness levels, so it has to be the format that filters everywhere.

And the reason the prefiltering lives in `amadeo-render` rather than beside `mip_chain` in
`amadeo-image`: `importance_sample_ggx` and `mesh.wgsl`'s `distribution_ggx` are **two views of one
model**. One generates directions, the other measures how many point a given way. Two crates that
cannot see each other is how those drift apart, and the symptom would be reflections subtly the wrong
shape for the material shading them.

### An environment map must not contain a light the scene already has

**The rule:** an environment map is the **indirect** half of lighting — everything arriving from
everywhere else. Anything in it that is *also* modelled as a direct light gets counted twice, and
every surface receives that light at double strength.

The Scarp's sun is a `DirectionalLight` **and** was a bright disc in its generated sky. Fixing an
unrelated bug — the sun had been below the horizon, so its disc contributed nothing — blew the entire
demo out to near-white, and the cause read as an exposure problem.

**What makes this easy to get badly wrong is solid angle.** Irradiance weighs each direction by how
much of the sky it covers, so a small bright thing contributes far more than its size suggests:

| | Covers | At | Contributes |
|---|---|---|---|
| A 5° sun disc | ~0.5% of the sky | 250× | **more than all the rest of the sky combined** |
| A 1.8° sun disc | ~0.05% of the sky | 40× | negligible |

Both read as a blazing sun when you look straight at them, because anything above about 2.0 tonemaps
to white. Only one of them wrecks the lighting.

> **So: keep the *energy* of a direct light in the light, where it can also cast a shadow, and keep
> only a token of it in the environment, for something to look at and reflect.**

The same trap waits for any bright emissive surface that is also a light — a lamp, a window, a fire.
Nothing in the engine detects it; the symptom is a scene that is inexplicably too bright, and the
instinct is to reach for exposure, which hides it rather than fixing it.

**And a related tuning point that is not a bug.** A real sky is a strong light source, so switching
from a constant ambient to a physically-bright sky changes the total light in a scene substantially.
Every existing light intensity was tuned against the old constant. `games/scarp`'s `bin/sky.rs`
carries a `SKY_SCALE` for exactly this reason: it lands the sky near where the constant was *on
average*, while keeping what the constant never had — direction and colour. Turning both up at once
would be changing two things and learning nothing from either.

### A constant whose units are another constant is a coupling nothing checks

**Two bugs in one session, both silent, both producing a plausible wrong picture, and neither
failing a test.** The code that broke did not change in either case.

- `DIG_RADIUS` counted **cells**. Coarsening the ground from one-metre cells to two-metre ones for
  the low-poly look doubled every dig in world units without a line of the digging code being
  touched. A four-metre hole punches through a hillside and reaches the bottom of the streamed
  region, where there is no geometry at all.
- `SUN_DIRECTION` in `games/scarp`'s `bin/sky` was a vector worked out by hand from the scene's Euler
  angles. The sign of Y was inverted, so light travelled *upward* — the sun sat below the horizon and
  the generated sky had no sun in it.

> **The rule: state a quantity in the units it means, and derive the rest.** The dig radius is now in
> metres and converted; the sun direction is derived from the same three angles the scene uses,
> through `Mat4::from_transform`, so the two cannot disagree about what those angles *mean*.

What makes this class nasty is that the coupling is invisible at both ends. Neither file mentions the
other, nothing imports anything, and the compiler is perfectly happy. The only defence is not writing
the derived value down.

### An environment map is the *indirect* half of lighting

Anything in it that is **also** a direct light gets counted twice, and solid angle makes it easy to
get badly wrong rather than slightly wrong — see the table in the double-counting entry above. The
Scarp's sun is a `DirectionalLight` *and* a disc in its sky; the disc is small and modest for exactly
that reason.

The same trap waits for any bright emissive surface that is also a light: a lamp, a window, a fire.
The symptom is a scene that is inexplicably too bright, and the instinct is to reach for exposure,
which hides it.

**And one that is not a bug but reads like one.** `bin/sky`'s ground hemisphere returned *before* the
`SKY_SCALE` applied to the sky above it, so the lower half was about two and a half times too bright.
It showed as a near-white band along the horizon and a wash over everything facing downward — and it
read as **the terrain being too pale**, not as the sky being wrong. It survived two looks. What
exposed it was switching the ground from a noise texture to flat colour, because there was then no
texture variation left to blame it on.

> **Simplifying the picture, rather than studying it harder, is what found that one.**

### Solid geometry is invisible from inside, and that is what "seeing the sky" means

Surface extraction — `amadeo-voxel`'s surface nets — produces the **boundary** between solid and air.
Solid rock therefore contains no geometry at all; only its surface does. Put a camera inside it and
there is nothing between the camera and whatever is beyond.

Before ADR 0052 that was worse still: the boundary's faces point outward, so from beneath they were
backface-culled and the ground vanished entirely. The sky pass then filled the frame, and the result
reads exactly like **terrain that failed to stream** — which is the complicated part of the system,
and therefore where suspicion goes.

Two things now hold it down, and they are different claims:

- **ADR 0052** makes geometry two-sided, so being inside something is *dark* rather than transparent.
- **`modules/amadeo-camera`** keeps the camera out of geometry in the first place (Q27).

Neither alone is enough. A camera fully embedded in rock still sees very little, because there is
genuinely nothing there — the honest fix for *that* is keeping the player out of solid ground, which
is a different problem again.

### Reading a mouse is not reading a key, in three specific ways

`games/scarp`'s window layer is the worked example.

1. **`DeviceEvent::MouseMotion`, never `WindowEvent::CursorMoved`.** The latter reports a *position
   inside the window*, so it stops changing the moment the pointer reaches an edge and the view stops
   turning with it.
2. **Accumulate, and clear only once a tick has consumed it.** A mouse reports displacement since its
   last report, on the window's schedule rather than the simulation's — and the loop runs uncapped,
   so most frames advance no tick at all. Overwriting an action's value every frame throws away every
   report that landed between two ticks, which is most of them.
   `App::advance_real_time` returns the tick count, which is what makes "has anything read this yet"
   answerable.
3. **A pointer is a displacement; a key is a rate.** An action like `turn` is multiplied by a turn
   speed and the timestep, so full deflection is a fixed degrees-per-second. Feeding mouse movement
   through it caps how fast the view can move at whatever that speed happens to be. Mouse look writes
   rotation directly instead, and Q/E keep the rate — which is correct for a key that is either held
   or not.

### A high-water mark against a zero-based counter drops the first item

**The bug:** the first sound a world ever made was silent. Every one after it worked.

One-shot audio plays each `SoundPlayed` event once, tracked by a mark on the `Audio` service, because
the render rate is not the tick rate and the same event sits in the readable buffer across every
frame drawn during a tick. The first version stored *"the highest sequence already played"*,
initialised to `0`, and filtered with `sequence > mark`.

`EventClock` hands out sequence numbers **starting at zero**. So event zero — the first event of any
kind a world ever sends — is never greater than the mark, and is dropped forever.

```rust
// Wrong: "the highest already played", starting at 0.
.filter(|record| record.sequence > mark)

// Right: "the lowest not yet played", a half-open bound.
.filter(|record| record.sequence >= next)
```

**The general rule: express a watermark as the half-open bound, not the inclusive one.** `next` and
`>=` has no special case at zero; `last` and `>` needs the field to be an `Option` or the counter to
start at one, and both of those are things somebody has to remember.

What makes this worth its own entry is the *shape of the symptom*. It is not "one-shots are broken" —
it is one missing sound at the very start of a session, which a person would blame on anything else.
`the_very_first_one_shot_a_world_ever_sends_is_heard` exists so the next person gets a failing test
instead of a hunch, and it asserts the world has sent no events first so it cannot quietly stop
testing the boundary.

### When nothing can test it, commit the procedure instead of the intention

The kira audio backend is the first thing in this engine that **no test can verify**. CI has no sound
card, a headless run has no sound card, and even on a machine with a device nothing in the process
can read back what left through the operating system. There is no `render.capture` for audio and
there cannot be one.

The temptation is to write tests that *look* like they cover it — a test that submits a frame and
asserts no error, named `sound_plays`. That is worse than nothing, because it converts "unverified"
into "verified" in the mind of whoever reads the test list next.

What this engine does instead is two things, and both are worth copying for anything else that ends
outside the process:

**1. Move the judgement somewhere testable, and leave the untestable part mechanical.**
`VoiceTracker` exists for this. Deciding which voices are new, which have gone, and which merely
moved is fiddly and every mistake in it is inaudible until it is not — a voice restarted every frame
is a stutter at sixty hertz; a voice never stopped is a hum that outlives the thing making it. All of
that is in `tracker.rs` and is exercised headlessly. What is left in `kira_backend.rs` is "start
this, stop that, set the other", which is reviewable by reading.

The rule that follows: **when adding an audio feature, ask which of the two files it belongs in, and
the answer is almost always the tracker.**

**2. Write the listening procedure down, as an `#[ignore]`d test.**
`crates/amadeo-audio/tests/you_can_hear_it.rs`:

```
cargo test -p amadeo-audio --features kira --test you_can_hear_it -- --ignored --nocapture
```

It opens a real device, plays for a few seconds, and **prints the acceptance criteria before it
starts** — "it should circle you, get quieter as it swings away, and stop without a click". The
`#[ignore]` keeps it out of `cargo test --workspace`, so CI never tries; `--nocapture` is what makes
the printed criteria visible.

It is not a test in the sense the rest of the suite is. It is a procedure, and keeping it in the
repository rather than in somebody's shell history is the whole point: the next person to touch the
backend gets the check handed to them.

It still asserts everything up to the speaker — a device opens, a sound uploads, no frame submission
errors — so it catches real failures. And where a claim the procedure is watching for *can* be
checked one layer down, it is: `the_tracker_agrees_with_what_that_procedure_expects` is not ignored,
and if it is red there is no point listening.

**A note on how these read.** Every one of those files says plainly that the last step is a person's.
That is not modesty; it is the load-bearing part. A file named as though it covered more is how an
unverified subsystem becomes a verified one without anybody deciding.

### Hiding a subtree hides it — but only if you ask its ancestors

`UiNode::visible` is a field rather than a despawn on purpose: toggling a pause menu must not move
entities between archetypes on every keypress. That has a consequence which caught two different
systems, an hour apart.

`layout_ui` skips a hidden node **and everything under it**. Skipping means it does not write those
descendants' `ComputedRect` — but it does not *remove* them either, because removing a component is
the structural change the flag exists to avoid. So after a menu closes:

- the root says `visible: false`;
- every button inside it still says `visible: true`, because it is true — it is their *parent* that
  is not;
- and every one of them still carries the rectangle it had while the menu was open.

Anything that asks "should I be dealing with this node" by reading `node.visible` therefore gets the
wrong answer for every node except the one that was toggled. Both callers got it wrong:

- **the draw pass** kept drawing a closed menu's buttons, off stale rectangles;
- **`focusable_in_order`** let the focus land inside a closed menu, so the next `confirm` activated a
  button nobody could see. That is the pause-menu bug ADR 0063 *names in its own consequences*, and
  it was in the code that named it.

The fix is one function, `layout::ancestry`, walking upward and shared by both — so the draw pass and
the simulation cannot disagree about what is on screen. It reads `Parent` and a `bool` and no
rectangle, which is what makes it safe on the deterministic side.

**The general shape:** when a flag on a parent changes what its children mean, every reader has to
walk up. If two readers walk separately they will drift, and the drift will show up as one of them
being right.

### Pausing: what still runs, and how to say so

```rust
app.add_system(
    Stage::Simulation,
    system(NAVIGATE_FOCUS, navigate_focus).while_paused(),
);
```

A `Paused` resource set to true makes `App::step` skip everything in `Simulation` and
`PostSimulation` except systems that declared `.while_paused()` (ADR 0065). `PreSimulation` always
runs, so input is sampled and the game can unpause; `Render` always runs, so the menu is drawn.

Three things about it that are not obvious:

- **The tick keeps advancing.** A paused tick samples input, moves the focus if asked, and does
  nothing else. That is load-bearing rather than lazy: menu navigation is hashed state driven by
  input that `amadeo-input` records **per tick**, so a frozen counter would leave a keypress in a
  menu with nowhere in a replay to live. It also means there is no backlog to burst through on
  unpause — `advance_real_time` never banks anything.
- **`PostSimulation` is skipped too**, and forgetting it is the interesting mistake. `play_footsteps`
  reads the character's velocity, which does not change while paused, so a game that skipped only
  `Simulation` would tap out footsteps forever in a room nobody is walking through. The symptom is
  audio; nothing about it points at the scheduler.
- **The engine never writes `Paused`, and has no idea what a screen is.** A game keeps its own
  resource — `games/atrium`'s `Screen` is `Playing | Paused | Quitting` — and one system projects it
  onto `Paused` and onto the menu's visibility, every tick. One writer for everything derived is what
  stops a menu being up over a game that is still running.

If a system mysteriously stops running, `amadeo schedule` reports `runs_while_paused` per stage, so
the answer does not require reading anyone's source.

### Animating something, and the two surprises in it

A `.anim` file names a component and a field. Nothing in `amadeo-anim` knows what a `Transform` is:

```text
tracks
  - component "PointLight"
    field "intensity"
    interpolation Linear
    keys
      - time 0.0
        value 22.0
      - time 1.1
        value 23.5
```

Put an `AnimationPlayer` on the entity, name the clip's asset id, and add three lines of setup — a
`ClipCache` (which `load_scene` installs itself), an `Animatable` allow-list naming the component
types clips may write, and the `animate` system in `Stage::Simulation`.

**The first surprise: animation is simulation.** The reflex from `GlobalTransform` and `ComputedRect`
says a computed value should be derived and out of the state hash. Not here — a clip that moves a
`Transform` is a **moving platform you can stand on**, physics reads it the same tick, and a save has
to restore where it was. So the clock is hashed, what it writes is hashed, and `animate` goes in
`Simulation`, not `PostSimulation`.

**The second: a missing clip changes the state hash.** Every other missing asset is cosmetic — a
missing texture draws magenta, a missing sound is silence. A missing clip means a platform does not
move, and every hash after it differs. That is why `ClipCache` has no placeholder, why `load_clips`
installs itself rather than waiting for a setup line, and why `ClipCache::failures` and
`Animatable::missing` both exist. If something is not animating, read those two before anything else.

### An empty list had no spelling either, and the engine wrote files it could not read

The sibling of the entry below, found the same afternoon and much worse. `inline_value` joined a
list's elements with spaces — and **joining nothing gives the empty string**, so an empty `Vec`
anywhere in a value wrote as a field name with a trailing space and no value at all. This format
does not have such a thing, and it parses back as `Unit`.

Every registered event queue holds two empty lists at rest. So `amadeo snapshot` followed by
`amadeo status --from` **failed on the engine's own demo game**, and had done since events were
first registered. Nothing noticed, because until save and load nothing had ever restored one.

An empty list is now written `[]`, checked by name on the way in — exactly as `Unit` is written `()`,
and for the identical reason. The format already had this shape; an empty list had simply been left
out of it.

**The general shape, and it is the third time this project has met it:** a value that "obviously"
serialises to nothing serialises to *nothing*, which is not a value. Any encoding with a
"just join the parts" path has this bug waiting in it for the empty case. The way it was found is
also the general answer — **build the thing that uses the format end to end**, because a
round-trip test written against hand-made values will happily never contain an empty one.

**And an empty *map* had exactly the same hole**, found the same way an hour later: `Facts` in
`modules/amadeo-behaviour` starts empty, so a monster that had never perceived anything could not be
written to a file. It is `{}` now, beside `[]` and `()`. Three explicit markers, one rule — *a field
with no value is not something this format has* — and the lesson is that finding one instance of this
shape is a reason to go looking for its siblings rather than to close the ticket.

### A one-element list has no inline spelling, and the type is what fixes it

`value 22.0` is one token. Layer 1 of the scene format has no schema, so it produces a **scalar**,
always — there is no way to write a `Vec<f32>` with one element in it.

`Vec<T>::from_value` therefore accepts a single value as a one-element list. That is the type
resolving an ambiguity the text genuinely has, which is the same job `f32::from_value` accepting an
integer already does. Worth knowing because the symptom is unhelpful: a lamp that did not flicker,
with everything else in the file working. `amadeo check` is what named it —
`list<f32>: expected list, found 64-bit float` — which is the validator paying for itself.

### A mechanism nobody can reach is not a mechanism — three instances in one session

`PhysicsBackend::reset` had been documented since ADR 0036 as the thing that makes a physics game
snapshot-able. The backend is private on purpose, so **no game could call it**; the only callers were
tests holding a backend directly. `Physics::reset` is the one-line pass-through that was missing.

That is the same shape as `ClipCache::failures` and `Animatable::missing`, which ADR 0066 made "the
whole diagnosis" for animation that silently does nothing and which nothing could read until
`anim.describe`. And as `AnimationPlayer::is_running`, nearly duplicated the moment a second caller
appeared.

**When you write a doc comment saying a thing is load-bearing, check that the thing can be reached
from where it is needed.** The comment is not the mechanism.

### …and when you check what a mechanism is worth, it may not be what the comment says

The follow-up is the more interesting half. `reset`'s documentation said a solver carrying another
world's contact caches would simulate differently after a restore. Measured against a settled,
sleeping stack of six dynamic bodies, **a warm solver matches a cold one exactly**.

That is not a bug — it is ADR 0036's own contract paying off. `PhysicsBackend::step` is handed the
complete input and returns the complete output, and the trait requires that a backend keep no state
*which cannot be rebuilt from the bodies it is given*. A solver honouring that has nothing to go
stale. **The decision that makes physics deterministic is what removes the hazard.**

`reset` is still right to call: it drops static geometry, which is derived data belonging to a level
rather than to a body, and a game that streams terrain would otherwise keep the ground of the level
it just left. But the reasoning in the docs now matches the measurement, and
`reset_clears_the_solver.rs` *reports* the contact-cache result rather than asserting it — a claim
about somebody else's solver at a pinned version is not a thing to fail a build over.

### A save and a snapshot are the same file read two different ways

This is the pattern ADR 0069 introduced, and the part worth carrying around is *why* it is two entry
points rather than one lenient reader.

`amadeo_snapshot::restore` is strict: every field required, every name known, and the recorded state
hash enforced. That last check is the format's whole integrity story — it turns "the restore silently
produced a slightly different world" into an error at the moment it happens, rather than into a run
that poisons every assertion after it.

`amadeo_snapshot::restore_save` reads the same bytes for a **player's** save, which has to survive
the game being patched.

**The thing that is not obvious, and cost a wrong answer before it was measured:** being lenient
about fields is not enough on its own. Filling in a missing field gets past the first error and
straight into a second one, because a defaulted field is still a field, it is still hashed, and so
the rebuilt world cannot hash to the number the file recorded:

```
strict:   BadComponent { reason: "missing field `b`; required fields are a, b" }
lenient:  HashMismatch { expected: 6783642539998936112, actual: 13968525498961532720 }
```

The world was rebuilt **correctly** and then rejected. So the answer is not to drop the check but to
make it **conditional on the layout**: the file records a fingerprint of the shape of everything in
it, and

- **fingerprint matches** → nothing has changed shape, so there is no version gap to blame for
  anything, and the load behaves exactly like `restore`;
- **fingerprint differs** → fields are defaulted, unknown names dropped, renames applied, everything
  reported, and the recorded hash is not enforced.

Two consequences worth holding on to. The good case — a player who has not updated — keeps the full
check, so **leniency costs something only when it is actually needed**. And the strict path stays
exercised by every ordinary load, which is why `an_ordinary_load_takes_the_strict_path_and_reports_nothing`
exists in `games/atrium`: a conditional check that quietly stops applying is worse than no check.

**When you add a field to a component**, the section above on Q32 still applies to `.scene` and
`.material` files — those are authored data and are meant to be strict. What changed is that a
**save** now survives it, and says so.

**When you rename one**, add a line to a `.redirects` file:

```text
amadeo-redirects 1
component Lantern Torch
field CharacterController top_speed max_speed
```

Component redirects apply first, and a field redirect names the component by its **new** name. Get
that backwards and you have a file that looks correct and silently does nothing. Without a redirect,
a rename is *silent data loss* — the old value is dropped and the new field is defaulted — which is
why `without_the_redirect_the_same_rename_loses_the_value` sits next to the test that fixes it.

**What the engine will not do is guess an enum.** `default_value` covers scalars, structs, lists,
maps and `Option`, and refuses an enum, because the first variant is a guess with gameplay meaning —
`ShadowMode::Off`, `Bus::Effects` and `Screen::Playing` are each plausible and each wrong somewhere.
A component that gains an enum field is reported by name, and a person decides.

### Taking a thing out of the world is removing one component

`modules/amadeo-inventory` stores an item by **removing its `Transform`**, and nothing else. It
works because three passes in three different crates all require one and skip an entity that lacks
it:

| Pass | Query |
|---|---|
| `collect_meshes` | `(&Mesh, &Transform, …)` |
| `step_physics` | `(&RigidBody, &Transform, …)` |
| `propagate_transforms` | `world.get::<Transform>(entity)`, `continue` when absent |

So an item in a bag keeps its mesh, its collider, its `Interactable` and all its own state, and
putting a `Transform` back drops it with everything intact. **Nothing is converted between two
representations**, which is the thing to hold on to: the design that stores items as *values*
converts on the way in and on the way out, and that is where the bugs live.

This is worth knowing outside inventory too. If you ever want an entity to exist but not be *in* the
world — a spawner's template, a disabled prop — the `Transform` is the switch, and it is already
enforced by everything that matters.

The obligation it creates: `a_stored_item_is_invisible_to_every_world_pass` pins the property, and
it belongs to those three crates rather than to the module. If one of them ever stops requiring a
`Transform`, an item in your bag starts being drawn at the world origin.

### Two things a first user found in a module that had none

`modules/amadeo-interaction` was built in session 17 with no game using it. `games/atrium`'s brass
key was its first, and it found both of these immediately — which is what `CLAUDE.md`'s rule about
treating the first user as a review is for.

**The sweep must ignore the body, not the interactor.** An `Interactor` is normally a *child* — a
camera or a reaching point on a character — and such a child has no collider of its own. So ignoring
the interactor ignored nothing, the cast started inside the parent's capsule, and every result came
back at `fraction: 0.0` against the player. `Looking::at` stayed `None` for ever, which looks exactly
like standing too far away. Every existing test put the interactor on a lone entity with no collider
anywhere, so **the arrangement the module's own docs called usual was the one arrangement nothing
covered** — a shape worth checking for in any module: what does the documentation say is typical, and
is that what the tests actually build?

**A sweep follows the interactor's forward, so reach is a band around that line** — and whatever an
object rests on blocks the sweep to it. A key on a plinth cannot be reached by an interactor at
plinth-top height, because the plinth is in the way for the whole 1.5 m to the middle. The Atrium
puts the interactor on a child *above* the plinth top.

**And a correction worth reading, because it is about how a limitation gets invented.** The first
version of this section said an item on the floor was unreachable and that "looking down is not
built". Both were wrong, and they went into three documents before anybody checked. An interactor is
an **ordinary entity with an ordinary `Transform`**, so pitching it in a scene file aims the sweep
downwards and a key on the floor is reachable with nothing built —
`aiming_down::a_pitched_interactor_reaches_it_with_nothing_built` pins it at −20°.

The angle is a real tuning number rather than a switch: level misses the floor, and −35° misses it
again, because the sweep is a line that has already passed under the key by the time it arrives.
Both ends are pinned, so nobody reads the middle case as "any downward angle works".

What *is* missing is a pitch **driven at runtime** by a camera or a mouse, which belongs to whatever
does the aiming rather than to this module. The general lesson: when a component composes out of
`Transform` and `Parent`, ask whether the thing you are about to call unbuilt is already authorable
— the answer here was one test away, and the wrong answer had already been written down as fact.

### A font that is not declared draws nothing, and no test will tell you

`FontCache::new()` starts with an **empty** database — deliberately, because a game that falls back
to whatever the operating system has installed looks different on every machine (ADR 0062). The
cache fills itself through `FontCache::ensure`, which reads the bytes **out of the asset store**. So
a font reaches the screen only if the scene **declares it in its `assets` block** (ADR 0021), by id:

```text
assets
  BebasNeue-Regular
```

Miss that line and every `Text` node shapes to **nothing at all**. Not a substitute typeface, not a
placeholder box — ADR 0060's rule, because a wrong typeface silently replacing the right one is how
a look drifts unnoticed.

**The trap is that nothing fails.** The scene loads, `collect_ui` runs, the layout is correct, and
`Text::content` holds exactly the right string. Session 18 shipped the Warren's HUD with a full
green suite and a `Text` whose content was asserted in three tests, and the screen had no words on
it. The tests assert what a line *says*; only a capture can tell you whether it was *drawn*.

Two things follow. When adding text to a game, declare the font in the same edit — it is the line
most easily forgotten because it is nowhere near the `Text`. And when a HUD is invisible, check the
`assets` block before anything else: `FontCache::failures` names the id, and is the whole diagnosis.

### `amadeo check` does not reach prefab override rules

`check` validates a scene against the game's **real component schema**: names resolve, fields exist,
values fit. That is a lot, and it is why ADR 0071 makes it the test of the level generator.

**It is not a load.** Session 18 generated a scene that `check` reported `ok` and that then failed
at load with:

```text
entity `room_0_n1` declares `Transform`, but its prefab already puts that component on the root.
Write `override Transform` to replace it
```

Every piece puts a `Transform` on its own root, and ADR 0029 refuses to replace one silently — an
override has to be spelled out so it is visible in the file (I1). The generator emitted the bare
form. Schema-valid, and wrong.

Two things to take from it. **When you write a scene by machine, load it as well as checking it** —
`amadeo capture --ticks 1` is the cheapest load there is. And when instancing a prefab, assume the
piece already has `Transform` on its root, because every piece worth instancing does.

**Session 19 found the next rung down: a load is not a game.** The generated level loaded, drew, and
captured cleanly at tick 1 while every collider in it was walking away from its own geometry. What
found that was a test that stood the player on the floor and another that walked them out of the
door. The ladder, cheapest first, and each rung sees something the one below cannot:

| | Catches |
|---|---|
| `amadeo fmt --check` | The writer's own output being non-canonical — three real faults, session 19 |
| `amadeo check` | A component or field the schema does not have |
| `amadeo capture --ticks 1` | Anything that stops the scene loading at all |
| `amadeo capture --ticks 5` | Children placed at their local transforms, and most lighting mistakes |
| A test that stands on the floor | Geometry that is not where the file says |
| A test that plays the loop | Everything else |

### A snapshot taken before the world is finished records a world that never existed

`games/warren` keeps the world exactly as it loaded so that a run can be started over, and captured
it at the end of `build_from_scene`. Restoring it failed with *"something about this build differs
from the one that took the snapshot"* — which was true, and was this: every caller installs an input
driver **after** `build_from_scene` returns, `amadeo_input::install` inserts `InputState`, and
`InputState` is a **hashed resource**. So the file described a world that was one resource short of
any world that ever ran, and ADR 0069's integrity check refused it.

Two things to take from it:

- **A snapshot is of the whole world, including the parts a caller has not added yet.** If you
  capture one during construction, capture it after *everything* is in — or insert the missing piece
  yourself first, which is what the Warren does with a default `InputState`.
- **The check earned its keep on the first thing that was not a deliberate test of it.** It is easy
  to read an integrity check as ceremony; this one turned a silent, subtly-wrong restore into an
  error message that named the cause.

### A game with more than one menu must not reimplement "which item is first"

`navigate_focus` deliberately will not seat the highlight when nothing is focused (ADR 0063), so a
game does it. `games/atrium` wrote its own three-line version and it is correct — because with one
menu, the only focusable items in the world are the ones that are on screen.

With **three** menus in one scene, two are hidden at any moment, and a highlight inside a hidden one
is unreachable: the player presses a direction and watches nothing happen. So use
`amadeo_ui::focusable_in_order`, which is the same list `navigate_focus` walks and already skips
anything hidden *anywhere above it*. Reimplementing it means the game and the engine can disagree
about what is reachable, and the disagreement has no symptom.

### The formatter is a regression test, if something runs it

ADR 0071 said `amadeo fmt --check` on generator output was "a free regression test". Free only if
something runs it — and until `what_the_generator_writes_is_already_canonical` existed, nothing did.
It immediately found three faults in the writer nothing else could see: quoted prefab ids where
canonical form is bare, an `assets` block sorted by the *constant names* rather than by the asset ids
they hold, and a spare blank line at the end.

None of those breaks anything on its own. Together they mean the first person to run `amadeo fmt` on
a generated level gets a diff that is entirely noise, and the next regeneration produces the reverse —
so the two tools fight and neither result is reviewable, which is I2 gone for generated content.

**If you write a file format by hand, assert that the canonical writer would write the same bytes.**
It is two lines: parse your output, re-emit it, compare.

*(More entries land as the engine takes shape: asset handles.)*

---

## Golden replays: how behaviour is regression-tested

This is the mechanism the whole project rests on, so it is worth understanding before you change
anything in a simulation system.

A **replay file** is a recorded stream of player actions plus expected world state hashes at
particular ticks. It is plain text and you can edit it by hand:

```text
amadeo-replay 1
tick-rate 60
seed 1234
ticks 300

0 axis move_x 1.0
20 button jump down
22 button jump up

checkpoint 60 667176d875001e4c
checkpoint 300 0b6e103ad3a5261b
```

Replaying it re-runs the simulation through *exactly* the same code a live player would drive, and
asserts the state hash at each checkpoint. If any simulation behaviour changed, the hashes stop
matching.

**When a golden test fails, do not regenerate the file to make it pass.** A changed hash means the
simulation now behaves differently. That is either a bug you just introduced, or an intended change —
and if it is intended, every other recorded replay in the project is invalidated at the same time,
which is worth knowing before you commit.

Once you are sure the change is correct:

```bash
UPDATE_GOLDEN=1 cargo test -p amadeo-app --test golden_replay
```

Then say so explicitly in the commit message.

**Things that will break a golden replay without being "wrong":** changing `FIXED_DT` (ADR 0007),
changing what `World::state_hash` includes, changing the hash algorithm, or changing system order.
Each of those is a deliberate decision with an ADR attached, not something to do casually.

**A useful debugging habit:** if a replay diverges, add more checkpoints. The first failing checkpoint
brackets the tick range where behaviour changed, and from there `App::step` plus `world.iter` narrows
it down quickly. `amadeo replay` reports *every* failing checkpoint, not just the first, which is
usually enough to bracket it in one run.

### If a golden replay fails on CI but passes locally, check line endings first

This cost a red CI afternoon, so it is written down.

The symptom is a `recording_matches_the_committed_golden_file` failure whose two strings have
**identical checkpoint hashes** and differ only in `\n` versus `\r\n`. That is not a determinism
failure — it is git rewriting the file on checkout.

`core.autocrlf` defaults to **true** on Windows, and GitHub's `windows-latest` runners ship with it
on. Without `.gitattributes`, a fixture committed with LF arrives on disk as CRLF, so a byte
comparison against freshly generated text fails. It reproduces nowhere on a machine that has
`core.autocrlf=false`.

`.gitattributes` at the repository root fixes it with `eol=lf`, and **must not be removed.** Line
endings are part of the `.replay` and `.scene` formats, not a platform preference — invariant I2
says an unchanged file saves byte-identically, which is meaningless if git rewrites it per platform.

The tell that it is this and not a real regression: **only the Windows jobs fail.** Linux checkout
does no conversion, so `test (ubuntu-latest)` stays green. The golden test now checks for CR bytes
before comparing and says so directly.

### The two halves: in-process and separate-process

The `cargo test` version above proves a recording survives a **rebuild**. It cannot prove one
survives a **fresh process**, because it never starts one — and process state (address-space layout,
hash seeds, anything cached in a static) is precisely where the remaining nondeterminism hides.

That is what `amadeo replay` is for:

```bash
amadeo replay games/quad-demo/replays/wander.replay
```

It launches the game binary, plays the recording, and checks the hashes in a process that did not
exist a second ago. CI runs it in the determinism job. Passing both is the actual claim.

**Writing a replay by hand.** The format is designed for it (`amadeo_input::Recording`). The awkward
part is the checkpoint hashes, which you cannot know in advance — so write the file with zeros, run
`amadeo replay`, and copy the `got` values out of the mismatch report. `wander.replay` was made
exactly that way.

**If your game needs replays, read the seed before building:**

```rust
let seed = amadeo_app::requested_seed().unwrap_or(DEFAULT_SEED);
let mut app = App::with_seed(seed);
```

`App::with_seed` fixes the seed at construction, which happens *before* the agent handover — so it
cannot be supplied afterwards. Skip this and a replay recorded at another seed fails with a clear
mismatch error rather than a mysterious divergence, which is survivable but annoying.

---

## If you get stuck or disagree with something Claude did

- **`git log` and the commit messages** are written to explain *why*. Start there.
- **`docs/adr/`** holds the decision records. If something looks arbitrary, there's likely an ADR
  explaining the trade-off and what was rejected.
- **`docs/06-open-questions.md`** lists what's deliberately undecided — so if something feels
  half-finished, check whether it's intentionally pending.
- **The eight invariants in `CLAUDE.md` §2** are the rules Claude is bound by. If code appears to
  violate one, that's a real bug worth raising — those are load-bearing.
- **Nothing here is sacred except the invariants.** If a design feels wrong to you, say so. The
  plan → build → re-plan cycle exists precisely for that, and an ADR can be superseded.
