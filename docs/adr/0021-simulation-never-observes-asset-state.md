# ADR 0021 — The simulation never observes asset state

**Status:** Accepted · **Date:** 2026-08-01

## Context

ADR 0020 settled what names an asset. This settles how one gets into a running game without breaking
invariant I3.

The hazard is specific and it is not hypothetical. If gameplay can ask *"has this finished
loading?"*, the answer depends on disk speed, file cache state, and OS scheduling — none of which
reproduce. Two runs of the same seed with the same inputs then disagree, and every replay test in the
project quietly becomes unreliable. `docs/04-subsystems.md` §5 flagged this as the thing to decide
before the first async load, which is now.

**What other engines do, and why it is not quite the answer here.** The industry-standard pattern is
a loading state: queue everything, watch it, and start gameplay when it is all resident. Bevy ships
an example of exactly this, and `bevy_asset_loader` exists to reduce the boilerplate; `AssetServer`
offers `is_loaded_with_dependencies` and `wait_for_asset` to support it.

But Bevy does not promise determinism, so that pattern is chosen there for *user experience* — no
pop-in, no half-drawn level. It tolerates assets arriving mid-game. Amadeo cannot, and a rule adopted
for a different reason will not hold under pressure: the first time someone streams a terrain chunk,
the loading-screen convention is simply not present to stop them.

The determinism literature agrees on the shape of the risk rather than the specific case: same seed
and same inputs still desync when anything branches on ordering that is not reproducible. Load
completion order is precisely that.

## Decision

**Two rules. The first is the invariant; the second is the practical default that follows from it.**

### 1. Gameplay may hold an asset id. It may never observe an asset's *state*.

No simulation system asks whether an asset is loaded, how big it is, what its pixels are, or how many
vertices it has. Components store an id (ADR 0020) and nothing else about the asset.

**Everything gameplay needs is authored, not derived.** A hitbox is a field in the scene file, not
something computed from a sprite's pixel dimensions. A collision shape is authored, not extracted
from a mesh. A footstep's timing is authored, not read from an audio clip's length.

This is what makes determinism *structural* rather than conventional. The simulation has nothing to
branch on, so load order cannot affect it — not "usually does not", cannot. It also means streaming
is safe whenever it is built, with no redesign: an asset arriving at tick 900 instead of tick 300
changes what is on screen and nothing else.

Rendering and audio sit outside the deterministic zone and are free to observe asset state. A missing
texture draws a placeholder; a missing sound is silent. Both report the absence through the agent
protocol rather than crashing (`docs/04` §5), because an agent has to be able to *see* what is
broken and keep working.

### 2. A scene declares the assets it needs, and no tick runs until they are resident.

The load barrier. Loading happens entirely outside the tick loop, so the very first tick already sees
a fully populated world.

This is belt-and-braces on top of rule 1, and it is worth having for two reasons that are not
determinism: a level does not appear half-textured, and a game that accidentally *does* depend on an
asset gets the same answer every run rather than an intermittent one.

It is a default, not a constraint. Streaming — loading past the barrier while ticks run — is
permitted precisely because rule 1 makes it harmless. That is the property this ADR is buying.

## Consequences

- **A real constraint on how games are written, and it must be stated plainly**: you cannot size a
  hitbox from a sprite. Authoring it is more typing and some duplication between an asset and the
  data describing it. That is the cost, it is accepted, and it is the same trade every deterministic
  engine makes.
- **`amadeo-assets` splits along the rule.** The catalogue (what exists, by id) is already a
  `Service` — engine machinery, excluded from the state hash. Loaded asset *data* is likewise never a
  `Resource`. Nothing about assets can reach `World::state_hash`, and ADR 0009's split already
  enforces that by trait bound.
- **A scene needs a way to declare its asset set**, which is a `.scene` format addition and therefore
  ADR 0014's business. It has to be visible in the file (I1) rather than inferred, so that reading a
  scene tells you what it needs.
- **`amadeo check` gains a job**: every asset id a scene mentions must exist in the catalogue. Same
  move it already makes for component names.
- **Placeholders are a required feature, not a nicety.** A missing asset must produce a visible
  stand-in plus a structured report, because the agent's only eyes are `render.describe` and
  `render.capture`.
- **Q9 (threading) gets easier.** Loading is off the simulation thread by construction and its
  results re-enter at a defined point — the barrier, or never-observed under rule 1. That removes the
  hardest case from the threading question rather than deferring it.

## Rejected alternatives

**The load barrier alone**, without rule 1. Simplest to build and to explain, and it is what most
engines ship. Rejected because it makes determinism a *convention*: nothing prevents a system asking
whether an asset is loaded, and the day someone streams a chunk the guarantee is gone with no test
failing to say so. It also forecloses streaming, which Palworld-shaped open worlds
(`docs/00-vision.md`) need in M2 — so it would be redesigned rather than extended.

**Record each asset's availability tick in the replay file**, and force those ticks on playback. Full
streaming with exact reproduction, and it does work. Rejected because it couples a replay to the disk
speed of the machine that recorded it, which makes hand-written replays — something this project
deliberately supports, and used to build `wander.replay` — effectively impossible. It also adds real
complexity to both the replay format and the loader to solve a problem rule 1 removes for free.

**Blocking synchronous loads inside the tick.** Trivially deterministic in ordering, since nothing is
concurrent. Rejected because it makes frame time depend on disk latency, which is exactly the stall
`MAX_TICKS_PER_FRAME` exists to keep the loop out of, and it would make hot-reload impossible.
