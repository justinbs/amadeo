# ADR 0032 — A block of named fields is a struct

**Status:** Accepted · **Date:** 2026-08-04 · **Resolves:** Q21 · **Extends:** ADR 0014

## Context

A scene file could not express a nested struct, an enum carrying data, or `Option::None`. Found in
session 8 by probing the format directly rather than reasoning about it: a nested struct came out as
`{height: 8}` — Rust's `Debug` — which nothing parses, a payload enum did the same, and `None` wrote
a bare field name that the parser refused outright.

It had never been hit, because every component in the engine was scalars, flat lists and marker
types. Two things made it urgent:

- **It had already distorted a type.** ADR 0031 wanted `Projection::Orthographic { height }` and had
  to settle for a fieldless enum with `height`, `fov`, `near` and `far` as sibling fields on
  `Camera`, half of them meaningless at any moment. That is exactly the shape the Vault's own `Phase`
  comment argues against — "unrepresentable rather than merely unlikely".
- **M2's material model runs straight into it.** `Material { base_colour, metallic, texture }` under
  a mesh is the natural shape, and deciding a material against a format that cannot hold one would
  produce a second flattened type.

## The grammar already had the slot

A field with no inline value **already opens an indented block** — ADR 0014 uses it for lists:

```text
      waypoints
        - 0.0 0.0
        - 4.0 0.0
```

So nested structs needed no new punctuation, only a rule for what a block of `name value` lines
means. This is an extension to ADR 0014 rather than a supersession: it adds a case to an existing
production and changes nothing about what was already legal.

## What the research found

**YAML** distinguishes a block sequence from a block mapping by exactly one thing: whether the lines
start with `- `. That is the most widely used indentation-based format in existence, so the rule is
familiar and proven rather than invented, and — critically — **it needs no schema**, which matters
because layer 1 of this crate deliberately has none.

**Godot** does not nest at all: a compound value in `.tscn` becomes a separate top-level
`[sub_resource]` with an id, referenced as `SubResource("...")`. That is a real alternative and it is
the right *steer* for materials specifically — a material is shared, so an asset id fits it, and
ADR 0029 already built that machinery. It is not a general answer: an asset reference cannot express
an enum payload, and forcing a small non-shared value like a rect or an anchor into its own file
would be absurd.

## Decision

### 1. A block of `name value` lines is a struct; a block of `- ` lines is a list

Decided by the block's own first line. No schema is consulted.

```text
  Material
    base_colour
      a 1.0
      b 0.25
      g 0.5
      r 1.0
    metallic 0.0
```

Nesting goes as deep as the indentation does.

### 2. A struct and a map are the same syntax

They are structurally identical and semantically opposite (ADR 0027), and only a schema tells them
apart — which layer 1 has not got. So both write as `name value` lines and both parse back as a
`Struct`; the component's own `from_value` is what turns one into a map, and `Reflect for BTreeMap`
already accepted either. **Maps became scene-expressible as a side effect**, closing the gap ADR 0027
recorded.

### 3. A bare variant name with a block beneath it is an enum carrying data

```text
    projection Orthographic
      height 8.0
```

The **fieldless** case is untouched — `state Patrol` still reads exactly as ADR 0014 designed it,
because a variant with no payload never opens a block. Anything else with both a value and a block is
an error naming the one case where the shape is legitimate.

### 4. `Option::None` is deliberately still unwritable

Every spelling is worse than the gap. `none` collides with an enum variant of that name; a sigil
would be this format's first punctuation, having chosen indentation over punctuation throughout;
omitting the field entirely destroys the distinction ADR 0014 wanted between "explicitly nothing" and
"whoever wrote this forgot". Nothing in the engine has an `Option` field, so this waits for a real
case to argue from.

**Empty is also still unwritable**, for a mechanical reason: an empty block is a parse error, so an
empty struct, map or list *as a field value* has no spelling. `describe.example` reports which and
why rather than emitting something that will not load.

## Consequences

**Good:**

- **`Projection` became the honest type immediately.** `Orthographic { height }` and
  `Perspective { fov, near, far }`, each carrying only what it needs, with `Projection::height()`
  returning `None` for a perspective camera rather than a fallback number.
- The material model can now be decided on its merits rather than around a limitation.
- Purely additive: every scene file valid before is valid after, and the four in the repo needed no
  change beyond the camera.

**Bad, and accepted:**

- **Both replays regenerated**, because `Camera`'s shape changed. Verified by snapshot diff first:
  the only difference between the two worlds was the camera's four flat fields collapsing into the
  projection's payload, and nothing else moved.
- **A `.scene` and a `.snapshot` are now more similar than they are different**, and they remain two
  formats with two parsers. That was already true; this narrows the gap without closing it.

## What it turned up

Two defects, both found by using the thing rather than by reasoning:

**The derive silently dropped `min`, `max` and `unit` on enum variant fields.** A field lost its
range simply by being moved into a variant — which is precisely what this ADR encourages. Found the
moment `Camera`'s annotated `height` moved into `Projection::Orthographic`. The struct and variant
paths now share one function, so they cannot drift again.

**A snapshot could not write a payload enum.** It came out as `Orthographic({height: 8})`, a `Debug`
form nothing reads back — so a snapshot of any world containing one would capture and then refuse to
restore. This is the *second* time that exact defect has been found in `amadeo-snapshot` (the first
was maps, in session 8), and both times by snapshotting a real game and reading the file. It now has
a test that builds a world holding every awkward shape and asserts the restored state hash matches.

## What was rejected

- **Inline nested values** — `base_colour { r 1.0, g 0.5 }`. Compact and unambiguous, but introduces
  braces and commas into a format that has neither, and ADR 0014 chose indentation over punctuation
  deliberately.
- **Dotted paths** — `base_colour.r 1.0`. No grammar change at all beyond allowing `.` in a name.
  Rejected because it makes the file stop showing its structure, which is the *stated* reason the
  ADR 0014 spike ruled TOML out.
- **Compound values as assets only**, per Godot. The right steer for materials and insufficient in
  general; see above.
- **Doing nested structs and leaving enum payloads.** Smaller, and covers M2's material model. But
  they are one mechanism, so it would mean reopening the grammar a second time — and it would leave
  ADR 0031's camera flat forever.
