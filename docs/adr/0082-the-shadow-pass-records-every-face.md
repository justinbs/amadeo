# ADR 0082 — The shadow pass records every face

**Status:** Accepted · **Date:** 2026-08-19 · **Supersedes part of:** ADR 0038 ·
**Implements:** item 12h of `docs/13-the-engine-gate.md`

## Context

A bright hairline ran along every **concave** junction in `games/atrium` — wall to ceiling, wall to
wall, floor to wall, and the top edge of a roof beam. At the wall-to-ceiling junction it measured
**143** against a 76 wall and an 81 ceiling.

**Four explanations were tried and all four were wrong.** This is worth recording in full, because
each was a standard technique applied for a plausible reason, and the loop only ended when the
experiments were allowed to falsify the idea rather than to tune it.

1. **Coplanar depth-fighting.** ADR 0052 turned backface culling off, and the wall tops shared a plane
   with the roof underside. Dropping the roof so the walls penetrate it changed nothing pixel for
   pixel; widening the roof to overhang them changed nothing. And a shadow *bias* cannot move a
   depth-fight between two surfaces, yet halving it halved the band.
2. **Normal-offset bias** (ADR 0081). Moved the peak 115 → 109 and widened the band slightly.
3. **Receiver-plane depth bias.** Written, compiled through FXC, correct — and across three A/B
   captures it changed **not one pixel**. See Consequences.
4. **A smaller constant bias.** Reducing `shadow_bias` twelvefold, 0.006 → 0.0005, moved the peak by
   **one level**. That is the observation that ends the bias hypothesis outright: nothing was wrong
   with the depths.

What was actually true was visible in two facts that had been measured early and not followed. The
band's **width scaled with shadow-map texel size** — three pixels at 2048, two at 4096 — while its
peak held. And with **shadows off, the whole wall read the band's exact value**, so the sun reaches
all of it and the shadow map was failing to record something.

## Decision

### The shadow pass culls nothing

`cull_mode` on the shadow pipeline goes from `Some(Face::Front)` to `None`.

Front-face culling is the cheap classic acne fix: record the far side of every object, so the stored
depth sits behind the surface being lit and no surface can shadow itself. The comment in `gpu.rs` said
it "costs correctness only for geometry with no thickness".

**That was wrong.** It also breaks whenever the surface being lit is the object's *front* face — and
that is the definition of a room. You stand inside a box; the light falls on the faces pointing at
you. Those faces were culled out of the shadow map, so the nearest recorded depth at their texels was
the wall's **outer** face half a metre further on, and every fragment of the inner face tested as
nearer than its own occluder. The roof hid most of it, being nearer the light and winning the depth
test — but along the junction the 3×3 filter straddles texels where it does not, and those taps came
back lit.

That is why the artefact appeared at **concave** junctions specifically, why its width tracked texel
size, and why no depth bias in any form could touch it.

## Consequences

- **Measured in the game: junction peak 74 against a 30 wall becomes 35.** A delta of 44 becomes 5.
- **No acne was traded for it**, which was the whole reason the culling was there. A 51-pixel run
  across the sunlit floor has the same mean (155.0) and the same standard deviation (11.54) before and
  after, to two decimal places, and the gallery's lit face is unchanged too.
- **The cost is that the shadow pass rasterises both faces of every occluder.** Real, and not measured
  here; the shadow maps are 2048² and this room is 27 drawables, so it is not the constraint. If it
  ever becomes one, the correct fix is per-object rather than global — a closed solid can be
  front-culled safely, and only surfaces you can stand inside cannot.
- **The receiver-plane depth bias was removed again rather than shipped.** It is correct, it compiles
  through FXC, and once the real cause was fixed three A/B captures showed it changing nothing. Four
  derivative instructions and a 2×2 solve per fragment for a measured benefit of zero is precisely the
  "written, tested, exercised by nothing" pattern the engine gate exists to close, one level in. It is
  the right technique for a steep receiver under a low sun; bring it back when a scene measures a need.
- **ADR 0081's normal-offset stays**, and its measured benefit turns out to have been a partial
  masking of this. It is three lines, it is standard, and it does no harm — but it should not be
  credited with more than it does.
- **There is no regression test, and that is a gap stated rather than papered over.** Four synthetic
  scenes were built to reproduce the artefact — a lit wall under an overhang, then with the overhang
  long enough to shadow the whole wall, then sampling from the top of the frame rather than below the
  junction, then with the wall penetrating the slab as the Atrium's does. **All four passed with the
  defect deliberately restored**, so all four were removed. A green test named "a shadowed wall has no
  bright seam" that cannot fail is worse than no test: it is a false assurance in exactly the place
  somebody would later trust one. The evidence for this fix is the measurement in the real room, and
  the reproduction condition is still not understood well enough to synthesise.
