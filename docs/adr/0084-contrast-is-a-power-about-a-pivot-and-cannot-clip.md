# ADR 0084 — Contrast is a power about a pivot, and a grade may not clip

**Status:** accepted (session 23)
**Supersedes:** the contrast term of ADR 0034's `Grade`, and nothing else in it.

## Context

`Environment::grade.contrast` was applied in `post.wgsl` as a straight line through mid-grey:

```wgsl
color = (color - vec3<f32>(0.5)) * post.grade.x + vec3<f32>(0.5);
```

followed, two lines later, by a `clamp` to `0..1`.

A straight line with a slope greater than 1 crosses zero **inside the visible range**. Solving
`(x − 0.5)·c + 0.5 = 0` gives `x = 0.5 − 0.5/c`, and on an sRGB target:

| authored `contrast` | crossing, linear | crossing, sRGB byte | fraction of the range destroyed |
|---|---|---|---|
| 1.05 | 0.0238 | **44** | 17% |
| 1.10 | 0.0455 | **60** | 24% |
| 1.15 | 0.0652 | **72** | 28% |

Everything below that byte was clamped to pure black. `games/warren` authored 1.05;
`games/vault`'s `corridor_dark.environment` authors 1.15.

**Nothing reported it, and the symptom is indistinguishable from a lighting bug.** Engine gate
reviews 15, 16 and 17 each found it as a *different object*: a 20 cm skirting kerb rendering as a
hard black band across the full width of every frame; an emergency light fitting rendering as a
silhouette beside its own tube; a sign's steel surround rendering as a hole. Each looked like a
material, or a normal, or a missing ambient. Two sessions were spent on the objects. Review 17
derived the crossing from the shader and measured it: **42.5% of a frame at exactly `RGB(0,0,0)` at
`contrast 1.05`, and 0.0% at `contrast 1.00`, with a minimum of 17.**

## Decision

**Contrast is a power about the same pivot**, not a line through it:

```wgsl
let pivot = 0.5;
color = pivot * pow(max(color, vec3<f32>(0.0)) / pivot, vec3<f32>(post.grade.x));
```

And the general rule this is one instance of: **a grade operator may not map an in-range input to an
out-of-range output.** Anything that can is a defect that will be blamed on the scene.

## Consequences

- **An authored number keeps its meaning.** The derivative of `p·(x/p)^c` at `x = p` is `c`, which is
  exactly the slope the old line had. A look authored against the old operator reads the same through
  the midtones and stops destroying its shadows.
- **`contrast = 1.0` is the exact identity**, so a look that does not grade is byte-for-byte what the
  renderer drew — the property ADR 0034 requires of the whole post chain and the reason the default
  `Environment` is a no-op.
- **It cannot reach zero from a non-zero input.** `pow(x, c)` is positive for every positive `x`, so
  a surface that received *any* light keeps some.
- **`max` before the power** because `pow` of a negative is undefined in WGSL, and a tonemap can hand
  the grade a small negative from a wide-gamut input.
- **Highlights can exceed 1 and are clamped**, as before: at pivot 0.5 and `c = 1.05`, white maps to
  1.035. That is the same behaviour the line had and is what the clamp is for.
- **Two games change appearance**, both for the better and both measured: `games/warren`'s pitch-down
  frame goes from 42.5% pure black to 0.0%, and `games/vault`'s corridor recovers a minimum of 20
  against a previous 0. `games/atrium` and `games/scarp` author `contrast 1.0` and are byte-identical.

## Alternatives considered

- **Leave it and tell authors not to exceed 1.0.** Rejected: a knob whose documented range is "1.0"
  is not a knob, and the failure is silent — nothing in `check`, `fmt` or the test suite can see it.
- **Pivot lower, at 0.18.** Moves the crossing rather than removing it, and 0.18 is a *scene-linear*
  mid-grey while this operates after the tonemap, where 0.5 is the right pivot.
- **Clamp the input away from the crossing.** Preserves the shape and produces a flat floor instead
  of black — a different artefact rather than a fix.
- **An S-curve (`smoothstep`) blend.** Preserves 0 and 1 and does not clip, but it has no slope
  parameter, so an authored `contrast` would stop meaning anything and every existing look would have
  to be re-tuned by eye.
