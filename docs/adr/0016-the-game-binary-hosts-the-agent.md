# ADR 0016 — The game binary hosts the agent; the CLI launches it

**Status:** Accepted · **Date:** 2026-07-31 · **Resolves:** Q14

## Context

`docs/05-roadmap.md` lists `amadeo-cli` with `describe`, `check`, and `inspect` as though `amadeo`
were a standalone tool that could answer questions about a project on its own. ADR 0011 makes that
impossible and the roadmap did not notice.

**Game logic is compiled into the game binary.** A game's components are Rust types in the game
crate. A separately-compiled `amadeo` binary has never linked them, so it cannot construct them,
cannot describe their schema, and cannot read their values out of a world. It can only ever describe
the engine's own types — which is the least interesting half of the answer, since Pillar 2 exists to
tell an agent about *the game it is working on*.

The commands split cleanly along that line:

| Command | Needs the game's registry? |
|---|---|
| `new`, `fmt` | no — scaffolding and pure syntax |
| `check`, `describe`, `inspect`, `run`, `replay` | **yes** |

Two further facts, found while looking at the code rather than at the roadmap:

**This is the same problem ADR 0010 already solved once.** The event loop lives in the game binary
because invariant I6 forbids `amadeo-app` from depending on `amadeo-render`, so no engine crate can
own a loop that drives both. The identical argument applies here: the game process is the only place
that holds the registry, the world, and the systems at the same time. "The game binary hosts the
agent" is therefore not one option among three — it is the substrate all three are built on. The only
real question is what wraps it.

**The registry currently has no home.** `ComponentRegistry` is constructed ad hoc in tests and
nowhere else. `games/quad-demo` registers nothing at all: its `Velocity` and `Player` exist in the
ECS but in no registry, so even a perfectly-wired `describe` would report an empty schema for them.
Any answer to this question has to say where a game's registry lives.

## Decision

### 1. The game binary is the agent host

Every game binary accepts a standard agent invocation. `amadeo-agent` provides the server; the game
hands it a built `App`. Nothing about a game's components has to be known by anything outside the
game's own process, ever.

This falls out as the thing the CLI launches, so it is not extra work — it is the work, with a
command-line entry point attached.

### 2. `App` owns the `ComponentRegistry`

`App` gains a registry and `App::register_component::<T>()`. A game registers a component in exactly
one place, and `amadeo-agent` reads the registry off the app it was handed.

The alternative — the game builds a registry separately and passes both — has a game registering in
one place and spawning in another, with nothing connecting them. A component that works fine at
runtime but is invisible to `describe` is precisely the trap ADR 0013 made `Component: Reflect`
compiler-enforced to avoid, and it would reintroduce it one level up.

### 3. `amadeo-cli` launches the game and speaks JSON-RPC to it over stdio

`amadeo describe` finds the project's game package, spawns it in agent mode, connects, asks, prints
the reply, exits. There is one `amadeo` command, and it works the same way in every project.

`fmt` and `new` stay standalone — they are syntax and scaffolding, and requiring a compiled game to
format a file would be absurd.

**Finding the binary:** the CLI runs `cargo run -p <package> -- --amadeo-agent`, taking `<package>`
from an `amadeo.toml` at the project root, with `--package` overriding it. Going through cargo means
the binary is rebuilt if stale, so the schema can never describe code that is no longer there. The
measured cost is 0.9–3.2 s (ADR 0011), which is cheap enough that always-correct beats fast-and-maybe-
wrong. This particular piece is easy to change later if it grates.

### 4. The first transport is one-shot batch, not a live session

Each CLI invocation is one fresh process running one deterministic run. Commands carry an explicit
tick count rather than attaching to something already going:

```text
amadeo describe
amadeo inspect --scene levels/one.scene --ticks 600 --query Transform2d
```

This is *more* reproducible than attaching, not less: a batch run is a replay-equivalent run, so the
same command twice is the same answer twice, and a question an agent asks is a question it can put in
a test. An attached session has a history that the transcript does not record.

It also covers what M1's exit gate actually asks for — building a small game and verifying it through
`inspect`, headless runs, and `render.describe`, none of which need interactive stepping.

**Deferred to when M4's editor needs it:** the persistent session, and with it `sim.step`,
`sim.pause`, `sim.set_speed`, and the mutating `world.*` calls. Those need a connection that outlives
one question. The method dispatch is written so adding them is adding methods, not reworking the
transport.

### 5. The JSON parser is hand-written

`amadeo-agent` already hand-writes the JSON *writer*, with sorted object keys so a dump is diffable.
The parser joins it, in the same crate.

This matches how everything load-bearing in this engine has been built — PCG32, FNV-1a, the `.replay`
format, the `.scene` format — and for the same reason: `CLAUDE.md` §6 requires Justin to be able to
read and fix this code, and a subset JSON parser is a few hundred well-commented lines. JSON-RPC needs
a subset of JSON, not the general case.

## Consequences

- **The transport exists from the first command, so I5 is real rather than retrofitted.** The editor
  at M4 is required to be an RPC client with no privileged path (ADR 0003). Building the transport now
  means the CLI and the editor are provably speaking the same protocol from the start, instead of the
  CLI growing shortcuts that the editor then has to be denied.
- **`docs/protocol/` gets written against the batch method set first**, and versioned. `describe`,
  `world.entity`, `world.query`, `scene.load`, `replay.play`, `replay.hash`.
- **The separate-process replay check carried over from M0's exit gate closes here.** `amadeo replay`
  runs the golden fixture in a genuinely separate process, which is the half the in-process golden test
  could not cover.
- **Every game needs one line at startup** to check for agent mode and hand over if present. Small,
  but it is boilerplate, and it is the second thing every game must remember after the event loop.
  When `amadeo-shell` is eventually extracted (ADR 0010 anticipates it once a second game exists),
  this belongs in it.
- **`amadeo check` costs a compile.** Validating a scene file means building the game. Accepted: the
  measurement says seconds, and the alternative is a cache that can lie.
- **Determinism is unaffected.** A batch run is an ordinary headless run of the existing deterministic
  loop. The agent layer stays read-only until the mutating calls land, so asking a world a question
  still cannot perturb it.
- **`amadeo-cli` sits at the top of the crate graph** and depends on `amadeo-agent` for the protocol
  types. It does *not* depend on any game, which is the whole point — it launches one.

## Rejected alternatives

**The game binary hosts the agent, with no CLI proxy at all** (`cargo run -p mygame -- describe`).
The smallest possible M1: no transport, no launcher, working in an afternoon. Rejected because
`amadeo describe` stops being one command — every project invents its own invocation, and the agent
has to learn each one. Worse, M4 needs the transport regardless, so this pays for the capability
twice and gets a period of CLI/RPC divergence in between, which is exactly the drift I5 exists to
prevent. It is not discarded so much as absorbed: it is decision 1, and the CLI is a client of it.

**A manifest generated at build time.** The game writes its schema to a file; the CLI reads it
offline and fast. Rejected because it is a cache, and a cache of "what components exist" that has
gone stale describes a component that is no longer there — the plausible-but-wrong failure that
Pillar 2 exists specifically to eliminate. Keeping it honest means tracking staleness against the
binary, which is real machinery for a saving that disappears once the transport exists anyway.

**A persistent live session as the first transport.** Gets `sim.step` and the mutating calls in M1
and delivers the full I5 surface immediately. Rejected for now because it is more work than M1's exit
gate needs, and because an attached session is the less reproducible shape — worth building when a
caller genuinely requires it (the editor does; an agent verifying a game mostly does not).

**`serde_json`.** Boring, correct, well-tested, and would save a day. Rejected because it would be
the first real external dependency in the engine graph beyond `thiserror`, it pulls serde's derive
machinery in behind it, and the parser it replaces is small and completely legible. The cost of
writing it is paid once; the cost of a dependency in the agent layer is paid at every future
question about what the protocol does.
