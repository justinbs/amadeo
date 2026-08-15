# ADR 0071 — A generated interior is a scene file, stitched from socketed pieces

**Status:** Accepted · **Date:** 2026-08-15 · **Builds on:** ADR 0014, ADR 0021, ADR 0029, ADR 0043 ·
**Resolves:** Q40 · **Settles:** `docs/05`'s M3 exit gate item 2

## Context

M3's exit gate asks for "bounded procedural interiors — assembled from handcrafted room pieces, not
one static level. Tests the scene composition and prefab instancing design under real use."

Q40 researched the three algorithm families that exist in practice — a room graph with socket
stitching, wave function collapse, and Unexplored's cyclic generation — and argued that **the
algorithm is the second question**. The first is what a generator *produces*, because invariant I1
says text files are the only source of truth and the editor is a client that reads and writes them.

Justin chose the room graph.

## Decision

### 1. The output is a scene file, and the generator is a tool

`amadeo` generates a `.scene` and writes it to disk. It does not build a world in memory that exists
only for one run.

**This is the whole reason the artefact question came first.** A generated level that is a text file
is one that every existing thing already works on, with nothing built: `amadeo fmt` formats it,
`amadeo check` validates it against the real component schema, `amadeo query` inspects it, prefab
instancing composes it, a snapshot captures it, and the editor — when it exists — opens it. A
designer or an agent generates twenty, keeps the good one, and **moves a door by hand**.

A seed-only level forecloses all of that. It would narrow I1 from "the level is text" to "the level's
*inputs* are text", which is exactly the kind of quiet narrowing `CLAUDE.md`'s trap 1 is about.

**The precedent that looks like a counterexample is not one.** ADR 0043's terrain *is* a function of
its seed, streamed at runtime — and ADR 0046 then had to add `TerrainEdits` as a hashed resource so
that digging survived at all. That is "generated base plus authored changes" arrived at the hard way,
and it is the same shape as this, reached from the other end. An interior is small, bounded, and
wants to feel *designed*; a horizon of hills is none of those.

### 2. A piece is a prefab, and a socket is authored data

A room piece is an ordinary scene file — geometry, colliders, lights, whatever it holds — with one
addition: **child entities carrying a `Socket`**, marking where another piece may join.

A socket is a place and a facing, both authored. Not inferred from geometry, for the reason ADR
0044 §2 gives about terrain: the *shape* of a thing is content, and a generator that guessed at
doorways from bounding boxes would be deriving authored intent from a mesh. It would also be
unfixable by hand, which is the property this whole decision exists to protect.

Two sockets join when they **face each other**, which makes stitching a placement rather than a
search: given a piece, a socket on it, and a socket on the piece being added, the second piece's
transform is fully determined.

Nothing new is needed to *have* pieces. ADR 0029 already instantiates a prefab by asset id with
top-level overrides, which is what a placed room is.

### 3. A room graph, and cycles are a first-class request

The generator picks a layout as a **graph of rooms** and realises it by stitching pieces at matching
sockets. Not a tree: Unexplored's argument is the one to take seriously here — a tree of rooms forces
backtracking, and *being chased down a dead end you have already cleared* is the exact failure a
horror slice cannot afford.

So the generator is asked for a loop, not merely permitted to produce one.

**Wave function collapse and cyclic generation are both still available**, and that is deliberate:
they sit on the same pieces and the same socket data. WFC is where to go if the *rooms* want more
variety than a library of hand-built ones gives; cyclic generation is where to go if the *layouts*
read as boring. Neither is foreclosed and neither is paid for now.

### 4. Determinism is by seed, and the seed is recorded in the file

Generation runs off a seeded `Rng` — no `HashMap` iteration, no wall clock. The seed is written into
the generated scene as a comment-equivalent so that a layout can be regenerated from the file that
came out of it.

This matters less than it does for terrain, precisely because the output is a file: the level does
not have to be reproducible at runtime, because it is not produced at runtime. What the seed buys is
the ability to say *"generate that one again, but with the corridor longer"* — which is a authoring
affordance rather than an I3 obligation.

### 5. It lives in the game first

The generator is built in `games/warren` and moves to `modules/` when a second game wants it.

That is this project's own rule and it has been paid for twice: `modules/amadeo-camera` lived in
`games/scarp` first and was better for it, and `modules/amadeo-interaction` was built straight as a
module against nobody and had a real defect that its first user found within an hour (session 18).
A level generator has far more genre in it than a camera rig does, so the risk of designing it
against zero users is correspondingly higher.

## Consequences

- **`amadeo check` becomes the generator's test.** A generated scene either validates against the
  real component schema or it does not, and that is a much stronger check than anything a generator
  can assert about itself.
- **A generated level is reviewable in a diff**, which is unusual and worth using: two seeds produce
  two files, and the difference between them is readable.
- **Byte-stability applies (I2).** The writer is `amadeo-scene`'s canonical one, so a generated scene
  is already sorted and stable; `amadeo fmt --check` on generator output is a free regression test.
- **Sockets have to be authored on every piece**, which is real work per piece and is the cost of not
  guessing. A piece with no sockets is a piece nothing can attach to, and that should be *reported*
  rather than silently dropped — `SoundCache::failures`' rule again.
- **Nothing generates at runtime yet**, so "a fresh layout per run" is not answered by this. The
  bounded answer, if it is wanted, is to generate *n* levels as files and pick one per run: still
  diffable, still varied, and it needs no runtime generator. That is deliberately left open.

## Rejected alternatives

**A seed, generated at load.** Rejected for §1's reasons. It is the cheaper design and it is what
most engines do, and it costs exactly the thing this engine is built to protect.

**Inferring sockets from geometry.** Rejected as guessing at authored intent, and as unfixable by
hand.

**Wave function collapse now.** Rejected as more machinery than the requirement justifies: the gate
wants a bounded space with a key, a door, and enough loop that a chase is not a dead end. It is also
a *search*, which can fail and backtrack — awkward to reason about, and awkward to give global
guarantees like "the key is reachable before the door".
