# ADR 0076 — A declared default is not a derived one, so a shape's dimensions get one

**Status:** Accepted · **Date:** 2026-08-18 · **Amends:** ADR 0075's illustrative example ·
**Builds on:** ADR 0069, ADR 0074

## Context

ADR 0075 let a field declare a default, opt-in per field, and used `BoxMesh::size` as its example of a
field that should stay **required**:

> `BoxMesh::size` has no sensible default and a zero-size box draws nothing while reporting no fault.

Session 21's Phase A review found the consequence, and it is not the one that sentence anticipated.
`describe BoxMesh --example` — which `docs/12-the-bar.md` §3 makes the primary way an agent learns to
author an asset — answers:

```
BoxMesh
  size 0.0 0.0 0.0
```

Because `--example` falls back to a range's **minimum** when a field declares no default, and
`BoxMesh::size` is bounded at zero. So the type used by 23 of 23 `.mesh` assets in this repository
hands an author a box that draws nothing, as advice on how to write one. That is exactly the defect
ADR 0075's own `--example` change was made to fix, in the type most likely to be met first.

**The reasoning in the quoted sentence conflates two decisions**, and the review named the split
precisely: *requiring `size` in a file* and *offering `0.0 0.0 0.0` as advice* are not the same
question.

## Decision

### 1. The argument against defaulting `size` was an argument against defaulting it to *zero*

ADR 0069's `default_value` derives a default from a **schema**, which for `[f32; 3]` is `[0, 0, 0]`.
ADR 0075 exists precisely because a derived default is untrustworthy — it is why a `.material`
omitting `base_colour` must not become transparent black. A **declared** default is a different
object: it is a value an author wrote down, and `[1.0, 1.0, 1.0]` is a unit cube, which draws, is
unmissable, and is what a `BoxMesh` with nothing else said should obviously be.

So `BoxMesh::size` and `PlaneMesh::size` declare defaults, and the illustrative sentence in ADR 0075
no longer describes the code. **ADR 0075's actual decision is unchanged** — defaults are opt-in per
field, a field without one still fails with `MissingField`, and canonical form still writes every
field. Only its example moves.

### 2. Every type an author writes in a file declares its defaults

Not just shapes. The review measured the same gap across the render crate: `components.rs` and
`environment.rs` declared **none**, so `describe --example` was handing an author a dead camera
(`active false`, a zero viewport), a black `Environment` (`exposure 0.0`, every effect at zero), and
three lights with `colour 0 0 0` and `intensity 0`. Every one of those types already had a
hand-written `Default` holding the right values; they were simply not in the schema.

The rule is therefore the plain one: **if a type is authored in a text file and has a sensible
`Default`, its fields declare that default.** A required field is the exception, and it needs a reason
that survives the question "what would `--example` show instead?"

### 3. A required field is still a real thing, and this is what one looks like now

`SimRng`'s generator words and an event queue's bookkeeping stay required, because there is no file
for a default to help and a wrong one would silently restore a different stream. That is the shape of
the remaining case: **data that is not authored**. A field a person or an agent types into a file
should almost always default.

## Consequences

- **`describe <T> --example` becomes usable for every authored type.** It was the primary discovery
  surface and it was handing back a black screen.
- **A `.mesh`, `.material`, `.environment` or `.scene` gets substantially shorter.** A `PointLight`
  is one line where it was four.
- **The drift hazard widens with the rule**, so it is answered the same way ADR 0075 answered it: one
  test per type asserting that a value built from an empty struct equals that type's `Default`. Nine
  more types, nine more assertions, all in one test per crate.
- **Nothing about loading an existing file changes**, and no state hash moves, for ADR 0075's reason:
  a default is applied while *building* a value, and every file in the repository still spells every
  field out.
- **`BoxMesh` with no `size` is now a unit cube rather than an error.** That is the one behaviour
  this ADR genuinely changes, and it is the trade being made deliberately: a file that meant to say a
  size and did not now draws a 1 m cube instead of refusing to load. A 1 m cube in the middle of a
  level is not a silent failure.
