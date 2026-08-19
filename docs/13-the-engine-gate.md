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
>
> **`docs/14-the-critic.md` §6 governs what may be written in the Status column, and review 12 wrote
> it because this file broke the rule.** Only a review may write ✅; implementation writes 🟡 **built,
> awaiting verdict**. Every ✅ carries the review number and its evidence. A reopened item keeps its
> history rather than reverting silently to ⬜.

---

## 1. Status at a glance

**Gate verdict: NOT POLISHED (review 13).** Phases A and B are closed and passed — A on review 4,
items 8 and 9 on review 5, item 11 on review 6, item 10 on review 7. **Phase C is open**; Phase D is
untouched.

### The order, as review 13 set it

Everything the Warren needs now exists, so nothing is gating it any more and it moves to the front.

1. **Item 31 — a capture route into `games/warren` mid-run.** Small, and item 24 cannot be *judged*
   without it. First in wall-clock even though it is second in value.
2. **Item 24 — `games/warren` stops being boxes.** POLISHED is defined on this frame and on no
   other, and twelve reviews have moved `13 / 13` and `6 / 6` by zero.
3. ~~Item 14's pendant~~ — **done in s22**, and it was one number in a clip rather than in the scene.
4. **Item 21 — make the occlusion pass reach a contact.**
5. **Item 22 — shadow-edge dithering**, now more visible because the pillar behind it is no longer
   blown out.
6. **Item 15 — the Warren's title screen**, promoted from eleventh: it is the first thing anyone
   sees, and once item 24 gives it a backdrop worth seeing, a black rectangle over it is the wrong
   frame to open on.
7. Then 12f part two (the sky), 13b (bloom and fog), 30 (the player is a box), 23 (normal-map mips),
   25 (the reticle, landed in s22), and Phase D.

**Review 12 found that every hour of Phase C had landed in the wrong game.** The measurements below
are per game from now on, because a single total is what hid it: `games/atrium` — a demo with no
premise, no objective and no fiction — carries all of the movement, and `games/warren`, the milestone
deliverable whose design passed the critic three sessions ago, is **exactly where review 1 left it in
session 20**. Nine reviews of gate work changed not one pixel of the only game here a player would
meet.

The measurements that define the problem, tracked across reviews so drift is visible. **Recount them
rather than copying them forward** — the row marked ⚠ was wrong in this file for a whole session:

| Measurement | Review 1 (s20) | Review 2 (s21) | Review 11 claimed | **Review 12, recounted** | Target |
|---|---|---|---|---|---|
| `.mesh` assets that are box-only | 23 / 23 | 23 / 23 | ⚠ 23 / 31 | **24 / 37** | a game whose meshes are not all boxes |
| — of which `games/atrium` | — | — | — | 9 / 22 | (a demo; does not satisfy the target) |
| — of which **`games/warren`** | 13 / 13 | 13 / 13 | — | **13 / 13** | **this is the number that matters** |
| Material `base_colour_texture` empty | 15 / 15 | 15 / 15 | 11 / 14 | **11 / 14** | a game whose materials sample textures |
| — of which **`games/warren`** | 6 / 6 | 6 / 6 | — | **6 / 6** | **this is the number that matters** |
| Material `normal_texture` empty | 15 / 15 | 15 / 15 | 11 / 14 | **11 / 14** | ADR 0047 has content in the Atrium only |
| Material `metallic_roughness_texture` empty | 15 / 15 | 15 / 15 | 11 / 14 | **11 / 14** | ADR 0048, likewise |
| Mutating agent-protocol methods | 0 / 17 | 0 / 17 | 0 / 17 | 0 / 17 | deferred to M4 — see item 20 |

**The three texture slots are tracked separately on purpose.** A single "43 of 45" read as nearly
finished when the honest state was that **one of three texture paths had a user**. All three have one
now, in `games/atrium`; the three materials that remain empty there are `amber`, `rust` and `glass` —
a lamp filament, a painted metal box and a pane, none of which would gain from a slab map.

**The denominator fell from 15 to 14** because ADR 0078's amendment deleted `plinth_stone.material`:
two materials for one stone, differing in one number, which was the evidence that texel density was
only half solved.

### One number in this file was silently voided, and the commit that did it said nothing

`c5697e7`, whose message is entirely about the shadow pass culling front faces, also changed
`atrium.environment`: **`exposure 1.0 → 0.8` and `sky_ambient 0.68 → 0.25`.** That is a 2.7× cut to
the whole ambient term plus a global exposure change — the largest look change of session 21 — inside
a commit about culling. Item 12c below still recorded the balance as `SCALE 0.5` / `sky_ambient 0.68`
and cited "framing stone reads 80,81,85 at both" as its evidence; **both the number and the evidence
were void and the file did not know.** Corrected in place, and noted here rather than only there,
because the failure is the one this whole file exists to prevent: a knob moved and the record of why
did not move with it.

**The first measured payoff from ADR 0074 §2, recorded because it is evidence rather than an
argument: three props, fourteen-plus primitives, three drawables.** The Atrium's table, generator and
lamp are assembled from four, four and seven authored parts, which the modifiers expand to more than
fourteen primitives -- ten of the generator's bolt heads come from one part with a `repeat` and a
`mirror`. As prefab children that would have been fourteen entities, fourteen transforms and fourteen
draw calls. It is three of each.

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
| 3c | r13 | **This document and `amadeo image`.** Justin asked for documentation that is only for the critic and for him; review 12 specified it | engine | A review can execute §3 cold, and the pixel probe is in the repository rather than rebuilt each time | ✅ **r13** — §3 #6 caught the mesh count drifting again (24/37 → 24/39), §4 #2 made the reviewer distrust its own experiment, and §4 #7 sent it to probe a contact instead of reasoning about a shader. `amadeo image` called *"better than mine"*; its three requested additions -- `diff`, a clipped-pixel count and a region for `stats` -- landed in s22 |
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
| 10 | 2 | **`uv_scale`, before any texture is attached to anything.** `mesh.wgsl:360` is `out.uv = vertex.uv;` — no density control exists. A 12 m wall and a 0.4 m crate would show one image at a 30× density difference, which reads as a bug rather than as art | A material controls its texel density, and **a capture of a game shows two very differently-sized surfaces at the same texel density** -- not a checker in a test. `games/atrium`'s 20 m floor at `uv_scale 5` and its 3 m plinth at `uv_scale 0.75` both land on 2 m slabs. `games/scarp`'s `TEXTURE_TILE` workaround goes when that game next samples anything | ✅ **done** (s21) — ADR 0078, passed on review 7 |
| 11 | 7 | **A sorted *blended* transparent pass.** `gpu.rs:2084` is `blend: None`, so no glass, water, ghost or world-space panel is expressible. **Needs no texture** — all of those are `base_colour.a < 1` — and it is the riskier renderer half (a second pipeline, a depth-write rule, a back-to-front sort within `SortOrder`), which is the argument for doing it early. **Shape, from reading the pipeline (s21):** an explicit `AlphaMode` on `Material` rather than inferring transparency from `base_colour.a < 1` — inference is "a derivation standing in for a decision", the pattern `docs/07` now documents three instances of, and ADR 0075/0076 make the field free to add. Partition into opaque and transparent **at collection**, not in the backend, so the order is decided where it is reproducible; sort the transparent run back-to-front by distance. **The predicted "entity tie-break" turned out not to be needed** -- `MeshInstance` carries no entity, and `sort_by` is stable over an already-deterministic order, so equal distances keep a reproducible order for free; `total_cmp` is still what stops a `NaN` making the comparison inconsistent. Second pipeline differs in exactly two states: `blend: ALPHA_BLENDING` and `depth_write_enabled: false`, keeping `depth_compare: Less`. Draw order is opaque → sky → transparent | A blended material composites in the right order from any angle, and an opaque scene's capture is byte-identical | ✅ **done** (s21) — ADR 0077, passed on review 6 |

### Phase C — content exists, and the pictures change

> **Re-planned by review 12. The old ordering is kept below it because the reason it expired is the
> useful part.**
>
> Phase C's original preamble routed every item into `games/atrium`, and said why: *"it is the Atrium
> rather than `games/warren` by elimination — the Warren is the stronger forcing function for
> atmosphere, but `docs/12` §4 puts design the game first and the Warren's design has not passed, so
> putting Phase C's content there would be building on an unpassed design."* That was correct when it
> was written. **`docs/11` passed on its sixth critique three sessions ago, and nothing re-routed the
> work.** So the compound meshes, the three texture maps, the per-slab tone, `uv_scale`, the
> transparent pass, the sky, the room proportions, the bloom and the fog all landed in a demo, and the
> game is 13 of 13 boxes and 6 of 6 untextured, unchanged since review 1.
>
> **The vehicle changes, the order does not.** Review 12 was asked directly whether "engine to AA
> before building a game" is the right sequencing and said keep it — Phase A found five defects no
> game would have surfaced, and reversing the order on one session's frustration discards a working
> mechanism. What changes is *where the output goes*.
>
> So this table has a **Lands in** column, and it is not decoration: `docs/13` §3's POLISHED condition
> is *"a frame from a real game"*, `games/atrium` is not one, and nothing in the plan previously said
> which game an item had to appear in. That omission is the whole reason Phase C drifted.
>
> **`games/atrium` keeps a job and it is a narrower one**: it is where a *feature* is proven with a
> capture test, on the same argument that put the primitive capture tests there. It stops being where
> content lives.
>
> **Ordering, from review 12's ranking.** Ambient occlusion first because nothing else on the list
> changes as many pixels and every contact in every frame is currently undarkened; then the fixtures,
> because three of five lights in the showcase scene come from nowhere; then the texture generator,
> because it is what the Warren needs to stop being grey boxes and because item 11b is explicitly
> gated on it. Particles and volumetric shafts move **after** it — they are atmosphere on top of
> surfaces that do not exist yet.

| # | R12 | Item | Lands in | Closed when | Status |
|---|---|---|---|---|---|
| 21 | 1 | **Ambient occlusion, both halves — the engine has none of any kind.** No SSAO, no GTAO, no baked AO, and `Material` has no occlusion slot; `mesh.wgsl` samples the ORM texture and uses `packed.g` and `packed.b` while **discarding `packed.r`, which is glTF's occlusion channel**. The consequence is in every frame: a pillar meets the floor with zero darkening, a table leg meets the floor with zero darkening, and a two-wall corner reads the same value as the flats. It is the largest single reason the output reads as composited rather than rendered, and Godot, Unity URP and Unreal all ship it on by default or one checkbox away. **Two halves, and both are wanted**: (a) read `packed.r` and generate a cavity/occlusion channel in the texture generator from the height field that already exists, which darkens joints and slab edges for one shader line; (b) a screen-space pass in the render graph for the corners and object contacts no texture can bake | engine, then both games | A capture shows measurable darkening where two surfaces meet that is absent with the pass disabled, at a named crop, in both games — and an authored-off scene is byte-identical | 🟡 **the baked half works; the screen-space half STAYS OPEN (r13).** Baked passed: the ORM red channel carries 255 on a face and 145-183 at a joint, the shader reads it, and the strength dial is mutation-checked. **The screen-space pass does not reach a contact.** Review 13 probed all four the item names -- lamp base, table leg, plinth base, pillar base -- and measured **0 to 3 levels**, two of them byte-identical. The 30 levels are at the pillar's convex *arris*, a silhouette, where they read as a banded comb; `occlusion.wgsl` own comment predicts that fringe and asks to be revisited when one appears. Three named fixes: a **depth-aware blur**, a **support radius on the baked cavity** (it is a step function coincident with the joint line, so it darkens a dark line and adds no relief), and a way for the pass to touch more than ambient -- URP and Unreal both ship a direct-lighting knob because a purely-ambient AO is invisible in a sunlit room. Then author it in `warren.environment`, where a dark interior will show it. **Cost is now measured**: 275 µs of 616 at 1280x720 (`docs/10`) |
| 14 | 2 | **No light with no readable cause in the frame** — amended by review 13 from "every light has a fixture and every fixture has a light", which is a *heuristic for* the defect rather than the defect. `games/atrium`'s `vestibule_light` carries no `Mesh` and is ruled **legitimate**: it stands for the sky beyond an opening, the chamber reads as a place, and column x=960 runs 191/176/149/187/204 down its height -- a real light distribution rather than a flat glowing patch. Sky portals in Unreal and `env_lightglow` in Source do exactly this. The original wording, kept because the two failures it names are both real: **every light has a fixture and every fixture has a light**, plus `gloom.rs`'s two-tone seam. Named by reviews 2, 11 and 12 and still failing on **three of five lights in `games/atrium`**: `lamp` (`PointLight 22.0`), `lantern` (`SpotLight 38.0`, shadow-casting) and `vestibule_light` (`34.0`) carry no `Mesh`, and `lamp` blows `pillar_nw` to clipped white from empty air 1.4 m away. The inverse also holds: `lamp_fitting` is the one light with a fixture and at `intensity 6.0` the floor reads 198/200/202/211 straight past it — **no pool, no local peak**. The defect is invisible reading a scene file top to bottom, because it is a component that is *absent* | atrium, then warren | No entity carries a light without a `Mesh` or a `Mesh` fixture without a light; a floor scanline through every fixture shows a local peak of at least 20 levels; and a wall is a smooth gradient at every angle | 🟡 **stays open (r13), and the fix it names is landed.** The structure passed -- the pendant and the lantern are `CompoundMesh` fixtures and the lantern's light is a child so roll sweeps the beam. But the intensity fix was **dead data**: `lamp_flicker.anim` drove the same field and the scene's number had no effect, proven byte-identical. Fixed in the clip in s22, and the pendant re-hung at 2.05 m over the table rather than 3.9 m level with a pillar -- the pillar crop `(400,150,200,300)` goes 6633 clipped pixels to **zero**. **What is still open is `gloom.rs`'s two-tone seam**, untouched and still the strongest line in every Warren frame |
| 13 | 3, 4 | **A texture generator, as an engine deliverable rather than a game's binary — and it must not emit a square lattice.** `docs/12` §3 rates textures "Partly": `pix` writes pixel art from hand-written text and does not reach a 512² tiling plaster with a normal map. `amadeo-noise` is already deterministic and already banned from `sin`/`cos`/`powf`. Needs Worley, brick and tile lattices, gradient ramps, height-to-normal, and roughness/metallic packing into the glTF channel layout. **Review 12's rank 4 folds in here rather than being its own item**, because it is a defect *of the generator*: `surfaces.rs:49` is `const SLABS: u32 = 4` — a 4 × 4 square lattice, stack bond, identical slab sizes, joints running continuously in both directions and repeating every tile. It is on the floor, the walls, the pillars, the galleries, the ceilings and the plinth, so it is the single biggest machine-made tell in the project, and the room reads as a municipal swimming pool. Needs a **running bond** with a half-slab course offset, **at least three slab widths per course**, a low-frequency **macro-tint at a different period from the slab grid** so the repeat does not land on the joint grid, and **grime accumulating in the joints**. *Not* stochastic hex-tiling — that technique is for noise-like textures and explicitly does not handle regular geometric patterns | engine, then warren | A game's textures are generated from committed text, including a normal map, a packed metallic-roughness map and an occlusion channel, the generator is engine code with its own tests, and **no two ADJACENT courses in a 512² tile share a joint line** | 🟡 **anti-lattice clause ✅ r13; the rest stays open.** Review 13 measured joint positions per course and found the closest approach between ADJACENT courses to be 20 px, with none shared — three non-adjacent pairs coincide, which real masonry does constantly, and the condition is amended to say so. **What it kept open is the part that matters**: `stone_slab.png` is min 109 / max 212 / mean 195 with 69% of pixels in one 16-level bucket, and ignoring the joints the whole slab field spans 194-210 -- a 6% range where a stone wall wants **25-40%**. Every slab is a uniform rectangle separated by a one-pixel line; there is no chipping, no staining, no differential weathering. *"It is a diagram of a wall."* |
| 22 | 5 | **Shadow edges do not step.** `c_shadowedge.png` at 5× shows the sun band's boundary on a wall as a run of discrete 8–20 px steps, on the most prominent lighting feature in the best frame this project has produced. `shadow_resolution 2048` over `shadow_distance 24` is 23 mm/texel, and the fixed 3 × 3 grid PCF at `mesh.wgsl:313` blurs *across* the edge while preserving the texel grid's step positions — **an axis-aligned kernel cannot break an axis-aligned quantisation, so a larger one will not fix it.** The mechanism is a **rotated Poisson disc with a per-pixel rotation** from interleaved gradient noise on the fragment coordinate, which is what Frostbite, Unreal and CryEngine all use, and which turns a staircase into dither the eye reads as softness. **This is what remains of 12h**, which review 12 rescoped: it is not contact hardening (PCSS grows a penumbra with occluder distance, a different and larger want) | engine | A 5× crop of the Atrium's sun band on a wall shows no run of more than 3 px at one value along the edge, and the floor shows no acne at four probed points | ⬜ |
| 13b | 6 | **Bloom and fog are authored at values that do nothing.** Measured: the standing lamp's halo is **3 px** and lifts 15 levels at 8 px, indistinguishable from edge antialiasing; the doorway, the brightest thing in the room, goes 60 → 229 **in five pixels with no halo either side**; the sky/wall boundary has no glow at all. At `threshold 1.0` with `exposure 0.8` and ACES almost nothing in this scene exceeds threshold in HDR. And fog: `factor = 1 − exp(−(d·density)²)` at `density 0.016, start 3.0` gives the far wall of a 20 m room `1 − exp(−(0.016 × 17)²) = 0.071` — **a 7% haze, 12% at the far corner**, which on a shadowed wall is about nine levels | atrium, then warren | A bright source shows a halo **≥ 12 px at 1080p**, and the Atrium's far wall sits **≥ 25 levels** closer to the fog colour than the near wall of the same material | ⬜ **reopened r12** — was marked ✅ in s21 with neither half of its condition ever captured. This is the failure `docs/14` §6 was written to close |
| 12f | 7 | **The glass does not read as glass, and the sky has no picture — one generator change closes both.** Part one landed (ADR 0080): `ALPHA_BLENDING` multiplies the whole shader output by coverage, so at alpha 0.34 a highlight was bounded above by `0.34·S + 0.66·W` and was **arithmetically impossible rather than dim**; premultiplied output took the pane from 1 level brighter than the wall beside it to 10, opaque byte-identical. Part two is that `daylight.rs:137` is `fn sky_colour(up: f32)` — **a function of elevation alone**, with no sun disc, no cloud and no azimuthal content whatever, so a reflection in it cannot move when the camera yaws and the pane shows a flat wedge ending in a hard diagonal where the reflected ray crosses the gradient's horizon. The same fact is why 65% of the best up-view is a bare ramp. The fix is ADR 0079's shape one level down: **structure and a sun disc into the specular chain, excluded from the irradiance convolution**, which is what keeping an analytic light out of the ambient probe means in every comparable engine | engine, then both games | The pane shows a specular sheet or Fresnel rim that **moves between two captures at different yaws**, and the sky occupying a capture is not a single monotonic ramp | 🟡 **part one done (ADR 0080); part two open** |
| 12c | — | **A sky for `games/atrium`.** `sky ""` blocked four things at once and the two existing `.hdr` generators were the precedent. The map is real and ADR 0079 split its two jobs (`Environment::sky_ambient`) | atrium | The Atrium names a real environment map, and the lamp, the glass and the wall gradient each read as themselves in a capture | 🟡 **built; held open behind 12f and 14** — review 12: the wall gradient reads (pass), the glass does not (12f), and the lamp reads as a glowing object but not as a light source (item 14). **Two of three.** Balance is now `SCALE 0.5` / `sky_ambient 0.25` / `exposure 0.8` — corrected here after `c5697e7` moved two of those silently, voiding the evidence this row used to cite. Review 12 adds a clause: **12c cannot close while the sky is a bare elevation ramp**, because it is the majority of every frame it appears in |
| 31 | r13-3 | **There is no way to capture `games/warren` in its playable state, and that blocks the gate's own condition.** Every frame any reviewer can take of it is its **title screen**: `--yaw` and `--pitch` aim the camera but cannot dismiss a menu, the protocol has 0 mutating methods, and no snapshot is committed. `docs/13` §3 defines POLISHED as *a frame from a real game* — **and nobody can currently take one.** Found by review 13 while trying to. `capture --from <snapshot>` already exists, so committing a `playing.snapshot` closes it; `capture --input <replay>` is the fuller answer. **Do this BEFORE item 24**, or item 24 cannot be judged | warren | A reviewer can capture `games/warren` mid-run without editing the game, and the route is one documented command | 🟡 **built, awaiting verdict** (s22) — `cargo run -p warren --bin moment` writes `snapshots/playing.snapshot` and `capture --from` photographs it, which needed no engine work because ADR 0028 already restores before drawing. The snapshot is **text**, 4147 lines, so a person can read where the player was standing. Documented in `docs/14` §5 |
| 24 | 3 | **`games/warren` stops being boxes.** The vehicle change made concrete: 13 of 13 box meshes and 6 of 6 untextured materials, unchanged across eleven reviews, in the game with the passed design, the fiction, the objective and the milestone attached to it. Everything above lands here once it exists. `docs/11` supplies the direction rather than taste — cast-iron rings, safety orange, warm hand-lamp against cold fittings, pooled light with real dark between | warren | `games/warren`'s tracked measurements move off 13 / 13 and 6 / 6, and a 1920 × 1080 capture of it shows a cast-iron-lined tunnel with pooled cold fittings, real dark between them, a warm hand lamp doing the near work, and a prop that implies somebody was here | ⬜ |
| 11b | — | **Alpha cutout**, moved here from Phase B because discarding below a threshold needs something to sample: with every `base_colour_texture` empty a cutout material cuts out a rectangle. Cheapest route to a non-box silhouette *once item 13 exists* — grating, cobweb, hanging cloth, foliage | warren | A cutout material draws its shape rather than its quad, against a generated foliage or grating texture | ⬜ **gated on 13** |
| 12 | — | **Particles.** Nothing in `crates/` or `modules/` mentions one. Dust is most of what makes an interior read as air rather than vacuum; it is also NMS's atmospheres, Zomboid's rain and Schedule I's smoke. ADR 0067's named-field list items are already the right format for an emitter's stages | engine, then warren | An emitter is authored in a `.scene`, it is deterministic under the fixed tick, and a capture shows lit motes drifting in front of a dark surface | ⬜ |
| 12b | — | **Volumetric light shafts.** Named by `STATUS.md` and ADR 0073 as the next visual step and by neither plan as an item, which is how it went unscheduled. Raymarches through exactly the fog ADR 0073 added, and is what makes a torch beam visible in the air rather than only on the surfaces it lands on. **Follows item 12** rather than leading it | engine, then warren | A dark interior shows a cone of light in the air from a spot light, and off is byte-identical to before it existed | ⬜ |
| 23 | 9 | **Normal-map mips do not renormalise.** `c_artifact1.png` at 3× shows every vertical joint on a receding parapet dissolving into a comb of horizontal dashes spreading sideways. A normal map averaged down a mip chain does not stay unit-length and the specular response goes non-monotonic. Very visible at 1080p, and it arrived *with* item 13a — the first content to exercise ADR 0047 is also the first to expose this. Either renormalise per level and push the lost variance into roughness (Toksvig / LEAN-style variance packing), or taper `normal_strength` with mip level | engine | A 3× crop of a receding normal-mapped surface shows continuous joints rather than a dash comb, and a flat-on crop is unchanged | ⬜ |
| 25 | 10 | **`games/warren` has no reticle.** `docs/11` §8 calls this *"a usability failure at the core verb, not a polish item"* — the interaction sphere is swept along an axis the player cannot see. There is none in the game; grep confirms it | warren | A capture shows a reticle at the sweep's centre, and it changes state when `Looking::at` is `Some` | ⬜ |
| 15 | 11 | **The Warren's title screen**, unchanged since review 2 and now measured: title left edge x=502, focus bar x=496, `BEGIN` glyph x≈505, `CONTINUE` glyph x=504 — **four left edges inside 9 px**, and the label shifts 1 px as the highlight moves. Panel padding 22 px top-left against 16 px at the bar and 33 px at the bottom. It is a hard-edged black rectangle composited dead centre over a lit render, where `docs/11` §8 specifies a 65% scrim with the options bottom-left as a caption to a sign | warren | One optical margin, one consistent rhythm, a rule under the title, a scrim rather than a black rectangle, and an off-centre placement against a composed frame | ⬜ |
| 26 | 12 | **Audio occlusion — a gameplay requirement of a passed design, in no plan until now.** `docs/11` §9: *"a warden exactly as loud through a wall as through a doorway makes the whole mechanic a lie… this is now a gameplay requirement rather than a polish item."* `amadeo-audio` has none. The design that passed the critic depends on it | engine, then warren | A spatial voice behind geometry is measurably quieter than the same voice at the same distance through an opening, and the test is headless | ⬜ |
| 30 | 8 | **The player is a box and the objective floats.** `body.mesh` is `BoxMesh 0.7 2.0 0.7`, at frame centre in every third-person capture; `brass_key.mesh` is `BoxMesh 0.18 0.4 0.05` at `y 1.2` with its plinth **2.9 m away**, so it hangs unsupported in mid-air. `CompoundMesh` landed two sessions ago and neither has used it | atrium | Neither is a bare box, and the key rests on something | ⬜ |
| 9b | — | **One-field tuple variants in `amadeo-derive`.** `Solid::Box { shape: BoxMesh }` costs three lines and two indents to say "a box this size". `#[derive(Reflect)]` already supports newtype structs and refuses tuple variants, and a one-field tuple variant is the shape the newtype case already handles. **Demoted below everything above by review 12** — a syntax nicety with a one-`fmt` migration. Keep refusing *wider* tuple variants, which genuinely have no names to write | engine | A compound part spells its shape in one line plus the shape's own fields, and every existing `.mesh` is migrated by `amadeo fmt` | ⬜ **demoted** |

**Closed in Phase C, kept with their evidence** (`docs/14` §6 rule 2):

| # | Item | Status |
|---|---|---|
| 12a | Make the Atrium a room rather than a showroom — an opening, a doorway for scale, somewhere to look into, adjacency instead of display | ✅ **r8** |
| 12d | The room was 4 m high with the camera 31 cm below the roof, so the oculus was invisible from anywhere a player could stand | ✅ **r10** — called "the best frame this project has produced", and the first time the lighting reads as daylight |
| 12e | `amadeo capture --pitch/--yaw`, so a reviewer can look anywhere without editing the game | ✅ **r10** — used seven times in that review; it found 12d, 12g and the mis-sampling behind 12f |
| 12g | Three of four quadrants were empty; `--yaw 180` was a single flat value | ✅ **r11** — four gallery runs at 3.7–4.9 m, a stair, a screen wall, a ledge, and the player start moved off the south wall |
| 12h | Normal-offset shadow bias (ADR 0081), for the roof-to-wall light leak | ✅ **r12** — the ceiling junction falls 103 → 93 → 72 → 59 → 40 within three pixels at `col x=1017`, no band, no overshoot, no acne at four points. **What remains is item 22, which is a different defect** |
| 13a | Normal and metallic-roughness maps for `stone` and `slate`, three maps from one height field | ✅ **r12** — `c_wall_lit.png` and `c_artifact2.png`: a bright edge on the sunward side of each joint and a dark line opposite, relief inverting correctly when the joint faces away; per-slab tone flat across a slab and discontinuous at the joint. Exposed item 23 on the way |
| — | The shadow pass was culling front faces (ADR 0082, `c5697e7`) | ✅ **r12** — "a load-bearing fix"; closed the leak that occupied reviews 9–11 |


### Phase D — throughput, before three games depend on the current shape

| # | R# | Item | Closed when | Status |
|---|---|---|---|---|
| 16 | 12 | **An ADR on crowd agents.** ADR 0036 puts `enhanced-determinism` on permanently, which forecloses rapier's `parallel` and `simd` features for ever, so the ceiling is architectural. `docs/10` measures 811 bodies at 11.49% of a frame and concludes nothing needs more — written against the *old* nine-game list. Project Zomboid needs hundreds to low thousands of navigating agents and gets them by not making them rigid bodies. **Hard to reverse: `CLAUDE.md` §5 says this is Justin's** | An ADR says what a crowd agent is, and it is decided before anything builds a crowd rather than at 800 agents | ⬜ **Justin** |
| 17 | 15 | **`MeshInstance`'s per-frame allocations.** It holds four `String`s (`backend.rs:150`) and is deep-cloned per visible mesh per camera per frame at `lib.rs:917`, `:973` and `:1189` — roughly eight heap allocations each, for names that never change. Invisible at 50 meshes, which is why `docs/10` looks clean; a wall at the prop counts Zomboid and Schedule I imply | A frame's mesh list carries interned ids and a resolved material index, and a 2,000-drawable scene is measured before and after | ⬜ |
| 18 | 13 | **`mod-pathfinding`, inside this gate rather than in M7.** Two of three first-class targets are defined by navigation. `amadeo-behaviour` gives an agent a mind and `amadeo-character` gives it legs, with nothing between — `games/atrium`'s watcher walks through pillars and says so in its own comments | An agent navigates around an obstacle it cannot see through, deterministically, and the watcher stops walking through pillars | ⬜ |
| 19 | 17 | **Widen the performance evidence.** Everything measured is 640×360 with 20 meshes, or 1280×720 with 11, plus 811 physics bodies and 20k sprites in the CPU batcher. Nothing has measured 1080p, textured shading, transparency, or a world of thousands. "AA performance rendering" cannot be claimed from four spot readings, and M3 gate item 8 cannot be closed by them | `docs/10` carries 1080p with textures and transparency on, 2,000+ drawables, and 1,000+ agents, each re-runnable | ⬜ |
| 27 | 12 | **`mod-tilemap` and isometric y-sorting**, which no plan has ever contained. `docs/05` says twice — at both of its ⬆ **RAISED BY THE BAR** boxes — that these are *"engine-gate work now, tracked in `docs/13-the-engine-gate.md`"*. **They are not tracked here and never were.** Item 7 fixed the document that pointed the wrong way and nothing scheduled the work, so the one capability Project Zomboid contributes to `docs/12` §2's first-class three is cited as planned and is in no plan. Found independently by this session and by review 12; it is the fourth instance of a citation naming a source that does not contain the claim, which is what item 6 exists for | A 2D tilemap is authored as text, drawn, and y-sorted isometrically, with a determinism test | ⬜ |
| 28 | 12 | **Skeletal animation and skinning — Q41.** Two of the three first-class targets are populated worlds. `modules/amadeo-anim` animates a reflected field by name and provably cannot reach a skeleton (ADR 0066 §1), so skinning needs a typed path: glTF skins, joint palettes, inverse bind matrices, a vertex shader. `docs/11` §3 converts its absence into an aesthetic — never clearly showing the warden — which is honest exactly once and cannot be the answer for Zomboid or Schedule I. **Review 12 ruled it belongs in the gate but last**: do it after the frame passes, not instead of the frame passing. The genuinely undecided half is where a rigged model comes from, which is Q41 rather than this item | A skinned mesh plays a clip deterministically, and `docs/12` §3's authoring question has an answer | ⬜ **after the frame passes** |
| 29 | 12 | **Decals.** `docs/11` §7 lists them as absent and designs around it. A real gap and a real want for wear, grime, signage and damage — and review 12's ruling is that it is not gating a frame, so it sits at the end of this phase rather than in Phase C | A decal projects onto geometry it does not own, sorted and deterministic | ⬜ **last** |

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
  meshes are not all boxes and whose materials sample textures.** **Review 12 sharpened this and it
  is the sharpening that matters**: the game is `games/warren`, not `games/atrium`, so the numbers
  that must move are the two rows marked *this is the number that matters* in §1 — `13 / 13` and
  `6 / 6`. A demo satisfying them satisfies nothing, and for eleven reviews it was allowed to look as
  though it did.
- **Phase D's item 16 decided**, because it is hard to reverse and gets more expensive with every
  system built on top of it. The rest of Phase D may trail.
- **A frame from a real game that would survive being put on a screen in front of a thousand people**,
  which is `docs/12` §1 verbatim, and is the only condition here that the critic rather than a test
  decides. Review 12 said what would settle it, and it is worth quoting because it is the first time
  any review has described the frame rather than only rejecting one: *"one capture from `games/warren`
  — not the Atrium — at 1920 × 1080, showing a cast-iron-lined tunnel with pooled cold fittings, real
  dark between them, a warm hand lamp doing the near work, and a prop that implies somebody was here.
  That is `docs/11`'s own design, and the engine is now within one texture generator and one AO pass
  of being able to draw it."*

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

### Phase C addenda from review 11

Review 11 added one item, **13b**, for bloom and fog having shipped built and authored off — ADR 0056
and ADR 0073 both landed and the game that exists to showcase the engine used neither, which is this
gate's own "written, tested, exercised by zero content" pattern twice in one file. It was marked ✅
**done** in session 21 at bloom 0.3 / threshold 1.0 and fog density 0.016 from 3 m.

**Review 12 reopened it and it now lives in the Phase C table above**, with a numeric close condition
instead of an impressionistic one. Its measurement: a 3 px bloom halo, no halo at all on the brightest
object in the room, and a maximum 12% fog contribution anywhere in a 20 m space. Nobody lied — the
work landed, the file was updated in good faith, and **no capture was ever taken**. That is exactly
the hole `docs/14` §6 closes by reserving ✅ to a review.

**Review 12 (session 22, `f1d7e28`)** — **NOT POLISHED.** Seventeen captures across three games,
probed at the pixel level. Its full record, including its ruling on the gate order and the six things
it asked for in a critic's brief, is in `docs/14-the-critic.md` §7. What it changed in this file: the
per-game measurement rows and the **Lands in** column in §2, the reopening of 13b, the closing of 12h
and 13a with evidence, four new items (21 ambient occlusion, 22 shadow-edge dithering, 23 normal-map
mips, 24 the Warren stops being boxes) and three more in Phase D (27 tilemap, 28 skeletal, 29 decals).
