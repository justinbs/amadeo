# ADR 0085 — A light has a radius, and the inverse square is clamped to it

**Status:** accepted (session 25)
**Extends:** ADR 0057's `PointLight`/`SpotLight`. Supersedes nothing.

## Context

`mesh.wgsl` fell off as a bare inverse square:

```wgsl
let falloff = 1.0 / max(distance * distance, 1e-4);
```

That is how light from a *point* falls off, and nothing physical is a point. The term is unbounded
as the distance goes to zero, so a surface close enough to a light saturates whatever the intensity
is, and the intensity that is right at range is wrong up close by the square of the ratio.

Engine gate review 23 measured it in `games/warren`. Its hand lamp is authored at intensity 26 over
a range of 18 m, which is correct down a twelve-metre bore. One mouse-flick from the spawn the
player faces a lining wall 1.5 m away, and:

| distance | windowed falloff | radiance | outgoing at 0.75 albedo |
|---|---|---|---|
| 1.5 m | 0.444 | 11.6 | **≈ 8.7** |
| 3 m | 0.111 | 2.9 | ≈ 2.2 |
| 5 m | 0.040 | 1.04 | 0.78 |

ACES saturates an order of magnitude below 8.7. The measured result was **118,040 clipped pixels,
5.69% of the frame**, with a 430 px run of row 700 reading 248–255 continuously. Review 15 failed
this whole gate item over 27,659 clipped pixels; this was four times that, one input away from the
opening shot.

**No single intensity fixes it.** The ratio between the near wall and the far bulkhead is over 60×,
and the same number has to serve both.

## Decision

**A light carries a `source_radius`, and the distance is clamped to it before squaring.**

```wgsl
let reach = max(distance, light.shadow.z);
let falloff = 1.0 / max(reach * reach, 1e-4);
```

This is the sphere-light form Karis specifies in *Real Shading in Unreal Engine 4*, which Unreal
exposes as `SourceRadius` and Frostbite's course notes give identically. Inside the source the
irradiance stops climbing, which is what a real bulb-and-reflector does.

Three things about the shape of it:

**It defaults to zero, and `max(d, 0)` is `d`.** Every light that does not author one shades exactly
as it did before, so no existing capture in any game moves by a pixel. That is the whole reason it is
an authored field rather than a constant in the shader — a constant would have been cheaper and would
have silently changed `games/atrium`, `games/scarp` and `games/vault` at the same time.

**It rides in `shadow.z`.** The `PunctualLight` uniform's fourth vec4 carried a layer index and a
depth bias and had two unused lanes, so this costs no change to the uniform layout, no change to the
bind group, and no second buffer. The comment in `view.wgsl` and the packing in `gpu.rs` are the only
two places that know.

**It is on both kinds.** A point light has the same defect — the Warren's hand-lamp spill was blowing
the same wall — and `PointLight` and `SpotLight` are two components over one shader path (ADR 0057),
so a field on one and not the other would be a difference the shader would have to branch on.

## Consequences

The Warren's yaw-270 frame goes from **118,040 clipped pixels to 0** at `source_radius 2.5` with the
beam retuned from 26 to 14, and five of its six standard aims report zero.

**The radius and the intensity move together and neither is meaningful alone.** The peak irradiance a
light can deliver is `intensity / radius²`, so raising the radius without lowering the intensity does
nothing at any distance already outside it — the first attempt authored 1.2 m against a wall 1.5 m
away and the capture came back byte-identical, which looked exactly like the field not being read.
**When tuning this, check that the radius is larger than the distance to the nearest thing the light
is aimed at, or it cannot be doing anything.**

It is not a physically complete sphere light: the specular lobe should also widen with the source's
solid angle (Karis gives a representative-point form for that) and this does not, so a large radius
keeps a point-sized highlight. That is a visible approximation only at radii far larger than anything
a hand lamp wants, and closing it is a change to the BRDF rather than to the falloff.

It does not help a light *inside* geometry, which is a placement problem rather than a falloff one.
