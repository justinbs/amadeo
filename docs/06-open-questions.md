# 06 — Open Questions

Check this before assuming any undecided thing. Resolve with an ADR, then move the entry to the
resolved section at the bottom.

Priority: **P0** blocks work now · **P1** needed for the current milestone · **P2** can wait.

---

## ~~Q37~~ · **Resolved — ADR 0069.** A save is a snapshot read leniently, and renames are authored data

**Settled in session 18**, and built in the same one. `restore` is unchanged; `restore_save` reads the
same bytes leniently, and `games/atrium` loads through it.

**The answer turned on the hash check, which this question originally did not account for** — see the
measured section below, which is left in because getting it wrong the first time is the useful part.
The check is now **conditional on the file's layout fingerprint** rather than dropped: when nothing
has changed shape, a load is byte-for-byte the strict path, hash check included, so a player who has
not updated loses no verification at all. Missing fields are filled from the **field's** type, an
enum is refused rather than guessed at, renames come from a text file of `old -> new`, and everything
defaulted, dropped or redirected comes back in a `SaveReport`.

Real per-version migrations were deliberately **not** built. They are the only thing that survives a
field changing *meaning* rather than name, and nothing needs one yet — so `TypeInfo::version` is now
written into every file and read by nothing, which is what keeps them an addition rather than a
rewrite. **Where a save file should live is still open, and is now Q38.**

---

**Raised in session 17, on making `games/atrium` save and resume.** That works — a resumed game and
one that never stopped are proven to be the same game — and it works by writing a `.snapshot`.

**A snapshot is explicitly a short-lived artefact.** `amadeo-snapshot`'s own docs say so: it captures
one moment of one run, there is **no migration path**, and a version mismatch is refused rather than
guessed at. That is the right design for "get back to the moment I was debugging".

A **save file is the opposite kind of thing.** It has to survive the game being updated, and today it
does not: `restore` rebuilds each component with `from_value`, which requires **every field**. So
adding one field to any component invalidates every existing save — the player's, not just a
developer's. This is Q32's shape with a much worse consequence, and it is the one open question that
can destroy something a person cares about.

Nothing about it is urgent until a build reaches somebody. It is P1 rather than P0 for that reason,
and it should be settled **before** one does.

### The shape of the answer, as far as it is understood

The engine already has most of the machinery, which is why this is a decision rather than a project:

- **A component's `version`.** `TypeInfo` carries one already (`version: 1`) and nothing reads it.
  That is the hook a migration would hang on.
- **A patch is already a thing.** ADR 0029's prefab overrides apply named fields over a base and
  leave the rest, and `ComponentRegistry::validate_patch` exists. "Restore leniently: take the fields
  the file has, default the rest" is that operation.
- **What it would not survive** is a field being renamed, removed, or changed in meaning — which is
  where a real migration (an old version's value tree in, a new one's out) is needed, and where the
  cost is.

### Measured in session 18: leniency alone does not work, and the reason is the hash check

The paragraph above used to claim a lenient restore would make a save survive an added field **with
no migration code at all**. It would not, and the correction matters enough to be pinned as a test:
`crates/amadeo-snapshot/tests/a_patch_invalidates_every_save.rs` runs two builds of one component in
one process, using `#[reflect(name = "Thing")]` so that both *are* the same component as far as ADR
0017's identity is concerned.

**Being lenient about fields gets past the first error and into a second one.** A defaulted field is
still a field, it is still hashed, and so the rebuilt world cannot hash to the number the file
recorded. `restore` compares those and refuses:

```
adding one field, strict:  BadComponent  { reason: "missing field `b`; required fields are a, b" }
adding one field, lenient: HashMismatch  { expected: 6783642539998936112, actual: 13968525498961532720 }
```

Note what the second one means: the world was rebuilt **correctly** and then rejected, because the
recorded hash describes a component layout that no longer exists.

So the snapshot's integrity check and a save's survival of a patch are **structurally exclusive**,
not two strictnesses of one idea. That check is not decoration — it is what turns "the restore
silently produced a slightly different world" into an error at the moment it happens — so a decision
here has to say explicitly where it goes, and "be lenient about fields" on its own is not an answer.

**The way to keep both is to make the check conditional on the build rather than drop it**, which
requires the file to record *which build wrote it*. Nothing does today, and that gap is real under
every option below except the last. It also buys the good case outright: a player who has not
updated still gets the full exact check, and leniency costs something only when it is actually
needed.

### Four ways to answer it

In increasing cost. Each includes the one before it.

| | Survives | Cost | Does not survive |
|---|---|---|---|
| **A. Lenient restore** | field added or removed, component added or removed | field-level `#[reflect(default)]`, a second restore entry point, a build id in the file | any rename; any change of meaning |
| **B. A + a redirect file** | the above, plus renames of components and fields | a small text file of `old -> new` and a lookup on restore | change of meaning |
| **C. B + versioned migrations** | everything, including a field changing units or splitting in two | `TypeInfo::version` written per component, a migration registry a game fills, and the discipline to bump | — |
| **D. A save is not a world dump** | n/a — the game writes its own save data | none in the engine; all of it in every game | introspection, `amadeo fmt`, one diffable text format |

Two notes on the table. **B's redirect file is Unreal's `CoreRedirects`** — a text file of renames
rather than migration code, which is the same "authored data, not registered functions" grain as ADR
0068's facts and ADR 0066's tracks. And **C can be added on top of B later without a format change**,
provided the version is recorded from the start, which is the argument `TypeInfo::version`'s own doc
comment already makes for having written it down.

Worth deciding together: whether saves and snapshots stay one format with two strictnesses, or
diverge into two. One format with two *entry points* keeps one parser, one writer, and `amadeo fmt`
working on both; two formats would duplicate the 1,600 lines in `amadeo-snapshot` for no gain that
has been identified.

**One consequence to decide deliberately rather than inherit:** a defaulted field is a silent
gameplay change. A save that loads with a new `battery: 0.0` reads as a bug in the game, not as a
bug in the save. So whatever lands has to *report* what it defaulted, dropped and redirected, in the
tradition of `asset_problems`, `SoundCache::failures` and `Animatable::missing`.

---

## Q39 · **P0** · Punctual lights do not light a room that has no directional light

**Found in session 18, by capturing `games/warren` and looking at it.** Every headless test passed.
The picture was black.

This is **P0 because it sits on M3's exit gate**, which asks for "a dark corridor with a moving
flashlight that reads as genuinely atmospheric" — that is, a shadow-casting spot light in a scene
with no sun, which is exactly the configuration that fails. Every scene the renderer has ever been
exercised against has a `DirectionalLight` in it: the Atrium, the Scarp, and every capture test.

### The reproduction, which is four captures apart

`games/warren` is a closed interior: floor, ceiling, four walls, two crates. Lighting is one
`PointLight` near the ceiling and one `SpotLight` on the camera.

| Scene | Result |
|---|---|
| `PointLight` 5, spot `shadows true` | **black** — nothing lit |
| `PointLight` **20**, spot intensity 24, `shadows true` | **black, essentially unchanged** |
| the same, plus a `DirectionalLight` at 0.35 | the room renders correctly |
| no directional light, spot `shadows` **false** | **the spot's cone lights the far wall** |
| no directional light, spot off, `PointLight` 20 alone | **black** |

Two separate faults, and the second is the surprising one:

1. **A shadow-casting spot in a scene with no directional light kills punctual lighting entirely** —
   its own included. Turning `shadows` off brings the spot back. This has the shape ADR 0058
   describes: a spot's shadow map is *a layer of the same array the cascades use*, and with no
   directional light there are no cascades and presumably no array, so the bind fails or the pass is
   skipped. `View::shadow_atlas` is named there as the one place that decides layers and size.
2. **A `PointLight` alone lights nothing**, even with the spot off and shadows off. At intensity 20,
   range 7, 3 m from a surface with albedo 0.28, it should be plainly visible. It is not. Whether
   this is the same root cause or a second one is **not yet established**, and finding out is the
   first thing to do.

### Why no test caught it

Because every test asserts on *numbers a headless run can produce*, and the GPU capture tests all
draw scenes with a sun. This is `amadeo-look-at-the-output` again, and the third time in this
project that a green suite has hidden something visible — after the inside-out mesher and the
shader that only failed under FXC.

**Worth adding with the fix:** a capture test whose scene has *no* directional light. That is one
line of scene text and it is the entire coverage gap.

### What `games/warren` does meanwhile

It carries a dim `DirectionalLight` called `spill`, at intensity 0.12, so the room is navigable and
somebody can look at it. **That is a placeholder standing in for a bug, not a lighting decision**,
and the `ceiling_lamp` beside it currently contributes nothing at all. The torch beam is authored
`shadows false` for the same reason — a flashlight that casts is most of the atmosphere in a game
like this, and getting it back is what fixing this buys.

---

## Q38 · P2 · Where a save file lives

**Split out of Q37 in session 18**, so that it did not disappear when that one was struck through. It
is the smaller half and none of ADR 0069 depends on it.

`games/atrium` writes `atrium.save` in the working directory, and `atrium.redirects` beside it, both
deliberately as placeholders. ADR 0022's marker-file rule is about **assets**; user data has
different conventions on every platform — `%APPDATA%` on Windows, `~/Library/Application Support` on
macOS, `$XDG_DATA_HOME` on Linux — and picking one is a decision nothing so far depends on.

Two things that will be wanted at the same time, so they are worth deciding together: **more than one
slot**, and whether a redirect file is **user data or shipped with the game**. The second is not
obvious. A redirect describes the *game's* history, so shipping it with the build is the natural
reading — but that means it has to be found through the asset root rather than beside the save, which
is a different lookup from the one written today.

---

## Q36 · P2 · A pointer cannot select a menu item the way ADR 0063 assumed

**Raised in session 17, on going to build the thing ADR 0063's consequences described.** That section
says pointer and spatial navigation belong in "a *presentation-side* system that writes through the
same `Focus` resource", and that "a replay records the resulting focus moves rather than the pointer
that caused them".

**Neither half works, and it is worth being precise about why**, because the sketch is plausible:

- `Focus` is a **hashed resource**. A `Render`-stage system writing it puts the pointer position — and
  through the rectangles it hit-tests, the **window size** — into the state hash. That is exactly the
  I3 break ADR 0063 exists to prevent, arriving through the door the ADR left open.
- Nothing in the replay format can record "the focus moved". `InputChange` is `Button` and `Axis`,
  and that is all it is.

So this was deferred rather than built, deliberately: nothing in M3's exit gate needs a mouse menu —
the horror slice is keyboard and controller — and the design is written down here so that deciding it
later costs the reading rather than the thinking.

### What the deterministic games actually do

Lockstep RTSs have mouse-driven interfaces and reproduce exactly: StarCraft, Age of Empires,
Factorio. The answer is uniform. **The interface is outside the simulation, the pointer resolves to a
*command*, and the command is what gets recorded.** The pointer position never enters the simulation
at all.

Amadeo already does a small version of this: `look_x` and `look_y` carry a mouse into the
deterministic zone as ordinary named axes (`modules/amadeo-camera`).

### The candidate, when this comes back

**Resolve the pointer to an ordinal and carry it as a named axis.** A presentation-side system
hit-tests `ComputedRect`s, works out which item that is *in the authored focus order*, and writes the
position as `ui_focus_index`. The simulation focuses the Nth focusable. A click is that plus
`ui_confirm`.

It is resolution-independent by construction — **an ordinal is not a rectangle** — and the replay
format does not change, because an axis is already recorded, hashed and replayed.

Two things to work out when it is built: a float axis carrying an integer needs a sentinel for "over
nothing", and the ordinal is only meaningful against the current set of focusable items. That set is
authored, so it is identical on every machine, but it is a coupling that "press down" does not have.

**Rejected on the way past:** extending `InputChange` with an "activated entity" command. It is the
most direct expression of the RTS answer, and it couples the replay format to **entity ids**, which
are allocator state — ADR 0028 exists because that allocator's free list matters, and a replay naming
entities would break whenever anything upstream spawned differently.

---

## ~~Q35~~ · **Resolved — ADR 0065.** Pausing is a per-system opt-in, and the engine has no screens

Four questions were bundled together and only one turned out to be expensive, which is worth noting
as a pattern — it is the third time now (see Q3 below, twice).

The expensive one was **what still runs while paused**, because the answer reaches into the schedule
that every game and module registers against. It is a `.while_paused()` flag on a system, which is
Unreal's `bTickEvenWhenPaused` and Godot's `process_mode`: two of the three large engines make this a
property of the *thing* rather than a global rule, and the third — Unity — is the one where every
game re-implements a pause by hand.

The other three answered themselves once the first was settled:

- **Where the screen lives: in the game.** What screens exist is genre knowledge (I4), and a game's
  own hashed resource already gets reflection, snapshots and `amadeo query` for nothing.
  `games/atrium` has a three-value `Screen`, and `Paused` is projected from it by one system.
- **Whether pausing stops the tick: no.** The counter keeps advancing, because menu navigation is
  hashed state driven by input recorded **per tick** — freeze the tick and a keypress in a menu has
  nowhere in a replay to live.
- **The unpause burst: cannot happen**, and for the same reason. `advance_real_time` keeps consuming
  its accumulator on cheap paused ticks, so nothing is ever banked. Nothing in the loop changed.

**What a transition does to the world is still undecided, and deliberately so** — pausing does not
touch entity lifetimes, so the expensive mistake Q35 warned about is not available. That question
comes back when something has to load a level from a title screen.

---

## ~~Q3~~ · **Resolved — ADR 0018, 0023, 0031.** Two passes in one graph, and the camera is an entity

Split into three across three sessions, which is what made it tractable. **ADR 0018** settled the two
expensive parts, both *data*: one 3D `Transform` with 2D as its degenerate case, and an explicit
`SortOrder` dominating depth. **ADR 0023** settled the batching rule against a measurement. **ADR
0031** closed the last third in session 8 — the sprite pass and the mesh pass are separate pipelines
sharing one render graph, neither built on the other.

**The last third turned out not to be the expensive part, for the second time.** Q3's framing
emphasised the pipeline; ADR 0018 pointed out the expensive decisions were the data around it, and
the same held again — the pipeline was a consequence, and the genuinely hard-to-reverse decision
hiding inside it was **the camera model**, which nothing had framed as a question. That is the half
Justin decided: a camera is an entity, not a resource.

Worth remembering as a pattern. "Which pipeline" is almost always the cheap question, because
`RenderBackend` isolates it and no file or hash can observe it. Ask what *data* the choice implies.

---

## ~~Q17~~ · **Resolved — ADR 0025.** Queries are tuples of terms, and a term may be optional

Justin chose the trait-plus-macro approach over hand-writing each shape or a lower-level accessor.
`world.query::<(&Transform, &Sprite, Option<&SortOrder>, Option<&GlobalTransform>)>()` now resolves
each column once per archetype. Sprite collection at 20,000 sprites went 3.32 ms → 2.58 ms, and
5.13 ms → 2.58 ms across ADR 0024 and 0025 together. Kept read-only: generic mutable queries would
need `unsafe`, which this crate forbids, and the measured problem was all on the read side.

The original text is below for the reasoning that led there.

<details>
<summary>Q17 as filed</summary>

### The ECS cannot express an optional component in a query

**Found in session 7**, as the successor to Q16 — fixing that one removed the id cost and left this
as the dominant remaining expense.

Archetype ECSs are fast for one structural reason: **a query matches whole archetypes, not individual
entities.** The column is located once per archetype and then iterated contiguously, which is why
Flecs describes archetype matching as the main source of query speed and why Bevy caches that
matching in `QueryState` — "gathering archetypes is a costly operation, so it is cached".

Amadeo's queries do this for their *required* components. They cannot do it for optional ones,
because there is no way to say "and this component if it is present".

**The consequence, measured.** The sprite batcher needs `SortOrder` and `GlobalTransform`, and both
are deliberately optional — an entity without them still draws, at order zero and at its local
transform. So it falls back to `world.get::<T>(entity)` per entity, which is exactly the per-entity
lookup archetype storage exists to avoid. At 20,000 sprites that is 40,000 individual lookups, each
one a location fetch plus a binary search plus a downcast, and after ADR 0024 it is what the
remaining ~3.3 ms is spent on.

The same shape appears in `render_quads`, and will appear in every system that wants to treat a
component as optional — which is most of them, since requiring a component means an entity silently
disappearing from a query when someone forgets to add it.

### What to build

An `Option<&T>` query term: resolve the column once per archetype, yield `Some(&value)` for
archetypes that have it and `None` for those that do not. No per-entity lookup at all.

Sub-questions worth settling first:

- **How far does it generalise?** Query shapes currently stop at three components (`iter_triple`) and
  each new shape is a hand-written method. Adding optional terms multiplies that combinatorially, so
  this may be the moment the query API needs a real abstraction rather than another method — which
  is a bigger design question, and one where `CLAUDE.md`'s "boring Rust over clever Rust" pulls hard
  against the generic machinery Bevy uses for it.
- **Does it change iteration order?** It must not: draw order and state hashes both depend on
  iteration being reproducible (I3).
- **Is the archetype match cached, as Bevy does, or recomputed per call?** Caching needs invalidation
  when a new archetype appears, which is a correctness risk to weigh against the saving.

### Why this matters now

The target list grew to eight games in session 7, and Stellaris, Terraria, RimWorld, and Project
Zomboid are all large-entity-count simulations. ECS throughput stopped being an aesthetic concern and
became a target requirement (`docs/00-vision.md` § Divergent).

**Worth doing before the sprite batcher reaches the GPU**, and worth doing on its own merits.

</details>

---

## Q25 · P1 · Level of detail across chunks at different resolutions

**New in session 11, and deliberately deferred by ADR 0042 rather than overlooked.**

Surface nets avoids marching cubes' seam problem *between chunks at the same resolution*: a chunk
that samples one cell into its neighbour already agrees with it about where the surface is. **Chunks
at different resolutions still crack**, because a coarse chunk's samples do not line up with a fine
one's.

This has to be answered before terrain covers real distances, because every open-world target needs
distant chunks to be cheaper than near ones.

### Two of the four options in the original version were not real — corrected in session 12

The research done for ADR 0043 removed two candidates this question used to list, so they are struck
here rather than left to be re-investigated:

- ~~**Transition cells**, "the surface-nets equivalent of TransVoxel"~~. **Transvoxel does not
  apply.** It is for marching cubes, a *primal* method; surface nets is *dual*. There is no
  equivalent to port.
- ~~**Seam meshes** (Nick Gildea's approach for dual contouring)~~. **Needs an adaptive octree with
  variable-size leaf nodes.** `amadeo-voxel` is a uniform grid, so this would be a rewrite of the
  mesher rather than an addition to it.

What is left, and every one of them would be derived here rather than ported:

- **Skirts.** Each chunk grows a downward-facing lip at its border, hiding the crack rather than
  fixing it. Cheapest by a long way, universally used, and visibly wrong at glancing angles — and one
  reported failure mode is worth knowing about: on flat ground the skirt can overlap geometry the
  mesher already produced there.
- **Derived transition geometry** — a chunk is told its neighbours' resolutions and generates matching
  boundary cells. Correct, and it makes a chunk's mesh depend on its neighbours' *choices*, not just
  their samples, so one chunk changing level dirties up to six others.
- **Sample the coarse level everywhere near a boundary**, so both sides agree. Simple and correct, and
  it gives up the saving exactly where chunk counts are highest.
- **Clipmaps** — concentric rings at fixed resolutions centred on the camera. Very well understood for
  heightfields, much less so for a full 3D field. Note that ADR 0043 already adopted concentric
  *integer boxes* for residency, which is the same idea one level up.

### It is now better posed, and deliberately still open

**ADR 0043 pinned colliders to the finest detail level**, which changes this question's character
entirely: a collider never changes resolution, so **the seam is purely a rendering problem** and sits
outside the state hash. It was previously entangled with determinism and no longer is.

So the question is no longer "which of four options". It is: **may a chunk's mesh depend on its
neighbours' resolutions?** Everything else follows from that answer.

Still not decided, for the reason this question always gave: the honest comparison needs a running
streaming system to look at, and ADR 0043 built the residency and meshing layers rather than the whole
pipeline. `ChunkKey` carries `lod` and `ChunkShape` does the arithmetic, so whichever answer wins is
an addition rather than a change to the key type everything is built on.

Nothing is blocked: terrain at one resolution works today.

---

## ~~Q29~~ · **CLOSED in session 13** · Terrain edits are a hashed resource

**Resolved by ADR 0046.** `TerrainEdits` is a reflected, hashed `Resource`, so it is in the state
hash and is captured and restored by a snapshot; gameplay writes it and the streamer is a **cache**
rebuilt from it, the same asymmetry ADR 0036 gives physics. A hole dug in `games/scarp` now survives
a save and a reload, and `a_dug_world_reloads_dug.rs` is the test.

Two things changed from the analysis below during implementation, both worth knowing:

- **The locality argument for per-entity storage does not survive contact with the code.**
  `World::state_hash` walks every entity *and* every resource regardless, so the two placements cost
  the same to hash. What was left was a multiplayer advantage against a second kind of chunk entity
  sitting next to the streamed one — a confusion that would have been paid for repeatedly.
- **Storage is flat, keyed by world sample, not grouped by chunk** — the opposite of what "keyed by
  chunk" suggested when the option was chosen. A sample near a boundary is read by up to eight chunks
  (ADR 0043 §4), so an owning chunk would leave the other seven meshing it differently, which is the
  exact seam bug `amadeo_voxel::Edits` is keyed by world sample to avoid. Per-chunk deltas stay
  derivable by integer division.

It also closed a gap nobody had noticed: **a game could not save or load at all**, because the
component registry a snapshot needs is crate-private. `App::capture_snapshot` and
`App::restore_snapshot` exist now.

The original entry follows.

---

## Q29 (original) · Where terrain edits live, now that chunk entities are despawned

**New in session 12, and found by building the thing ADR 0042 §4 describes rather than by reading
it.**

ADR 0042 §4 says: *"Edits are a reflected, hashed component on a chunk entity."* That was written
before chunk entities existed. They exist now, and `stream_terrain` **despawns them when a chunk
streams out** — so an edit stored on one would be destroyed by walking away from it.

So the ADR's placement cannot be implemented as written. Today edits live in the `Terrain` service,
which means **they are not in the state hash and a snapshot does not restore them**: a dug world
saves and reloads undug. That is a real gap, not a subtlety — it breaks ADR 0042's central promise
that a save file is a seed plus a diff.

### The options

- **An entity per *edited* chunk, whose existence is driven by having been edited rather than by
  being loaded.** Closest to ADR 0042 §4's intent, sparse in the world exactly as the edits are
  sparse in the field, and it survives streaming because nothing streams it. The likely answer.
  Costs a second kind of chunk entity, and something has to keep the two from being confused.
- **One hashed `TerrainEdits` resource holding every edit in the world.** Simplest, and it is a
  single growing blob — every edit anywhere is in the state hash of every tick, and hashing walks all
  of it. Fine for a small world, and the thing ADR 0042 §3 was explicitly avoiding for a large one.
- **Edits as their own asset, written to a file.** Matches how the rest of the project treats
  authored data (I1), and puts save/load in the asset layer rather than the snapshot layer. Awkward
  because edits change every time somebody digs, which is not what an asset is.

### Why it is not decided here

It needs the snapshot round-trip to be exercised against a terrain world. **That world now exists** —
`games/scarp`, session 13 — so the condition this question was waiting on is met and it is ready to
be decided against something running rather than against a design.

**Blocking for destructible terrain specifically** — Minecraft, Terraria and Project Zomboid all need
it. Not blocking for a generated world you walk around in, which is why exit gate 1 could close
without it.

---

## Q30 · P2 · There is no way to move a physics body from outside the simulation

**New in session 13, found by a test that teleported a character and silently did nothing.**

`step_physics` reads `GlobalTransform` in preference to `Transform`, and `propagate_transforms` runs
in `PostSimulation` — at the end of the tick. So writing a `Transform` from outside the tick is read
back **stale** on the next one: physics steps from the old position and writes it straight back over
the new one. The entity does not move, nothing errors, and the only symptom is that whatever you
expected at the new position did not happen.

Preferring `GlobalTransform` is *correct* — a body parented to something needs its world position —
so this is not simply a bug to invert. What is missing is any supported way to say "this body is now
somewhere else", which respawns, fast travel, level transitions and editor drag-and-drop all need,
and four of the eight target games have at least one of those.

### The options

- **A `Teleport` component the physics step consumes**, clearing it after applying. Explicit, hashed,
  replays for free, and it reads as an intent rather than as a mutation. Costs a component whose
  whole life is one tick.
- **Run `propagate_transforms` before physics as well as after.** Makes a written `Transform` take
  effect next tick with no new API. Doubles the propagation cost and makes "when is `GlobalTransform`
  current" a subtler question than it is now.
- **A method on `Physics` that moves a body by entity**, alongside `insert_static_mesh`. Matches the
  precedent for things that genuinely cannot travel through components — but a position *can*, which
  is the argument against.

**Not blocking.** `games/scarp` walks its character rather than teleporting it, which is what its
exit gate asked for anyway. It becomes blocking the first time a game needs a respawn point.

### It has now cost two debug cycles rather than one, which is a nudge on the priority

**Session 17 hit it again**, writing tests for the watcher: the obvious way to change what an AI can
see is to move the player in and out of range, and the first teleport appeared to work while the
second silently did not. `docs/07` already described the trap and it was read *after* the debugging
rather than before.

What it sharpened is **where the boundary actually is**, which the title says and is easy to read past.
`games/atrium`'s "return to start" menu button writes a `Transform` directly and works perfectly —
because `choose_from_menu` runs in `Simulation`, so `propagate_transforms` refreshes
`GlobalTransform` in the same tick's `PostSimulation` and physics reads the new value next tick.

So the rule is not "you cannot write a `Transform`". It is:

| Where the write happens | Result |
|---|---|
| A system in `PreSimulation` or `Simulation` | **Works.** Propagation happens later in the same tick. |
| A system in `PostSimulation` after `propagate_transforms`, or `Render` | Stale by one tick. |
| **Between ticks** — a test, an editor, a load — | **Silently ignored.** This is the gap. |

That third row is the whole of Q30, and it is worth stating as a table because the first row working
is exactly what makes somebody believe the third one will.

Still P2 by the letter, since nothing is blocked. Worth promoting the next time anything needs it.

---

## ~~Q34~~ · CLOSED in session 15 by ADR 0054 · There is no pure shape cast, only a sliding move

**Raised in session 14, closed in session 15 when the workaround produced a visible bug.**

`PhysicsBackend::cast_shape` exists: sweep a shape along a line, get back the fraction travelled, the
position — **on the line, by construction** — and the surface normal. `None` means clear. The camera
uses it for both sweeps and has no projection left.

What closed it was the second failure of the workaround, and it is worth keeping because the shape
recurs: **a correction layered on a borrowed operation has its own failure mode.** Projecting the
travel onto the query axis fixed the case where a slide went sideways. It could not fix the case where
the slide went *along* the axis — the camera tilted up has an arm pointing down and back, it slid
backward along the ground, and backward was 0.87 of the arm, so the projection reported nearly full
progress for a shape that had gone nowhere. The camera ended up under the terrain, and what you see
from there is its unlit underside, which reads as sky.

By the end the call carried two corrections. That is the signal.

The original text follows.

---

**New in session 14, found by using the wrong one for a camera.**

`PhysicsBackend::move_shape` is a *character* move: it slides along whatever it hits, steps over
small obstacles, and snaps to ground. That is exactly right for something walking, and it is the only
query the engine has.

A camera wants a different question — **how far along this ray before something is in the way** — and
using the sliding move for it produced a distance with little relation to the axis asked for. A
camera brushing a slope slid sideways, the straight-line distance travelled counted that as progress,
and the result swung as the player moved. Projecting the result onto the desired direction gets most
of the way there and is a workaround rather than an answer.

Everything else that will want this wants it too: a bullet, a line of sight, a "can this fit here"
placement check, a mouse pick in the editor.

### The options

- **`cast_shape` on `PhysicsBackend`**, returning the first hit and the fraction of the motion
  reached. rapier has this directly; `NullPhysics` returns "no hit", which is the same honest
  useless answer it gives everywhere else (ADR 0037 §5).
- **A `slide: bool` on `ShapeMove`**, which is fewer concepts but muddles two questions in one type —
  half its fields are meaningless when sliding is off.
- **Leave it**, and let each caller project onto its own axis. Cheap, and it means every caller
  repeats a correction for a behaviour it did not want.

**Not blocking.** The camera works with the projection, and nothing else has needed a cast yet.

**Session 15 raised the priority of the argument without changing the priority of the question.** The
camera's flicker turned out to be a *second* symptom of asking a character-move to answer a camera
question: `move_shape` starting inside the followed body's own collider made rapier report
`sliding_down_slope` and cancel the motion, intermittently. That is fixed by `.ignoring()` the parent
and is a real fix — but a pure `cast_shape` would not have had the failure mode at all, because "how
far until something blocks this" has an obvious answer when the start point is occupied and "where
does this character end up" does not.

So the workaround now has two corrections stacked on it (project onto the axis, and exclude the
parent), which is the usual sign that the borrowed operation is the wrong one.

---

## ~~Q33~~ · **Resolved in session 14 — a `flat` flag on the mesh asset** · Nothing lets a mesh ask to be flat-shaded

**The first of the three options below won**: `MeshData::flat_shade` splits vertices per face and
recomputes normals, and `flat` is an authored field on the mesh asset, so a `.mesh` file and the
terrain streamer's `.flat_shaded()` both reach the same code. The note about ADR 0047 held — it runs
*before* `generate_tangents`, because splitting vertices changes the tangent frame too.

**This entry stayed open by mistake until session 15's documentation pass.** It shipped in `f3b19f9`,
`CLAUDE.md` and `STATUS.md` both recorded it, and only this file was missed — which is worth noting
because an open-questions list that lies about what is open is worse than a shorter one that does not.

The original text follows.

---

**New in session 14, raised by ADR 0050's decision that Amadeo's own content is low-poly.**

Low-poly depends on **per-face normals** — the faceting *is* the look. A surface whose vertices carry
averaged normals shades as a smooth blob, which is the one thing low-poly must not do.

`BoxMesh` already gets this right, tessellating twenty-four vertices rather than eight so every face
carries its own normal, and `a_box_has_flat_faces_rather_than_averaged_corners` pins it. **Nothing
else does.** A glTF exported with smooth normals imports smooth, and a generator has to remember to
duplicate vertices per face. Raised to P1 because it blocks the look rather than merely limiting it.

### The options

- **A flag on the mesh asset**, so a `.mesh` file can say `flat true` and the loader splits vertices
  and recomputes normals per face. Authorable, and it fits how `.mesh` already carries a shape.
- **An import setting in the `.ama-meta` sidecar**, matching where `color_space` lives — the file's
  own property rather than the scene's. But a mesh's shading is arguably content rather than import.
- **The generator's job**, with the engine staying dumb. Cheapest, and it means every producer has to
  remember — which is the shape of defect `docs/07` now has four entries about.

Note the interaction with **ADR 0047**: splitting vertices per face changes the tangent frame too, so
whichever option wins has to run *before* `generate_tangents` rather than after.

---

## Q31 · P2 · Nothing warns when a normal map forgets to declare itself linear

**New in session 14, and named in ADR 0047 as the sharpest edge that feature ships with.**

A normal map's bytes are directions, not colour, so its `.ama-meta` sidecar must say
`color_space = "linear"`. A `.png` cannot say which it is — the bytes are identical either way — so
the sidecar is the only declaration there is.

**Forgetting it is silent.** The map decodes through the sRGB curve, every direction it stores is
bent, and the surface is lit as though its bumps face somewhere they do not. No error, no fallback, no
entry in `TextureCache::failures` — because nothing failed. It renders, slightly wrong, forever.

Unity and Godot both solve this in the importer, with a "this texture is not marked as a normal map —
fix now?" prompt. That is the right shape here too. What is missing is the path: `amadeo check`
validates a scene document against the asset catalogue and has no knowledge of `Material`, because
`amadeo-scene` does not depend on `amadeo-render` and should not.

### The options

- **A generic rule in `amadeo_scene::validate`**: any component field named `*_texture` whose asset
  declares no `color_space`, checked against a small table of which slots are data. Cheap and
  stringly-typed — a naming convention baked into a validator.
- **A diagnostics pass in `amadeo-app`**, which already has both the `Material` and the `Assets` and
  sits above both. The natural home, and it needs somewhere to report *to*: the engine has no logging
  convention, and inventing one is the actual work.
- **Infer it from the slot instead of declaring it**, making the sidecar advisory. Removes the class
  of error entirely, and does not generalise — `TextureCache` is keyed by id, so one image used in two
  slots would need two decodes.

**Not blocking.** Nothing in the repository ships a normal map yet. It becomes blocking the moment
authored art does, which is M3's exit gate.

---

## Q32 · P2 · Every field added to `Material` rewrites every `.material` file

**Noticed in session 14 while adding two fields, and it is about the scene format rather than about
materials.**

Reflection requires **every** field to be present when reading a value, so adding one to a component
invalidates every file that spells it out. `Material` gained `normal_texture` and `normal_strength`
and five files had to change. PBR will do it again; so will triplanar; so will every texture slot.

It is trivial churn at five files and it is not trivial at five hundred, and it is a real barrier to
`Material` growing the fields a modern renderer needs. It also affects *authoring by hand*, which
invariant I1 is about: a person writing a `.material` must currently name every field including the
ones they do not care about.

The tension is genuine and this is not simply an oversight. `MissingField` is what catches a typo'd
field name and what makes a prefab that lost a component refuse to load rather than silently reverting
(ADR 0029's deliberate opposite of Unity). Making fields optional trades that away.

### The options

- **A field may declare a default, and a missing field takes it.** Familiar and easy, and it weakens
  the guarantee above unless "optional" is opt-in per field rather than blanket.
- **A `version` per component, with a migration step.** The heavyweight answer, already on M3's list
  for save files ("save/load built on snapshots, with versioning and migration"), and possibly the
  same mechanism.
- **Leave it.** Five files is nothing, and `amadeo fmt` could grow a `--migrate` that adds missing
  fields at their defaults — turning a schema change into a command rather than a hand edit.

**Not blocking**, and worth deciding before `Material` grows the four or five more slots PBR and
image-based lighting want.

---

## ~~Q26~~ · **CLOSED in session 13** · `render.describe` can see meshes

**Resolved.** `describe_frame` picks the first active window camera of **either** projection
(`primary_view`), projects through the same `view_projection` the backend builds, and reports a
`DrawnKind::Mesh` per mesh entity with the screen rectangle its bounds project to.

Asked about `games/scarp`, it now answers with the real perspective camera at `(0, 10.1, 7.0)`, its
actual fov and clip planes, and **50 drawn, 20 visible, 30 off-screen** — which is exactly the
baseline M2.5's exit gate 3 needs and could not previously obtain. Before this it reported a default
orthographic camera nobody authored and zero entities.

Three pieces, all of which turned out to matter:

- **`Mat4::transform_point4` returns `w`** rather than dividing it out, because its *sign* says
  whether a point is in front of the camera. Dividing regardless mirrors a point behind the eye onto
  the screen as a perfectly ordinary-looking position.
- **All eight corners of a mesh's bounding box are projected**, not two extremes: a rotated box's
  image is not the image of its extremes, and under perspective the near face is larger than the far.
- **`FrameDescription::eye` widened to three components**, and the reply's `camera.center` with it.
  A 2D camera's z is zero so nothing that read the first two changes meaning. Widened now rather than
  later because nothing outside this repository consumes the protocol yet, which is exactly when a
  shape change is cheap.

The original entry follows, because the *reason* it mattered is still the best statement of what this
method is for.

---

**Found in session 10 by reaching for it and getting nothing.** Asked what `games/atrium` was
drawing, it reported a default orthographic camera and zero entities — because it only knows the 2D
path: quads and sprites.

That made it useless for the one debugging job it was actually reached for, and the workaround was
writing a throwaway test that printed `FrameData` by hand.

**It matters beyond that afternoon.** `render.describe` is the agent's main way of seeing a frame
*without* a GPU, and `docs/03` makes "what is on screen" a pillar. An editor will need it in M4, and
by then 3D is most of what there is to see.

The shape is probably obvious — a `DrawnKind::Mesh` alongside the existing kinds, carrying the mesh
id, the resolved material and the model matrix. What needs thought is what "on screen" means for a
perspective camera, since the existing `visible` / `off_screen` split is computed against a 2D
viewport rectangle.

### Raised in priority to P1 in session 13, because it blocks a gate

**M2.5 exit gate 3 says frustum culling must be "measured through `render.describe` rather than
believed".** It cannot be, while `render.describe` cannot see a mesh. The gate was written before
anyone noticed, and closing this is now a prerequisite rather than a nicety.

It also cost a real debugging detour in session 13: reached for while diagnosing terrain that would
not draw, it answered about a **default orthographic camera that does not exist in that world** and
reported zero entities. Confidently wrong is worse than absent, which is worth weighing when
deciding what a 3D-blind describe should do about a 3D world.

### What is already built for it

`MeshData::bounds()` (session 13) returns the axis-aligned box containing a mesh's vertices, in mesh
space, and `None` for an empty mesh. Both this and frustum culling need exactly that box, and having
two of them would let a culling bug and a reporting bug disagree about what is on screen.

### What is left, and why it was not just done

Three things, and the third is why it needs a decision rather than an afternoon:

1. **`DrawnKind::Mesh { mesh, material }`** — additive, and one new match arm in `rpc.rs`.
2. **Project the eight corners of the transformed box** and take the screen-space rectangle. Eight
   corners rather than two extremes, because under rotation the two extremes of a box are not the
   extremes of its image.
3. **`FrameDescription::eye` is `[f32; 2]`**, and a 3D camera's eye is not. Widening it changes a
   public struct *and* the `render.describe` reply shape, which is the agent protocol — so it wants
   the same care ADR 0030 gave `describe`. The alternative, reporting a 3D camera's position with its
   z silently dropped, is the same class of confidently-wrong answer that caused the detour above.

---

## ~~Q27~~ · **CLOSED in session 14** · `modules/amadeo-camera`

**Resolved.** `FollowCamera` sweeps a sphere from its pivot toward where it wants to sit and puts
itself where the sweep stopped. Snap in, ease out — reacting to an obstruction has to happen the same
tick or the camera spends a frame inside a wall, while going back out is eased because a sweep
grazing an edge is noisy and snapping both ways flickers visibly.

**It was reported as something else entirely**, which is the part worth remembering: *"digging down
shows the sky"*. The camera had gone under the terrain, and terrain is an open surface — before
ADR 0052 made geometry two-sided, looking at it from beneath showed nothing at all and the sky pass
filled the frame. Two fixes, two different claims: ADR 0052 makes being inside something *dark*, and
this keeps the camera out of it.

**Two things in it that a second attempt would get wrong.** The pivot is swept for as well — a cast
that *starts* inside geometry has no reliable answer, and in any tunnel the point above the player is
inside the ceiling. And the result is projected onto the axis asked for rather than measured as a
distance, because `move_shape` slides (**Q34**) and a slide counted as progress makes the camera
swing.

It lived in `games/scarp` first and moved when `games/atrium` wanted it, which is this project's rule
for promoting to `modules/`. Q27's original wording was about **walls**, and walls are the Atrium's
case.

The original question follows.

---

## Q27 (original) · A third-person camera clips through walls

**Found in session 10 by `games/atrium`**, whose follow camera is a child entity at a fixed offset.
Backing the character into a wall pushes the camera outside the room, and the world turns inside out.

This is a solved problem everywhere — a spring arm that shortens the offset when something is in the
way — and **`PhysicsBackend::move_shape` is already the right tool**: ADR 0037 explicitly names "a
camera that must not clip through a wall" as something the query describes.

The decision is not *how*, it is **where**. A camera rig is not geometry and is not gameplay; it sits
in the same awkward place `CharacterController` did. `docs/00-vision.md` says the camera rig must be
separate from the character controller because the targets are a mix of first- and third-person, so
this is probably a second module — `modules/amadeo-camera` — rather than anything in `crates/`.

Worth deciding when a game needs it. The Atrium tolerates it because you can see the whole room.

---

## ~~Q28~~ · **CLOSED in session 14** · The sky is a light source now — ADR 0049

**Resolved.** `let ambient = 0.12;` is gone from `mesh.wgsl`. An environment is decoded from a
Radiance `.hdr`, projected onto a cube and convolved twice at load — irradiance for diffuse, a
GGX-prefiltered chain for specular — and the shader reads both. Shadowed areas are filled by the sky
rather than by a constant, and a metal reflects its surroundings instead of rendering black.

**Both halves of the original question were answered.** It asked whether ambient should be a flat
colour or a sky/ground gradient: neither, in the end — Justin was given four options with costs and
chose full image-based lighting over the cheaper gradient, because M3's exit gate is an indoor scene
that a sky gradient does nothing for. And it asked whether it belongs to `Environment` or to a light
entity: **`Environment`**, because a `DirectionalLight` is a *direct* light and an environment map is
the indirect half.

**What it did not do: draw the sky.** The background is still a flat clear colour. That is a separate
pass and is now probably the largest remaining visual gap.

Also still true, and recorded here when this question was opened: **`Grade::contrast` above 1.0
crushes shadows to pure black**, because the operation is `(colour - 0.5) * contrast + 0.5` and
near-black values go negative and clamp. Inherent to a pivot with no toe.

The original question follows, for the reasoning it carried.

---

## Q28 (original) · Ambient light is a hardcoded constant, and shadows made that visible

**Found in session 10 the moment shadows landed.** Before shadow maps the only ambient-only pixels
were faces turned away from the light — small, and fine as near-black. With shadows, whole areas of
*floor* are ambient-only, and at 0.03 they came out as holes in the world rather than as shade.

Raised to 0.12, which is a stopgap and is marked as one in `mesh.wgsl`. **The real answer is an
authored sky colour on `Environment`**, because ambient is standing in for the sky being a light —
and `Environment` is already the place a game says what its look is (ADR 0034).

Two things to decide with it: whether ambient is a flat colour or a simple sky/ground gradient (the
cheap version of image-based lighting, and the one that makes an outdoor scene stop looking flat);
and whether it belongs to the `Environment` asset or to a light entity.

Also found alongside it, and worth remembering: **`Grade::contrast` above 1.0 crushes shadows to pure
black**, because the operation is `(colour - 0.5) * contrast + 0.5` and near-black values go negative
and clamp. Inherent to a pivot with no toe. Nothing in a scene had ever been that dark before shadows
existed.

---

## Q15 · P1 · Modding, and whether ADR 0011 still holds

**Raised session 7, when the target list went from three games to eight.** Four of the five
additions — Minecraft, RimWorld, Terraria, Stellaris — are games substantially *defined* by their
modding ecosystems. That was not true of any of the original three.

**ADR 0011 decided game logic is plain Rust in the game crate**, no scripting layer, no dynamic
reload. It was a good decision and it was made properly: four candidates prototyped and measured
against one benchmark, with the recorded Luau prior refuted on evidence (`spikes/q1-game-logic/`).

**But it answered a different question than this one.** Q1 asked how *the developer* authors game
logic, and the deciding evidence was iteration speed — the feared 30-second rebuild measured at
0.9–3.2 s, so no architectural cost was worth paying for it. A **mod author is not the developer**.
They do not have the source, do not have a Rust toolchain, and cannot rebuild the engine. "Recompile
the game to add a mod" is not a modding story at any speed.

So the trigger ADR 0011 recorded does not cover this. It reserved WASM as a pre-selected escape hatch
behind a measured threshold — *a gameplay rebuild sustaining above 5 s* — which is an
iteration-speed trigger. Modding would open that hatch for an entirely unrelated reason.

**What is genuinely encouraging:** the escape hatch that was reserved happens to be the right tool.
The Q1 spike measured WASM as **bit-identical to native Rust** across two optimisation levels at
1.24× runtime cost, which is precisely what a deterministic engine needs from a mod sandbox — plus
it is sandboxed by construction, which matters far more for third-party code than for first-party.
And it is the same artefact M5's web export needs. So this is likely to be a *confirmation* of a
reserved option rather than a reversal of a decision.

### What to decide, and when

Not now. Nothing built so far is invalidated, and nothing in M1 or M2 depends on the answer. But it
should be settled before the module system hardens (M2–M3), because "what can a mod do" is really
"what is the module boundary", and retrofitting a sandbox boundary is far worse than designing to one.

Specific sub-questions when it is time:

- **Do mods get code, or only data?** RimWorld and Stellaris are heavily data-modded (XML-ish
  definitions) with code as the escalation. Amadeo's reflection registry plus the `.scene` and
  sidecar formats already give a strong data-modding story almost for free — that may cover most of
  the ground at very little cost, and it is worth measuring how far it reaches before assuming a VM.
- **If code: WASM, per the reserved hatch?** Re-run `spikes/q1-game-logic/measure.ps1` rather than
  arguing from the old numbers; the engine has grown since.
- **How does a mod stay inside I3?** A mod running simulation logic is simulation logic. Determinism
  is not negotiable, which rules out anything with the `f64`-versus-`f32` divergence that killed Luau.
- **How does a mod register components?** I8 says everything reflectable; a mod adding a component
  has to reach the same registry, and `ComponentId` is a name hash (ADR 0017), so mod-defined names
  need a collision story.

**Do not pre-emptively build a scripting layer for this.** ADR 0011's reasoning against paying a
permanent architectural cost up front still stands; what has changed is that a second, independent
reason to open the hatch now exists, and it should be recorded rather than rediscovered late.

**ADR 0036 adds a constraint worth knowing here:** physics is now bit-deterministic on any IEEE
754-2008 target, **including WASM**. So the reserved hatch has no physics exception — a mod running
in WASM sees the same simulation as native, which removes one of the harder objections to WASM
modding before it was ever raised.

### A second instance arrived in session 9 — ADR 0034

**ADR 0034 drew part of the module boundary, and it drew it closed.** The render graph is internal,
so a game or module cannot add a render pass; it configures a look through reflected data instead.
For a solo project that is the same act — adding an effect means editing `amadeo-render` — but for a
mod author it is a wall, and it is the *first* place this project has said "no" to third-party code
rather than simply not having got to it yet.

**That makes the question sharper rather than more urgent.** ADR 0034 reserves a Rust extension trait
as the escape hatch and names its trigger (an effect that genuinely is not parameters on an engine
effect), so nothing is foreclosed. But it is now the second subsystem where "what can a mod do" has a
concrete, already-decided answer, and the answers should agree with each other rather than be reached
one at a time. Worth reading ADR 0034 §5 alongside this question when it is time to settle it.

---

## Q18 · P2 · A reflected `ActionId` is a number nobody can read

New in session 8, created by ADR 0027 rather than found by it — the gap only became visible once
`world.resources` existed and could be pointed at a running game.

`InputState` is two maps keyed by `ActionId`, and an `ActionId` is the FNV-1a hash of an action's
name **with the name not kept** — deliberately, since that is what makes it fixed-size, `Copy`, and
cheap enough to look up every tick. So the protocol reports:

```json
"InputState": { "axes": { "8831028638596390904": { "previous": 0.0, "value": 0.0 } } }
```

Faithful, and useless. Pillar 3 is "what did I just do?", and this is the one resource whose whole
purpose is answering that.

**The names do exist.** The input driver holds a table of them — that is how a `.replay` file writes
`axis move_x 1.0` rather than a number. They sit outside `InputState` on purpose: a resource is part
of `state_hash`, and two runs that registered different *names* for the same actions must not
diverge.

### What to decide

Where the join happens, and how general it is:

- **At the presentation layer**, in `world.resources`: look up the driver's table when rendering. The
  narrow fix. Costs a crate edge from `amadeo-agent` to `amadeo-input`, which couples a generic
  protocol layer to one specific subsystem — the part worth thinking about.
- **A general "key alias" mechanism** in reflection, so any hash-derived key can carry a display
  form. More honest about the fact that this will recur — a chunk coordinate, a texture id, an
  entity relation could all want it — and more machinery.
- **Leave it.** The raw id is sufficient for a machine that also has `describe`, and an agent could
  hash the names itself to match them up.

Recommendation is deliberately not stated yet; this needs the second real instance before the general
shape is knowable. Nothing is blocked — `amadeo describe` and the `.replay` format both read fine.

**Session 10 added the first real consumer, and it went the other way than expected.**
`modules/amadeo-character` reads four named actions (`move_forward`, `move_right`, `turn`, `jump`) and
publishes them as `pub const` strings rather than putting `ActionId`s on the `CharacterController`
component. That was the right call for the module — a scene file carrying unreadable integers would
be worse than one carrying none — but it means the question is now **more** likely to bite: a game
inspecting a running character sees four opaque numbers in `InputState` and has to know the names to
match them up. Still not blocking. Worth revisiting when the second module wants input.

---

## Q23 · P1 · One environment per frame, when a world may hold several cameras

**New in session 9**, created by building ADR 0034's post-processing rather than found by reasoning
about it.

ADR 0034 puts the look on the **camera** — `environment "corridor_dark"` — which is what Godot,
Unity and Unreal all do, and what makes a render-to-texture security monitor showing night vision the
same mechanism as everything else.

**But ADR 0031 has every camera compose into *one* image.** A HUD camera loads what the world camera
left rather than clearing, which is what makes composition work without extra machinery. By the time
the post pass runs there is one picture and the cameras are no longer separable, so there is nothing
to apply two different looks to.

The current rule is therefore: **the post pass uses the environment of the camera that draws first**,
which is the same "which camera when there are several" rule ADR 0031 gave `render.describe`.
`FrameData::look` is where it happens, and every `View` still carries its own camera's environment —
the information is resolved and then deliberately not used, so nothing has to be re-plumbed later.

### What this actually costs today

A HUD camera cannot have a different grade from the world beneath it. That is the whole of it, and
nothing in either game wants it. `the_frames_look_comes_from_the_camera_that_draws_first` in
`crates/amadeo-render/tests/environment.rs` pins the behaviour so it is a known state rather than a
surprise.

### Why it should be decided with `Camera::target` rather than on its own

Per-camera post-processing means each camera drawing into **its own** transient, being post-processed
separately, and the results being composited. That is the same machinery `Camera::target` needs —
ADR 0031 shipped the field and nothing implements it yet, so a camera cannot actually render to a
texture. Both are "a camera owns its own image" and solving them twice would be building the same
thing twice.

Sub-questions when it is time:

- **Does compositing replace ADR 0031's load-rather-than-clear rule, or sit beside it?** The current
  rule is cheap and correct for a HUD; per-camera targets are correct for a minimap. They may both
  need to exist, selected by whether the camera names a target.
- **What does `render.describe` say** once cameras genuinely have separate images?
- **Does the frame get one graph or one per camera?** The graph already runs a pass per camera; this
  would make it a *subgraph* per camera, which is where Bevy landed.

Nothing is blocked. Decide before M2's exit gate, since gate 1 wants a 3D scene and gate 2 wants the
M1 2D scene still rendering unchanged — neither needs two looks, but render-to-texture is on M2's
list and that is the trigger.

---

## ~~Q24~~ · **Resolved — ADR 0036.** Physics is deterministic before it is fast

**Justin chose determinism, permanently.** `enhanced-determinism` is on, physics is single-threaded
and scalar, physics state is in the state hash, and the rapier version is pinned exactly so that an
upgrade is a deliberate replay regeneration rather than a mystery.

The deciding argument was the *failure mode* of the alternative rather than the merits of either: a
replay that passes on one machine and fails on another does not look like a physics configuration
problem, it looks like a bug in the game, and it is close to unattributable.

Two things the research settled that the question had not anticipated. **A per-game switch is not
available** — Cargo unifies features across a build, so two games in one workspace cannot disagree
about a feature rapier forbids combining. And **excluding physics from the state hash is not really
possible** either: a physics-driven character's position *is* gameplay state, so a replay that
skipped it would prove almost nothing about any game using physics.

The original text is below for the reasoning that led there.

<details>
<summary>Q24 as filed</summary>

### Rapier's determinism costs parallelism, and pins the version

**Raised in session 9**, prompted by Justin asking whether the engine needs a physics engine. It
does, and sooner than the question assumed: `amadeo-physics` is an **M2** item, and two of M2's four
exit gates depend on it — gate 1 wants "a physics-driven character controller you can walk around
with", gate 3 wants "a physics-heavy replay (200+ bodies) reproduces bit-identically across runs and
processes".

Gate 3 is the problem. **Invariant I3 is the keystone of this project**, and physics is the single
largest body of floating-point arithmetic the engine will ever run.

### What rapier actually guarantees

Better than feared, and with a price tag. Rapier offers an **`enhanced-determinism`** feature giving
bit-level cross-platform determinism — serialise the state after N steps on two different machines
with different CPUs and operating systems, and the bytes match. That is exactly what gate 3 asks for.

The conditions are the decision:

- **`enhanced-determinism` cannot be enabled alongside `parallel`, `simd-stable` or `simd-nightly`.**
  Determinism and multi-threaded or vectorised physics are **mutually exclusive**. So the choice is
  bit-identical replays or fast physics, and it cannot be deferred by taking both.
- **The target must comply strictly with IEEE 754-2008.** Fine for desktop and for WASM, which
  matters because WASM is both M5's web export and ADR 0011's reserved modding hatch.
- **Determinism holds for one rapier version.** An upgrade may legitimately change results, which
  invalidates every committed replay that contains physics — the same class of event ADR 0017
  described for an identity change, but triggered by a dependency rather than by us.

### What to decide, before `amadeo-physics` exists

1. **Is `enhanced-determinism` on, permanently?** The prior is **yes**, and strongly: I3 is
   non-negotiable and gate 3 is written against it. Then physics is single-threaded and scalar, and
   the throughput budget for gate 4 has to be set with that known rather than discovered.
2. **What does that do to Q9?** Q9 asks what runs off the simulation thread. This removes the most
   obvious candidate, which makes Q9 easier rather than harder — worth resolving the two together.
3. **How is a rapier upgrade handled?** Pinning an exact version is the minimum. Beyond that, the
   honest options are to treat a physics upgrade like a deliberate replay regeneration (with the
   `docs/07` procedure) or to keep a physics-free replay set that an upgrade cannot move. Probably
   both.
4. **Does the engine-owned trait boundary hide the version?** ADR 0002 already says rapier sits
   behind engine-owned traits. Worth checking that a rapier type never reaches a scene file, a
   snapshot or the state hash — because if one does, the wrapper is not actually a boundary.

### Why this is P0 rather than P2

Not because anything is blocked today, but because **the answer changes what gets built**. A physics
layer written assuming it may parallelise later is a different layer from one that knows it never
will, and `CLAUDE.md`'s trap list puts retrofitting determinism first: it is "the single most
expensive mistake available in this project". Decide before the crate exists, which is the same
rhythm ADRs 0031, 0033, 0034 and 0035 used.

---

## Q12 · P1 · `Service: Send + Sync` excludes every non-`Sync` runtime

Found by the Q1 spike (ADR 0011), which could not put a script VM in the world.

`Service` requires `Send + Sync`, added speculatively so the scheduler could run systems in parallel
later. Neither `mlua::Lua` nor `wasmtime::Store` is `Sync`, so **neither candidate could store its
runtime in the `World` at all** — both had to hide it in an `Rc<RefCell<..>>` captured by the system
closure, where `world.resources` and the rest of the introspection layer cannot see it.

Q1 was resolved in a direction that makes this moot for scripting. It is **not** moot generally:
`kira`'s audio manager, an asset loader holding a file watcher, and a `wgpu` surface are all likely
to hit the same wall in M3.

Options: relax `Service` to `Send` only and gate parallelism on a narrower bound; keep `Sync` and add
a `LocalService` store for main-thread-only machinery; or wrap offenders in a `Mutex` and pay for a
lock the single-threaded simulation does not need.

Prior: **a separate `LocalService` store.** It keeps the parallel-execution promise honest for things
that can keep it, and stays visible to introspection — which is the actual loss today.

Decide when the first real offender lands, which is M3 at the latest. Do not decide speculatively.

**Session 11 update: ADR 0041 changed the argument without resolving the question.** `Send + Sync` on
`Service` was added speculatively so the *scheduler* could run systems in parallel later — and
ADR 0041 decided it never will: system execution stays sequential and ordered, because the whole
simulation tick is 8.3 µs and parallelising it would optimise nothing.

So the original justification for the bound is gone. What replaced it is a better one: a `Service` is
where a background job's results land (ADR 0041), and `JobPool` and `Inbox` both genuinely cross
threads. The bound is now **earned rather than speculative**, which makes the `LocalService` prior
stronger rather than weaker — the things that need to be `Sync` really do, and the things that cannot
be have no reason to pretend.

**Session 15 update: the offender this entry named is now one step away.** ADR 0059 chose kira, and
`Audio` is a `Service` holding a `Box<dyn AudioBackend>` where the trait requires `Send + Sync`. The
`NullAudio` that ships today satisfies that trivially; **`kira::AudioManager` is the thing this
question predicted would not.**

So this is no longer "decide when the first real offender lands" — the next session to write the kira
backend is the one that has to decide, and it should decide *before* writing it rather than
discovering the bound halfway through. The three options are unchanged, and the prior is still a
separate `LocalService` store.

One cheap escape worth weighing against them, which was not obvious before the backend's shape was
settled: the backend could hold kira behind a `Mutex` and pay for a lock **once per frame**, because
`submit` is called exactly once per frame from one place. That is a very different cost from the
per-access lock the "wrap offenders in a `Mutex`" option originally imagined, and it may make the
cheapest option also the right one here — while leaving `LocalService` for a genuinely per-access
case like a file watcher.

**Session 16 update: the offender this question named for five sessions is not one, and none of the
three options was needed.** `kira::AudioManager<DefaultBackend>` is `Send + Sync` in kira 0.12, as
are `StaticSoundHandle`, `TrackHandle`, `SpatialTrackHandle` and `ListenerHandle`. Checked by
compiling the bound rather than by reading the source, **with a control case that fails** — a probe
that cannot fail proves nothing. `KiraAudio` now goes into the service like any other value, and
`the_backend_fits_in_a_service_without_a_mutex_or_a_local_store` pins it so a future kira release
that regresses it turns red with a name that says what happened.

**The reason is worth more than the result**, because it is a prior for the next candidate: kira's
desktop backend does not hold the `cpal` stream itself — it hands it to a stream-manager thread and
keeps a controller. **A library that already owns a thread has usually had to become `Send + Sync` in
order to.** So the suspicion belongs on libraries that expect to be driven from *your* thread — a
script VM, a `wgpu` surface tied to a window — rather than on libraries that merely feel low level.
`mlua::Lua` and `wasmtime::Store`, the two that actually failed in the Q1 spike, are both the former.

So this question keeps its priority and loses an example. The remaining candidates are an asset
loader holding a file watcher and a `wgpu` surface, and **neither has been tried**. Deciding now
would be deciding speculatively, which this entry has said to avoid since it was written. See
ADR 0060 §3.

**Session 16, second data point: `cosmic-text` is also fine.** `FontSystem`, `SwashCache` and
`Buffer` are all `Send + Sync`, probed the same way and with the same failing control. The reason is
different from kira's and the difference is the useful part: kira is `Sync` because it *already owns
a thread* and had to be; cosmic-text is `Sync` because it is **pure computation with no device
handle at all** — shaping a string touches nothing but memory.

Two of the three shapes a library can have are therefore safe, and the entry's remaining candidates
are both the third: a file watcher and a `wgpu` surface each hold something the *operating system*
gave them and expect to be driven from the thread that asked. **That is the shape to suspect** —
not "low level", and not "big".

---

## Q6 · P2 · Editor in-process or separate process?

Separate process is architecturally purer — it *forces* the RPC protocol to be complete, so a gap
becomes an immediate visible bug rather than a slow drift toward editor privilege. Also gives crash
isolation. Costs latency and complexity.

Prior: **separate**, specifically because the discipline it imposes protects invariant I5, which is
the hardest invariant to keep honest.

Decide before M4.

---

## ~~Q7~~ · **Resolved — ADR 0029.** Prefabs are assets; an override reaches only the instance root

**Resolved in session 8**, after `games/vault` ran straight into it — forty-four wall tiles would be
four hundred lines of near-identical scene text.

- **`from` holds an asset id**, superseding ADR 0014's path grammar. A prefab is an asset, so the
  whole asset toolchain applies to it for nothing.
- **An override is a patch on the instance root, and reaches nothing inside.**
- **A dangling override refuses to load**, naming the entity, the component, and the prefab.

**The research is what decided the middle one.** Unity's overrides evaporate with nesting because an
override names something *inside* a prefab and has to track it across every future edit of that
prefab; Godot's editable children can write back to the source scene. Both failures come from
overrides reaching inward — so here they do not, which makes nesting **structurally** safe rather
than merely carefully handled.

Proof: the Vault's scene went from 223 lines to 142, and `collect-three.replay` matched all four
checkpoints **unchanged** — the same world, authored differently. Full reasoning in `docs/adr/0029`,
including what it deliberately does *not* fix (the wall grid, which wants a tilemap rather than
prefabs).

<details><summary>Q7 as filed</summary>

### Prefab override semantics

The hardest problem in the scene subsystem. Instance-level field overrides, nested prefabs, and
propagation of prefab changes to non-overridden fields on instances.

Unity gets this genuinely wrong (hidden override state, confusing propagation). Godot is better but
still surprising. Requirement here: **all override state is visible in the text file.** No hidden
state, ever.

Needs design work in M1, and it's worth studying both engines' failure modes first.

### A smaller question inside it, found in session 7 and needing an answer first

**What does `from` actually hold — a path or an asset id?** Two accepted ADRs disagree:

- **ADR 0014** specifies the grammar as `entity <id> "<name>" from <path>` and its worked example,
  which a test pins byte-for-byte, uses `from prefabs/door_metal`.
- **ADR 0020** decided an asset is named by a declared id rather than a path, and its worked example
  uses `entity a1 "Wall" from wall_concrete`.

These are not reconcilable as written: `prefabs/door_metal` is not a usable asset id, because
`is_usable_id` rejects `/` — an id appears bare in a scene line and a slash would be ambiguous.

The likely answer is that ADR 0020 supersedes 0014 here, since under 0020 a prefab is simply an
asset with a declared id, which makes `from` an ordinary asset reference and unifies the two ideas.
But that has consequences this question owns: a prefab reference would then count toward ADR 0021's
load barrier, and `amadeo check` would validate it against the catalogue.

Nothing is broken today because prefab instancing is refused outright (`PrefabNotSupported`), and
`SceneDocument::required_assets` deliberately covers only the declared `assets` block and says why.
**Decide this before building prefab instancing**, and supersede whichever ADR loses.

</details>

---

## ~~Q19~~ · **Resolved — `amadeo import --assets <dir>`**

**Found in session 8 by hitting it, fixed the same session.** A bootstrapping deadlock created by
ADR 0029 making prefabs assets:

1. a prefab needs a `.ama-meta` sidecar before anything can reference it;
2. `amadeo import` writes sidecars;
3. but `import` launched the game (ADR 0016) to ask where its asset directory is;
4. and the game refuses to start while a prefab its scene names has no sidecar.

So the tool that fixes the problem could not run, and the Vault's two sidecars were written by hand.

**`--assets <dir>` names the directory directly**, which breaks the cycle and makes `import` work on
a project that does not compile at all — the right property for a repair tool. Asking the game stays
the default, because it is authoritative: the path is a constant in the game's own source, so nothing
can disagree with it.

**Verified by reproducing the deadlock**: the two sidecars were deleted, `amadeo import --package
vault` failed exactly as before, `amadeo import --assets games/vault/assets` wrote them without
launching anything, and the files came back **byte-identical** to the hand-written ones.

### Why not a manifest key

The first attempt put `assets = "..."` in `amadeo.toml`, and it was wrong: a manifest is per-*project*
and an asset directory is per-*game*, so in this repo — which has two games — the key could only ever
describe one of them. The Vault, which is the case that motivated the question, runs under
`--package vault` and would have fallen straight back to launching the game. A flag has no such
blind spot, and it avoids growing the hand-written TOML subset toward tables.

### The rest of the question, checked

`amadeo check` and `amadeo assets` launch the game for the same reason and are **not** deadlocked by
it, because neither is the tool that repairs the problem — they report it. `check` in particular gives
the error that sends you to `import`, which now works.

---

## Q8 · P2 · General entity relations, or just parent/child?

Games want many relationships: equipped-by, targeting, owned-by, docked-to. Parent/child covers
transforms only.

Prior: plain components first (`Targeting(Entity)`), revisit if it becomes painful. General relations
are a significant ECS complexity increase and it's not yet clear we need them.

---

## ~~Q9~~ · **Resolved — ADR 0041.** Parallelism is deterministic by construction, or it does not exist

Raised in session 2, resolved in session 11 — and resolved the way it asked to be. It said *"decide
before adding the first background task, not after"*, and Justin asking for multithreaded asset
loading and parallel ECS queries **was** the first background task.

**The research found three specific ways parallelism destroys determinism**, all demonstrated in
shipping engines: Bevy's own `ParallelCommands` documents that command order depends on thread count;
floating-point addition is not associative, so parallel reduction changes results (the same wall
ADR 0036 hit); and whether a job finished by tick N depends on the wall clock, which diverges a
replay even when every computation was correct.

**But ECS is a viable deterministic concurrency model on a precise condition**: each parallel unit
must write only its own entity's components, with no shared accumulator and no cross-entity reads.
That is enforceable by API shape, which is what ADR 0041 does — the unsafe shapes are made
*unspellable* rather than discouraged.

**And one measurement set the priority.** Gate 4 says the whole simulation tick is 8.3 µs. Parallel
*system execution* would optimise something that costs nothing; asset loading and chunk meshing are
where the work is, and neither is a gameplay system.

What was built: `amadeo-jobs` (barrier or `Service`, `Inbox` draining in key order),
`par_for_each_mut` (`Fn + Sync` closure), background asset loading. System execution stays sequential
and ordered.

**Q12 is not moot** — see below. But `Service: Send + Sync` is now an earned bound rather than a
speculative one.

---

## Q11 · P2 · Agent introspection across a client/server split

Once M6 lands, does `world.query` report the server's authoritative state or a client's predicted
state? Both are useful for different debugging tasks, so this may need to be an explicit target
parameter on the RPC rather than a single choice.

Worth deciding before M6 rather than during it. Networked gameplay is the hardest thing in this project
to debug, which makes it the place where good introspection pays off most — and the place where bad
introspection hurts most.

---

## ~~Q10~~ · **Resolved — ADR 0031.** Both simultaneously, and the camera is what selects

Asked whether a project picks 2D or 3D at build time or can freely mix them. **Both, always** — there
is no build-time switch and there never was a good case for one, because the thing that decides what
gets drawn is a *camera*, and a camera is now an entity. A world can hold an orthographic camera
drawing sprites and a perspective camera drawing meshes at the same time, each with its own target.

The question anticipated "2D UI over a 3D world" as the hard case and guessed it might be an
`amadeo-ui` concern rather than a renderer one. That guess was right and ADR 0018 had already made it
moot: UI over the world is a higher `SortOrder`, not a separate hierarchy or a separate projection.

The case it did *not* anticipate is the one that mattered: **isometric**. Project Zomboid is neither
2D nor 3D — an orthographic projection feeding sprite drawing with Y-sorting. A build-time dimension
switch would have had no answer for it. A camera does: it is a projection setting.

---

## ~~Q22~~ · **Resolved — ADR 0017's second instalment**, which had been waiting for ADR 0027

**Found in session 8** while building the control experiment for ADR 0031's replay regeneration — by
writing a stand-in type with the same canonical name and the same fields, and finding it hashed
differently. `ResourceId::of` hashed `std::any::type_name`, the **Rust path**, where `ComponentId`
hashes the *canonical* name. Opposite rules, so **moving a resource between crates silently
invalidated every golden replay** with nothing in the type's own definition having changed.

**It turned out not to be a new question at all.** ADR 0017 had already decided it and deferred only
the timing:

> `ResourceId` and `ServiceId` keep the path. Neither is reflected, so neither has a canonical name
> to use. **Resources get this treatment when `Resource: Reflect` lands**; services never will, since
> they are engine machinery that no file names.

ADR 0027 landed that bound earlier in the same session. The trigger had fired and been missed —
which is worth noticing as a *class* of mistake: a deferred obligation inside an accepted ADR has
nothing watching it, and only surfaces when something trips over the inconsistency.

ADR 0017 also anticipated the cost and argued for exactly this timing. It rejected "defer and change
both together" because **"the cost of this decision grows with every recorded replay"** — so the
right moment to act on the trigger is as soon as it fires.

Services keep the Rust path, permanently and for the reason ADR 0017 gave: a service is not reflected,
is not in any state hash (ADR 0009), and is named by no file.

**Three replays regenerated**, and the signature was exactly the one ADR 0017 recorded for an
identity change: the input streams are byte-identical and only the checkpoint lines moved. Confirmed
independently by snapshot diff — the world before and after is byte-identical apart from the
`state-hash` line, so nothing about *behaviour* moved.

---

## ~~Q21~~ · **Resolved — ADR 0032.** A block of named fields is a struct

**Found in session 8 by probing the format**, resolved the same session. A block of `name value`
lines is a struct; a block of `- ` lines is a list; a bare variant name with a block beneath it is an
enum carrying data. That is YAML's rule, it needs no schema — which matters, because layer 1 has
none — and the grammar already had the slot, since a field with no inline value already opened a
block.

What a component field can be now:

| Shape | |
|---|---|
| scalar (`f32`, `bool`, `string`, ints) | ✅ |
| flat list (`[f32; 3]`, `Vec<f32>`) | ✅ |
| fieldless enum variant (`phase Playing`) | ✅ |
| nested struct, to any depth | ✅ ADR 0032 |
| enum with a payload | ✅ ADR 0032 |
| map | ✅ ADR 0032 — same syntax as a struct, which closes ADR 0027's recorded gap |
| **`Option::None`** | ❌ **still**, deliberately |
| **anything empty** as a field value | ❌ an empty block is a parse error |

**`None` was left unsolved on purpose.** `none` collides with an enum variant of that name; a sigil
would be this format's first punctuation, having chosen indentation over punctuation throughout; and
omitting the field destroys ADR 0014's distinction between "explicitly nothing" and "whoever wrote
this forgot". Nothing in the engine has an `Option` field, so it waits for a real case to argue from.

**`Projection` was un-flattened immediately**, which was the point: `Orthographic { height }` and
`Perspective { fov, near, far }` each carry only what they need, and `Projection::height()` returns
`None` for a perspective camera rather than a fallback number.

Two defects fell out of doing it, both found by use rather than reasoning: the derive was silently
dropping `min`/`max`/`unit` on **enum variant fields**, so a field lost its range simply by moving
into a variant; and `amadeo-snapshot` could not write a payload enum, so a snapshot of any world
holding one would capture and then refuse to restore. Both fixed, both now tested.

---

## Q20 · P2 · Gate 4's stronger test has never been run

**Left over from ADR 0030**, and stated as a caveat in `docs/09-gate-4-describe-is-not-enough.md`
rather than hidden.

The gate-4 experiment was run by an agent that had **already read the engine source in the same
session**. So the five gaps it reported are ones that agent *noticed* while reaching for knowledge
`describe` did not supply — not ones it was actually stopped by. The gaps are structural and can be
verified by reading the output, which is why acting on them was safe. But the question the gate was
really asking is unanswered.

**The honest test:** hand `describe` output to a reader with no prior exposure to this engine and see
what they produce. That is now more interesting than it was, because ADR 0030 changed the answer —
resources are visible, the schema is closed, and `describe.example` shows the spellings.

Cheap to run and worth doing once M2 has added enough that the schema is not trivially small. It
would also be the first real evidence for or against `docs/03-ai-native-design.md`'s central premise,
which has so far been argued rather than measured.

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
| Human-legibility of the code | Hard requirement — boring Rust over clever Rust | `CLAUDE.md` §6 |
| Target games | Palworld, Schedule I, Inside the Backrooms — used as a prioritisation signal | `00-vision.md` |
| First game to finish | Single-player first-person atmospheric horror slice, at M3 | `00-vision.md` |
| Multiplayer | No longer a non-goal. Client-server with server authority; hooks reserved M0–M2, netcode at M6 | `adr/0006` |
| **Q5** — fixed timestep rate | 60 Hz logic tick, configurable physics substeps | `adr/0007` |
| ECS storage strategy | Archetype tables, columns as concrete `Vec<T>` behind a safe trait object, downcast once per archetype per query. No `unsafe` | `adr/0008` |
| Where non-simulation globals live | Two stores: `Resource` (hashed) and `Service` (not hashed), enforced by trait bounds | `adr/0009` |
| `ComponentId` derivation | Hash of a *name*, never `TypeId` — `TypeId` is not stable across builds. **Which** name is `adr/0017` | `adr/0008` |
| System ordering tie-break | Alphabetical by label, never registration order | `amadeo-app` schedule docs |
| Spawning from a command buffer | `spawn_with(closure)` rather than a reserved-id handle; new entity not referenceable by other commands in the same batch | `amadeo-ecs` commands docs |
| **Q1** — game logic authoring and hot reload | **Rust, compiled in. No scripting layer.** WASM reserved as the escape hatch behind a measured threshold; snapshots promoted as the real iteration-loop fix | `adr/0011` |
| Reflection shape | Value tree plus two derives, not dynamic field access; metadata vocabulary fixed | `adr/0012` |
| Is reflection optional? | No — `Component: Reflect` makes I8 a compiler-enforced bound | `adr/0013` |
| **Q13** — `ComponentId` derivation | **The canonical name** (`Reflect::type_name`), not the Rust path. Moving a component between crates is free; renaming one is a deliberate, visible change | `adr/0017` |
| Asset load timing vs determinism | **The simulation never observes asset state** — gameplay holds an id and never asks whether it is loaded, and anything gameplay needs is authored rather than derived. Plus a load barrier at scene entry. Makes streaming safe to add later without a redesign | `adr/0021` |
| **Q4** — asset identity | **A declared `id` in the asset's sidecar**, not its path and not a GUID. Defaults to the filename stem on import, so it reads like a path but survives a move. Duplicate ids refused at scan time, naming both files | `adr/0020` |
| **Q3** (two thirds) — transform and sort order | **One 3D `Transform`**, 2D is its degenerate case; rotation is Euler degrees so it stays hand-writable. **`SortOrder`** replaces `Quad::layer` and dominates depth. Pipeline shape still open | `adr/0018` |
| **Q2** — scene file syntax | **A custom, indentation-based, line-oriented format.** Chosen by Justin from four hand-written candidates; TOML is the fallback, *not* KDL | `adr/0014` |
| Where hierarchy components live | `amadeo-transform`, below `amadeo-scene` — render, physics, and anim all need transforms | `adr/0015` |
| **Q14** — where `describe` runs | **The game binary hosts the agent; `amadeo-cli` launches it and speaks JSON-RPC over stdio.** First transport is one-shot batch, not a live session; `App` owns the `ComponentRegistry`; the JSON parser is hand-written | `adr/0016` |
