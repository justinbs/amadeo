# ADR 0052 — Meshes are drawn from both sides

**Status:** Accepted · **Date:** 2026-08-11 · **Builds on:** ADR 0035, ADR 0042, ADR 0049

## Context

Justin reported seeing the sky while underground. It survived two wrong diagnoses before a screenshot
settled it, and the mechanism is worth stating precisely because none of the three suspects was
obviously wrong.

**What it actually is.** Terrain from surface nets (ADR 0042) is an open **surface** — the boundary
between rock and air — and it has no underside. The mesh pipeline culled back faces, so from beneath,
that surface was not drawn at all. A camera under the ground therefore looked straight through the
world, and what filled the gap was the sky pass (ADR 0049): blue above the horizon, the environment
map's dark ground hemisphere below.

**Why it read as something else.** "The ground has vanished and I can see sky" is exactly what a
terrain-streaming failure looks like, and streaming is the complicated part. The first diagnosis
blamed the dig radius — which *had* doubled and *was* a real bug, just not this one. The second blamed
the camera going underground, which is true and was worth fixing (**Q27**) but is a different
statement: the camera being somewhere it should not be, versus the world being invisible from there.

## Decision

**The mesh pipeline culls nothing, and the shader flips the normal for back faces.**

```wgsl
@builtin(front_facing) front_facing: bool
let facing = select(-1.0, 1.0, front_facing);
```

Without the flip, the underside of the ground would be lit as though it faced the sky — which under
image-based lighting means it picks up sky irradiance and glows. With it, a back face samples the
environment's *lower* hemisphere and reads as the dark underside of the world, which is what being
underground should look like.

### Why this is close to free, rather than a trade

**For a closed mesh it changes nothing at all.** A box's back faces are always behind its front faces,
so the depth test rejects them and the picture is identical. The cost is rasterising fragments that
early-Z then discards — no extra shading, no extra draw calls, no extra state.

That was checked rather than assumed: `games/scarp`'s capture is **byte-identical** before and after.
The only views that change are the ones that were wrong.

### Why not per-material

A `two_sided` flag on `Material` is the grown-up version and is deliberately deferred. It would be a
field on every `.material` file (**Q32**, five occurrences and counting) to buy back overdraw that
`docs/10`'s frame budget says is nowhere near mattering — the Scarp costs 61 µs of GPU time against a
16.67 ms budget. The moment that stops being true, the flag is an additive change.

### What this costs conceptually, and it is real

Backface culling was **load-bearing for a test**. ADR 0035's tessellation tests assert winding, and
their value came partly from a wrongly-wound face becoming *invisible* rather than merely mis-lit —
which is how session 13's inside-out mesher was eventually caught by looking at a picture.

That signal is now weaker: a wrongly-wound face still draws, just with its normal flipped. The
compensation already exists and is stronger than the old one —
`triangles_are_wound_to_match_their_own_normals` and `every_box_triangle_faces_outward` check winding
against the normal directly, on the CPU, with no GPU and no eyes. Those are what the winding claim
rests on now, and they were always the better test.

## Consequences

**Good.**

- Being underground looks like being underground. The class of bug — *any* geometry seen from inside
  becoming transparent — is closed rather than patched for terrain specifically.
- One line and one shader change, no schema change, no new asset field, nothing to author.
- `geometry_is_visible_from_the_inside_rather_than_transparent` pins it, and was watched failing:
  with culling restored, the centre of the capture is `[69, 75, 85]`, exactly the clear colour.

**Bad, and accepted.**

- Overdraw on closed geometry, bounded by early-Z and measured as irrelevant at present scale.
- The winding-error signal is weaker on screen, as above.
- **A camera fully inside solid rock still sees very little**, because solid rock genuinely contains
  no geometry — only its boundary does. This makes that case *dark* instead of *sky*, which is a much
  better failure, but the real answer is keeping the camera out of solid ground (Q27, and its pivot
  is the part still unhandled).

**Explicitly not decided here.** Whether transparency, foliage cards or billboards later need
single-sided rendering back as an opt-in; and anything about the shadow pass, which keeps its own
front-face culling because that is an acne-avoidance trick rather than a visibility one.
