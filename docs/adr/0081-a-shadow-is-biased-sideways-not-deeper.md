# ADR 0081 — A shadow is biased sideways, not deeper

**Status:** Accepted · **Date:** 2026-08-19 · **Builds on:** ADR 0038, ADR 0055 ·
**Implements:** item 12h of `docs/13-the-engine-gate.md` (partly — see Consequences)

## Context

A bright hairline ran along every **concave** junction in `games/atrium` — wall to ceiling, wall to
wall, and the underside of a roof beam. Measured at the wall-to-ceiling junction: **143** against a
76 wall and an 81 ceiling, over five pixels.

The engine gate's first diagnosis was depth precision from coplanar geometry, since ADR 0052 turned
backface culling off and the wall tops shared a plane with the roof underside. That was wrong, and
three experiments said so: moving the roof so the walls penetrate it changed nothing pixel for pixel;
widening the roof to overhang them changed nothing; and **a shadow bias cannot move a depth-fight
between two surfaces**, but halving it halved the band. Turning the sun off removed the band
entirely; turning shadows off made the *whole wall* read 143, the band's exact value.

So the wall is meant to be wholly shadowed by the roof, and the shadow map was leaking along its top.
The "hole" in the line, offered as evidence of z-fighting, is a pillar's shadow — the one place the
leak is covered by a second occluder.

## Decision

### Offset the shadow lookup along the surface normal, not along depth

Before projecting a fragment into light space, walk it out along its own **geometric** normal by
roughly one shadow-map texel:

```wgsl
let texel_world = 2.0 * view.cascade_far[cascade] * view.shadow_params.x;
let along_normal = texel_world * (0.6 + 1.4 * clamp(1.0 - lambert, 0.0, 1.0));
let sample_at = world + normal * along_normal;
```

A depth bias asks "how much deeper than the recorded occluder may this fragment be", and **one number
cannot answer that for a floor facing the light and a wall backing away from it at once**. Too little
is acne; too much detaches the shadow at the contact, which is the hairline. Moving *sideways*
sidesteps the trade: one texel covers a real distance in the world, a fragment within that distance of
a corner is genuinely ambiguous, and shifting the lookup out of the corner resolves it without ever
claiming the fragment is nearer the light than it is.

The **geometric** normal rather than the mapped one, because this is about where the fragment sits on
real geometry relative to the shadow map's texels, and a normal map describes bumps that are not in
the depth buffer at all.

## Consequences

- **It helps, and the amount is measured rather than asserted.** A/B in the same room, same frame:
  the wall-to-ceiling junction peak falls **115 → 109** and the wall-to-wall corner **111 → 106**,
  with no acne appearing on the floor or on any wall flat (both checked, at two points each).
- **It does not close item 12h, and the reason is worth more than the fix.** What remains is a
  **one-texel contact row**, not a bias artifact, and the experiment that shows it is doubling
  `shadow_resolution` to 4096: the band narrows from three pixels to two — **its width scales with
  texel size** — while the peak stays at ~114. That is the signature of the topmost row of wall
  fragments being genuinely within one texel of the occluder, where some of the 3×3 PCF taps land
  beyond the roof and return "lit". At the peak the fragment reads about 40% of the way from shadowed
  to lit, which is two or three taps of nine.
- **So the remaining fix is a kernel problem, not a bias problem.** Contact-hardening — narrowing the
  filter as the occluder distance falls — is the standard answer, and it is a different piece of work
  from this one. Recorded so the next attempt does not start by tuning a bias again, which is where
  this one started and where it would have stayed.
- **Trading depth bias for more normal offset was tried and rejected.** Raising the offset to
  `0.8 + 3.0·slope` while cutting the slope-scaled depth bias from `×3` to `×0.5` measured *worse* at
  the junction (112 against 109) and no better at the corner. The two are not interchangeable here.
- **Every capture test passes unchanged**, all 50 through FXC. The offset is smaller than a texel, so
  it moves no shadow edge far enough to change a pixel any existing test asserts on.
- **The spot-light path (`spot_shadow_factor`) does not have this yet.** It is the same fix and the
  same argument; it is left for when a spot-lit contact is measured rather than assumed, because the
  cascaded path is where the artifact was actually seen.
