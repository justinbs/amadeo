# ADR 0022 — A game finds its asset directory by walking up to `amadeo.toml`

**Status:** Accepted · **Date:** 2026-08-02

## Context

ADR 0020 settled what names an asset and ADR 0021 settled how one gets loaded without breaking I3.
Building the scan surfaced a third question neither had asked, and `STATUS.md`'s claim that the
loading half had "no open decisions left in it" was wrong on this one point.

A game names its asset directory with a relative path — `assets`. **Relative to what?**

The obvious answer is the working directory, and the obvious answer is wrong, because the working
directory is different in every way a game gets started:

| how it was started | working directory |
|---|---|
| `amadeo describe …` | the project root — the CLI sets it with `Command::current_dir` |
| `cargo run -p quad-demo` from the repo root | the repo root |
| `cargo run` from inside `games/quad-demo` | `games/quad-demo` |
| running `target/debug/quad-demo.exe` directly | anywhere at all |

Two of those find the assets and two do not. It is a bad failure to debug because the game is not
*wrong* — it is looking somewhere else, and says so only as "that directory does not exist".

This matters more here than in most engines because of ADR 0016: the same question gets asked by two
different processes. The CLI resolves the project to know which package to launch; the game resolves
the asset root to know what to scan. If they use different rules they can disagree about which
project they are standing in, and every answer after that is subtly about the wrong thing.

## What other engines do

**Bevy** reads `BEVY_ASSET_ROOT`, then `CARGO_MANIFEST_DIR`, then falls back to the executable's own
directory. It works, and the fallback is right for a shipped game. Two things make it a poor fit
here. First, the answer depends on the environment the process was launched with, which is invisible
in a bug report and easy to get wrong from an IDE or a debugger. Second, `CARGO_MANIFEST_DIR` is
per-*crate*, not per-workspace: in a workspace shaped like this one it points at `games/quad-demo`,
not at the repository root. That is a coherent design — assets live beside each game crate — but it
is not what anyone guesses, and Bevy's own issue tracker describes setting that variable by hand as
"unsanitary" and steers people to configure the path in code instead.

**Godot** anchors on a marker file. `res://` is *defined* as the directory containing
`project.godot`. No environment variables, no working-directory dependence, one rule that holds
whether you launched from the editor or ran the exported binary.

## Decision

**Amadeo takes Godot's approach: walk up from the working directory for `amadeo.toml`, and resolve
the asset directory relative to whatever contains it.**

The order, in full:

1. Walk up from the working directory looking for `amadeo.toml`. This is the development case.
2. Failing that, walk up from the **executable's** directory looking for the same marker. This
   catches `cargo run` invoked from a subdirectory, where the working directory is below the project
   but the binary still lives inside it.
3. Failing that, use the executable's own directory. This is the shipped-game case, where the
   manifest was never packaged — Bevy's fallback, and it is the right one.
4. Failing even that, the working directory.

Which rule fired is recorded and reported by `assets.list` as `root_anchor`, because *"I looked in
the wrong place"* and *"the files are missing"* have identical symptoms and different fixes.

### Why this project in particular should use the marker

**It already has one, and already walks up for it.** `amadeo-cli`'s `Project::discover` finds
`amadeo.toml` exactly this way, so that `amadeo describe` works from any subdirectory. Adopting a
second, different rule for the game side would be inventing a disagreement.

**It needs no shared code to stay consistent.** `amadeo-cli` does not depend on `amadeo-app` — that
is ADR 0016's separation and it is load-bearing. A rule that required the two processes to share a
manifest *parser* would have forced a new crate or pushed project discovery somewhere it does not
belong. Finding a marker file needs no parser at all: it is ten lines, stated identically in two
places, and the thing they must agree on is a filename.

### Which directory is named in code, not in the manifest

The game says `app.scan_assets("assets")`. The manifest is not extended with an `assets = …` key.

This looked wrong at first against I1 — text files are the source of truth — but the manifest is
about locating a *project*, and which directory a particular game keeps its assets in is a fact
about that game, in the same category as which components it registers and which systems it adds.
Those are already in code, and ADR 0011 makes editing that code the normal way to change a game, for
an agent as much as for a human. Adding a manifest key would mean two places to look and a way for
them to conflict.

## Consequences

- **A game finds the same assets however it was started.** This is the whole point, and it is
  testable: the resolution rule is a pure function of a starting directory.
- **The CLI never needs to know where assets are.** `amadeo assets` and `amadeo import` ask the game,
  which is the process that knows. That is the same division `scene.check` already uses — the game
  holds the knowledge, the CLI holds the filesystem.
- **`amadeo import` costs a game build**, because it learns the root by asking. Measured at 0.9–3.2 s
  (ADR 0011). The alternative was a second implementation of the rule in the CLI that could drift
  from the game's, which is the failure this ADR exists to prevent.
- **A project nested inside another resolves to the nearest marker**, which is what "the project I am
  in" should mean.
- **A missing asset directory is an error, not an empty catalogue.** A mistyped path that quietly
  catalogues nothing reads exactly like "this project has no assets" — the plausible-but-wrong
  answer Pillar 2 exists to eliminate.
- **A shipped game needs its assets beside the executable.** That falls out of rule 3 and is what
  packaging will have to do anyway. Export is M5's problem and this does not constrain it.

## Rejected alternatives

**Environment variables, as Bevy does.** Genuinely flexible, and the standard answer in the Rust
ecosystem. Rejected because it makes the answer depend on invisible process state, and because
`CARGO_MANIFEST_DIR` resolves per-crate in a workspace — so the natural-looking configuration would
silently mean `games/quad-demo/assets`. An environment variable can still be added later as an
override without disturbing this rule; it is a strictly larger design.

**The CLI passes `--assets <absolute path>` at launch.** There is real precedent: `--replay` is made
absolute by the CLI for exactly this reason, with a comment saying so. Rejected because it only
solves the case where the CLI is the launcher. `cargo run -p quad-demo` passes no such flag, so a
fallback rule is needed anyway — and once the fallback exists, the flag is a second mechanism that
can disagree with it.

**Resolve against the working directory and document it.** Simplest possible thing. Rejected because
"run this only from the repository root" is a rule nobody remembers and nothing enforces, and the
failure is a confusing error rather than a caught mistake.

**Put `assets = "assets"` in `amadeo.toml`.** Would satisfy a strict reading of I1 and would let the
CLI resolve the root without launching the game. Rejected because it needs a manifest parser
reachable from both `amadeo-cli` and `amadeo-app`, which do not share a dependency edge, and because
it creates a second place where a game's asset directory is declared. Worth revisiting if a future
command genuinely needs the root without a build.
