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

**Gate verdict: NOT POLISHED (review 27).** **Item 24 is CLOSED** — ✅ written by review 27 on
clause (g) alone, after twelve delivered reviews and two that died to session limits. **The remainder
is fourteen open rows below rather than a list attached to one item**, which is review 25's scope
ruling: *"everything else is a specific object, not a property of the game, and specific objects belong
in specific rows."* ~~Review 27's order is **39 (`games/scarp`), 35 (yaw 270), 33 (the grime blur), R2's
bunks, 38 (the warden), then 34 / 36 / 37 and the call plate.**~~ **Superseded by §1b** — Justin
re-scoped the gate in session 26, and review 28 then re-planned the re-scope. The order is
**F2, F2b, F6, F5, F4, F1, F3** — seven rows, with everything else cut or moved to the editor or to the first published game. **Read §1b before §2.**
Phases A and B are closed and passed — A on review 4,
items 8 and 9 on review 5, item 11 on review 6, item 10 on review 7. **Phase C is open**; Phase D is
untouched.


---

## 1b. The re-scope — session 26, instructed by Justin

> **This section overrides the execution order in §1 and the phase assignment in §2.** It does not
> delete a row, change a verdict, or touch anything in §4 or `docs/14` §8. Every open row below is
> assigned a **bucket**, and the bucket is what decides whether it is worked on now, later, or never.

**What Justin instructed, in his own terms:**

> *"Progress seems quite slow, the demo game is still far from finished. Better lower the scope and
> get rid of any redundancies and hundreds of tests on singular things. Let's focus on finishing this
> game in maximum 2-3 sessions. AA Indie studio level not AAA hundreds of millions of dollar budget
> studio. Take note that after this demo game is the editor, and make sure we add that after the
> editor the next is not a demo game but a full fledged survival game like Project Zomboid so 2d and
> 3d i think but Isometric, basically in that style and perspective and quality level… That next game
> isn't a demo game anymore but will be our first published game."*

**Three consequences, and the third is the one that changes this file most.**

1. **The project order is now: finish `games/warren` → the editor (M4) → the first published game.**
   Item 40 stops being *"the next demo game"*. It is **the first published game**, it comes **after
   the editor**, and its scope is a full survival game in the Project Zomboid line — isometric, 2D
   and 3D. Its design document still goes to the critic before any code, per `docs/12` §4.
2. **The bar does not move.** AA indie is the standard and the critic is still binding. What is being
   cut is *how many objects the demo is allowed to spend a review cycle on*, not how good the ones it
   keeps have to be.
3. **`games/atrium` and `games/scarp` are frozen.** Neither is the deliverable. Review 12 found that
   nine reviews of gate work had landed in `games/atrium`, and the answer at the time was to re-route
   the work. The answer now is stronger: a demo of features is not held to the bar at all, because
   `docs/13` §3's POLISHED condition is already *"a frame from a real game"* and neither of these is
   one. **This is the single largest cut in the re-scope**, and it takes eight open rows with it.

### Why the current rate does not fit the budget

Item 24 took **twelve delivered reviews across six sessions** and closed on one clause. There are
**thirty-four open rows**. At item 24's rate the gate closes some time next year, which is the
observation Justin is making. The rate is not going to improve by trying harder, so the row count has
to come down instead.

Two structural causes, both addressed below rather than deplored:

- **A row per object.** Review 25 ruled that *"everything else is a specific object, not a property of
  the game, and specific objects belong in specific rows"*, which was right for auditing item 24 and
  is wrong as a work plan: it turns one lighting pass into six rows that each want their own capture,
  their own number and their own review. **Rows 33, 34, 36, 37, 25, the call plate and R2's bunks are merged
  into one row, F5**, closed by one pass over the objects a player actually stops in front of. **Review
  28 upheld the merge and rejected the merged condition** — see the table below; item 30 is cut outright,
  because its subject is the third-person player box in a frozen game.
- **Feature rows with no player.** Eight open rows exist to prove a renderer feature in a game nobody
  plays. They are cut, not deferred.

### The buckets

**FINISH** — the demo game's remaining work, and the whole of it. Seven rows.
**EDITOR** — needed by M4, worked on when M4 starts.
**GAME 2** — needed by the first published game, whose design decides the shape.
**CUT** — not done, and the row records why rather than staying open forever.

| Bucket | Rows | Count |
|---|---|---|
| **FINISH** | **F1** (15 + 35, the opening and the ending) · **F2** (38, the warden's form) · **F2b** (new r28, the warden respects the level) · **F3** (32, wayfinding) · **F4** (12b, the torch beam) · **F5** (33 + 34 + 36 + 37 + 25 + the call plate + R2's bunks) · **F6** (26, the ear) | **7** |
| **EDITOR** | 20 (the protocol's write half — a real M4 prerequisite) · 17 (`MeshInstance` allocations — editor responsiveness) | 2 |
| **GAME 2** | 40 (**the game itself**) · 27 (tilemap + isometric sort) · 18 (`mod-pathfinding`) · 28 (skinning, Q41) · 12 (particles) · 29 (decals) · 11b (alpha cutout) · 19 (widen the performance evidence) · 16 (the crowd-agent ADR) · 21's screen-space half · 23 (normal-map mips) · **22** (shadow-edge dithering) · **12f part two** (the sky has no picture) · **G-tri** (new r28: terrain triplanar mapping) | 14 |
| **CUT** | 39 (`games/scarp`) · 14 · 12c · 13b · 13's remainder · **30** · 9b | 7 |
| **awaiting a verdict, not a bucket** | 31 (the capture route) — 🟡 since s22, used by six reviews, closed by none | 1 |

**The arithmetic closes now, and review 28 found that it did not.** `grep -c "FINISH (s26)"` returned
**12** against a table naming **11**: the twelfth was item **25**, the reticle, tagged and then absent
from the six. Item **40** carried a GAME 2 tag while missing from the GAME 2 list — it is not merely
*in* that bucket, it **is** the bucket, and it heads the list now. Item **3c** was counted under EDITOR
while being ✅ closed on r13, and is removed. Item **31** is not work and never was: it needs somebody
to write its verdict.

### Every cut, with the reason it is a cut rather than a deferral

A cut row keeps its ⬜ and gains **✂ CUT (s26)** plus one line. None of them is closed and none of
them is claimed to be done.

| Row | Why it is cut |
|---|---|
| **39** `games/scarp` | Review 27 ordered it first, and that was the right order *for a gate with no deadline*. It is a terrain proof whose M2.5 exit gate was met on function — you walk on a generated world and dig into it, which is what the gate asked for. **Review 28 upheld the cut and corrected the reason**: *"texturing it is a second art pass"* is **wrong**, because `crates/amadeo-terrain/src/world.rs`'s own comment says a planar projection stretches on vertical faces and that *"triplanar mapping is the usual fix"*. That is engine work, and an isometric outdoor game looks at cliff faces all day — so it does **not** leave the file with this row. See **G-tri**. |
| **14** no light with no readable cause | `games/atrium` only. The Warren's own lights all have fittings, which reviews 24–27 credited. |
| **12c** a sky for `games/atrium` | Frozen game. |
| **13b** bloom and fog authored at values that do nothing | Reopened against `games/atrium`. The Warren's own bloom-threshold defect was found and fixed in s24, and **F4 re-derives the fog** because a volumetric beam scatters through it. |
| **13** the remainder of the texture generator | `amadeo-texture` shipped, its anti-lattice clause passed on r13, and the Warren's sixteen materials are generated by it. |
| **30** the player is a box and the objective floats | **Cut by review 28, out of F5.** Its subject is `body.mesh` *"at frame centre in every third-person capture"* and its Lands-in column says `atrium` — a frozen game. **`games/warren` is first person and its landmark captures cannot see the player's body.** The half of the row that *was* about the Warren — the key's plinth 2.9 m from the key — was fixed in session 25. |
| **9b** one-field tuple variants in `amadeo-derive` | Already demoted. An authoring convenience. |

**Five rows move to GAME 2 rather than being cut**, because the case that breaks each of them is that
game's normal case and not this one's. **Review 28 found the first version of this list inconsistent
with its own logic and it is corrected here** — 21 and 23 were moved on this reasoning while 22 and
12f part two were cut despite failing for the identical reason:

- **21's screen-space half.** SSAO is a graph pass, a shader, a blur and a resolve, and its visible
  payoff in a torch-lit tunnel is contact darkening the torch's own falloff already supplies. An
  isometric daylight scene has hundreds of object-to-ground contacts and no torch.
- **23, normal-map mips.** Measured on a receding parapet in `games/atrium`. Nothing in a 12 m bore
  recedes far enough to reach the mip levels where it shows. An isometric camera looks at everything
  from far away, which is exactly the case this breaks.
- **22, shadow-edge dithering.** An axis-aligned kernel cannot break an axis-aligned quantisation.
  M4b is a daylight game under one directional light, which is the configuration this was measured in.
- **12f part two, the sky has no picture.** `daylight.rs`'s `sky_colour` takes elevation alone — no
  azimuthal content, no sun disc — and an outdoor game's sky is the majority of every frame.
- **G-tri, terrain triplanar mapping.** New, filed by review 28 so it does not leave with item 39.
  Closed when a vertical cliff face samples its material at the same texel density as the flat ground
  beside it, measured off a capture.

### The seven FINISH rows

**Every close condition below was rewritten by review 28** — five of six failed `docs/14` §6, and the
failure was always the same one this repository has now made six times: **a condition that can be
satisfied by measuring the wrong object.** Two of them were satisfiable with the defect fully intact,
and one had already been passing on the unmodified model by a factor of four. The conditions are the
critic's own wording, pasted in.

| # | Was | Item | Closed when | Status |
|---|---|---|---|---|
| **F1** | 15 + 35 | **The opening and the ending — the first frame you see and the last.** The title screen is unchanged since review 2: title left edge x=502, focus bar x=496, `BEGIN` x≈505, `CONTINUE` x=504 — four left edges inside 9 px — the label shifts 1 px as the highlight moves, and it is a hard-edged black rectangle dead centre where `docs/11` §8 specifies a 65% scrim with the options bottom-left. Then you wake and your first input is the mouse, and **yaw 270 from the spawn is the worst frame in the game**. **Review 28 added the `Ended` screen** — the same panel family in `hud.scene`, the last thing a player sees, and covered by no row | Two captures at 1920 × 1080: the title screen and the **`Ended`** screen. **(a)** The panel is a **scrim, not a fill**: inside its own rectangle, a named 200 × 200 px crop has mean adjacent \|ΔL\| ≥ 2.0 and a luma range ≥ 25 levels — the tunnel is measurably present *through* it. **(b)** The four left edges are within 1 px of one column or separated by ≥ 24 px, by first-non-background pixel on named rows; the label's left edge is identical in two captures with the highlight on different items. **(c)** The title block's centroid is ≥ 15% of frame width off the horizontal centre. **(d) The spawn:** `layout.rs` asserts the player start is **≥ 1.5 m** from any wall face, and four captures at yaws 0/90/180/270 each put **no more than 12% of pixels above luma 144**. Measured today: 3.5% / 8.5% / 0.2% / **36.7%** | ⬜ |
| **F2** | 38 | **The warden's form.** Open across seven passes. `warden_figure.mesh` was nine axis-aligned `solid Box` parts; the first rebuild made it fourteen solids of revolution, which review 29 called *"a bollard in a hat"* — part count was never the problem, **rotational symmetry is a stronger machine-made tell than a box is** | **All clauses are measured on a MATTE, not photometrically** (`docs/14` §4 #16): set every material the figure wears to `base_colour 0 0 0 / emissive 12 0 12`, capture, restore with `git checkout`, re-capture and assert byte-identical. Figure pixels are `R ≥ 200 ∧ G ≤ 60`. **Usability:** a row is usable only if its figure pixels form ONE unbroken run; **≥ 16 of 20 must be usable**, and a submission below 16 re-frames rather than re-measures. **(a1)** ≥ 8 distinct left-profile x over 20 samples. **(a2)** no value repeats for more than 3 consecutive usable samples. **(a3) the coat test**, where `H` is brim-underside to hem-bottom: `waist` = median width over 0.40–0.65H, `shoulder` = max over 0.00–0.25H, `hem` = max over 0.75–0.95H; require `hem ≥ 1.35 × waist`, `shoulder ≥ 1.35 × waist`, **and the widest row of the shoulder band in its upper half**. **(b)** mean \|left − right\| about the median centre line ≥ 6% of max width, usable rows only. **(c)** vertical mean adjacent \|ΔL\| ≥ 3.0 over ≥ 60 px, non-monotonic, **with the on-figure sample count published for the subject AND any control**. **(d) not a stack:** at most 3 adjacent-row width changes exceed 6% of max width. **(e) value:** the figure's p90 luma over its own matte ≤ the p90 of an equal-area patch of lit lining in the same frame | ⬜ **NOT PASSED (reopened r30 — silhouette relief 1.0–1.4 cm on all three added features; hem/waist 1.14 and shoulder/waist 1.01; usability 10/20; hem cap annulus +61/−93 at y716).** **The submission measured width as a COUNT of figure pixels and the clause means EXTENT** — on a row the bunk crosses, a count loses the occluded pixels and inflates every ratio that divides by the waist, so 1.14 and 1.01 were reported as 1.80 and 1.59. §4 #14's eighth instance, committed inside the instrument written to prevent it. Against the model rather than the pixels the geometry cleared the bar by three per cent, and three per cent does not survive projection. **The structural verdict matters more than the numbers:** *"a stack of coaxial cones cannot produce a shoulder line, because a cone's widest place is a circle and a shoulder is a horizontal edge."* Session 26 then applied ordered changes 1–3 and 8 to the stacked-cone body and **made it worse twice running** — narrow plackets standing 7–9 cm proud of a cone read as planks bolted on rather than as cloth — so the figure was reverted to the state review 30 measured rather than thrashing further. **What landed and stands: the hem cap annulus is closed (0.311 → 0.322, ordered change 5) and the lantern is outboard at radial 0.50 (ordered change 1), which is the figure's first enclosed negative space.** The rest wants a **box- and wedge-based figure with mass asymmetric about the vertical**, not a stack of solids of revolution, and that is a session's work rather than an increment. Now fourteen parts with **a front closure, a rear vent and a diagonal strap** so the body is not a solid of revolution; cones **overlap** with each inner radius below the outer's value at the joint, so the six full-circumference ledges are gone; the shoulder cone is **inverted** (`0.297 → 0.355`, widest at the top) against a 0.245 waist; the head is a **separate near-black `shadow` material** dropped inside a taller storm collar; the arms are **deleted** and the lamp hangs on the strap, which is review 29's own preferred option 6. **Review 29's ordered change 1's second half was TRIED AND REVERTED with evidence:** rotating each cone about `y` to break the facet phase produced a **sawtooth** — at 9 sides an inner cone's corner pokes through an outer's flat unless the radius step exceeds ~6%, which is the very ledge the overlap exists to remove, so **differing phases and soft overlaps are geometrically incompatible** and the asymmetry comes from the boxes instead. The lamp is fixed: `bulb` and `glow` moved onto the lantern (they were **47 px apart on screen**), the lantern gained an **emissive glass** part and its **own `lantern_case` material** — needed anyway, because a figure whose parts share materials with the level cannot be matted whole — and the spot is renormalised on **peak channel** (`8.3 × 0.95` against the old `11.0 × 0.72`). **Measured, matte, `at_warden`, span y 512–772:** (a1) **16 distinct ✓**; (a2) **max run 2 ✓**; (b) **13.8 px against a 5.8 bar, 2.4× ✓**; (a3) hem/waist **1.80 ✓**, shoulder/waist **1.59 ✓**, but the widest shoulder row falls **7 px into the band's lower half ✗** — the collar occupies the top of the band, so the shoulders sit just below it; (d) **11 changes against a bar of 3 ✗**, though most fall on occluded rows; **usability 13 of 20 ✗**. **The framing was searched, not guessed:** across −1.9 gave 11/20, −0.55 gave 12/20, and **+1.35 and +1.9 lose the figure entirely** (0/20) because the camera leaves the bore. The occluder is a bunk the warden's own post stands beside, so **no camera inside that bore reaches 16/20 at this warden position** — which is a finding about where the level puts the warden, not about the lens |
| **F2b** | **new, r28** | **The warden respects the level — the largest gap in the plan, and it was not in it.** `move_the_warden` writes `Transform::translation` straight toward the player with no collider and no sweep; `watch_for_you` sets `sees_you` from `distance(...) <= WARDEN_SIGHT` alone, with no line of sight. **The antagonist sees through cast-iron bulkheads and walks through them.** Its own doc comment says *"the room is open enough that it does not read as broken"* — **that was written for `scenes/warren.scene`, and the shipped level is `scenes/generated.scene`**, fourteen sections divided by bulkheads. **F6 exists to stop a warden being as loud through a wall as through a doorway; without this, F6 removes the lie and the omission puts it straight back in the same thirty seconds.** Not item 18 and not pathfinding: `cast_shape` for sight and `move_shape` for motion, both built, both already used in this game | A headless test placing the warden across a bore wall from the player, in `pursue`: after 300 ticks **(a)** `Facts::get("sees_you")` has been `false` throughout, and **(b)** the warden has never been within 0.30 m of a static collider face and has never crossed a wall plane — with a second case through an open cross-passage in which it **does** reach the player. Plus one capture from `at_warden.snapshot` after 120 ticks showing the figure **against**, not inside, the bulkhead | ✅ **r30 — `the_run_can_end.rs::a_wall_between_you_is_a_wall` / `a_clear_line_is_a_clear_line`; `w_warden_t150.png` byte-identical to `w_at_warden.png`, figure against the bulkhead, `occlusion 1.0` in the snapshot.** `watch_for_you` casts a 0.12 m sphere from the warden's eye to the player's and reads `ShapeHit::entity` -- the field session 17 added for exactly this and nothing used for it. `move_the_warden` uses `move_shape` with a 0.28 m capsule instead of writing a translation, so it slides along what it hits, and **it turns to face where it is going**, which nothing did before. Both systems moved `.after(amadeo_physics::STEP_PHYSICS)`; without that the cast queries an empty index and finds open space everywhere. Two tests in `the_run_can_end.rs` that discriminate against each other: a warden across the lining at the same distance stays **idle** and never crosses `BORE_HALF_WIDTH` over 300 ticks, while one with a clear line at that distance goes to **pursue** and catches you |
| **F3** | 32 | **Wayfinding.** The section letters are `manhattan % 5` — a distance ring wearing a name. `docs/11` §5.4 states three requirements and this meets one; **review 28 found the first rewrite dropped the third**, which §5.4 calls the whole scheme: *"the generator places sections in alphabetical order along the spine. Without this the letters convey no direction and the whole scheme is set dressing."* Five letter meshes exist (`H I M O T`) | Two captures at two different signs showing **two different names**, each `LETTER · NAME` in the game's own stencil, with a **cap height ≥ 24 px at 1920 × 1080** from the nearest position a player can stand, legible at 4×; **and** `layout.rs` places sections so the letters run in **alphabetical order along the spine**, asserted by a test over the generated graph. A pointer to the way out is optional and is **not** a substitute for the ordering | ⬜ |
| **F4** | 12b | **The torch beam in the air.** `docs/05` M3 exit gate 5 names it the biggest remaining visual step. **Review 28 re-derived the shipped fog and the first rewrite's numbers were impossible**: at `density 0.055, start 1.5`, exponential-squared gives **0.68% at 3 m** and **3.6% at 5 m** — the air a torch crosses is essentially clear — and `fog.colour` is `0.02 0.028 0.024`, near black, so the same medium cannot both absorb to black and glow | **Decide first, in one line in this row rather than mid-session:** whether the scattering coefficient is `fog.density` or a new `Environment` field, and where its colour comes from. Then: two captures from `playing.snapshot`, beam on and off. **(a)** In a named ≥ 100 × 100 px crop centred on the cone, lying on a region reading < 60 luma with the beam off, the mean rises by **≥ 12 levels**. **(b)** The delta **falls monotonically along the beam axis** across three crops at increasing distance — a cone, not a lift. **(c)** In a named crop containing no light's cone, the mean moves by **≤ 3 levels**. **(d)** An `Environment` that does not ask for it produces a **byte-identical** capture | ⬜ |
| **F5** | 33 34 36 37 25 + call plate + R2 | **The objects you stop in front of**, merged from seven rows. **The merge is upheld — one pass, one submission, one review — and review 28 rejected the merged *condition*:** four sentences of universal prose deleted the rows and ranges that already existed, and one clause (*"nothing brighter than the lining"*) was **wrong art direction**, since the lamp, the tube and the accent must all be brighter than the lining. The sub-conditions below are the original rows', verbatim | **All of these pass in the same review.** **(a)** Mean adjacent \|ΔL\| ≥ 3.0 on `w_pm30` row 600 x 1150–1270 **and** on `w_y180` row 550 x 900–1020, with the far bulkhead's plate joints visible through the dirt. **(b)** The crate patch `(1080, 470, 420 × 300)` means below the lit lining's 71.2, **and every crate face carries a stencil**. **(c)** At least three of the key board's four orange bars unbroken across their full length from `at_key`. **(d)** No non-interactive surface in `at_key` exceeds `accent`'s saturation, as (R−B)/R over patches ≥ 20 × 20 px at named crops. **(e)** The call plate is modelled — minimum on-screen dimension ≥ 40 px from `at_exit`, with ≥ 20 levels of prominence at its own edge. **(f)** R2's bunks: 12 instances over **≥ 3** dressings. **(g)** The reticle is readable against what it sits on: measured at `at_exit` it is a 4 px dot at luma **122 against a door reading 200–233** — *darker* than its background and invisible at the game's final objective — so it needs a contrasting outline or halo, and ≥ 60 levels of separation from the mean of the 12 px surrounding it in **all** landmark captures | ⬜ **NOT PASSED (reopened r30 — six of seven; (a) failed on the screed at a derived 2.32 m, 2.19 against 3.0).** Review 30 **retired** the `w_y180` crop with arithmetic — at 12 m the fog term is 28.3% toward a near-black colour and a surface behind that, several mip levels down, cannot carry per-texel grain — and **held the other by deriving its distance**: eye 1.59 m, focal 771.2, row 600 at pitch −30 gives **2.32 m**, inside the 3 m the clause names. Its cause: *"the fix went into the lining and not into the floor"*, `shelter_floor.png` measuring 2.82–6.18 at native against the lining's 10.0–12.7. **Fixed in s26 by ordered change 7**: `Surface::aggregate` is a **continuous** per-texel term applied straight to the albedo, separate from `grimy`'s thresholded `max` because grime is patchy and concrete is not. The floor is **3.24 → 11.54 at native**, level with the lining, and the three rewritten crops now read **9.73 (bar 4.0) / 3.40 (bar 3.0) / 3.97 (bar 3.0)** — all pass. The other six clauses were credited by review 30 without reservation. **(f) bunks — done and counted:** a third dressing, `bunk_rolled`, so the generated level is **6 stripped, 3 made, 3 rolled** against R2's 12-over-2. It is a third *history* rather than a third look — made means somebody was living here when it stopped, stripped means somebody cleared it out afterwards, **rolled means somebody left properly and expected the place to be used again** — and which berth was rolled alternates by cell, so it is a property of the level rather than a rule a player can read. **(g) the reticle — done and measured:** a `Surface` halo behind an `Ink` dot, because no single colour holds 60 levels against both a lit door and a dark bore. At `at_exit` the halo reads **3 against a surround of ~199**; at `playing` the dot reads **225 against ~62**. Both frames carry a component well past the bar, in opposite directions. **(a) the grime — the cause was found and it was not the renderer.** The texture measured **0.82 mean adjacent |ΔL| at NATIVE resolution**, range 89–107: a wash, in the source. Reviews 20, 22 and 25 each measured the render and concluded the map was right and something downstream flattened it. **`grimy` called `tiling(.., 704)` on a 1024 map under a comment claiming per-texel grain, and gradient noise is exactly zero at every lattice point** — at 1.45 texels per cell it returns almost nothing. `amadeo-texture::noise::speck` is a per-texel value hash instead, and the same row now reads **3.24**. In the render, near lining in `w_y180` reads **7.41** and **4.19** against the 3.0 bar. **The two named crops still do not pass and the honest reason is that neither is at 3 m**: `w_pm30` row 600 is 1.55 → **2.19**, and `w_y180` row 550 x900–1020 lands on the **far bulkhead about twelve metres away**, where mipmapping has correctly averaged per-texel grain out — 0.60 → 0.65. A clause asking for per-texel contrast at twelve metres is asking mipmapping not to work; the surfaces at the distance the clause's own preamble names now pass by a wide margin. **(b) the crates — passes, and the cause was their material.** They were painted `fittings`: **the same pale institutional steel as a light fitting**, which is why they were the brightest thing in the shelter at 117.3 against the lining's 71.2. They are battened timber now (`crate_board`), measuring **45.5 against the lit lining's 115.3** — 0.39x rather than 1.65x — and each carries a stencilled letter on **two** faces. **(c) the key board — passes at three of four.** The board was mounted flush with the lining plane and the plates stand proud of it, so a rib ate the whole right vertical. It is `BOARD_PROUD` off the wall now, the way a notice board is actually screwed on, and the four bars read **92 / 81 / 99 / 59 per cent** along their lengths; the 59 is the bottom bar, crossed by a bunk in the foreground, and three unbroken is what the clause asks for. **(d) saturation — passes at comparable exposure.** Every non-interactive surface in `at_key` sits at **0.16 to 0.29** on (R-B)/R; `accent` reads **0.876**, three times the highest of them. It reads **0.223** on the bar the torch is hardest on, and that is the tonemapper desaturating a highlight rather than a second orange in the frame — the clause compares patches without controlling for exposure, and both readings are published. **(e) the call plate — passes.** It was a flat `sign_plate` scaled down and painted orange. It is a modelled cast housing with a proud button now, measuring **50 x 59 px** with **78 and 57 levels** of prominence at its own edges, against bars of 40 px and 20 levels. |
| **F6** | 26 | **The ear.** `docs/11` §9 makes occlusion a *gameplay* requirement rather than polish: *"a warden exactly as loud through a wall as through a doorway makes the whole mechanic a lie"*. **Review 28 moved it out of last place** — it was sixth of six while being the only one of the six the design calls gameplay, which makes it the row most likely to be dropped when the budget runs out. It also corrected this file: the submission claimed *"there is no music at all"* and **`assets/pieces/ambience.scene` plays `warren_tone` on `Bus::Music`**, looping, forever. What is absent is anything **reactive** | **Decide first:** how `collect_audio` learns a wall is in the way. `amadeo-audio` does **not** depend on `amadeo-physics`; a new edge is hard to reverse, so `CLAUDE.md` §5 makes it **Justin's**, and it wants an ADR before the row starts. Then, headlessly in `VoiceTracker`'s terms: **(a)** the same spatial voice at the same distance reads a gain **≤ 0.30×** (≈ −10 dB) through a bore wall against a clear line, and the transition across a cross-passage opening is continuous — no tick changes gain by more than 0.15×. **(b)** `footstep` resolves to a **different clip on timber duckboards, on screed and in standing water**, asserted by the id the event carries — there is one clip today and the game has three floor surfaces. **(c)** `warren_tone`'s gain is a function of distance to the nearest warden, spanning at least 2:1 between `WARDEN_SIGHT` and arm's length, and restores correctly from a snapshot | ✅ **r30 — `it_makes_a_noise.rs` (a)/(b)/(c); `playing.snapshot` carries `occlusion 1.0` on the warden and `gain 0.55` on `warren_tone`. The low-pass stays open as the descope ADR 0086 names, and review 30 recorded that `docs/11` §9 is NOT thereby satisfied: gain-only at 0.30x is −10.5 dB, which is arithmetically indistinguishable from the warden standing 1.8x further away.** **(a)** `AudioSource::occlusion` (ADR 0086) is a hashed scalar the audio crate owns and `amadeo-app::occlude_voices` fills, so `amadeo-audio` gained no dependency. `a_wall_makes_the_warden_quieter_than_an_open_line_at_the_same_distance` holds the warden at the same range twice — once along the bore, once beyond the lining — and reads **0.30× against 1.00×**, which is `docs/11` §9's number exactly. `occlusion_never_jumps_in_one_tick` flips the cast's answer in a single tick and asserts no tick moves gain by more than 0.15 across ninety of them. **(b)** three clips replace one: `step_screed`, `step_timber`, `step_water`, chosen by a `Footing` volume authored beside the surface it describes — **a downward cast cannot work here, because the duckboards have no collider** and the only one in a bore section is the slab beneath, so a cast reports screed where a player is audibly on timber. **(c)** `voice_the_warden` leans `warren_tone` from **0.55 to 1.20** between sight range and arm's length, written into a hashed field so `the_bed_survives_a_save` finds it in the snapshot text. **Designer decision 10 is in here too**: the warden **treads** rather than breathing until it sees you, because a thing that breathes continuously reads as an animal and `docs/11` §3 says it is an institution — so the change of sound *is* the moment of being noticed. **Two things not built, and named rather than hidden:** the **low-pass** ADR 0086 records as the scalar's intended consumer is not wired — gain-only is the descope the ADR names, and the field does not change when it comes back — and `EARSHOT_PROBE`'s cost has not been measured against `docs/10`'s budget |

**Order, set by review 28: F2, F2b, F6, F5, F4, F1, F3.** The warden's form and the warden's respect
for the level are one object and one session. **F6 goes third, not last** — it and F2b are one mechanic
and one architectural question, and the row the design calls gameplay must not be the one that falls
off the end.

### The budget is two to three FULL CONTEXT WINDOWS, not two to three exchanges

Justin clarified this in session 26, after watching the first re-scope read it too conservatively:

> *"by finish this demo game in 2-3 sessions you know I mean 2-3 full context windows right?"*

That is a substantial budget, and it changes how the fallback below should be used: **it is a
fallback, not a plan.** Do not pre-emptively descope a row because it looks large — descope only when
a window is genuinely running out. The re-scope's purpose was to stop thirty-four rows consuming six
sessions, not to make seven rows cheap.

### What ships if the budget runs out — decided now, while it is a decision rather than fatigue

Two-to-three sessions is not elastic and review 28 required this be stated up front:

- **Mandatory: F2, F2b, F5, F1.** The antagonist, the props, the opening and the ending.
- **F6 may descope** to occlusion and footstep surfaces, without the reactive bed.
- **F3 may descope** to §5.4's requirements 1 and 3, without a pointer.
- **F4 is the stretch and is the row allowed to slip.** Highest value and highest risk in the set, and
  a half-built volumetric pass is worse than none. `docs/13` §3 does not require it.

### On the audio nobody here can judge

Review 28 declined to rule on six generated clips it cannot hear, and named what **Justin** must check
once, on headphones, so it cannot be hand-waved:

1. Does `warden_breath` read as a creature, or as filtered noise?
2. Does `warren_tone`'s level leave silence audible, per §9's *"near-silence is the default… so that a
   single sound is an event"*?
3. Do `caught` and `escaped` read as two different endings?

**Any "no" is a row.** Nothing else in audio is.

### On "hundreds of tests on singular things"

The workspace holds **1,760 `#[test]` functions**. That is not itself the defect — a deterministic
engine earns its determinism tests, and item 5 deliberately requires a capture assertion per
primitive. The instruction is read as a rule about **new** work, and adopted as one:

> **A FINISH row ships with the capture that closes it and with nothing else.** No per-object unit
> test, no helper test, no second assertion of something already asserted. If the capture named in
> the close condition would not catch the regression, then the close condition is wrong — fix that
> instead of adding a test beside it.

**The one exception is F2b**, which is a headless test rather than a capture, because a warden walking
through a wall is a *simulation* defect and a photograph of one moment cannot prove three hundred ticks
of it.

**Existing green tests are not deleted.** Deleting one to make a number smaller is the same category
of mistake as moving a knob to chase another knob's symptom (`docs/14` §4 #2).

---

### Where session 25 got to

**Item 24 closed.** Reviews 22–27 took it from *"a near miss"* to a ✅ written by a review with its own
evidence. What landed across them: **ADR 0085's sphere-light falloff** (118,040 clipped pixels → 0),
`MAX_SHADOW_SPOTS` 2 → 4, the **cross-passage stagger** that removed `docs/11` §5.3's forbidden
sixty-metre sightline, **timber duckboards** broken into six variants, **conduit down both haunches**,
per-texel grain and normal maps that are no longer the identity, an **orange-edged key board**, and a
**way out that is a door** — its own steel surface, a 50 mm reveal, a closed handwheel that casts onto
the leaf, and its own lamp actually aimed at it.

**The next thing is `games/scarp` (item 39).** Five consecutive reviews recorded it and none could act
on it while item 24 was open. It is an M2.5 exit gate and it now reads as the thing review 12 condemned
`games/warren` for being.

### Where session 24 got to

**Review 19 is the fifth delivered review of item 24 and it returned NOT POLISHED**, with a finding
no previous review had named: **`games/warren`'s tunnel lining is lit by the ambient probe and by
nothing else**, so a twelve-metre bore is painted one flat value at every depth — the far end
measurably *brighter* than the near. Two of review 17's four open items turned out to be the same
defect seen from two angles; the other two were re-filed off item 24 entirely (**new item 32** for the
section letters, **item 18** for the warden). Its full record, its three A/Bs and its rewritten
close condition are in `docs/14` §8.

**It also credited ADR 0084 independently** — nine frames, minima 10–19, not one pixel at
`RGB(0,0,0)`, eight of nine with zero clipped pixels — and passed three of the old condition's five
clauses outright, including *"it is a cast-iron-lined tunnel"* and the hand lamp.

### Where session 23 got to


**Review 14 was a *planning* review** — the first one taken before any code was written — and it
returned NOT POLISHED on the plan with ten ordered changes. Its full record is `docs/14` §8. Session
23 built the revised plan: items **31** and **24** are now both 🟡 awaiting a verdict, and item 24's
geometry half is the thing twelve reviews had not moved.

**The next three, unchanged from review 13's order:** item 21's screen-space half, item 22's
shadow-edge dithering, item 15's title screen.

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

| Measurement | Review 1 (s20) | Review 11 claimed | Review 12 | Review 13 / s22 | **End of s23** | Target |
|---|---|---|---|---|---|---|
| `.mesh` assets that are box-only | 23 / 23 | ⚠ 23 / 31 | 24 / 37 | 24 / 39 | **11 / 53** | a game whose meshes are not all boxes |
| — of which `games/atrium` | — | — | 9 / 22 | 9 / 24 | 9 / 24 | (a demo; does not satisfy the target) |
| — of which **`games/warren`** | 13 / 13 | — | 13 / 13 | 13 / 13 ⚠ | **0 / 27** 🟡 | **this is the number that matters** |
| Material `base_colour_texture` empty | 15 / 15 | 11 / 14 | 11 / 14 | 7 / 14 | **7 / 19** | a game whose materials sample textures |
| — of which **`games/warren`** | 6 / 6 | — | 6 / 6 | 2 / 6 ✅ | **2 / 11** | **this is the number that matters** |
| Material `normal_texture` empty | 15 / 15 | 11 / 14 | 11 / 14 | 7 / 14 | **7 / 19** | ADR 0047 has content in two games now |
| Material `metallic_roughness_texture` empty | 15 / 15 | 11 / 14 | 11 / 14 | 7 / 14 | **8 / 19** | ADR 0048, likewise |
| Mutating agent-protocol methods | 0 / 17 | 0 / 17 | 0 / 17 | 0 / 17 | 0 / 17 | deferred to M4 — see item 20 |

**Review 19 recounted these and the drift this time was the implementer's.** The submission it was
given claimed `0 / 25` and `2 / 10` for `games/warren` against a true **`0 / 27`** and **`2 / 11`**,
because the numbers were copied out of this table rather than counted off the filesystem. `docs/14`
§3 #6 exists to stop a *reviewer* doing that, and nothing had said it applies just as hard to whoever
writes the submission. **Recount before quoting, in either direction.** The numerators have been
honest for four reviews running; it is the denominators that move, because a denominator grows
quietly every time an asset is added.

One reading recorded here so a later review does not find it as a defect: under a strict *made of
nothing but boxes*, **14 of `games/warren`'s 27** qualify — the five section letters, both sign
plates, both bore walls, the deck, the cross-passage, the flood plane and the crate. That is not the
metric review 1 set, review 19 declined to change it, and it is worth knowing before `0 / 27` is
quoted as meaning more than it does.

### The tracked measurements have saturated, and these three replace them

`games/warren` is **0 / 35** box-only meshes, and **14 / 14 non-emissive materials are textured on all
three slots** — of sixteen, `base_colour_texture` is empty on two and `metallic_roughness_texture` on
three, and those are `glow`, `glow_dead` and `flood`, which are an emissive tube, its dark twin and
standing water. **This sentence previously read "0 / 16 on all three slots, no material leaves one
empty", and that was false**; review 26 recounted and corrected it. The honest form is review 24's, and
the lesson is the one this table exists for: a summary written from memory of a count is not a count. Neither number can move again, and review 24 pointed out that a metric which
only ever improves is one nobody is counting. Review 25 specified the replacements mechanically, and the
answer to *"does a prefab placed fourteen times score 14 / 1?"* is **no — score it by distinct override
sets**, because fourteen bore sections in fourteen different conditions is content, not repetition.

| # | What is counted | Target | Today |
|---|---|---|---|
| **R1** | **Intra-mesh repeat on wear-bearing props only** — every `repeat` block in a prop that should have an individual history, by `count` against **distinct variants** | nothing repeated more than six times with fewer than three variants | `duckboards.mesh` is **27 slats over 6 variants** |

**R1's first form was wrong in kind and review 26 corrected it.** It flagged any `repeat` with `count > 6`, and the Today cell claimed `duckboards.mesh` was the only one over the line. Recounted: **13 blocks in 9 assets** exceed it — `bore_crown` ×5 at 20, `bore_side` at 20, `bunk_frame` at 10, `conduit_run` at 9. But twelve of those are **manufactured** repetition: a cast-iron lining genuinely is twenty identical segments, and conduit clips genuinely are at a fixed pitch. **Repetition is only a tell on objects that should have individual histories** — a duckboard walked on for forty years, a crate somebody packed — and a metric that cannot tell a manufactured run from a hand-made one flags the tunnel for being a tunnel.
| **R2** | **Placement repeat, scored over the piece FAMILY** — every family instanced more than three times, by *N* and its number of distinct pieces or dressings | nothing placed more than six times has fewer than three variants | signs **14 / 5**, bore sections **14 / 6**, fittings 28 / 2 and cross-passages 16 / 1 (both manufactured), **bunks 12 / 2 — fails** |
| **R3** | **Objective legibility** — each `Interactable`'s collider AABB projected through the camera of the snapshot named for it, **and** that of any `accent`-material sibling in the same instance | the **marker** is ≥ 40 px on its minimum dimension **and ≤ 25% occluded** by nearer geometry | key board 347 × 230 px but its bars broken — see item 36 |

**R2 must be scored over the family, and review 27 found out why by counting it.** All **160** instances
in `generated.scene` carry exactly one override — `Transform` — so scored by override set every prefab is
*N* / 1 and the metric answers trivially. Variation in this level is expressed by **choosing a different
piece**, not by overriding one. Scored by family it finds the real defect: **twelve bunks over two
variants**, which review 17 had already named — *the bunks were perfectly made, identically, everywhere*.

**R3's occlusion half is not optional and is the whole reason it is two numbers.** The key board passes
40 px by a factor of five and is still eaten by a lining plate and a bunk; without the second clause the
metric would have reported it clean, which is exactly the failure the mesh and material counts ended in.

**End of session 24, counted off the filesystem rather than carried forward.** `games/warren` is
**0 / 27** box-only meshes and **3 / 14** untextured materials; the repository is **11 / 53** and
**8 / 22** on `base_colour_texture`, **8 / 22** on `normal_texture`, **9 / 22** on
`metallic_roughness_texture`.

**The material row moved the wrong way and it is recorded rather than smoothed over.** Session 24
added three materials — `bulkhead_exit` and `greatcoat`, both textured, and `glow_dead`, which is not.
So the numerator went 2 → 3 while the denominator went 11 → 14. `glow_dead` is the dark twin of
`glow`: a 4 cm fluorescent tube that is *off*, whose whole read is one flat dull grey, and a texture
on it would sample six identical texels. That is `games/atrium`'s `amber` argument and `glow`'s own,
and it is the third instance of it — but a metric that only ever improves is a metric nobody is
counting, so it goes in the file as a rise.

**Review 15 confirmed both `games/warren` rows independently.** It also credited the geometry in terms
worth keeping -- *"it is a tunnel"*, and the pitched-up frame *"the best frame this project has
produced"* -- and then failed the item on what was laid over it: a fitting clipping 27,659 pixels, a
colour grade cancelling the palette, and props that were literally `RGB(0,0,0)`. Session 23's second
half is its eight ordered changes; `docs/14` §8 has the measurements.

**Session 23 moved the mesh row for the first time in thirteen reviews.** `games/warren` is
**0 / 25**: the thirteen boxes are deleted and twenty-five `CompoundMesh` assets replace them — the bore
crown with its modelled flange rings, the deck, side walls solid and open, bulkhead heads, the
cross-passage and its blind cap, the haunch fitting and its tube, a two-tier bunk frame and a
mattress, a battened crate, an enamel section plate, a dogged bulkhead door, the hand lamp, the brass
key, and the warden as a non-articulated greatcoat silhouette with the lamp it carries.

**The material denominator moved 6 → 10 rather than staying put**, and the two figures are not the
same two: `rust` is gone (the warden wears `bulkhead` now, which is textured), and `signage`,
`accent`, `lining_wall` and `ticking` are all new and all textured. The one new untextured material is
`glow` — a 4 cm fluorescent tube whose whole job is `emissive`, which is `games/atrium`'s `amber`
argument exactly.

<details>
<summary>Session 22's note on the material row, kept for its reasoning</summary>

**Session 22 moved the row that matters, for the first time in thirteen reviews.** `games/warren`'s
materials went **6 / 6 → 2 / 6** on all three slots: its palette was `plaster`, `carpet`, `timber` —
a Victorian interior authored before `docs/11` existed — and is now §5a's cast-iron ring lining, dust
over concrete, institutional steel and lead-grey bulkheads, each sampling three generated maps. The
two left are the brass key and the warden's post, both small props.

**The three texture slots are tracked separately on purpose.** A single "43 of 45" read as nearly
finished when the honest state was that **one of three texture paths had a user**. All three have one
now, in `games/atrium`; the three materials that remain empty there are `amber`, `rust` and `glass` —
a lamp filament, a painted metal box and a pane, none of which would gain from a slab map.

**The denominator fell from 15 to 14** because ADR 0078's amendment deleted `plinth_stone.material`:
two materials for one stone, differing in one number, which was the evidence that texel density was
only half solved.

</details>

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
| 21 | 1 | **Ambient occlusion, both halves — the engine has none of any kind.** No SSAO, no GTAO, no baked AO, and `Material` has no occlusion slot; `mesh.wgsl` samples the ORM texture and uses `packed.g` and `packed.b` while **discarding `packed.r`, which is glTF's occlusion channel**. The consequence is in every frame: a pillar meets the floor with zero darkening, a table leg meets the floor with zero darkening, and a two-wall corner reads the same value as the flats. It is the largest single reason the output reads as composited rather than rendered, and Godot, Unity URP and Unreal all ship it on by default or one checkbox away. **Two halves, and both are wanted**: (a) read `packed.r` and generate a cavity/occlusion channel in the texture generator from the height field that already exists, which darkens joints and slab edges for one shader line; (b) a screen-space pass in the render graph for the corners and object contacts no texture can bake | engine, then both games | A capture shows measurable darkening where two surfaces meet that is absent with the pass disabled, at a named crop, in both games — and an authored-off scene is byte-identical | 🟡 **the baked half works; the screen-space half STAYS OPEN (r13).** Baked passed: the ORM red channel carries 255 on a face and 145-183 at a joint, the shader reads it, and the strength dial is mutation-checked. **The screen-space pass does not reach a contact.** Review 13 probed all four the item names -- lamp base, table leg, plinth base, pillar base -- and measured **0 to 3 levels**, two of them byte-identical. The 30 levels are at the pillar's convex *arris*, a silhouette, where they read as a banded comb; `occlusion.wgsl` own comment predicts that fringe and asks to be revisited when one appears. Three named fixes: a **depth-aware blur**, a **support radius on the baked cavity** (it is a step function coincident with the joint line, so it darkens a dark line and adds no relief), and a way for the pass to touch more than ambient -- URP and Unreal both ship a direct-lighting knob because a purely-ambient AO is invisible in a sunlit room. Then author it in `warren.environment`, where a dark interior will show it. **Cost is now measured**: 275 µs of 616 at 1280x720 (`docs/10`)  — ➡ **GAME 2 (s26)** |
| 14 | 2 | **No light with no readable cause in the frame** — amended by review 13 from "every light has a fixture and every fixture has a light", which is a *heuristic for* the defect rather than the defect. `games/atrium`'s `vestibule_light` carries no `Mesh` and is ruled **legitimate**: it stands for the sky beyond an opening, the chamber reads as a place, and column x=960 runs 191/176/149/187/204 down its height -- a real light distribution rather than a flat glowing patch. Sky portals in Unreal and `env_lightglow` in Source do exactly this. The original wording, kept because the two failures it names are both real: **every light has a fixture and every fixture has a light**, plus `gloom.rs`'s two-tone seam. Named by reviews 2, 11 and 12 and still failing on **three of five lights in `games/atrium`**: `lamp` (`PointLight 22.0`), `lantern` (`SpotLight 38.0`, shadow-casting) and `vestibule_light` (`34.0`) carry no `Mesh`, and `lamp` blows `pillar_nw` to clipped white from empty air 1.4 m away. The inverse also holds: `lamp_fitting` is the one light with a fixture and at `intensity 6.0` the floor reads 198/200/202/211 straight past it — **no pool, no local peak**. The defect is invisible reading a scene file top to bottom, because it is a component that is *absent* | atrium, then warren | No entity carries a light without a `Mesh` or a `Mesh` fixture without a light; a floor scanline through every fixture shows a local peak of at least 20 levels; and a wall is a smooth gradient at every angle | 🟡 **stays open (r13), and the fix it names is landed.** The structure passed -- the pendant and the lantern are `CompoundMesh` fixtures and the lantern's light is a child so roll sweeps the beam. But the intensity fix was **dead data**: `lamp_flicker.anim` drove the same field and the scene's number had no effect, proven byte-identical. Fixed in the clip in s22, and the pendant re-hung at 2.05 m over the table rather than 3.9 m level with a pillar -- the pillar crop `(400,150,200,300)` goes 6633 clipped pixels to **zero**. **What is still open is `gloom.rs`'s two-tone seam**, untouched and still the strongest line in every Warren frame  — ✂ **CUT (s26)** — see §1b |
| 13 | 3, 4 | **A texture generator, as an engine deliverable rather than a game's binary — and it must not emit a square lattice.** `docs/12` §3 rates textures "Partly": `pix` writes pixel art from hand-written text and does not reach a 512² tiling plaster with a normal map. `amadeo-noise` is already deterministic and already banned from `sin`/`cos`/`powf`. Needs Worley, brick and tile lattices, gradient ramps, height-to-normal, and roughness/metallic packing into the glTF channel layout. **Review 12's rank 4 folds in here rather than being its own item**, because it is a defect *of the generator*: `surfaces.rs:49` is `const SLABS: u32 = 4` — a 4 × 4 square lattice, stack bond, identical slab sizes, joints running continuously in both directions and repeating every tile. It is on the floor, the walls, the pillars, the galleries, the ceilings and the plinth, so it is the single biggest machine-made tell in the project, and the room reads as a municipal swimming pool. Needs a **running bond** with a half-slab course offset, **at least three slab widths per course**, a low-frequency **macro-tint at a different period from the slab grid** so the repeat does not land on the joint grid, and **grime accumulating in the joints**. *Not* stochastic hex-tiling — that technique is for noise-like textures and explicitly does not handle regular geometric patterns | engine, then warren | A game's textures are generated from committed text, including a normal map, a packed metallic-roughness map and an occlusion channel, the generator is engine code with its own tests, and **no two ADJACENT courses in a 512² tile share a joint line** | 🟡 **anti-lattice clause ✅ r13; the rest stays open.** Review 13 measured joint positions per course and found the closest approach between ADJACENT courses to be 20 px, with none shared — three non-adjacent pairs coincide, which real masonry does constantly, and the condition is amended to say so. **What it kept open is the part that matters**: `stone_slab.png` is min 109 / max 212 / mean 195 with 69% of pixels in one 16-level bucket, and ignoring the joints the whole slab field spans 194-210 -- a 6% range where a stone wall wants **25-40%**. Every slab is a uniform rectangle separated by a one-pixel line; there is no chipping, no staining, no differential weathering. *"It is a diagram of a wall."*  — ✂ **CUT (s26)** — see §1b |
| 22 | 5 | **Shadow edges do not step.** `c_shadowedge.png` at 5× shows the sun band's boundary on a wall as a run of discrete 8–20 px steps, on the most prominent lighting feature in the best frame this project has produced. `shadow_resolution 2048` over `shadow_distance 24` is 23 mm/texel, and the fixed 3 × 3 grid PCF at `mesh.wgsl:313` blurs *across* the edge while preserving the texel grid's step positions — **an axis-aligned kernel cannot break an axis-aligned quantisation, so a larger one will not fix it.** The mechanism is a **rotated Poisson disc with a per-pixel rotation** from interleaved gradient noise on the fragment coordinate, which is what Frostbite, Unreal and CryEngine all use, and which turns a staircase into dither the eye reads as softness. **This is what remains of 12h**, which review 12 rescoped: it is not contact hardening (PCSS grows a penumbra with occluder distance, a different and larger want) | engine | A 5× crop of the Atrium's sun band on a wall shows no run of more than 3 px at one value along the edge, and the floor shows no acne at four probed points | ⬜  — ➡ **GAME 2 (r28)** — moved out of CUT: an axis-aligned kernel cannot break an axis-aligned quantisation, and M4b is a daylight game under one directional light |
| 13b | 6 | **Bloom and fog are authored at values that do nothing.** Measured: the standing lamp's halo is **3 px** and lifts 15 levels at 8 px, indistinguishable from edge antialiasing; the doorway, the brightest thing in the room, goes 60 → 229 **in five pixels with no halo either side**; the sky/wall boundary has no glow at all. At `threshold 1.0` with `exposure 0.8` and ACES almost nothing in this scene exceeds threshold in HDR. And fog: `factor = 1 − exp(−(d·density)²)` at `density 0.016, start 3.0` gives the far wall of a 20 m room `1 − exp(−(0.016 × 17)²) = 0.071` — **a 7% haze, 12% at the far corner**, which on a shadowed wall is about nine levels | atrium, then warren | A bright source shows a halo **≥ 12 px at 1080p**, and the Atrium's far wall sits **≥ 25 levels** closer to the fog colour than the near wall of the same material | ⬜ **reopened r12** — was marked ✅ in s21 with neither half of its condition ever captured. This is the failure `docs/14` §6 was written to close  — ✂ **CUT (s26)** — see §1b |
| 12f | 7 | **The glass does not read as glass, and the sky has no picture — one generator change closes both.** Part one landed (ADR 0080): `ALPHA_BLENDING` multiplies the whole shader output by coverage, so at alpha 0.34 a highlight was bounded above by `0.34·S + 0.66·W` and was **arithmetically impossible rather than dim**; premultiplied output took the pane from 1 level brighter than the wall beside it to 10, opaque byte-identical. Part two is that `daylight.rs:137` is `fn sky_colour(up: f32)` — **a function of elevation alone**, with no sun disc, no cloud and no azimuthal content whatever, so a reflection in it cannot move when the camera yaws and the pane shows a flat wedge ending in a hard diagonal where the reflected ray crosses the gradient's horizon. The same fact is why 65% of the best up-view is a bare ramp. The fix is ADR 0079's shape one level down: **structure and a sun disc into the specular chain, excluded from the irradiance convolution**, which is what keeping an analytic light out of the ambient probe means in every comparable engine | engine, then both games | The pane shows a specular sheet or Fresnel rim that **moves between two captures at different yaws**, and the sky occupying a capture is not a single monotonic ramp | 🟡 **part one done (ADR 0080); part two open**  — ➡ **GAME 2 (r28)** — moved out of CUT: `sky_colour` takes elevation alone, and an outdoor game's sky is most of every frame |
| 12c | — | **A sky for `games/atrium`.** `sky ""` blocked four things at once and the two existing `.hdr` generators were the precedent. The map is real and ADR 0079 split its two jobs (`Environment::sky_ambient`) | atrium | The Atrium names a real environment map, and the lamp, the glass and the wall gradient each read as themselves in a capture | 🟡 **built; held open behind 12f and 14** — review 12: the wall gradient reads (pass), the glass does not (12f), and the lamp reads as a glowing object but not as a light source (item 14). **Two of three.** Balance is now `SCALE 0.5` / `sky_ambient 0.25` / `exposure 0.8` — corrected here after `c5697e7` moved two of those silently, voiding the evidence this row used to cite. Review 12 adds a clause: **12c cannot close while the sky is a bare elevation ramp**, because it is the majority of every frame it appears in  — ✂ **CUT (s26)** — see §1b |
| 31 | r13-3 | **There is no way to capture `games/warren` in its playable state, and that blocks the gate's own condition.** Every frame any reviewer can take of it is its **title screen**: `--yaw` and `--pitch` aim the camera but cannot dismiss a menu, the protocol has 0 mutating methods, and no snapshot is committed. `docs/13` §3 defines POLISHED as *a frame from a real game* — **and nobody can currently take one.** Found by review 13 while trying to. `capture --from <snapshot>` already exists, so committing a `playing.snapshot` closes it; `capture --input <replay>` is the fuller answer. **Do this BEFORE item 24**, or item 24 cannot be judged | warren | A reviewer can capture `games/warren` mid-run without editing the game, and the route is one documented command | 🟡 **built, awaiting verdict** (s22) — `cargo run -p warren --bin moment` writes `snapshots/playing.snapshot` and `capture --from` photographs it, which needed no engine work because ADR 0028 already restores before drawing. The snapshot is **text**, 4147 lines, so a person can read where the player was standing. Documented in `docs/14` §5  — ⏳ **no bucket, and it does not need one (r28)**: 🟡 since s22, used by six reviews and closed by none. It needs a verdict, not work |
| 24 | 3 | **`games/warren` stops being boxes.** The vehicle change made concrete: 13 of 13 box meshes and 6 of 6 untextured materials, unchanged across eleven reviews, in the game with the passed design, the fiction, the objective and the milestone attached to it. Everything above lands here once it exists. `docs/11` supplies the direction rather than taste — cast-iron rings, safety orange, warm hand-lamp against cold fittings, pooled light with real dark between | warren | **Rewritten by review 19**, because *"real dark between them"* had been argued four ways by four reviews and each measured a different surface. A 1920 × 1080 capture from the authored camera of `games/warren` **in play**, in which: **(a)** the luma histogram puts no more than **65%** of pixels inside any one 64-level band and the frame's maximum exceeds **200**; **(b)** the left lining wall spans **≥ 60 levels** along a horizontal profile of its full visible depth, with the near quarter brighter than the far; **(c)** mean `|L(x) − L(1919−x)|` on row 600 exceeds **30 levels**; **(d)** a hand-lamp pool at **R − B ≥ +15** and a fitting pool at **G − R ≥ +8** are both present in the same frame; **(e) — RETIRED by review 26.** It read *"at least one prop that is not lining, deck or crown occupies ≥ 3% of the frame area"* and three reviews running closed it on inspection, *"and a clause closed on inspection three times running is a clause doing no work"*. It was written when the open question was **are there any props at all**, and that question is settled: the duckboards clear 3% on their own without saying anything about whether they are worth looking at. **R3 measures what now matters and measures it better.** Not replaced. **(f)** an object darkens a lit surface by **≥ 30 levels within 20 px** at a named crop, and the shadow's shape is recognisable as the object's. **(g) — added by review 25, pinned to screen coordinates by review 26, and it decides this item alone.** **(g1)** On **row 480** of a 1920 × 1080 capture from `at_exit.snapshot`, the **maximum prominence within the joint band** over ±12 px at the **left** and **right** leaf-to-frame joints is **each at least twice** the greatest prominence in the leaf's interior. **"Maximum within the band", not the trough's global minimum** — review 27 found row 480's left trough gives 56 at one pixel and 46 at the next, so a reviewer taking the minimum gets 1.84× and one taking the maximum gets 2.24×, and only the second is consistent with the interior figure, which is also a maximum over a band. **(g2)** No plate joint crosses the leaf boundary. **(g3)** The leaf's mean luma over its own bounding box exceeds that of an **equal-area** patch of wall either side.

**Why (g1) names pixels.** Review 25's wording cost review 26 an entire pass: *"the joint between the door leaf and its frame"* was measured by the implementer at **x 930–1010**, which contains the **frame-to-wall** band at 992–998 and does not contain a leaf-to-frame joint at all. Review 26 located the boundary three independent ways — by projection from the snapshot's camera, by horizontal profile and by column — and found the real leaf-to-frame joint had a prominence of **zero**. **The leaf runs 549–960 (557–951 after the rebate narrowed it), the stiles 531–549 and 960–992, and the band at 992–998 is the bulkhead.** A close condition that can be satisfied by measuring the wrong object is not falsifiable, which is what §6 requires.

**And (g1) is narrower than it looks — review 27 profiled ten rows and published all of them.** Only
440 and 480 pass, and it credited the clause anyway because the reason is structural rather than a
lucky row: the door's three dogs sit at world y 1.8 / 1.2 / 0.6, which project to screen rows **≈ 480,
660 and 845**, and *"a dogged door's joint is supposed to disappear at its dogs"*; rows 560–900 pass
through the handwheel, whose relief legitimately out-prominences a joint, and **(g2) tests the
plate-joint question directly**. Cite the clause with this table or not at all.

| row | left | right | interior | left ratio |
|---|---|---|---|---|
| 400 | 13 | 36 | 43 | 0.30 |
| **440** | 39 | 87 | 15 | **2.60** |
| **480** | **56** | **147** | **25** | **2.24** |
| 560 | 64 | 84 | 38 | 1.68 |
| 600 | 62 | 77 | 50 | 1.24 |
| 660 | 7 | 14 | 119 | 0.06 |
| 700 | 23 | 102 | 86 | 0.27 |
| 850 | 30 | 145 | 35 | 0.86 |
| 900 | 43 | 68 | 62 | 0.69 | Review 25's ruling on scope: *"Item 24 should keep exactly one thing: the way out. Everything else is a specific object, not a property of the game, and specific objects belong in specific rows"* — which is what items 33 to 38 are. Every clause is a command anyone can run and none can be satisfied by squinting. The measurement rows stay tracked in §1 but are no longer part of this condition — they are met | ✅ **r27 — CLOSED after twelve delivered reviews.** Evidence: `w_at_exit.png`, row 480 left/right/interior **56 / 147 / 25** (**2.24×** and **5.88×** against the 2× clause (g1) asks), leaf mean **147.1** against wall **86.6** and **27.6** on equal 370 × 540 boxes, and no wall plate-joint row landing on a leaf dip at column x 620 or x 880. Closed on **clause (g) alone**, per review 25's scope ruling that the item keeps exactly one thing. **Cite (g1) only with review 27's ten-row table beside it** — it holds at rows 440 and 480 and not at 660 or 850, because the dogs project to rows ≈ 480 / 660 / 845 and a dogged door's joint is *supposed* to disappear at its dogs. History kept: **built, awaiting verdict — four reviews (s23).** The cell is **12 m of 4.8 m arched bore**, crown 3.2 m; east/west doors are **cross-passages**, ends without doors are **bulkheads**. **13 / 13 → 0 / 25 box meshes**, 2 / 10 untextured materials. Review 14 refuted the cheap alternative by arithmetic; review 15 credited the geometry and failed the lighting over it; review 16 called the remainder *"a near miss"*; **review 17 credited eight things and then found the engine defect three reviews had been chasing one object at a time** — `grade.contrast` was a straight line through mid-grey, which crosses zero *inside* the visible range and was clamping everything below byte 44 to pure black. `games/warren` had 42.5% of a frame at exactly `RGB(0,0,0)`; it is now **0.0%** with a minimum of 15, and `games/vault`, which authors `contrast 1.15` and was losing its bottom 28%, is fixed with it. Also landed: **six section conditions** rather than three, including the ones that change what a room *looks* like (standing water, a fallen ring and its spoil, bunks re-racked as an archive); five **stencil section letters** on flag-mounted plates readable from both directions; the fitting became a **spot** with end caps, a wire guard and a conduit; and **`moment --at`** commits four snapshots so a reviewer can stand at the key, the way out and the warden post. **What review 17 asked for and is not done**: the letters convey no direction and carry no name; three of eight frames have no light source in them; the rust does not survive to render scale; and the warden still walks through walls. **Review 19 (`efdc90d`) then found the cause under two of those and re-filed the other two.** It verified ADR 0084 independently (nine frames, minima 10–19, zero pixels at `RGB(0,0,0)`, eight of nine with **0 clipped pixels**) and passed clauses [a], [b] and [d] of the old condition outright — then failed *real dark between them* on a mechanism no review had named: **the lining is lit by the ambient probe and by nothing else.** The left wall of `at_key` reads 84–101 flat over twelve metres with the **far end brighter than the near**, and an A/B at `LEVEL 0.0` takes it to literal zero for 500 px — so an ambient that is direction-only and distance-independent is painting the whole tunnel one value. **And `LEVEL 8.0` is a compensation for a defect fixed one commit later**: the 5.0 → 8.0 raise is `6d82f7e`, ADR 0084 is `0124db7`. It also measured the authored frame as **mirror-symmetric to within 4.87 levels** (`games/atrium` scores 96 on the same test) and the way out as **31.3 m from the nearest light** with an 11-level range across its face. **Re-filed off this item: the section letters → new item 32; the warden walking through walls → item 18, which already names the identical defect.** **Session 24 built all seven of its ordered changes** and item 24 stays 🟡. What moved, measured on the same commands the condition names: the ambient compensation is out (`LEVEL` 8.0 → 4.5) and **every section has a fitting** — two of them, on opposite haunches, dead or alive by whether the section flooded or collapsed rather than by a coin — so the lining is lit by things that have positions. The hand lamp gained a **housing spill** whose first attempt at 4.2 turned the opening frame into a white tiled corridor and which is 3.0 now. The player **wakes off the bore's axis facing along it**, which the level generator was not doing: the start section's only door is often a cross-passage, so the authored camera had been pointing through it down a sixty-metre straight run that `docs/11` §5.3 prohibits by name. Every generated surface gained a **grime** field, because `wear` only ever exposed what was *under* the paint and nothing modelled what had settled *on* it. The way out is an arrival: its own lamp, an orange rule across its head, an orange call plate, its own paint, and duckboards leading to it — **and `moment`'s `at_exit` snapshot was facing the wrong way**, so every measurement anybody has taken of that door, review 19's included, was of the bulkhead at the far end. The warden is a coat rather than a bollard, in wool rather than in the tunnel's own plate joints, with a spot instead of a bare bulb. Prompts follow §8 at last (`Locked`, `Way out`, `Torch`, `Brass key`). **Measured against the rewritten condition: (a) largest 64-level band 48.4% and max 225 — met; (b) left lining wall near 90.9 against far 52.7, span 60 — met; (c) symmetry 70.7 against 4.87 — met; (d) warm R−B +36 and cold G−R +10 in one frame — met; (e) a bunk frame in the authored frame — met on inspection, not measured to 3%.** Zero clipped pixels in the authored frame. **None of that is a verdict** — `docs/14` §6 reserves ✅ to a review, and the numbers above are the implementer's own and want checking. **Review 20 then ruled: NOT POLISHED, ten ordered changes, and it credited the fix on its own evidence** — at `LEVEL 0.0` the near end of `at_key`'s wall still reads 63 against the far end's 1, so ~85% of the near wall's light is punctual, and it ruled clause (a)'s 65% the right threshold with the range *earned* rather than brightened. Its remainder: **the near half of the tunnel is lit and the far half is still a grey wash**. **Session 25 built its items 1, 2, 4, 6, 8 and 10**: the near-wall patch went **61.7% → 4.7%** in one 16-level band (and the thing that moved it was *amplitude* — a fourth octave and 512 → 1024 moved it by 0.4 of a per cent); the key is 9 cm on a key board with five empty hooks rather than 32 cm balanced on its tip on a crate, which was `docs/14` §1's founding complaint verbatim six reviews on; the ambient is 2.6; the tube's emissive is restored, having been **session 24's own regression** — `over = max(brightness − threshold, 0)` was exactly zero, so the game's only emissive object could not bloom; the section letters are black; the crates are off the lining's plate. **Not built — its items 3, 5, 7 and 9**: the exit's lamp reads as four floating fragments, yaw 270 is still a sixty-metre sightline, the warden is fifteen levels from its background, and the hand lamp has no edge. **Reviews 23 and 24 then took it to the closest it has been: review 24 records that all six clauses pass, for the first time in nine passes** — (a) 57.8% and max 250, (b) span 163 on the lit wall, (c) 67.35, (d) +17/+9 same-material, (e) on inspection, (f) 30 levels closed **on a picture**, a bunk leg throwing a rod-shaped shadow with a rounded cap and contact darkening. Between them session 25 built: **ADR 0085's sphere-light falloff** (the torch was putting **118,040 pixels, 5.69% of the yaw-270 frame, at paper white**, derived from `mesh.wgsl`; now **0**, and 0 on five of six aims); the **exit lamp**, which was authored *behind the bulkhead plate* and which review 23 found by an emissive A/B and a projection calibrated to five pixels; **six normal maps that were the identity** (`relief` is a divisor tuned for surfaces carrying a lattice, so it flattened cloth and concrete twentyfold); `MAX_SHADOW_SPOTS` **2 → 4** against eighteen casters; the **cross-passage stagger** (yaw 270 asymmetry 28.4 → 69.4); **timber duckboards on bearers**; a **segmented handwheel, rebate and hinges** on the way out; **orange edging** on the key board; and a **conduit run along both haunches**, which is the first thing in the game that connects a fitting to anything. **Why it still fails, and it is not the close condition:** the item's title is *stops being boxes* and **no clause can see uniform repetition** — `duckboards.mesh` was one `repeat count 30` with no variation, in four of nine frames. Review 24 proposes replacing the saturated mesh/material counts with **repeat exposure** (count against distinct variants; the duckboards scored 30/1) and **objective legibility** (the key measured **4 � 30 px** from the camera named for it). Its remaining open items: the warden's right angles, yaw 270's near-plane clipping, the crates interpenetrating a bunk in yaw 90, and grime that blurs at 6–12 m |
| 32 | r19-1 | **The section letters name nothing and point nowhere.** Filed by review 19 off item 24, because it is a wayfinding failure `docs/11` §5.4 owns rather than a property of a frame, and a stencil alphabet is real work that should not sit behind a frame's verdict. The level carries **five letters — H, I, M, O, T — over fifteen signs**, each recurring three to five times, placed by `manhattan % 5`: a distance *ring*, so two cells on opposite sides of the start read the same and the letter repeats every fifth ring. §5.4 states three requirements — the sign carries **the letter and the name**, the docket names a letter, and the generator places sections **in order along the spine** — and this meets one. The five glyphs exist because each is its own mirror image, which a flag-mounted sign needs when the back face is made with `mirror false false true`; emitting the back copy at **(−x, y, −z)** instead is a rotation rather than a reflection and lifts that constraint entirely, so a full alphabet is available and a name becomes spellable | warren | A sign in the game reads `H · HARDY` — letter and name, in the game's own stencil — and the generator places sections in alphabetical order along the spine, so a player crossing a door can say which way is out without a map | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 33 | r25-2 | **The grime is a blur pass, and it is the largest artefact in the set.** Review 25 measured mean adjacent \|ΔL\| of **1.55** on `w_pm30` row 600, x 1150–1270 against a 3.0 bar, while row 700 of the same x-range gives 3.71 — so a surface loses its own grain with distance and nothing replaces it. In `w_y180` the whole far bulkhead is a smear that hides its own hatch. *"It is doing this at three metres."* The general blur is the wrong instrument: what the surface wants is **specific** — a tide line at one height, a wear track where boots fall along the duckboards, drip runs under the conduit clips. Frictional's surfaces in Amnesia and SOMA are almost never uniformly dirty; they are dirty **where water ran**, which is what makes a room read as having had weather in it | warren | Mean adjacent \|ΔL\| ≥ 3.0 on `w_pm30` row 600 x 1150–1270 **and** on `w_y180` row 550 x 900–1020, with the far bulkhead's plate joints visible through the dirt | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 34 | r25-3 | **The crates are the brightest thing in the shelter.** Crate patch (1080,470,420×300) means **117.3** against the lit cast-iron lining behind it at **71.2** — 1.65× the brightness of the tunnel they sit in — and at 3× they carry two faint plank lines, no battens, no strapping and **no stencil**. In a game whose whole argument is that props say who was here, the largest prop in the yaw-90 frame is a pale blank slab | warren | The crate patch means below the lit lining's 71.2, and every crate face carries a stencil | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 35 | r25-4 | **Yaw 270 is the worst frame in the game and the player's first input is the mouse.** Mean **109.7**, the brightest of nine, with the camera near-clipping a lining wall and two thirds of the frame an incoherent tangle of plate edges at 190–230 with no depth cue. The cross-passage stagger removed the sixty-metre sightline (asymmetry 28.4 → 69.4, credited); what replaced it is a wall at arm's length | warren | The spawn stands ≥ 1.5 m clear of any wall, and the yaw-270 frame from it contains a legible space rather than a near-plane surface | ⬜  — 🎯 **FINISH (s26)**, folded into **F1** — see §1b |
| 36 | r25-5 | **The key board's mark is eaten by what is in front of it.** Its orange edging measures 347 × 230 px — five times review 24's 40 px floor — and is still broken: 12–15 orange pixels on rows 520/560/600, because the right vertical sits behind a proud lining plate and the bottom-left behind a bunk, so at 4× it reads as two disconnected strips rather than an outline. **This is why the legibility metric has an occlusion half** | warren | At least three of the board's four orange bars are unbroken across their full length from `at_key` | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 37 | r25-6 | **There is a second, brighter orange in the frame that you cannot act on.** The section sign's edge-on gold at (880–980, 470–670) is warmer and more saturated than `accent` itself, which inverts `docs/11` §5a's rule — *the only orange things in the Warren are the things you can act on* — in the one frame where it matters most | warren | No non-interactive surface in `at_key` exceeds `accent`'s saturation | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 38 | r20-4 | **The warden still has right angles.** Open across five passes. Four axis-aligned boxes at 5×, one flat pale grey-green, nothing on it varying more than ~15 levels across its whole front face — outline without form. It wants a hem that flares and breaks the silhouette, shoulders that slope, a brim that is not a slab, and one asymmetry so the figure is never bilaterally identical | warren | No countable right angle in the silhouette at 5×, and ≥ 30 levels of variation across the coat's front face | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 39 | r25-scarp | **`games/scarp` is now what review 12 condemned `games/warren` for being, and it is an M2.5 exit gate.** `min 60, max 188, mean 159.5` — the entire frame inside 128 levels of the upper half. One flat-shaded green heightfield under a gradient sky, one salmon box for a player, **zero textures, zero props**, one material at one roughness at every distance. Reviews 22, 23, 24 and 25 each recorded it and asked for it to stop sitting behind item 24's verdict. `amadeo-texture` and the whole surface pipeline now exist and have never been pointed at it | scarp | A 1920 × 1080 capture of `games/scarp` puts no more than 65% of pixels in one 64-level band, its terrain samples a real material, and something other than the ground is in the frame | ⬜  — ✂ **CUT (s26)** — see §1b |
| 40 | justin-s25 | **A survival game, in the Project Zomboid line — the next demo game.** Asked for directly by Justin. `docs/00` has had Zomboid on the target list since session 7 and **no game here has ever exercised the half of the engine it needs**: `games/vault` is 2D but a single hand-authored screen, and the Warren, the Atrium and the Scarp are all 3D and all first- or third-person. This is the game that makes trap 9 real — *"letting 2D become second-class"* — and it forces, in order: an **isometric camera** on ADR 0031's existing per-entity projection, the **tilemap** (Phase D item 27), `modules/amadeo-inventory` under a real load rather than one key, `modules/amadeo-behaviour` over many agents rather than one warden, and a **needs-and-time** loop the engine has never been asked for. It is the second game to want `modules/amadeo-pathfinding` (item 18), which is what turns that from a Warren-only fix into a module with two users — ADR 0037's own rule for when something earns its place in `modules/`. **Not started, and not to be started until item 24 closes**; `docs/12` §4 is explicit that nothing proceeds until the critic passes the current piece. Its design document comes first and goes to the critic before any code, which is the order `docs/11` established and review 14 proved out | new | A design document passes the critic, and then a playable slice: an isometric block of houses you can enter, loot, barricade and be driven out of, with hunger and light level both mattering | ⬜  — ➡ **GAME 2 (s26)** — and it is no longer a demo game. Justin, session 26: it comes **after the editor** and it is **the first published game**. See §1b |
| 11b | — | **Alpha cutout**, moved here from Phase B because discarding below a threshold needs something to sample: with every `base_colour_texture` empty a cutout material cuts out a rectangle. Cheapest route to a non-box silhouette *once item 13 exists* — grating, cobweb, hanging cloth, foliage | warren | A cutout material draws its shape rather than its quad, against a generated foliage or grating texture | ⬜ **gated on 13**  — ➡ **GAME 2 (s26)** |
| 12 | — | **Particles.** Nothing in `crates/` or `modules/` mentions one. Dust is most of what makes an interior read as air rather than vacuum; it is also NMS's atmospheres, Zomboid's rain and Schedule I's smoke. ADR 0067's named-field list items are already the right format for an emitter's stages | engine, then warren | An emitter is authored in a `.scene`, it is deterministic under the fixed tick, and a capture shows lit motes drifting in front of a dark surface | ⬜  — ➡ **GAME 2 (s26)** |
| 12b | — | **Volumetric light shafts.** Named by `STATUS.md` and ADR 0073 as the next visual step and by neither plan as an item, which is how it went unscheduled. Raymarches through exactly the fog ADR 0073 added, and is what makes a torch beam visible in the air rather than only on the surfaces it lands on. **Follows item 12** rather than leading it | engine, then warren | A dark interior shows a cone of light in the air from a spot light, and off is byte-identical to before it existed | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 23 | 9 | **Normal-map mips do not renormalise.** `c_artifact1.png` at 3× shows every vertical joint on a receding parapet dissolving into a comb of horizontal dashes spreading sideways. A normal map averaged down a mip chain does not stay unit-length and the specular response goes non-monotonic. Very visible at 1080p, and it arrived *with* item 13a — the first content to exercise ADR 0047 is also the first to expose this. Either renormalise per level and push the lost variance into roughness (Toksvig / LEAN-style variance packing), or taper `normal_strength` with mip level | engine | A 3× crop of a receding normal-mapped surface shows continuous joints rather than a dash comb, and a flat-on crop is unchanged | ⬜  — ➡ **GAME 2 (s26)** |
| 25 | 10 | **`games/warren` has no reticle.** `docs/11` §8 calls this *"a usability failure at the core verb, not a polish item"* — the interaction sphere is swept along an axis the player cannot see. There is none in the game; grep confirms it | warren | A capture shows a reticle at the sweep's centre, and it changes state when `Looking::at` is `Some` | ⬜  — 🎯 **FINISH (s26)** — folded into **F5 (g)**: measured at `at_exit` it is a 4 px dot at luma 122 against a door reading 200–233, darker than its background at the final objective |
| 15 | 11 | **The Warren's title screen**, unchanged since review 2 and now measured: title left edge x=502, focus bar x=496, `BEGIN` glyph x≈505, `CONTINUE` glyph x=504 — **four left edges inside 9 px**, and the label shifts 1 px as the highlight moves. Panel padding 22 px top-left against 16 px at the bar and 33 px at the bottom. It is a hard-edged black rectangle composited dead centre over a lit render, where `docs/11` §8 specifies a 65% scrim with the options bottom-left as a caption to a sign | warren | One optical margin, one consistent rhythm, a rule under the title, a scrim rather than a black rectangle, and an off-centre placement against a composed frame | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 26 | 12 | **Audio occlusion — a gameplay requirement of a passed design, in no plan until now.** `docs/11` §9: *"a warden exactly as loud through a wall as through a doorway makes the whole mechanic a lie… this is now a gameplay requirement rather than a polish item."* `amadeo-audio` has none. The design that passed the critic depends on it | engine, then warren | A spatial voice behind geometry is measurably quieter than the same voice at the same distance through an opening, and the test is headless | ⬜  — 🎯 **FINISH (s26)** — see §1b |
| 30 | 8 | **The player is a box and the objective floats.** `body.mesh` is `BoxMesh 0.7 2.0 0.7`, at frame centre in every third-person capture; `brass_key.mesh` is `BoxMesh 0.18 0.4 0.05` at `y 1.2` with its plinth **2.9 m away**, so it hangs unsupported in mid-air. `CompoundMesh` landed two sessions ago and neither has used it | atrium | Neither is a bare box, and the key rests on something | ⬜  — ✂ **CUT (r28)** — first-person game, frozen-game subject; the Warren half was fixed in s25. See §1b |
| 9b | — | **One-field tuple variants in `amadeo-derive`.** `Solid::Box { shape: BoxMesh }` costs three lines and two indents to say "a box this size". `#[derive(Reflect)]` already supports newtype structs and refuses tuple variants, and a one-field tuple variant is the shape the newtype case already handles. **Demoted below everything above by review 12** — a syntax nicety with a one-`fmt` migration. Keep refusing *wider* tuple variants, which genuinely have no names to write | engine | A compound part spells its shape in one line plus the shape's own fields, and every existing `.mesh` is migrated by `amadeo fmt` | ⬜ **demoted**  — ✂ **CUT (s26)** — see §1b |

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
| 16 | 12 | **An ADR on crowd agents.** ADR 0036 puts `enhanced-determinism` on permanently, which forecloses rapier's `parallel` and `simd` features for ever, so the ceiling is architectural. `docs/10` measures 811 bodies at 11.49% of a frame and concludes nothing needs more — written against the *old* nine-game list. Project Zomboid needs hundreds to low thousands of navigating agents and gets them by not making them rigid bodies. **Hard to reverse: `CLAUDE.md` §5 says this is Justin's** | An ADR says what a crowd agent is, and it is decided before anything builds a crowd rather than at 800 agents | ⬜ **Justin**  — ➡ **GAME 2 (s26)** |
| 17 | 15 | **`MeshInstance`'s per-frame allocations.** It holds four `String`s (`backend.rs:150`) and is deep-cloned per visible mesh per camera per frame at `lib.rs:917`, `:973` and `:1189` — roughly eight heap allocations each, for names that never change. Invisible at 50 meshes, which is why `docs/10` looks clean; a wall at the prop counts Zomboid and Schedule I imply | A frame's mesh list carries interned ids and a resolved material index, and a 2,000-drawable scene is measured before and after | ⬜  — ➡ **EDITOR (s26)** |
| 18 | 13 | **`mod-pathfinding`, inside this gate rather than in M7.** Two of three first-class targets are defined by navigation. `amadeo-behaviour` gives an agent a mind and `amadeo-character` gives it legs, with nothing between — `games/atrium`'s watcher walks through pillars and says so in its own comments | An agent navigates around an obstacle it cannot see through, deterministically, and the watcher stops walking through pillars — **and `games/warren`'s warden stops walking through bulkheads**, moved here by review 19 off item 24 because this row already names the identical defect one game along | ⬜ (widened r19)  — ➡ **GAME 2 (s26)** |
| 19 | 17 | **Widen the performance evidence.** Everything measured is 640×360 with 20 meshes, or 1280×720 with 11, plus 811 physics bodies and 20k sprites in the CPU batcher. Nothing has measured 1080p, textured shading, transparency, or a world of thousands. "AA performance rendering" cannot be claimed from four spot readings, and M3 gate item 8 cannot be closed by them | `docs/10` carries 1080p with textures and transparency on, 2,000+ drawables, and 1,000+ agents, each re-runnable | ⬜  — ➡ **GAME 2 (s26)** |
| 27 | 12 | **`mod-tilemap` and isometric y-sorting**, which no plan has ever contained. `docs/05` says twice — at both of its ⬆ **RAISED BY THE BAR** boxes — that these are *"engine-gate work now, tracked in `docs/13-the-engine-gate.md`"*. **They are not tracked here and never were.** Item 7 fixed the document that pointed the wrong way and nothing scheduled the work, so the one capability Project Zomboid contributes to `docs/12` §2's first-class three is cited as planned and is in no plan. Found independently by this session and by review 12; it is the fourth instance of a citation naming a source that does not contain the claim, which is what item 6 exists for | A 2D tilemap is authored as text, drawn, and y-sorted isometrically, with a determinism test | ⬜  — ➡ **GAME 2 (s26)** |
| 28 | 12 | **Skeletal animation and skinning — Q41.** Two of the three first-class targets are populated worlds. `modules/amadeo-anim` animates a reflected field by name and provably cannot reach a skeleton (ADR 0066 §1), so skinning needs a typed path: glTF skins, joint palettes, inverse bind matrices, a vertex shader. `docs/11` §3 converts its absence into an aesthetic — never clearly showing the warden — which is honest exactly once and cannot be the answer for Zomboid or Schedule I. **Review 12 ruled it belongs in the gate but last**: do it after the frame passes, not instead of the frame passing. The genuinely undecided half is where a rigged model comes from, which is Q41 rather than this item | A skinned mesh plays a clip deterministically, and `docs/12` §3's authoring question has an answer | ⬜ **after the frame passes**  — ➡ **GAME 2 (s26)** |
| 41 | r28-1 | **The warden respects the level** — filed by review 28 as **F2b**, the largest gap in the re-scoped plan and in no plan before it. `move_the_warden` writes `Transform::translation` straight at the player with no collider and no sweep; `watch_for_you` sets `sees_you` from `distance(...) <= WARDEN_SIGHT` alone. The antagonist sees through cast-iron bulkheads and walks through them, and its own doc comment excuses this on a room that is **not the level the game ships** — that sentence was written for `scenes/warren.scene` and the shipped level is `scenes/generated.scene`. **Not item 18**: `cast_shape` for sight, `move_shape` for motion, both built | warren | See **F2b** in §1b for the full condition: a headless test over 300 ticks in which `sees_you` never becomes true through a wall and the warden never crosses a wall plane, a second case through an open cross-passage in which it *does* reach the player, and one capture showing the figure against rather than inside a bulkhead | ⬜ — 🎯 **FINISH (r28)** |
| G-tri | r28-9 | **Terrain triplanar mapping**, filed by review 28 so it does not leave the file with item 39. `crates/amadeo-terrain/src/world.rs` projects UVs planar from x and z and its own comment states the consequence — the projection stretches on vertical faces, and *"triplanar mapping is the usual fix"*. **This is engine work, not an art pass**, which is the correction review 28 made to item 39's stated cut reason. An isometric outdoor survival game looks at cliff faces all day | game 2 | A vertical cliff face samples its material at the same texel density as the flat ground beside it, measured off a capture at a named crop | ⬜ — ➡ **GAME 2 (r28)** |
| 29 | 12 | **Decals.** `docs/11` §7 lists them as absent and designs around it. A real gap and a real want for wear, grime, signage and damage — and review 12's ruling is that it is not gating a frame, so it sits at the end of this phase rather than in Phase C | A decal projects onto geometry it does not own, sorted and deterministic | ⬜ **last**  — ➡ **GAME 2 (s26)** |

### Phase E — deliberately deferred

| # | R# | Item | Why it moved | Status |
|---|---|---|---|---|
| 20 | 10 | **The write half of the agent protocol** — was item 4 of the first plan | `docs/protocol/v1.md:594` already says these methods need a persistent session, and that session's only real client is M4's editor's undo/redo and live-tweak loop. Building it now means **designing against no client**, which is precisely how this repository ended up with cascaded shadows, PBR, normal mapping and 16× anisotropy that have never drawn a textured pixel. I5 is not violated today because the editor does not exist. **Do it as the opening act of M4**, with the editor's needs in front of you | ⏸ **M4**  — ➡ **EDITOR (s26)** |

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
it asked for in a critic's brief, is in `docs/14-the-critic.md` §8. What it changed in this file: the
per-game measurement rows and the **Lands in** column in §2, the reopening of 13b, the closing of 12h
and 13a with evidence, four new items (21 ambient occlusion, 22 shadow-edge dithering, 23 normal-map
mips, 24 the Warren stops being boxes) and three more in Phase D (27 tilemap, 28 skeletal, 29 decals).
