# ADR 0014 — The scene format is a custom, line-oriented syntax

**Status:** Accepted · **Date:** 2026-07-31 · **Resolves:** open question Q2

## Context

Q2 asked which concrete syntax scene files should use. `docs/06-open-questions.md` calls it
"arguably the most user-visible decision in the project — it's the file both authors literally type
into", and prescribed the method: hand-write the same moderately complex nested scene in RON, TOML,
KDL, and a custom format, then judge readability and diff behaviour.

That spike is in `spikes/q2-scene-format/`, with all four files, a reproducible comparison script,
and the full per-format trade-offs. Summary of what it found:

| | content lines | tune a value | add an entity | 3-way merge |
|---|---|---|---|---|
| RON | 72 | 1 line | 17 lines | clean |
| KDL | 50 | 1 line | 13 lines | clean |
| TOML | 46 | 1 line | 15 lines | clean |
| **custom** | **37** | 1 line | **10 lines** | clean |

**The prescribed criterion turned out not to discriminate.** Tuning a value — by a wide margin the
most common edit anyone makes — is an identical one-line diff in all four, and a three-way merge of
two unrelated edits is clean in all four. The question assumed diff quality would decide it. It does
not, and that is worth recording so the argument is not re-run.

What decided it was qualitative, and Justin made the call after reviewing the trade-offs.

## Decision

**A custom, indentation-based, line-oriented syntax**, specified below and implemented in
`amadeo-scene`. File extension `.scene`.

### Worked example

Shown in **canonical form** — this is exactly what `amadeo fmt` emits, components sorted and all.
Hand-written input need not be sorted; the formatter puts it in this order.

`the_adr_example_is_canonical` in `crates/amadeo-scene/tests/round_trip.rs` asserts this exact text
round-trips byte-identically, so this block is executable documentation rather than a thing that
drifts away from the implementation.

```text
scene corridor_a
version 1

entity a1 "Corridor"
  Transform2d
    position 0.0 0.0
    rotation 0.0
    scale 1.0 1.0

  entity a2 "CeilingLight"
    Flicker
      pattern Irregular
      speed 12.5
    PointLight
      color 1.0 0.85 0.6
      intensity 3.2
      range 8.0

  entity a3 "Door" from prefabs/door_metal
    override Door
      key_id "rusted_key"
      locked true
    override Transform2d
      position 4.0 0.0

  entity a4 "Wanderer"
    Enemy
      sight_range 3.5
      state Patrol
      waypoints
        - 0.0 0.0
        - 4.0 0.0
        - 4.0 3.0
```

Comments run from `#` to end of line and are stripped by the parser, so they do not survive a
format. That is a real limitation — a commented scene loses its comments when the editor saves it —
and is recorded in Consequences below.

### Grammar

Indentation is **two spaces per level**. Tabs are rejected with a dedicated error rather than
silently accepted, because mixed indentation is invisible and produces baffling failures.

A line's first word says what it is:

| First word | Meaning |
|---|---|
| `scene <id>` | the document header, at indent 0 |
| `version <n>` | schema version of the document, at indent 0 |
| `entity <id> "<name>"` | an entity; `from <path>` makes it a prefab instance |
| `override <Component>` | a prefab override block; only valid on an entity with `from` |
| `- <values...>` | one element of the enclosing list |
| anything else | a component name (inside an entity) or a field name (inside a component) |

Nesting is by indentation: what is indented under an entity belongs to it, and an `entity` line
indented under another entity is its child.

### Values

| Written | Parsed as |
|---|---|
| `true` / `false` | boolean |
| `12`, `-3` | integer |
| `1.0`, `-2.5`, `1e3` | float |
| `"quoted text"` | string |
| `Patrol` | bare identifier — an enum variant |
| `0.0 0.0` (several on one line) | a list |

Bare words are identifiers and quoted words are strings, so `pattern Irregular` and
`key_id "rusted_key"` are unambiguously different things. `true` and `false` are booleans only in
lowercase; `True` is an identifier.

### Canonical form (invariant I2)

- two-space indentation
- **fields sorted by name** within a component — they are a set, and this matches how
  `amadeo_reflect::Value` already sorts struct fields
- **components sorted by name** within an entity
- **child entities in declaration order** — they are a *sequence*, not a set. Sibling order is
  meaningful (draw order, iteration order), so sorting them would destroy information and reordering
  them is a real change that should produce a real diff.
- one blank line between sibling entities, none between fields
- `amadeo fmt` is the sole authority and is idempotent

### Two layers, deliberately

**Layer 1 — syntax.** Parsing and formatting need no schema. This is what `amadeo fmt` uses, and it
is what makes byte-stable round-tripping testable on its own. A scene referencing a component from a
module you have not loaded still formats correctly.

**Layer 2 — schema.** Binding parsed values to real component types, checking fields against the
reflection registry, and coercing numbers to their declared widths. This is what `amadeo check` and
scene loading use.

Splitting them means a syntax error and a schema error are different things with different messages,
and it matches the two CLI commands the roadmap already lists.

## Rationale

1. **Half the size of RON, a quarter smaller than TOML or KDL**, for identical content. On a real
   level file that is the difference between scrolling and reading — and for an agent it is the
   difference between holding one screen of a level in context and holding two.

2. **The tree stays visible.** This is what ruled TOML out. TOML cannot nest without misery, so its
   honest design is flat with `parent = "a1"` references, and you rebuild the hierarchy by following
   ids. Machines do that effortlessly; humans do not, and a scene file exists to be read by both.

3. **We own the error messages.** `docs/03-ai-native-design.md` Pillar 5 makes error quality a
   functional requirement with a worked example of the standard, and an agent's only feedback channel
   *is* the error message. A third-party parser gives us whatever that crate decided to emit.

4. **The tooling argument for a standard format is weaker than it looks.** I1 and I2 require
   `amadeo fmt` as the sole canonical-formatting authority and `amadeo check` for schema errors
   carrying file, line, and a suggested fix. No third-party parser supplies either. Those get written
   regardless, which makes owning the parser a marginal cost rather than a new one.

5. **The precedent exists and works.** `amadeo-input`'s `.replay` format is a custom text format this
   project already built to exactly these rules — hand-writable, line-oriented, canonically ordered,
   byte-stable, with parse errors carrying line numbers. This is a second application of a proven
   pattern, not a leap.

## Consequences

- **We own the parser, the formatter, and any editor support that ever exists.** No syntax
  highlighting, no LSP, no GitHub rendering unless we build them. The mitigation is that `amadeo
  check` has to be genuinely good, which it had to be anyway — but the standard is now higher,
  because it is the *only* thing catching a typo before load.

- **Indentation sensitivity brings the Python problem.** Mitigated by rejecting tabs outright and by
  reporting a wrong indent with its line number rather than guessing, but a badly indented file is a
  real failure mode that a brace-delimited format would not have. Note that `amadeo fmt` cannot
  *repair* indentation — indentation is the structure, so a mis-indented line is ambiguous rather
  than merely untidy.

- **Comments do not survive a format.** The parser strips them, so a scene the editor saves loses
  any comments a human wrote. Every engine with a graphical editor has this problem, and the usual
  answer is to attach comments to the node they precede and re-emit them. Worth doing eventually;
  not worth doing before the format has users.

- **The parser is engine code Justin may need to debug.** Deliberately kept as a straightforward
  line-oriented recursive descent — considerably easier to follow than the ECS, and one of the more
  approachable files in the codebase by design rather than by luck.

- **KDL was rejected partly on an agent-specific ground**, which is unusual enough to record: it is
  the format Claude is least reliable at, being materially less represented in training data than
  TOML or JSON, and the KDL v1→v2 syntax change makes emitting stale syntax a live failure mode on
  the most common artefact in the project. The errors that would catch that are not ours to improve.

- **An asymmetry that will grow.** After M4 Justin has an editor and his hand-editing burden drops
  sharply; the agent's does not. Tooling arguments therefore decay in weight over the project's life
  while compactness and controllable diagnostics grow. Worth remembering if this is ever revisited.

## Rejected alternatives

**TOML.** The strongest contender and the one to fall back to if owning a parser ever becomes
unattractive. Most familiar, best tooling, most transferable, and its flat structure makes it the
most greppable and most merge-proof of the four — Godot's `.tscn` and Unity's YAML both chose
effectively flat for large scenes, which is real evidence. Rejected because flattening removes the
hierarchy from the file, which is the single thing a scene file exists to show.

**KDL.** Its node/property/children model fits a scene tree better than anything else here, and the
spec and parser are somebody else's problem. Rejected on the agent-accuracy ground above plus a
thinner ecosystem, an awkward list syntax, and error messages we do not control. **If this decision
is revisited, prefer TOML over KDL** — the reverse of what the format's fit would suggest.

**RON.** Rejected on size (roughly 2× everything) and because its one distinguishing feature is
unusable: typed enums would require a closed `enum Component`, and modules add components (I4), so
the component set has to stay open and components degrade to an untyped map.
