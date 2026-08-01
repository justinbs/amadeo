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

  entity a3 "Door" from prefabs/door_metal
    override Door              # only valid on an entity with `from`
      locked true

  entity a4 "Wanderer"
    Enemy
      state Patrol             # a bare word is an identifier / enum variant
      label "Patrol"           # a quoted word is a string. Different things.
      waypoints
        - 0.0 0.0              # a list whose elements are themselves lists
        - 4.0 0.0
```

**Two rules worth internalising:**

- **Bare words are identifiers, quoted words are strings.** `state Patrol` sets an enum variant;
  `label "Patrol"` sets text. The file says which, so no schema is needed to tell them apart.
- **`1` is an integer, `1.0` is a float.** The decimal point is what carries the type.

**Canonical form** (`amadeo fmt`): fields sorted by name, components sorted by name, **children in
declaration order** — siblings are a sequence and their order is meaningful, so sorting them would
destroy information.

**Indentation is structure, so `amadeo fmt` cannot repair it.** A mis-indented line is ambiguous
rather than untidy, and you get a line-numbered error instead of a guess. Tabs are rejected outright.

**Two layers.** `amadeo-scene` today is layer 1 — syntax only, no schema. It will happily parse a
scene naming a component that does not exist, which is what lets `amadeo fmt` work on a file whose
module is not loaded. Checking that `Transform` exists and has a `translation` field is layer 2,
against the reflection registry, and is not built yet.

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
