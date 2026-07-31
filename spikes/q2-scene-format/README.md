# Q2 spike — which concrete syntax for scene files?

Evidence for open question **Q2**, which `docs/06-open-questions.md` calls "arguably the most
user-visible decision in the project — it's the file both authors literally type into."

The method is the one the question itself prescribes: write the *same* moderately complex nested
scene by hand in all four candidates, then judge readability and diff behaviour.

```
pwsh -File compare.ps1
```

Measured 2026-07-31. Unlike the Q1 spike, **this one does not settle itself** — see "What the
numbers do and do not decide".

---

## The scene

A fragment of the kind of level M3's horror slice needs: a corridor with a flickering ceiling light,
a door instanced from a prefab with overrides, and a patrolling entity. Deliberately exercises the
things that are hard rather than the things that demo well:

- **two levels of nesting** — the thing TOML is known to struggle with
- **arrays** (`position`), and **arrays of arrays** (`waypoints`)
- **an enum value** (`state Patrol`)
- **a prefab instance with field-level overrides**, which `docs/04-subsystems.md` §9 calls the
  hardest problem in the subsystem and requires to be visible in the text with no hidden state
- **comments**, since a scene a human maintains will have them

Each candidate carries a header comment explaining how it was written and where it was given the
benefit of the doubt. All four are written as someone fluent in that format would write it, not as a
strawman.

## Results

| | content lines | edit a value | add an entity | 3-way merge |
|---|---|---|---|---|
| **RON** | 72 | 1 line | 17 lines | clean |
| **KDL** | 50 | 1 line | 13 lines | clean |
| **TOML** | 46 | 1 line | 15 lines | clean |
| **custom** | **37** | 1 line | **10 lines** | clean |

Content lines exclude blanks and comments — each candidate's header comment is a different length,
and counting those would measure how much I wrote *about* a format rather than the format.

## What the numbers do and do not decide

**Diff behaviour does not separate these formats, and that is the main empirical finding.**

The question assumed diff quality would be the discriminator. It is not. Tuning a value — by a wide
margin the most common edit anyone makes to a scene — produces an identical one-line diff in all
four. All four are line-oriented enough that a three-way merge of two unrelated edits succeeds
without conflict. A compactness trap was worth checking (tighter files put unrelated edits closer
together, where git's context windows might overlap) and did not materialise at this scale.

So the arguments that remain are qualitative:

**Verbosity.** The custom format is roughly **half** the size of RON for identical content, and
about 25% smaller than TOML or KDL. On a real level file of thousands of lines that is the
difference between scrolling and reading.

**Whether the tree is visible.** TOML cannot nest without misery — a grandchild needs
`[[entity.children.children]]` and every level repeats the whole path. So the honest TOML design is
*flat*, with explicit `parent = "a1"` references. That is a real design (Unity and Godot both do
essentially this), and it costs the thing a scene file is for: you can no longer see the hierarchy,
you reconstruct it by following ids. A machine does that effortlessly; a human does not. **This is
the strongest single differentiator in the comparison**, and it is not visible in any number above.

**RON loses its best feature here.** RON is worth choosing for its enum support, and a scene format
cannot use it: a closed `enum Component { Transform2d(..), .. }` would have to name every component
in the engine, and modules add components (invariant I4). So components degrade to an untyped map,
at which point RON is a verbose JSON with better comments.

**Tooling cost is smaller than it looks.** The obvious argument for a third-party format is a free
parser. But I1 and I2 require `amadeo fmt` as the single canonical-formatting authority and
`amadeo check` for schema validation with file, line, and a suggested fix — and a third-party parser
gives us neither. Those get written whichever way this goes. The marginal cost of owning the parser
too is one hand-written recursive-descent pass over a line-oriented grammar.

**Error quality is a stated functional requirement, not a nicety.** `docs/03-ai-native-design.md`
Pillar 5 sets a specific bar and gives a worked example. With `serde` + a third-party format, the
error you get is the one that crate decided to produce. With our own parser, the error is ours to
design — and an agent's only feedback channel is the error message.

**The precedent already exists.** `amadeo-input`'s `.replay` format is a custom text format this
project already built to exactly these rules: hand-writable, line-oriented, canonically ordered,
byte-stable round-trip, and parse errors carrying line numbers. It works, it is small, and it is a
template rather than a leap.

## Recommendation

**The custom format, with KDL as the fallback** if the appetite for owning a parser is lower than I
am assuming.

The case for custom: it is the most compact by a clear margin, it keeps the tree visible, it makes
byte-stability structural rather than something the formatter has to be careful about, it puts error
quality under our control where Pillar 5 says it belongs — and the tooling we would have to write
anyway is most of the work.

The case for KDL instead: the node/property/children model genuinely fits a scene tree, the spec and
parser are somebody else's problem, and it gets syntax highlighting for free. Its costs are real
but modest — a slightly awkward list syntax (`- 0.0 0.0`), the unfamiliar KDL v2 `#true`, and error
messages we do not own.

**TOML and RON are not recommended.** TOML because it forces the hierarchy out of the file, which is
the one thing a scene file exists to show. RON because the component set has to stay open, which
removes the only reason to prefer it.

## What this spike does not settle

Prefab override *semantics* (Q7) — nesting, propagation, and what happens when a prefab changes
under an instance that overrode some of its fields. This spike only shows that all four syntaxes can
represent overrides visibly. The semantics are a separate and harder design problem.
