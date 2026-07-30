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

*(More entries land as the engine takes shape: the reflection derive macro and asset handles.)*

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
it down quickly.

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
