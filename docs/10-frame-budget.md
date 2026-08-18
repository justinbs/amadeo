# 10 — The frame budget, and the numbers behind it

**M2 exit gate 4**: *frame time within a declared budget at a declared scene complexity. Numbers
written down.*

This is the numbers. Regenerate them with:

```bash
cargo test -p atrium --release --test frame_budget -- --nocapture --test-threads=1
```

Everything here is **measured, not estimated**, and the test that produced it is committed. If a
number in this file disagrees with what that command prints, the command is right and this file is
stale.

---

## The budget

**16.67 ms per frame, for everything.** That is one frame at 60 Hz, and it is the whole budget —
simulation, frame preparation, GPU execution and presentation all come out of it.

The engine does not divide it into per-subsystem allowances, deliberately. A fixed split would be
wrong for most games: a physics sandbox and a text adventure want completely different shares of the
same 16.67 ms. What is useful is knowing what each part *actually costs*, which is what the rest of
this file is.

**Simulation runs on one core and always will.** ADR 0036 put `enhanced-determinism` on permanently,
which is mutually exclusive with rapier's `parallel` and `simd-*` features. So every physics number
below is single-threaded by design, and if physics ever becomes the limit the answers are fewer
bodies, better culling, or sleeping inactive bodies — **not** relaxing that.

## The machine

| | |
|---|---|
| CPU | AMD Ryzen 7 5700X3D (8C/16T) |
| GPU | NVIDIA RTX 4060 Ti |
| OS | Windows 11 Pro 26200 |
| Rust | 1.97.1, `--release` unless stated |

A different machine will produce different numbers. What should survive is the *shape*: which system
is expensive, and how the cost grows.

## The scene

`games/atrium` — M2's demo room. Its complexity is **asserted** by
`the_scene_complexity_the_budget_is_quoted_against`, because a budget quoted against a scene nobody
checked is a number about an unknown scene.

| | |
|---|---|
| Drawn meshes | 11 at the time of measurement; **16 since session 21** |
| Bodies with a collider | 11 (10 static, 1 kinematic character) |
| Characters | 1 |
| Shadow-casting lights | 1, orthogonal, 2048² map |
| Render target | 1280 × 720, HDR scene target plus a post pass |

**The scene has grown since these numbers were taken and they have not been re-measured.** Session
21 added a table, a bolted generator and a lamp fitting to `games/atrium` — the first non-box meshes
in any game here (ADR 0074 §2). They add **three** drawables, not fifteen: a `CompoundMesh` is one
mesh, one asset and one draw call however many parts it has, which is the argument for it over the
prefab-children arrangement the games used before. Re-measuring is item 19 of
`docs/13-the-engine-gate.md`, which is about widening this evidence base rather than nudging it.

---

## Simulation: 8.3 µs per tick

600 ticks, after 120 discarded as warm-up.

| System | Mean | Worst | Share of a frame |
|---|---:|---:|---:|
| `step_physics` | 4.36 µs | 102.8 µs | 0.03% |
| `drive_characters` | 2.35 µs | 13.0 µs | 0.01% |
| `propagate_transforms` | 1.39 µs | 16.8 µs | 0.01% |
| `sample_input` | 0.22 µs | 4.4 µs | 0.00% |
| **Total per tick** | **8.31 µs** | | **0.05%** |

**Read the worst column, not just the mean.** `step_physics` averages 4.4 µs and its slowest single
run was 102.8 µs — 23× its own average. That is the shape of a solver doing real work on the tick
something changes, and it is still 0.6% of a frame, so it is a fact rather than a problem. But an
average alone would have hidden it entirely, which is why `SystemTiming` keeps both.

**Debug builds cost about 7× more**: 56.8 µs per tick, dominated by `step_physics` at 30.0 µs. Worth
knowing before anyone profiles a debug build and panics.

## Frame preparation on the CPU: 126 µs

Walking the world into a `FrameData`, compiling the render graph, uploading what changed, and
recording command buffers, at 1280 × 720 with the shadow pass active.

| | Mean | Share of a frame |
|---|---:|---:|
| Release | 125.5 µs | 0.75% |
| Debug | 261.8 µs | 1.57% |

**⚠️ GPU execution time is not measured.** This is the CPU's half of the frame — the engine's own
work up to submitting commands. How long the GPU then takes to run them needs **timestamp queries**,
which the wgpu backend does not have. That is a real gap in this gate's answer and it is stated
rather than papered over: on a scene this small the GPU is almost certainly idle, but "almost
certainly" is not a measurement.

## How simulation scales with body count

The Atrium is eleven bodies, which is a room rather than a stress test. This is the same room plus N
falling dynamic boxes, spread over a grid rather than stacked in a column — a column settles into one
contact island and measures the sleeping path, which is the easy case.

| Bodies | Mean per tick | Share of a frame |
|---:|---:|---:|
| 11 | 7.8 µs | 0.05% |
| 61 | 117.1 µs | 0.70% |
| **211** | **450.2 µs** | **2.70%** |
| 811 | 1914.3 µs | 11.49% |

**211 bodies is the row that matters**, because M2's exit gate 3 asks for a physics-heavy replay of
200+ bodies. It costs 2.7% of a frame.

Growth is roughly linear and slightly worse: 13.3× the bodies from 61 to 811 costs 16.3× the time,
which is what contact-heavy physics looks like. The test asserts only that the ratio stays within 4×
of linear — enough to catch a changed complexity class, not tight enough to be flaky.

**Extrapolating, single-threaded**: the simulation would reach a whole 60 Hz frame somewhere around
5,000–7,000 active dynamic bodies. Nothing in the target-game list needs that from *physics*;
Minecraft-scale voxel work is a different subsystem with a different budget, and it is M7's problem.

---

## What is asserted, and what is only reported

The split this project settled on in `crates/amadeo-render/tests/sprite_throughput.rs`, applied
again here:

- **Scene complexity is asserted.** Counts are a pure function of the world with no clock involved,
  so a change in them is a real change.
- **Times are printed, with one deliberately enormous ceiling** at half a frame. It catches an
  algorithmic collapse and nothing subtler.
- **The scaling ratio is asserted, loosely.** Four measurements an order of magnitude apart can tell
  a slow constant from a bad complexity class, where one measurement cannot.

Nothing here fails CI on a timing regression, and that is on purpose. `CLAUDE.md` §6 forbids tests
that depend on wall-clock; CI runners are shared and variable; and **a flaky performance gate is one
people learn to ignore, which is worse than not having one.** `docs/04-subsystems.md` §18 wants
"declared frame budgets per system, so a regression is an automatic failure" — that is the right
ambition and it needs dedicated hardware and a baseline history, which this project does not have.
Until it does, the honest version is a committed measurement anyone can re-run.

## Asking the engine instead

`profile.frame` reports the same per-system numbers over the agent protocol (ADR 0040), so an agent
can find a performance problem it cannot feel:

```bash
amadeo call profile.frame --package atrium --ticks 600
```

---

# M2.5 exit gate 4 — an open world, with GPU time this time

**The thing M2's gate could not answer.** Item 1 below said GPU execution time "needs timestamp
queries in the wgpu backend", and now it has them.

```bash
cargo test -p scarp --release --test frame_budget -- --nocapture
```

## The scene

`games/scarp` — a streamed terrain world. Nothing in it is authored but the player, the camera and
the sun; the ground is generated and streamed in chunks. Measured after the streamer has gone quiet,
so the numbers are about a settled world rather than a half-built one.

| | |
|---|---|
| Meshes in the world | 50 |
| Meshes in view | 20 |
| Meshes submitted | **20** — the other 30 are frustum-culled (gate 3) |
| Target | 640×360 offscreen |

## GPU time: 61 µs per frame

| Pass | GPU time |
|---|---:|
| `shadow 0` | 9.2 µs |
| `view 0` | 24.6 µs |
| `post` | 4.1 µs |
| `present` | 4.1 µs |
| **Total** | **61.4 µs** |

Against a 16.67 ms budget, that is **0.4%**.

**The passes sum to 42 µs and the total is 61 µs**, and the gap is not an error — the total is wall
time on the GPU from the first pass beginning to the last one ending, so it includes the gaps
*between* passes. A frame that is mostly gap is a frame whose bottleneck is elsewhere, which is
exactly what this one is: the GPU spends more time waiting to be given the next pass than drawing.

> **The shadow pass was invisible in the first version of this measurement.** It takes a different
> code path — it is the one pass with no colour attachment — and so it was never given timestamps.
> The breakdown summed to 27 µs of a 37 µs frame with nothing accounting for the difference. A
> profiler that silently loses a pass is worse than one that reports only a total.

## What shadow cascades cost — measured in session 15

Cascades (ADR 0055) draw the casters **four times** instead of once and add a selection to the mesh
shader's fragment stage. Both halves are visible, and the same scene measured both ways is the only
honest way to say how much:

| Pass | `Orthogonal` | `Cascaded { blend 0.5 }` |
|---|---:|---:|
| `shadow 0.0` | 7.2 µs | 8.2 µs |
| `shadow 0.1` | — | 5.1 µs |
| `shadow 0.2` | — | 5.1 µs |
| `shadow 0.3` | — | 5.1 µs |
| `view 0` | 37.9 µs | 45.1 µs |
| `post` | 3.9 µs | 3.9 µs |
| `present` | 3.1 µs | 4.1 µs |
| **Total** | **71.7 µs** | **113.7 µs** |

**About 1.6× the GPU frame, and still 0.7% of a 60 Hz budget.** One sample each, on the machine above,
at 640×360 — the numbers are quantised by the timestamp period and move by a microsecond or two
between runs, so treat the shape rather than the digits as the result.

Two things worth reading out of it. The three extra shadow passes cost **less** than the first one
each, because they draw the same casters into smaller boxes and more of the geometry falls outside.
And `view 0` grew by 7 µs, which is the fragment stage's cascade selection — a loop over at most four
comparisons per pixel, and the price of the whole feature at the shading end.

**What is bought for that**: the near cascade covers about a seventh of the distance at the same
resolution, so a shadow-map texel near the camera is about **1 cm of ground rather than 7 cm**.
`cascades_make_the_near_shadow_map_far_finer_than_a_single_one` pins that ratio.

The measurement is also the first time the per-pass breakdown has earned its keep as a *design*
check rather than a budget one: four separately-labelled shadow passes is what says the layers are
being drawn independently, and a single pass reporting four times the cost would have meant the
loop was in the wrong place.

## What these numbers are worth

The target is small and the world is one material with no textures beyond a single ground image, so
**this is a measurement of draw submission rather than of shading**. A real scene moves the cost into
`view 0` by orders of magnitude, and the passes that would grow — more lights, more shadow cascades,
bloom's blur chain — are exactly the ones ADR 0045 puts on M3's list.

What is worth keeping is the **shape**: with culling on and a modest scene, the GPU is nowhere near
the bottleneck, and the frame is dominated by per-pass overhead rather than by pixels.

## Measuring is not free, and is off by default

Reading the timestamps back means waiting for the GPU to finish — a full pipeline stall, which is
what a real frame loop exists to avoid. `WgpuBackend::set_gpu_timing(true)` is a thing a measurement
harness does and nothing else. `supports_gpu_timing()` is `false` on an adapter without
`TIMESTAMP_QUERY`, and the engine renders identically there and reports nothing, because ADR 0002's
argument for wgpu is that the engine runs broadly.

---

## What M2's gate did not answer

1. ✅ **GPU execution time** — answered above by M2.5's gate 4.
2. **A scene with real art in it.** Eleven boxes is not a level; a real one has thousands of
   triangles per mesh, textures, and enough draw calls to make batching matter. glTF import landed
   in the same session as this measurement and nothing has been through Blender yet.
3. **Sustained frame time in a window**, with a swapchain, vsync and a compositor. Everything here is
   headless or offscreen. `advance_real_time` caps at 8 ticks per frame precisely because real
   frames stall, and that path is untested against a stall.
4. **Memory.** Not measured at all, and `docs/04` §18 wants it.
