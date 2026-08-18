# 13 — The engine gate

> **This is the binding plan for gate 2 of `docs/12-the-bar.md` §4** — "improve and change the engine
> to the bar". It exists as a file because the first engine review's output was a fourteen-item ordered
> plan that lived only in a conversation, and the second review could not read it. `docs/12`'s own
> opening paragraph says why that matters: *"a bar that lives in one conversation is a bar that quietly
> slips."*
>
> **Update this file in place as items land.** The verdict column is not decoration — `docs/12` §4 says
> nothing proceeds to the next part until the critic passes the current one, and an item is closed by
> its stated condition rather than by an impression that it is done.

---

## 1. Status at a glance

**Gate verdict: NOT POLISHED.** **Phase A is closed and passed** (review 4, session 21, at `9e10f03`).
Phase B is in progress: items 8 and 9 have landed and **the first tracked measurement has moved for
the first time** — `games/atrium` now holds a table, a bolted generator and a lamp fitting, which are
the first meshes in any game here that are not axis-aligned boxes.

The three measurements that define the problem, tracked across reviews so drift is visible:

| Measurement | Review 1 (s20) | Review 2 (s21) | Now | Target |
|---|---|---|---|---|
| `.mesh` assets that are `BoxMesh` | 23 / 23 | 23 / 23 | **23 / 26** | a game whose meshes are not all boxes |
| Material texture slots that are `""` | 36 / 36 | 36 / 36 | 36 / 36 | a game whose materials sample textures |
| Mutating agent-protocol methods | 0 / 17 | 0 / 17 | 0 / 17 | deferred to M4 — see item 19 |

**The finding both reviews reached, stated once:** the engine's *foundations* are above the bar — the
determinism spine, ADR 0041's reproducible parallelism, the `describe` diagnostic family, ADR 0069's
save versioning, terrain streaming, and a cascaded shadow system that looks correct on a screen are
several of them better than what commercial indie engines ship. Its *output* is nowhere near the bar.
Every item below exists to close that gap, and none of it is renderer work.

---

## 2. The plan

Ordered for execution rather than by severity: an item's phase is decided by what it unblocks. The
critic's own ranking is preserved in the **R#** column so its report stays traceable.

### Phase A — make what already exists honest and usable

Cheap, and everything after it is worth more once these are done.

| # | R# | Item | Closed when | Status |
|---|---|---|---|---|
| 1 | 1 | **This file.** The gate's plan lives in the repository | `docs/13` exists, is linked from `CLAUDE.md` §8 and `docs/05`, and carries every item with a falsifiable condition | ✅ **done** (s21) |
| 2 | 4 | **Register every mesh shape the engine ships.** `amadeo check` rejected a cylinder and `describe CylinderMesh` said the type did not exist, in every game | `amadeo describe CylinderMesh --example` answers in all four games, and `amadeo check` accepts a `.mesh` holding each shape the engine ships. No game lists shapes by hand | ✅ **done** (s21) |
| 3 | 19 | **Two small defects in `solid.rs`.** A dead `half` binding silenced with `let _ = half;`, and `WedgeMesh::height_back` defaulting to `0.0`, which emits degenerate faces on every default use | The dead binding is gone rather than silenced, and a default `WedgeMesh` has no zero-area triangle | ✅ **done** (s21) |
| 3b | — | **Three things item 2 turned up on the way**, none of them in the review. Fields had no declared defaults, so `describe --example` answered `BoxMesh size 0.0 0.0 0.0` — the type 23 of 23 assets use — plus a dead `Camera`, a black `Environment` and three lights at zero intensity. `--example` preferred a range's *minimum* over a declared default. And `Value::F32` was widened to `f64` before formatting, so `0.18` was written `0.18000000715255737` | **Every type authored in a text file** declares defaults agreeing with its `Default` impl (ADR 0076), `--example` prefers a declared default, and an `f32` prints at `f32` precision **in both the scene and the JSON spellings** | ✅ **done** (s21) |
| 4 | 5 | **Faceting.** Every ADR 0074 primitive was smooth-shaded, so a six-sided cylinder shaded as a smooth tube and a 12×6 sphere as a smooth ball with a polygonal outline. `docs/12` §3 makes low poly first-class and the set could not produce it | Each curved primitive takes a `flat` flag, and a smooth `SphereMesh` shares its vertex grid rather than paying the flat cost without the faceting | ✅ **done** (s21) |
| 5 | 16 | **A committed capture test per primitive.** `ArchMesh` had `an_arch_draws_as_a_vault_rather_than_a_box`; the four new shapes had CPU geometry tests only, and the commit message's "looked at the picture" was not in the repository | Every shape the engine ships has a GPU capture assertion that would fail if it drew as a box. **Each renders a `BoxMesh` of the same bounds as a control inside the same test**, so the discrimination is structural rather than something somebody ran once — copy this for every shape assertion in Phase B | ✅ **done** (s21) |
| 6 | 14 | **The skeletal-animation citation was false in three documents.** `docs/12:72`, `STATUS.md:293` and `docs/11:157` all cited `docs/06` as recording it blocked on a rigged model. `docs/06` had zero mentions of it; the claim is in `docs/04` §14 and ADR 0066 §5 | All three cite a source that contains the claim, and skeletal animation is a real numbered question in `docs/06` | ✅ **done** (s21) — now **Q41** |
| 7 | 11 | **The roadmap contradicted the bar and did not cite it.** `docs/05` deferred `mod-tilemap` and `mod-pathfinding` to M7; `docs/12` §2 and §5 make both required. The roadmap also recorded no gate order, no critic, and no ADR 0074 | `docs/05` points at `docs/12` and this file at the top, and no two documents give opposite instructions about the same work | ✅ **done** (s21) |

### Phase B — the content language can express a model

The authoring surface, which both reviews identify as the actual cause of how the games look.

> **Ordering within Phase B, set by review 3, and the tiebreak is not which item is more valuable.**
> It is **which item's value is gated on something that has not landed**.
>
> **Alpha cutout is worth nothing without a texture.** The mechanism is "discard where the sampled
> alpha is below a threshold", and with every `base_colour_texture` empty there is nothing to discard
> against — a cutout material cuts out a rectangle. Its value arrives with item 13, in Phase C.
> Building it first would be a pipeline, a shader define, a sort key and a test, **exercised by zero
> content until two items later**: the pattern this repository has already run five times, and the
> exact failure this file exists to stop.
>
> So **item 8 first**, which delivers the day it lands and depends on nothing. And **item 11 splits**:
> the *blended* pass needs no texture at all (coloured glass, water, a ghost, a world-space panel are
> all `base_colour.a < 1`) and is the riskier renderer half, so it comes early; the *cutout* half
> moves to Phase C beside item 13, where there are ferns to cut out on day one.

| # | R# | Item | Closed when | Status |
|---|---|---|---|---|
| 8 | 6 | ✅ **ADR 0074 §2 and §3: `CompoundMesh`, `array` and `mirror`.** The ADR calls §3 *"where the leverage is"*; four more nouns without composition still cannot make a table, a lamp fitting or a run of racking. `StairMesh` composes parts in a private loop, which is the mechanism not existing. **`taper` is deferred out of the first landing** — it is a deformation of one primitive rather than a composition operator, and it hits normals the same way rotation does, so it belongs after the normal-transform test exists. **Three requirements, not advice:** (a) per-part rotation must rotate normals *and* tangents, and `wound_to_match_normals` **provably cannot catch** a mesh whose normals are all wrong by the same rotation — winding and normals rotate together — so write the test first: rotate a part 90° and assert its normals equal the unrotated part's put through the same matrix; (b) generate tangents **once, after assembly**, or a seam between two parts carries two frames and a normal map lights across it wrong; (c) `array` and `mirror` operate on a part list, so they follow `CompoundMesh` rather than accompanying it | A table, a lamp fitting and a bolted assembly are each one `.mesh` file, authored as text, with no new Rust — each with a `BoxMesh` control in its capture test | ✅ **done** (s21) |
| 9 | 6 | **ADR 0074 §4: raw `vertices`/`indices`, landing *with* item 8.** Not after it: `CompoundMesh` tessellating a part list into one `MeshData` is the same operation an importer needs a target for, and Q41's skinned meshes will need somewhere to land. Building the assembler and the dump target together means one representation rather than two that later have to be reconciled | A `.mesh` may carry vertex data directly, and `amadeo fmt` round-trips it byte-stably | ✅ **done** (s21) |
| 10 | 2 | **`uv_scale`, before any texture is attached to anything.** `mesh.wgsl:360` is `out.uv = vertex.uv;` — no density control exists. A 12 m wall and a 0.4 m crate would show one image at a 30× density difference, which reads as a bug rather than as art | A material controls its texel density, and a capture of two surfaces of very different sizes shows the same **checker** at the same density. *(Was "the same texture", which could not be satisfied inside Phase B: there is no texture to attach until item 13. A checker generated in the test is enough to measure density, and does not import the whole of item 13 into this item.)* `games/scarp`'s `TEXTURE_TILE` workaround goes when item 13 lands | ⬜ |
| 11 | 7 | **A sorted *blended* transparent pass.** `gpu.rs:2084` is `blend: None`, so no glass, water, ghost or world-space panel is expressible. **Needs no texture** — all of those are `base_colour.a < 1` — and it is the riskier renderer half (a second pipeline, a depth-write rule, a back-to-front sort within `SortOrder`), which is the argument for doing it early | A blended material composites in the right order from any angle, and an opaque scene's capture is byte-identical | ⬜ |
| 11b | 7 | **Alpha cutout**, moved to Phase C beside item 13. Discarding below a threshold needs something to sample: with every `base_colour_texture` empty, a cutout material cuts out a rectangle. It is the cheapest route to a non-box silhouette *once there is a texture* — grating, cobweb, hanging cloth, foliage | A cutout material draws its shape rather than its quad, against a generated foliage texture | ⬜ **Phase C** |
| 12 | 8 | **Particles.** Nothing in `crates/` or `modules/` mentions one. Dust in a torch beam is most of what makes an interior read as air rather than vacuum; it is also NMS's atmospheres, Zomboid's rain and Schedule I's smoke. ADR 0067's named-field list items are already the right format for an emitter's stages | An emitter is authored in a `.scene`, it is deterministic under the fixed tick, and a capture shows lit motes drifting in front of a dark surface. *(Was "a torch beam has motes in it", which needs the **beam** to be visible in air — that is volumetric light shafts, item 12b, not particles. Motes in an invisible beam are motes in the dark.)* | ⬜ |
| 12b | — | **Volumetric light shafts.** Named by `STATUS.md` and by ADR 0073 as the next visual step and by neither plan as an item, which is how it went unscheduled. Raymarches through exactly the fog ADR 0073 added, and is what makes a torch beam visible in the air rather than only on the surfaces it lands on | A dark interior shows a cone of light in the air from a spot light, and off is byte-identical to before it existed | ⬜ |

### Phase C — content exists, and the pictures change

| # | R# | Item | Closed when | Status |
|---|---|---|---|---|
| 13 | 3 | **A texture generator, as an engine deliverable rather than a game's binary.** `docs/12` §3 rates textures "Partly" — `pix` writes pixel art from hand-written text and does not reach a 512² tiling plaster with a normal map. `amadeo-noise` is already deterministic and already banned from `sin`/`cos`/`powf`. Needs Worley, brick and tile lattices, gradient ramps, height-to-normal, and roughness/metallic packing into the glTF channel layout `Material::metallic_roughness_texture` documents. **Item 10 has nothing to connect until this exists** | A game's textures are generated from committed text, including a normal map and a packed metallic-roughness map, and the generator is engine code with its own tests | ⬜ |
| 14 | 9 | **Two visual defects both reviews have now named.** `gloom.rs`'s two-tone environment map puts a hard seam across every wall — in the Warren's title frame it is the strongest compositional line in the image, a maroon band across the ceiling and an orange one across the floor. And lamps still have no mesh, so light comes from a white smear | A wall is a smooth gradient at every angle, every light with a fixture has one, and a capture shows both | ⬜ |
| 15 | 18 | **The Warren's title screen.** Four stacked elements on three left edges (title x≈496, filled button x≈504, unfilled buttons x≈504 with no fill), vertical gaps of 35/44/27 px, and a bare black rectangle composited dead-centre over a lit render. The typeface choice is good and is doing all the work | One optical margin, one consistent rhythm, a rule under the title, and an off-centre placement against a composed frame rather than a wall | ⬜ |

### Phase D — throughput, before three games depend on the current shape

| # | R# | Item | Closed when | Status |
|---|---|---|---|---|
| 16 | 12 | **An ADR on crowd agents.** ADR 0036 puts `enhanced-determinism` on permanently, which forecloses rapier's `parallel` and `simd` features for ever, so the ceiling is architectural. `docs/10` measures 811 bodies at 11.49% of a frame and concludes nothing needs more — written against the *old* nine-game list. Project Zomboid needs hundreds to low thousands of navigating agents and gets them by not making them rigid bodies. **Hard to reverse: `CLAUDE.md` §5 says this is Justin's** | An ADR says what a crowd agent is, and it is decided before anything builds a crowd rather than at 800 agents | ⬜ **Justin** |
| 17 | 15 | **`MeshInstance`'s per-frame allocations.** It holds four `String`s (`backend.rs:150`) and is deep-cloned per visible mesh per camera per frame at `lib.rs:917`, `:973` and `:1189` — roughly eight heap allocations each, for names that never change. Invisible at 50 meshes, which is why `docs/10` looks clean; a wall at the prop counts Zomboid and Schedule I imply | A frame's mesh list carries interned ids and a resolved material index, and a 2,000-drawable scene is measured before and after | ⬜ |
| 18 | 13 | **`mod-pathfinding`, inside this gate rather than in M7.** Two of three first-class targets are defined by navigation. `amadeo-behaviour` gives an agent a mind and `amadeo-character` gives it legs, with nothing between — `games/atrium`'s watcher walks through pillars and says so in its own comments | An agent navigates around an obstacle it cannot see through, deterministically, and the watcher stops walking through pillars | ⬜ |
| 19 | 17 | **Widen the performance evidence.** Everything measured is 640×360 with 20 meshes, or 1280×720 with 11, plus 811 physics bodies and 20k sprites in the CPU batcher. Nothing has measured 1080p, textured shading, transparency, or a world of thousands. "AA performance rendering" cannot be claimed from four spot readings, and M3 gate item 8 cannot be closed by them | `docs/10` carries 1080p with textures and transparency on, 2,000+ drawables, and 1,000+ agents, each re-runnable | ⬜ |

### Phase E — deliberately deferred

| # | R# | Item | Why it moved | Status |
|---|---|---|---|---|
| 20 | 10 | **The write half of the agent protocol** — was item 4 of the first plan | `docs/protocol/v1.md:594` already says these methods need a persistent session, and that session's only real client is M4's editor's undo/redo and live-tweak loop. Building it now means **designing against no client**, which is precisely how this repository ended up with cascaded shadows, PBR, normal mapping and 16× anisotropy that have never drawn a textured pixel. I5 is not violated today because the editor does not exist. **Do it as the opening act of M4**, with the editor's needs in front of you | ⏸ **M4** |

---

## 3. What POLISHED requires

The first review's condition was items 1–4 of its plan plus one game whose meshes are not all boxes
and whose materials sample textures. The second review did not lower it. Stated against this file:

- **Phases A and B complete** — the content language can express a model, and the engine can draw a
  transparent one.
- **Phase C complete** — and specifically the second half of the original condition: **a game whose
  meshes are not all boxes and whose materials sample textures.** Both tracked measurements in §1
  must move off `23 / 23` and `36 / 36`.
- **Phase D's item 16 decided**, because it is hard to reverse and gets more expensive with every
  system built on top of it. The rest of Phase D may trail.
- **A frame from a real game that would survive being put on a screen in front of a thousand people**,
  which is `docs/12` §1 verbatim, and is the only condition here that the critic rather than a test
  decides.

---

## 4. How this gate has gone, kept because it is the measurement

**Review 1 (session 20)** — NOT POLISHED. Measured the three numbers in §1 for the first time and
produced the fourteen-item plan this file replaces. Its plan is gone; the numbers survived, which is
the argument for this file existing.

**Review 4 (session 21, `9e10f03`)** — **Phase A: POLISHED.** All seven items plus 3b meet their
stated conditions, verified by running the shipped binaries, by mutation, and by looking at eight
rendered images across three rounds.

It corrected one more piece of my reasoning on the way, and the correction is the useful kind. Its own
item-2 wording said to discriminate the cone *and the cylinder* by silhouette width at two heights —
which is **false for the cylinder**, whose silhouette is a near-rectangle indistinguishable from a
box's. Shading is what separates those two. It also singled out the assertion that the *control* box
must **not** taper, as catching the case where perspective alone produces the effect the cone
assertion claims credit for: "the difference between a test that measures a shape and one that
measures a camera".

**Review 3 (session 21, `751489a`)** — Phase A only: NOT POLISHED, five of seven items closed. It
**conceded item 3**, which is worth recording because the concession is the mechanism working: it had
asked for `WedgeMesh::height_back` to default to something non-zero, and agreed that dropping
collapsed triangles at *any* authored value is strictly better, on a reason it had not made — a
zero-area triangle is the one thing the winding test must skip, so fewer of them means more of the
mesh is actually checked.

What it found that a green suite did not:

- **Item 3b was one type-family short.** Shapes and `Material` declared defaults; `Camera`,
  `Environment` and the three lights declared none, so `describe --example` handed an author a dead
  camera and a black screen. Closed by ADR 0076.
- **The `f32` fix landed in one of two writers.** The scene spelling was right and the **JSON** one —
  the half an agent parses — still read `0.18000000715255737`.
- **Item 5's capture test could not tell a stair from a box**, which in this repository is the wrong
  test to have. Fixing it needed the camera moved twice: once because the stair sat below the
  crosshair, then again because a `StairMesh` climbs along +Z, so a camera on the +Z axis looks at the
  *back* of the flight where the top step occludes every step behind it and the whole thing renders as
  a slab. **Both framings passed their assertions.**
- **Two clauses of my own reasoning were wrong.** A frustum's lateral facet *is* planar, so the stated
  reason for not using `flat_shade` was wrong even though the decision was right; and the comment
  illustrating the faceting scanline implied a signal ten times stronger than what the code measures.
- **Two close conditions in this file could not be satisfied as written** — item 10 depended on a
  Phase C item, and item 12 asked for motes in a beam that nothing makes visible. Both rewritten, and
  item 12b exists because of it.

**Review 2 (session 21, `dc78b6a`)** — NOT POLISHED, nineteen ranked defects. Took live captures from
the game binaries rather than reading the code, which is what found that the Warren's environment seam
is now the strongest line in its title frame. Three findings changed the plan rather than adding to it:
`uv_scale` must precede any texture, the write protocol should move to M4, and particles are not a
"basic" to be deferred. It also found the false skeletal-animation citation **in `docs/12` itself** —
the fourth time a document here has described something nobody checked, and the first time in the
document that sets the standard.

One thing worth recording about how review 2 was handled: five of its highest-consequence claims were
verified against the repository before being acted on, and all five held. `STATUS.md` notes the critic
has been factually wrong once and withdrew on evidence. Both halves of that are load-bearing — take it
seriously, and check it.
