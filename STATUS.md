# Amadeo — Current Status

**Last updated:** 2026-08-22 (session 23)
**Current phase:** **M0 complete. M1 closed. M2 COMPLETE. M2.5 COMPLETE — all four exit gates met.**

Every expensive decision in M2 and M2.5 was made before its code, and all twelve are decided *and*
built: ADR 0031 (2D/3D coexistence, camera becomes an entity), 0033 (material and shader model), 0034
(render graph is internal, a look is an asset), 0035 (a mesh is a procedural shape or vertex data),
0036 (physics is deterministic before it is fast), **0037** (a character is a move-and-slide query,
and the character is a module), **0038** (one shadow map now, the mode is authored data), **0039**
(glTF geometry stays art, the scene graph becomes text), **0040** (the profiler is a service and is
always on), **0041** (parallelism is deterministic by construction or absent — resolves Q9), and
**0042** (terrain is a generated base plus hashed edits).

**M2's four exit gates, all met:**

1. **A 3D scene** — imported glTF, dynamic lighting, shadows, and a physics-driven character
   controller you can walk around with. `cargo run -p atrium`.
2. **A 2D scene from M1 still renders unchanged.** `games/vault` is untouched and its tests, replays
   and capture all still pass.
3. **A physics-heavy replay reproduces bit-identically across runs and processes**, with a literal
   state hash pinned on Windows *and* Linux.
4. **Frame time within a declared budget, numbers written down** — `docs/10-frame-budget.md`.
   8.3 µs per simulation tick, 125 µs of CPU-side frame preparation, and 2.7% of a frame at gate 3's
   200-body complexity.

## 📬 For the next session — read this box first, then `docs/14-the-critic.md` §6

> ### Session 24: review 19 found what was lighting the tunnel, and the answer was nothing
>
> **The interrupted review 18 was re-sent at `efdc90d` and delivered as review 19: NOT POLISHED**,
> with a finding no previous review had reached and two re-filings. Its full record is `docs/14` §8.
> Every factual claim in it was checked against the repository before any of it was acted on, and
> **all six checked out** — including two the submission got wrong.
>
> #### The finding
>
> **`games/warren`'s lining was lit by the ambient probe and by nothing else.** The left wall of
> `at_key` read 84–101 flat over twelve metres with the far end *brighter* than the near, because an
> ambient probe is direction-only and distance-independent by construction. Two megapixels of the
> yaw-270 frame held no pixel above luma 130.
>
> **And the ambient was at 8.0 to compensate for a defect that was fixed one commit later.** Review
> 16 found three objects at `RGB(0,0,0)`; `6d82f7e` raised `LEVEL` 5.0 → 8.0 to give them something
> to reflect; `0124db7` — the very next commit — proved with ADR 0084 that they were black because
> the colour grade was clamping below byte 44. **The compensation was never taken back out.**
> `docs/14` §4 #2 in its purest form: a knob moved to chase a symptom belonging to a different knob.
>
> #### The submission's own numbers were stale, and that is worth more than the finding
>
> It recounted **0 / 27** meshes and **2 / 11** materials against the submission's 0 / 25 and 2 / 10,
> because the submission copied them out of `docs/13` instead of counting them off the filesystem.
> `docs/14` §3 #6 exists to stop a *reviewer* doing that and nothing had said it applies just as hard
> to whoever writes the submission. **Recount before quoting, in either direction.**
>
> #### All seven ordered changes are built
>
> - **Every section has two fittings, on opposite haunches**, dead or alive by whether it flooded or
>   collapsed rather than by a coin. Six lit fittings across fourteen sections is what left eight
>   sections with no light source in them at all.
> - **The ambient is back to 4.5** and the hand lamp has a **housing spill**.
> - **The player wakes off-axis facing along the bore.** The generator had been facing them at
>   whichever door sorted first, which for the start section is usually a cross-passage — so the
>   authored camera pointed down a sixty-metre straight run that `docs/11` §5.3 forbids by name.
> - **Every generated surface has grime.** `wear` only ever exposed what was *under* the paint;
>   nothing modelled what had settled *on* it, which is why three reviews measured six levels of
>   variation inside one texture.
> - **The way out is an arrival**: its own lamp, an orange rule, an orange call plate, its own paint.
> - **The warden is a coat**, in wool rather than in the tunnel's own plate joints, with a spot rather
>   than a bare bulb.
> - Prompts follow `docs/11` §8 at last: `Locked`, `Way out`, `Torch`, `Brass key`.
>
> #### Six things worth not rediscovering
>
> 1. **`moment`'s `at_exit` snapshot faced away from the exit.** It stood at `cell + 4.2` in `z` and
>    faced north for all three landmarks, but the door sits on a bulkhead at one *end* of its cell —
>    so the snapshot committed for a reviewer to photograph the way out had the way out **behind the
>    camera**, and every measurement of that door ever taken, including review 19's "11-level range
>    across the entire face", was of the far bulkhead instead. `exit_side` already knew.
> 2. **A light at the camera makes a frame MORE symmetric, not less.** The spill at 4.2 over 4.5 m lit
>    both walls of a 4.8 m bore to about 180 and turned the opening shot into a white tiled corridor.
>    It is the near-field falloff nothing else can give, and it is small or it is a disaster.
> 3. **An A/B that comes back byte-identical may mean the thing is not in the frame.** Sweeping the
>    fitting intensity 8 → 13 → 20 moved not one pixel, which looked exactly like `docs/14` §4 #11
>    biting again. It was not: the composition at the time pointed away from every fitting. A magenta
>    test settled it in one command — **change the variable to something unmissable before concluding
>    the pipeline is broken.**
> 4. **Check the indentation before believing a `perl -i -pe` did anything.** Three separate edits to
>    `room_lamp.scene` and `way_out.scene` silently did nothing because the pattern assumed eight
>    spaces and a scene file uses six. Every one of them produced a byte-identical capture and a
>    plausible wrong conclusion.
> 5. **The target directory had artefacts from a previous checkout path.** `cargo test --workspace`
>    failed in `amadeo-app` and `amadeo-cli` looking for
>    `C:\Users\justi\Desktop\Personal\Amadeo\...`, which is where this repository used to live. It is
>    not a code failure and no amount of reading the test explains it; `cargo clean` does.
> 6. **A dark frame concentrates its histogram, and that is not the same as being flat.** Review 19's
>    clause (a) asks for no more than 65% of pixels in any one 64-level band. For reference measured
>    the same way: `games/atrium` scores 62.1% and `games/scarp` 0.4%. The Warren went 92.9% to 48.4%,
>    and the thing that moved it was the spill putting midtones on the near lining — not brightness.
>
> #### Review 20 then ruled on it, credited the fix, and found two false beliefs
>
> **NOT POLISHED**, with ten ordered changes. It proved review 19's finding was answered rather than
> taking the claim: at `LEVEL 0.0` the near end of `at_key`'s wall still reads 63 against the far
> end's 1, so ~85% of the near wall's light is punctual now. It confirmed all five rewritten clauses
> and **ruled that clause (a)'s 65% was the right threshold and the range was earned** — *"if you had
> merely lifted the exposure the A/B would have shown a uniform scale."* But it added that the near
> half of the tunnel is lit and **the far half is still a grey wash**.
>
> **Two things this repository believed that are measurably false.** `sky ""` supplies a *brighter*
> ambient than an authored map, not none — `ibl.rs`'s `DEFAULT_SKY` is 0.12 grey, deliberately
> non-black — and `gloom.rs` and `CLAUDE.md` both said the opposite. And **the one emissive object in
> the game could not bloom**: `bloom.wgsl` gives `over = max(brightness − threshold, 0)`, and
> `emissive 0.62 0.82 0.7` against `threshold 1.1` is exactly zero. That was **session 24's own
> regression**, introduced during an A/B chasing a blob that turned out to be the torch.
>
> **`moment` had the same defect twice in one function.** Review 19 found the exit snapshot facing
> away from the exit; review 20 found the key **61.9° off the view axis** in the snapshot committed so
> a reviewer could photograph the key. Fixing one landmark did not prompt anybody to check the other
> two — the lesson is that a bug found in one branch of a `match` is a reason to read the others.
>
> #### What session 25 built, and what it did not
>
> **Done:** the lining's near-wall patch went **61.7% → 4.7%** in one 16-level band, and the thing
> that moved it was *amplitude*, not frequency — a fourth octave and 512 → 1024 moved it by 0.4 of a
> per cent, because the fine grain spanned about nine sRGB levels. The tube's emissive is restored.
> The key is 9 cm on a key board with five empty hooks instead of 32 cm balanced on its tip on a
> crate, and `brass` has a real surface. All three landmark snapshots look at their subject and stand
> off its axis. Ambient 4.5 → 2.6. Section letters are black enamel. Crates are off `bulkhead`.
>
> **Not done, and these are review 20's own items 3, 5, 7 and 9:**
>
> - **The exit's lamp reads as four floating fragments** — `ring_fitting` at scale 0.8 seen head-on.
>   It wants its own purpose-built bulkhead lamp, and the light moved so the pool falls *from* it
>   rather than being brightest at the wheel.
> - **Yaw 270 is a sixty-metre straight sightline** through the cross-passage, with eight identical
>   door frames to a vanishing point — `docs/11` §5.3's named prohibition. Turning the *spawn* away
>   from it is not removing it. Needs a bulkhead, a dogleg or a collapse within ~15 m.
> - **The warden is fifteen levels from its background** (coat 14/23/27 against wall 29/30/49).
> - **The hand lamp has no edge** — the omnidirectional spill is doing the beam's job.
>
> Also open and re-filed off item 24: **item 32** (the section letters name nothing) and **item 18**
> (the warden walks through walls).

> #### What is next
>
> **Send item 24 to the critic again.**
 All seven ordered changes are built and the five rewritten
> clauses measure as met, but `docs/14` §6 reserves ✅ to a review and those numbers are the
> implementer's own. Two items were re-filed off it and are open: **item 32** (the section letters
> name nothing and point nowhere — needs a stencil alphabet a binary can emit; note that emitting the
> back face at **(−x, y, −z)** is a rotation rather than a reflection, which lifts the
> symmetric-alphabet constraint and makes all 26 letters available) and **item 18**, which now also
> owns the warden walking through walls.
>
> `gh` is not on PATH: prefix with `$env:PATH = "C:\Program Files\GitHub CLI;$env:PATH"`.


<details>
<summary>Session 23's box, kept because review 17's four open items are traced through it</summary>

> ### Session 23: the Warren stops being boxes, and a planning review happened first
>
> **Review 14 was the first review of a *plan* rather than of built work.** It returned NOT POLISHED
> with ten ordered changes, and session 23 built the revised plan. Its record is `docs/14` §8; the
> work it produced is `docs/13` item 24, now 🟡 **built, awaiting verdict**.
>
> #### The number that had not moved in thirteen reviews
>
> `games/warren` went from **13 / 13 box meshes to 0 / 18**. A cell is no longer a room: it is
> **12 m of 4.8 m-wide arched bore** with a segmental cast-iron crown 3.2 m above the deck, every
> bore runs north–south, an east or west door is a **cross-passage** through the ground between two
> tubes, and any end without a door is closed by a **bulkhead** — `docs/11` §5.2, realised out of the
> room graph that was already there rather than by rewriting `lay_out`.
>
> Review 14 ruled out the cheap alternative — an arched lid over the existing 12 m rooms — by
> arithmetic rather than by taste: `ArchMesh` takes its segmental branch at `width 12, height 3`, and
> headroom falls below the player's 1.95 m beyond `|x| = 3.93`, so **34% of every floor would be
> unwalkable**. It also set the crown height as a number (≤ 3.4 m), because a 4.8 m tube with 4.7 m
> of headroom is a running tunnel rather than a shelter deck.
>
> #### What else landed
>
> - **`docs/11` §5.2's binding rule** — *"Rooms may repeat. No two may be in the same condition"* — as
>   a `Condition` drawn per room in `lay_out`: slept-in, stripped, stores. Dressing only, so it cost
>   no geometry and no change to the room graph.
> - **A section plate at every junction**, with the orange rule and a stencilled `H`, from a fifth
>   generated surface in `finishes.rs`. There is no glyph rasteriser a texture generator can reach, so
>   the alphabet is the rectilinear one — which is why the sections take naval names whose initials a
>   stencil can cut.
> - **The warden stopped being a box and did not become a person**: a non-articulated greatcoat
>   silhouette carrying its own lamp. Review 14's ruling, and it is right — limbs promise motion this
>   engine cannot deliver, so a limbed figure would have been *worse* than the box.
> - **`spill` is deleted.** A `DirectionalLight` called *"Spill from somewhere"* a hundred feet
>   underground is light with no readable cause, and `gloom.rs` had already taken over its job.
>
> #### Three things worth not rediscovering
>
> 1. **A `capture --from <snapshot>` does not see a scene edit.** A snapshot restores *components*, so
>    editing a `.scene` and re-capturing gives a byte-identical frame — and "this light contributes
>    nothing" is the wrong conclusion. It produced two confident false findings here in ten minutes,
>    and one of them was acted on before being caught. Re-run `--bin moment` **between** the edit and
>    the capture. Materials, meshes and the `Environment` *are* re-read from disk, so an A/B on those
>    is valid. (`docs/07`, `docs/14` §4 #11.)
> 2. **A prefab override replaces the root's component, so a piece must not author its height on its
>    own root.** Every wall in the shipped level had been half underground since session 20, with the
>    top 1.5 m of every bay open to the sky map. `amadeo check` passed, `fmt` was clean, the suite was
>    green and walking around felt right, because the collider sank with the mesh. (`docs/14` §4 #10.)
> 3. **Do not diagnose a texture from a render.** A pass was spent transposing the lining's lattice
>    because the crown *looked* wrong; opening the PNG settled it in one glance and the original was
>    right. `docs/14` §4 #2 in miniature.
>
> #### Then review 15 judged the built thing, and failed it on what was laid over the geometry
>
> It credited the geometry without reservation — *"it is a tunnel"*, and the pitched-up frame *"the
> best frame this project has produced"* — confirmed both measurements, and re-derived the crown
> independently. Then it failed three of the close condition's five clauses on measurement, and all
> eight of its ordered changes landed in the same session:
>
> - **The pool was a hole.** 27,659 clipped pixels; one row ran 242–254 for 252 px unbroken. A
>   `PointLight` 0.3 m off 0.6-albedo plate cannot be dimmed out of that, and it inverted the read —
>   an emissive tube cannot out-brighten a wall already at 254, so the fixture rendered as a shadow.
>   It is a **spot aimed down and away** now, and the frame clips **0 pixels**.
> - **The grade was cancelling the palette.** Hand lamp R−B = +12, fitting pool R−B = −10: both read
>   white. Safety orange measured **(107, 0, 0)** with two channels clamped to exactly zero, against
>   (199, 81, 34) for the same orange in the UI — so the walls and the interface, which `docs/11` §2
>   calls the luckiest coincidence in the project, were two different colours.
> - **The props were not there.** Bunk frames at literally `RGB(0,0,0)`, because `metallic 1.0` has no
>   diffuse term and §5a specifies that surface *by its base colour*. Both mattresses wore `screed`,
>   the floor material — so the section conditions were computed and invisible, which the review
>   correctly called not built.
>
> Also: black fog can only subtract, so nothing could emerge from it; the lining stopped at the
> springing and met the arch on a dead-straight seam; and `Condition` was an i.i.d. die roll, which
> contradicts the very sentence it was written to satisfy.
>
> #### And fixing its first item found a gameplay bug that had no symptom
>
> Making the fittings spot lights meant `carry_the_torch` — which wrote **every `SpotLight` in the
> world** — started driving them: picking the torch up blazed every fitting in the level to the
> beam's intensity, and dropping it put them all out, in a game whose whole lighting design is a warm
> lamp you carry against cold fittings you do not control. The test suite could not catch it because
> the test helper had the same bug (`the first SpotLight`), so it was measuring a fitting too.
>
> It surfaced only as an A/B on the fittings' authored intensity coming back **byte-identical** — the
> third instance in one session of an authored value silently overwritten at runtime. **Find things
> by their place in the world, not by the type of component they carry**; "the only one" is a
> property of today's content, not of the system. (`docs/07`.)
>
> #### Review 16 then called it "a near miss with a short, concrete remainder"
>
> It credited ten things on measurement — *"it is a tunnel"*, the pitch-up frame as *"the best frame
> this project has produced"*, real pools with real dark between them (an impression it had formed
> from the histogram and then **withdrew** when the profiles refuted it), the warm/cold split
> working at R−B = +22 against G−R = +13 in one frame, and both measurements honest. Then it failed
> the item on five things, all of which landed:
>
> - **Fourteen signs and every one said H.** The submission had claimed this answered; moving the
>   letter into geometry made a per-section letter *possible* and did not deliver one. There are five
>   now — H, I, M, O, T — chosen by grid distance from the start, so the alphabet advances outward and
>   two adjacent sections can never match on any seed.
> - **Nothing had a fill light.** Three authored objects at exactly `RGB(0,0,0)`. The critic suspected
>   a metallic material, A/B'd it, **withdrew**, and A/B'd the ambient instead — `gloom.rs`'s `LEVEL`
>   was 5.0 and is 8.0.
> - **Every material had flat roughness and flat occlusion** — six levels of variation inside any one
>   texture. Damp and dust fields now drive both.
> - **The fitting's cone was a 136° flood**, so its pool had no edge. Narrowed; the deck now falls 40
>   levels in 40 px at the boundary.
> - **The sign collapsed at a grazing angle.** It is flag-mounted now, projecting from the wall and
>   readable from both directions the way a tunnel sign actually is.
>
> #### Two things worth carrying
>
> **A double-sided sign needs a symmetric alphabet.** E, F and L were built first and read backwards
> from one side; mirroring geometry mirrors the letter. H, I, M, O and T are their own mirror images,
> which is why the alphabet is those five — and they are still real admirals in alphabetical order.
>
> **`bulkhead` is dark enough to read as a hole.** The fitting housing went from 34.6% of its
> rectangle below luma 16 to **1.0%** on a material swap alone, with no geometry change. If something
> renders as a silhouette, check its albedo before its normals.
>
> #### Review 17 found an engine defect, with a derivation, that three reviews had been chasing
>
> **`Environment::grade.contrast` was clipping blacks to zero.** It was `(x − 0.5) · c + 0.5`, a
> straight line, and a line steeper than 1 crosses zero *inside* the visible range: at `c = 1.05` the
> crossing is byte **44**, and the clamp after it turned everything below into pure black.
> `games/warren` authored 1.05 and had **42.5% of a frame at exactly `RGB(0,0,0)`**; `games/vault`
> authors 1.15 and was losing everything below byte 72.
>
> **Three reviews had been finding this one object at a time** — a skirting kerb as a black band, a
> fitting housing as a silhouette, a sign surround as a hole — and none of them were the objects'
> fault. It is a power about the pivot now, which has the same slope at the pivot so authored numbers
> keep their meaning, and `contrast 1.0` stays the exact identity. The Warren's pitch-down frame went
> **42.5% → 0.0%** below luma 16, minimum 15.
>
> **The general lesson, in `docs/07`:** any post-process operator that can map an in-range input to an
> out-of-range output is a defect waiting to be blamed on something upstream.
>
> Also landed: **six section conditions** instead of three, including the ones that change what a room
> looks like — standing water, a fallen ring with spoil under it, and bunks re-racked as an archive;
> **`moment --at`** with four committed snapshots, so a reviewer can stand at the key, the way out and
> the warden post rather than only at the start line; and the fitting got end caps, a wire guard and a
> conduit, and stopped interpenetrating an arch rib.
>
> #### What is next
>
> **A fifth review was sent at `0124db7` and was cut off by an API session limit before it ruled**
> (`docs/14` §8, review 18). It produced no findings and no verdict, so **item 24's standing verdict
> is review 17's NOT POLISHED** and the first thing the next session should do is send that review
> again — the tree is clean, CI is green on that commit, and nothing has changed since.
>
> `docs/13` §1 still carries review 13's order. Items 31 and 24 are both 🟡 — **item 24 has been
> through four delivered reviews and has not passed.** What review 17 asked for and is *not* done, so
> the next session does not have to rediscover it:
>
> - **The section letters convey no direction and carry no name.** They are `manhattan % 5`, which is
>   a distance *ring*, and Howe, Inglefield, Mountbatten, Osborn and Torrington exist only in a Rust
>   comment. `docs/11` §5.4 states three requirements and this meets one. A name needs a stencil
>   alphabet a binary can emit, which is the real job.
> - **Three of eight frames have no light source in them.** The fix is where the fittings go, not how
>   bright they are.
> - **The rust does not survive to render scale** — the texture is right and what reaches 3–6 m is
>   not.
> - **The warden**, still walking through walls and still out of the frames. After it: **item 21**'s
> screen-space occlusion half, **item 22**'s shadow-edge dithering, and **item 15**'s title screen.
>
> **One thing review 15 declined to credit and was right to:** the warden was kept out of the
> submitted frames because it still walks through walls, so its change stays open on that basis
> rather than on any finding. Put it in a frame next time, or constrain it to the cell graph first.
>
> `gh` is not on PATH: prefix with `$env:PATH = "C:\Program Files\GitHub CLI;$env:PATH"`. **Run the
> fourth check (`cargo doc`) before pushing, not after.**

</details>

<details>
<summary>Session 22's box, kept because its four lessons are still live</summary>

> ### Session 22: the gate reached review 13, and the Warren finally has a look
>
> **The project changed hands between sessions** — the previous account hit its weekly limit
> mid-stream — so session 22 opened by re-reading the documentation cold and sending a full
> re-assessment to the critic rather than continuing from a plan in a transcript. That is
> `docs/12`'s own argument working: the plan was in a *file*, so it survived.
>
> **Two reviews happened. Both returned NOT POLISHED.** Their records are in
> `docs/14-the-critic.md` §8 and the plan they rewrote is `docs/13` §2. `docs/13` §1 now carries the
> execution order review 13 set, and **that order is the answer to "what do I do next".**
>
> #### The one number that moved, after thirteen reviews of it not moving
>
> `games/warren` — the milestone deliverable, the only game here with a passed design — had sat at
> **6 / 6 untextured materials and 13 / 13 box meshes since session 20**. Every hour of gate work had
> been landing in `games/atrium`, a demo with no premise, because Phase C's preamble routed it there
> while the Warren's design was unpassed. **That design passed in session 20 and nothing re-routed
> the work.** `docs/13` §2 has a **Lands in** column now, and it is `games/warren` for everything
> remaining.
>
> Its materials are `docs/11` §5a's palette at last: **6 / 6 → 2 / 6** on all three texture slots.
> **Its meshes are still 13 / 13 boxes**, and that is the next job.
>
> #### What landed
>
> - **`docs/14-the-critic.md`** — the critic's standing brief, which Justin asked for. §3 is the
>   evidence a review must gather; **§6 reserves a ✅ to a review**; §4 is nine things this
>   repository has fooled a reviewer with. Review 13 said §3 and §4 changed how it worked.
> - **`amadeo image`** — `probe` / `row` / `col` / `crop` / `stats` / `diff`. Standalone like `fmt`.
> - **Ambient occlusion, both halves (ADR 0083)**, and its cost measured in `docs/10`: 275 µs of a
>   616 µs frame, of which the depth prepass is 12 and the full-resolution spiral is 204.
> - **`crates/amadeo-texture`** — the generator as engine code, 15 tests. `Bond::Broken` places each
>   joint over the middle of the stone below, so the lap is arithmetic rather than statistical.
> - **A way to photograph `games/warren` in play**, which had never existed.
> - The Atrium's pendant, the Warren's palette and its reticle.
>
> #### Four things worth not rediscovering
>
> 1. **An authored value may be dead data, because a clip is writing the same field.** `atrium.scene`
>    authored a `PointLight::intensity`; `lamp_flicker.anim` drove the same field from tick 1, so a
>    whole commit's "fix" changed nothing and the capture was **byte-identical**. Nothing reports
>    this — not `amadeo check`, not a test, not reading the scene top to bottom. **Grep the `.anim`
>    files before believing a number in a scene file.** (`docs/07`, `docs/14` §4 #9.)
> 2. **A rejected render graph draws nothing, and the symptom is a black screen.** The depth prepass
>    declared its target through `with_depth` and not in `writes`; `compile` builds its writer table
>    from `writes` alone, so the occlusion pass read something no pass wrote and the whole frame was
>    refused. 1920 × 1080 of pure black with no other clue.
> 3. **A wrong constant looks exactly like a wiring failure.** The occlusion estimator was normalised
>    by `radius` on dimensional grounds, which throttled it to a few per cent, and the first capture
>    with it on probed byte-identical to the one without. Three debug renders separated it: the depth
>    was right, the normals were right, the estimator was returning one. **Measure the intermediate.**
> 4. **Read a design document against the engine, not against itself.** `docs/11` §10's asset budget
>    rested on "there is no raw-geometry path and the only meshes are boxes, planes and arches",
>    falsified by ADR 0074 a session earlier — and it had *cut content*: the trolley was struck out
>    because "wheels are the one thing boxes cannot fake", and a wheel is a cylinder.
>
> #### The process, which is not negotiable and is the reason any of this worked
>
> **The critic's verdict is binding** (`docs/12` §4). Nothing is done until it says so, **only a
> review may write ✅ in `docs/13`**, and implementation writes 🟡 *built, awaiting verdict*. Where
> the critic is factually wrong about the repository it is corrected with evidence — that has now
> happened twice, and both times it verified and withdrew, which is the mechanism working rather
> than failing.
>
> Do not edit game content while a review is in flight. Review 13 opened its report by noting the
> working tree was not clean, and it was right to.
>
> #### Housekeeping
>
> **Everything is pushed through `11ee7c1`.** `gh` is not on PATH: prefix with
> `$env:PATH = "C:\Program Files\GitHub CLI;$env:PATH"`, and check per-job with
> `gh run view <id> --json jobs` rather than waiting on the summary. **Run the fourth check
> (`cargo doc`) before pushing, not after** — two binaries named `surfaces` collided in rustdoc and
> nothing else noticed, so one push went out red.

</details>

<details>
<summary>Session 21's box, kept because its three lessons are still live</summary>

> ### Where session 21 left off: the engine gate reached review 11
>
> **Reviews 9 and 10 both returned NOT POLISHED on item 12c; every item from both is
> addressed and review 11 was sent.** `docs/13-the-engine-gate.md` is the authority — items
> **12d** (the room's proportions) and **12e** (`capture --pitch/--yaw`) passed on review 10;
> **12c** is held open behind the new **12f**, and **12g**/**12h** record what review 10 found.
>
> **What landed:** ADR 0079 split the sky's two jobs (`Environment::sky_ambient`), the Atrium
> went from 20 × 20 × 4 m to 8 m high, `amadeo capture` gained `--pitch/--yaw`, and a
> shadow-map light leak at the roof-to-wall junction was traced and cut.
>
> ### Two diagnoses were wrong this session and both were corrected by splitting a variable
>
> This is the thread worth carrying forward, and `docs/07` now has all of it.
>
> 1. **The dark band was not the sky.** I proved it was by recolouring the map magenta and
>    watching the band go magenta — an experiment that cannot distinguish the two, because
>    recolouring the map moves the backdrop *and* the fill together. **A knob feeding two
>    consumers cannot tell them apart**, and moving it returns a confident wrong answer rather
>    than no answer.
> 2. **The ceiling seam was not z-fighting.** Review 10 measured it correctly (143 between a
>    76 wall and an 81 ceiling) and diagnosed coplanar geometry. Sun to zero: the band
>    vanishes. Shadows off: the whole wall reads 143. It was a shadow leak, caused by my own
>    `shadow_distance` 22 → 30 — **a shadow bias is per-BOX, not just per-cascade, so resizing
>    the box silently retunes it.** The "hole" offered as proof of depth precision was a
>    pillar's shadow.
> 3. **The glass is not a binding bug.** `draw_run` is one closure for both pipelines, and
>    `metallic 1.0` moves the pane from (204,209,205) to (123,133,138) — the specular path
>    works on blended surfaces. The real cause is that **the environment map is a featureless
>    gradient, so there is nothing to reflect**. Item 12f: split the map's *content*, not just
>    its intensity.
>
> **And two documentation rules came out of the same week.** *An ADR records a decision, not a
> measurement* — Consequences rot whenever they count something, and ADR 0074 confirmed it
> (three of its four consequences are properties and fine; the fourth counted generators).
> *An insertion has two halves* — I grepped for text I had added, found it, and did not notice
> it had replaced `docs/13`'s title, which was missing for three commits.
>
> **ADR 0075 was wrong about `amadeo fmt`, and wrong when written** — `format_scenes` is
> parse-then-write with no registry and by ADR 0016 never will have one. Amended in place. It
> had already cost something: `uv_scale` and `alpha_mode` were declared in **1 of 14**
> `.material` files. All fourteen are complete now, capture byte-identical.
>
> **Open, flagged, not fixed:** the bright ellipse is **closed** — review 10 could not find it
> and it did not survive the room change. Items 12f, 12g, 12h and 13a are open, and review 10
> asked for 13a to be widened to `slate` as well as `stone`, since the walls are the largest
> surface in the game.
>
> ### The plan is a file now. Read `docs/13-the-engine-gate.md` and work from it.
>
> **The engine gate's plan used to live in a conversation, and that is exactly what `docs/12` was
> written to prevent.** The first review's output was a fourteen-item ordered list; session 21's
> reviewer could not read it and had to be handed four of the fourteen in a briefing. It is now
> `docs/13-the-engine-gate.md` — nineteen items in execution order, each with a falsifiable close
> condition and a status column, plus the three tracked measurements across reviews so drift is
> visible rather than re-derived. **Update it in place as items land.** Do not let the next plan go
> back into a transcript.
>
> **Phase A passed on review 4. Items 8 and 9 passed on review 5. Item 11 passed on review 6.**
> **Item 10 is built and awaiting a verdict** — its three follow-ups from review 6 are done and
> self-audited against the code, and the reviewer hit a session limit before ruling. `docs/13` marks
> it amber rather than green, because the file must not claim a verdict nobody gave. What is left of
> Phase B after it is **particles** (item 12), and item 12b (volumetric light shafts) exists because
> item 12's old close condition asked for motes in a beam nothing makes visible.
>
> **Two of the three defining measurements have moved, both for the first time.**
>
> - `.mesh` assets that are `BoxMesh`: **23 of 23 → 23 of 26.** A table, a bolted generator and a
>   standing lamp, each a `CompoundMesh` authored as text, each one asset and one draw call. The first
>   things in any game here that are not axis-aligned boxes.
> - Material texture slots that are `""`: **36 of 36 → 43 of 45.** The Atrium's floor and plinth sample
>   a generated slab texture at a matched texel density. **That is the first textured pixel this engine
>   has ever drawn in a game** — mipmaps in linear light, 16x anisotropy and the whole sampler path
>   were written and tested in session 14 and had never reached a picture.
>
> The third has not moved and is deliberately deferred: 0 of 17 protocol methods mutate, which is
> item 20 and belongs to M4.
>
> **Phase A itself moved none of them, and that is the honest summary of it** — it was about making
> it *possible*, and every hour of it paid off in Phase B taking an afternoon.
>
> ### Two things waiting on Justin, neither blocking
>
> - **Crowd agents** (`docs/13` item 16). ADR 0036 puts `enhanced-determinism` on permanently, which
>   forecloses rapier's `parallel` and `simd` features for ever, so the throughput ceiling is
>   architectural. `docs/10` measures 811 bodies at 11.49% of a frame and concludes nothing needs more
>   — written against the *old* nine-game list. Project Zomboid needs hundreds to low thousands of
>   navigating agents and gets them by not making them rigid bodies. Hard to reverse, so it is
>   Justin's by `CLAUDE.md` §5.
> - **Whether "engine to AA before building a game" should be revisited.** Still open from session 20.
>   Session 21 is evidence for both sides: Phase A found five real defects, and every one of them was
>   found by *tooling and review* rather than by building a game — but equally, none of them would
>   have mattered if a game were using the shapes.
>
> ### The project changed shape in session 20. Read this before planning anything.
>
> **Justin judged the Warren a bland engine test, and he was right.** The levels were "rooms
> literally straightly connected", there was no life or colour, the menu was three rectangles, and
> the objective was "a locked door and there's a key for it — a simpleton's idea of a game". Every
> one of those traces to the same cause: **there was no fiction, so every decision was made on
> engineering grounds.**
>
> Three things follow, and they are now project process rather than this session's activity:
>
> **1. There is a critic agent, and its verdict is binding.** `.claude/agents/critic.md`. Every piece
> of player-facing work goes to it and **nothing is left until it returns POLISHED**. Where it
> disagrees, its changes are followed; where it is factually wrong about the repository it is
> corrected with evidence, which has happened once and it verified and withdrew.
>
> **2. There is a design document**, `docs/11-the-warren.md`, and a **bar**, `docs/12-the-bar.md`.
> The bar is the important one: AA indie, Hello Games as the reference, and the requirement that
> **Claude can author a game's assets rather than asking Justin for them**. That is stronger than
> invariant I5 and it is the thing most likely to be quietly dodged.
>
> **3. The gate order is fixed**: design the game → change and improve the engine → add what the
> engine is missing → build the game. **Nothing proceeds to the next part until the critic passes the
> current one.**
>
> Where it stands: the design has been through **five** rounds of critique, each one finding
> something real. It is not yet passed. The engine gate has not started.

> ### The failure mode that recurred five times, and is worth watching for
>
> **Repairing one section while contradicting a neighbouring one.** Every round of design critique
> found at least one, and twice the contradiction was introduced by the *previous* round's repair:
>
> - "your lamp is safe" against "you can no longer stand still in a lit section" — if it cannot see,
>   light costs nothing, so the central trade did not exist;
> - three sections made compulsorily lit, against "the warden is never clearly seen" (which is
>   justified on the engine having no skeletal animation) and against "near-silence is the default";
> - charging moved into the lit pools, against "the warden never lingers in a pool";
> - the panels moved back into the dark, against an accent-colour list that still did not include
>   them.
>
> The same shape appears in the code: the pause bug (the Atrium's systems copied without its `Paused`
> insertion) and the vacuous test that hid it. **When you fix something, re-read what depends on it.**

> ### The tick-0 hazard is closed at the source, and there was a worse one under it
>
> **ADR 0072.** `amadeo_scene::instantiate_with` now composes the hierarchy before it returns, so a
> loaded world has a correct `GlobalTransform` on everything with no tick required. Session 18's
> warning — "a capture at tick 0 is not a picture of your game" — no longer applies. `amadeo capture`
> still defaults to one tick and should stay that way, but the hazard behind that default is gone.
>
> **Underneath it was a real defect that had scattered every generated interior across a hundred
> metres.** A backend returns *world* poses; a `Transform` on a child is relative to its parent;
> `step_physics` wrote one into the other, and propagation then applied the parent a second time. So
> a parented collider walked away from its piece by the piece's own offset, once per tick.
>
> Two things about how it hid are worth carrying forward:
>
> - **It was invisible at tick 1**, because nothing had propagated yet and the fallback to the local
>   transform was still in play. Session 18's capture of the generated level was taken at exactly the
>   one tick that looks right, and STATUS recorded "it loads and draws".
> - **It was unreachable before ADR 0071.** A prefab has one root, so a piece with two colliders must
>   put them on children — and nothing before room pieces had a reason to put a collider anywhere but
>   a root. A whole class of defect can sit in an engine until one design decision makes it reachable.
>
> And fixing the first exposed a third: composing at load gives a *root* a `GlobalTransform` it did
> not have, and `step_physics` preferred it to the entity's own transform — which is always a tick
> fresher — so anything written between ticks was silently undone. **The fallback was hiding all
> three.**

**Everything is pushed through `7ba77f3`.** Check with `gh run list` rather than assuming — `gh` is
not on PATH, so prefix with `$env:PATH = "C:\Program Files\GitHub CLI;$env:PATH"`. A run takes about
29 minutes and `gh run view <id> --json jobs` shows five jobs separately, so check once at a plausible
time and keep working. One historical red mark to ignore for ever: `6e56c0b`'s docs job, fixed by
`f91229d`.
</details>


### Session 22 in one paragraph

**A cold takeover, two reviews, and the first session whose output landed in the game.** The project
changed accounts mid-stream, so this one began by reading the documentation from scratch and asking
the critic for a full re-assessment rather than resuming a plan nobody could see. Review 12 returned
thirteen ranked findings; four were built and sent back; review 13 closed two of them, **corrected
one of my fixes as dead data**, withdrew one of its own predecessor's claims, and found that the
gate's own POLISHED condition — *a frame from a real game* — had been **unreachable for thirteen
reviews**, because nobody could photograph `games/warren` in play at all.

- **The engine gained ambient occlusion (ADR 0083)** — it had none of any kind, and `mesh.wgsl` was
  discarding glTF's occlusion channel outright. It multiplies **ambient only**, which is the whole
  difference between occlusion and grime, and is why it cannot be a post-process.
- **And a texture generator as engine code** (`crates/amadeo-texture`), which is engine gate item 13
  and `docs/12` §3's requirement that *Claude can author a game's assets*. It is also the third home
  for a noise routine two games had copied, exactly as the second copy's own comment predicted.
- **The Atrium's stone stopped being a 4 × 4 square lattice in stack bond** — the single biggest
  machine-made tell in the project, on every surface at once.
- **The Warren stopped wearing another game's palette.** Its materials were `plaster`, `carpet` and
  `timber`, authored before `docs/11` existed and never revisited when it did.

### The four things worth not rediscovering, session 22

- **An authored value may be dead data, because a clip is writing the same field.** The proof was the
  cheapest possible experiment and the conclusion was unarguable: zero the scene's value, capture,
  and the image comes back byte-identical. Nothing in the toolchain reports this.
- **A rejected render graph draws nothing**, so a graph-validation error and a catastrophic renderer
  bug have the same symptom: a black frame.
- **A wrong constant is indistinguishable from a wiring failure**, and the way through is to render
  the intermediate rather than to reason about the whole. Three debug renders separated a bad
  normalisation from four other candidates in ten minutes.
- **Read a design document against the engine, not against itself.** `docs/11` §10 had *cut content*
  on a constraint that expired a session earlier.

### Session 21 in one paragraph

**The engine gate's second review, and Phase A of what it ordered.** The reviewer returned NOT
POLISHED with nineteen ranked defects, took live captures from the game binaries rather than reading
the code, and was accurate on all five claims spot-checked against the repository before anything was
acted on. Its first recommendation was that its own plan had to become a file, which it now is.

- **Q32 fell to a re-reading rather than to a trade-off (ADR 0075).** It had blocked `Material` from
  growing since session 14 on the grounds that `MissingField` is what catches a typo'd field name. It
  is not — `from_value` checks for *unknown* fields before it reads any field, so a typo was already
  caught by the check that exists for it. A field may now declare a default; canonical form still
  writes every field, so `amadeo fmt` is the migration tool with no new flag.
- **ADR 0074's shapes were registered by no game**, so `amadeo check` rejected a cylinder and
  `describe CylinderMesh` said the type did not exist — while a hand-written one still *loaded*,
  because the mesh loader never consults the registry. Works when tried, broken when checked.
- **The set could not produce the art direction it was built for.** Everything curved was
  smooth-shaded with no way to ask otherwise, which is ADR 0050's "shades like a blob" defect
  reintroduced in the primitives added to serve low poly.
- **Three documents cited a source that did not contain the claim** — including `docs/12-the-bar.md`,
  the document that sets the standard. Now **Q41**.
- **Then Phase B, because Phase A had cleared the way.** `CompoundMesh` assembles primitives into one
  mesh with per-part rotation, a translational `array` and a `mirror` (ADR 0074 §2, §3); `VertexMesh`
  is §4's dump target, which ADR 0035 promised five milestones ago and nothing had built. **ADR 0077**
  gave the engine transparency -- a declared `AlphaMode`, a second pipeline, a back-to-front sort done
  at collection so `NullBackend` sees it too. **ADR 0078** gave it texel density. The Atrium got
  furniture, a glazed screen you can see through, and a textured floor.
- **And the same defaults defect was one type-family over (ADR 0076).** Shapes and `Material`
  declared defaults; `Camera`, `Environment` and the three lights declared none, so
  `describe --example` — which `docs/12` §3 makes the primary way an agent learns to author an asset
  — handed back a dead camera, a black environment and three lights at zero intensity.

### The four things worth not rediscovering, session 21

- **A stated tension can be wrong, and checking it is cheaper than resolving it.** Q32 sat at P2 for
  six sessions and cost nothing to close once its own reasoning was read against the code. Before
  weighing a documented trade-off, verify both of its arms still exist.
- **`describe --example` was advice that did not work.** It answered `radius 0.0`, `height 0.0`,
  `sides 3` for a cylinder — legal, and draws nothing. It came from preferring a range's *minimum*
  over a default, and a range minimum is the lowest value the schema calls legal rather than a value
  anyone wants. **An example an agent cannot use is worse than no example, because it looks like an
  answer.**
- **Two capture tests failed first, and neither for the reason I would have guessed.** A wedge
  reported as missing because it sits *on* y = 0 while a sphere is centred on it, so the crosshair was
  on its bottom edge. And a faceted sphere came out byte-identical to a smooth one because at
  intensity 3.0 every lit pixel clipped to 255 — the entire shading being compared was above the top
  of the range. **Printing the scanline found both in one run**, after reasoning had produced two
  wrong theories. Session 19's lesson, third instalment.
- **A scripted edit that matches nothing still exits zero.** I reported a test assertion as changed
  when it had not been, and found out only because the reviewer read the file. `perl -0pi -e` and
  `sed -i` both succeed whether or not the pattern matched, and the usual next step -- `cargo fmt &&
  cargo test` -- passes, because the file is unchanged and a green test stays green. What made it miss
  was `cargo fmt` having **reflowed** the target text since the pattern was written. The rule is to
  grep for the *result* rather than the exit code, and it is now in `docs/07`.
- **A guard outlives the limitation it was written for, and nothing fails when it does.**
  `describe --example` refused a scene form for `CompoundMesh` — "it has an empty list field, and an
  empty block is a parse error" — three sessions after the format grew `[]` for exactly that case.
  Nothing broke; an answer simply stopped being given, for the flagship type of ADR 0074. This is the
  same shape as the `append_translated` doc comment that asserted rotation was refused *deliberately*
  and became false the day a compound needed one. **When you remove a limitation, grep for what
  refuses on account of it** — the refusal is usually in a different crate from the fix.
- **`Value::F32` was written by widening to `f64`**, so `0.18` came out `0.18000000715255737` in every
  `describe --example`, `world.entity` dump, snapshot and glTF import. It survived because
  **`amadeo fmt` has no schema**: it reads every number as an `f64` and writes the same `f64` back, so
  a hand-written `0.18` round-trips untouched and the formatter looked correct. A round-trip test over
  values the format itself produced would never have found it. **And fixing it in one writer felt like
  fixing it** — the scene spelling was right and the JSON one, which is the half an agent parses, was
  still wrong for another hour.
- **Write the test that the existing tests provably cannot subsume.** `wound_to_match_normals` compares
  a triangle's winding against its own normals — and a rotation moves *both*, together, consistently.
  So a compound part whose normals were left unrotated would pass it perfectly and render as a shape
  that shades correctly until a light moves. That test was written *before* `append_transformed`
  existed and watched to fail for the right reason. **A mirror is the opposite case** and the existing
  check does catch it, because a reflection reverses orientation — which is why `mirror_across` was
  the safer of the two to write. Ask which existing check covers a new operation *before* trusting a
  green suite to cover it.
- **A capture test can pass against a picture that shows nothing.** `every_parametric_shape_draws_*`
  reported a wedge as *missing* (it sat below the crosshair) and then, framed better, could not tell a
  stair from a box — because a `StairMesh` climbs along +Z, so a camera on the +Z axis is looking at
  the back of the flight where the top step occludes every step behind it. It renders as a truncated
  slab. **Both framings passed their assertions**, and both were found by writing the PNG and looking
  at it. The rule from session 13 keeps earning: a green test is not a picture.

### Session 20 in one paragraph

**The direction changed and almost nothing was built, deliberately.** Justin's audit landed, the
critic agent was set up, and the session went into research, design and judgement instead of code.
What did land: two bugs the critic and Justin found between them, one engine primitive, and two
documents.

- **The pause never paused.** `apply_screen` writes the engine's `Paused` with `resource_mut`, which
  is silent when the resource was never inserted — and `games/warren` never inserted it. Every pause
  had been a no-op; the world simulated behind the title screen and the mouse turned the view. Found
  by *playing the game*, and the test that should have caught it was vacuous because it compared the
  player's translation and a player with no input does not move.
- **The key-placement rule did the opposite of its own documentation.** It scores the largest detour,
  which ties on every room in a branchless layout — so the tie-break *was* the rule, and
  `max_by_key` returns the **last** maximum, which with a cell-sorted table is the highest
  coordinate, reliably near the exit. The shipped seed put the key one door from the door it opens.
- **`Layout::shortcomings` and a gate.** Nothing had an opinion about whether a layout was any
  *good*, only whether it was valid — so `--bin layout` now **refuses** to write a level whose key is
  too close to the exit or the start. The shipped seed moved from a straight eight-room line to a
  fourteen-room layout over seven cells by five.
- **`ArchMesh`**, the engine's first curved primitive. Every mesh in every game here was an
  axis-aligned box, which is most of why the result read as a test scene.

### The critic's baseline review, kept because it is the measurement to beat

Its findings on the game as it stood, all verified: **thirteen meshes, thirteen boxes, and every
material with all three texture slots empty** — the engine grew PBR, normal mapping and anisotropic
sampling in session 14 and the game uses none of it. **Eight rooms in a dead-straight line with all
seven doorways at exactly `z = −12.0`**, so a 72 m sightline to a grey dot. **The props at three
hard-coded corner offsets in every level the generator will ever produce.** **Lamps with no mesh** —
light from nowhere. **Zero shadows before the torch is picked up**, because point lights do not cast
and the only caster starts at intensity 0. And the sharpest one:

> *The worldbuilding exists, and it is in the source code.* The materials are named "Sour carpet" and
> "Damp plaster"; the lights are "Spill from somewhere"; the ambience entity is "The Warren itself".
> A player sees grey boxes. And *"a warren is cramped, dug, twisting, low. This is a square grid of
> 144 m² halls"* — the name promises the opposite of the space.

**One diagnosed and not yet fixed**: the horizontal seam across every wall in every frame. It is not
`BoxMesh` — an isolated wall under one light is a smooth gradient — and not the tangents. It is
`gloom.rs`'s two-tone environment map: at grazing angles the Fresnel term makes the ambient
reflection dominate and the reflected ray sweeps through the map's horizon at eye level. A uniform
map removes it completely. The fix is a gentler gradient and it belongs with the lighting pass.

### Session 19 in one paragraph

**M3's exit gate went from two items to five.** Item 2 (bounded procedural interiors) is done rather
than demonstrated, item 1's shell exists, and item 6 (audio) is built. What is left of the gate is
**item 5, atmosphere**, plus the parts of item 1 that are wiring rather than building.

The generator now chooses five **landmarks** out of the room graph — start, exit, key, torch, warden
— places them as instances of eleven content **pieces**, and `games/warren` boots into the generated
level. `scenes/warren.scene` survives as the handcrafted room and instances the same pieces, so each
has two users. The game has a title screen, a pause menu, save and resume, and a way to start over;
it has a room tone, footsteps, a chime, two stings, and a **spatial breath on the warden** so you can
tell where it is without seeing it.

Along the way it found the engine defect in the box above (**ADR 0072**), which had made every
generated interior wrong since the day the generator was written and was invisible to `amadeo check`,
to a green test suite, and to a capture taken at tick 1.

**Three engine changes, all small and all forced by a game wanting something:** scenes compose their
hierarchy at load, physics stores a body's pose in its own space, and `amadeo_ui::focusable_in_order`
is public because a game with more than one menu has to seat the focus and must not reimplement the
visibility rule.

### The three things worth not rediscovering, session 19

- **A green suite, a passing validator and a picture can all be wrong together, and the reason is
  usually a fallback.** `GlobalTransform` falls back to the local transform when absent — right for
  a root, wrong for a child — and that one line hid three separate defects at once. When something
  is wrong only *sometimes*, look for the code that quietly substitutes a plausible value.
- **The formatter is a test, and ADR 0071 said so before anything ran it.** `amadeo fmt --check` on
  generator output caught three real faults in the writer — quoted prefab ids, an `assets` block
  sorted by constant name rather than by asset id, and a spare trailing blank line — none of which
  any other test could see. `what_the_generator_writes_is_already_canonical` runs it now.
- **Isolating beat reasoning, twice, and reasoning nearly won.** A wild-looking capture produced two
  confident wrong theories about the geometry. Capturing the *handcrafted* room — known good, and
  now sharing the same pieces — located the fault in ten minutes. The second time, `render.describe`
  and a printed position settled in one run what three paragraphs of arithmetic had not. This is
  session 18's lesson 4 applied rather than restated.
- **And a fourth, which is the diagnostic tools paying for themselves.** Both of the session's
  silent failures were named by a `describe` in one line. `amadeo audio` said "nothing in the world
  has an `AudioListener`" rather than the true and useless "there are no voices" — ADR 0060's
  ordering rule catching a real fault. And ADR 0069's save integrity check refused a snapshot taken
  before `amadeo_input::install` adds `InputState`, saying that something about the build differed
  from the one that took it. It did: the file recorded a world that never quite existed. **Both of
  those were checks written for a hypothetical and earned on a real mistake within a session.**

### Session 18 in one paragraph

**Three decisions decided and built, and one filed then withdrawn.** Q37 (save versioning) closed
with **ADR 0069**; the `mod-inventory` fork closed with **ADR 0070**; Q40 (procedural interiors)
closed with **ADR 0071**. `modules/amadeo-inventory` exists and `games/warren` — a new game — has a
first-person loop you can win and lose, a HUD, and a working level generator.

Every module session 17 left without a user now has one, and each review earned its keep:
`amadeo-interaction` had a real defect, and `FirstPersonCamera` had never been driven by anything.

**Q39 was mine and was wrong** — a P0 filed against two renderer faults that do not exist. It is
withdrawn rather than deleted, because how the diagnosis went wrong is the useful part.

### The three things worth not rediscovering

- **Being lenient about fields is not enough to make a save survive a patch**, and the reason is the
  state hash. A defaulted field is still hashed, so the world is rebuilt *correctly* and then
  rejected. ADR 0069's answer is to make the check **conditional on a layout fingerprint** rather
  than drop it — matching means no version gap exists, so the load *is* the strict path. Pinned from
  both ends: `a_patch_invalidates_every_save.rs` is the problem, `a_save_survives_a_patch.rs` is the
  answer.
- **A module's own docs said "an interactor is usually a child", and no test built one.** That was
  the one arrangement uncovered, and it was broken: the sweep ignored the interactor, which has no
  collider, so it started inside the *parent's* and reported it at `fraction: 0.0` for ever. Worth
  asking of any module: what does the documentation call typical, and is that what the tests build?
- **A test written expecting one answer found a better one.** `contents` on a despawned container
  keeps answering, and should: filtering by liveness would make an orphan invisible to every function
  in the module while it still exists.

### What is waiting on Justin now

Nothing blocking. The eyeball list below still stands, plus these:

- **The brass key's numbers** — the reach is 2.5 m with a 0.25 m sweep radius, and the reaching child
  sits 0.35 m above the player's centre. That height is not cosmetic: **reach is a band around the
  interactor's forward line**, and it had to clear the plinth top.
- **The key's place and size**, standing upright on the plinth at `y = 1.2`. It stands rather than
  lies for the same geometric reason.

Verify pushes the way session 15 learned to: `git fetch`, then
`git log --oneline origin/main..HEAD`. **The fetch is the load-bearing half** — session 15 hit a
network failure where the fetch died and the comparison ran against a stale ref, printing "all
pushed" without having checked anything. Read the fetch's exit code, not just the log's output.

> **Do not spawn a long background sleep to watch CI.** A run takes ~27 minutes and
> `gh run view <id> --json jobs` shows the five jobs separately, so four-of-five green is knowable
> minutes in. Check once at a plausible time, report honestly, and keep building in between.

> **One historical red mark, so nobody re-investigates it.** `6e56c0b`'s `docs build without warnings`
> job is red and always will be — a doc link named a type the crate does not import. `f91229d` is the
> fix and its docs job is confirmed green. Nothing else has ever been red.

> **Run the checks with `--all-features`.** `CLAUDE.md` §4b said plain `cargo test --workspace` until
> session 17; CI has always used `--all-features`, which is what compiles in everything behind
> `rapier` and `gpu`. `modules/amadeo-interaction` makes the difference obvious — without the flag its
> whole test file reduces to a null-backend control case.

### Where the project actually is

**M3 is close.** Every named M3 subsystem exists, all four named genre modules exist, and the exit
gate's save-and-resume loop works. Session 17 was long: 17 commits, four ADRs (0065–0068), three new
crates.

| Subsystem | State |
|---|---|
| `amadeo-audio` | Complete enough for a game. Missing: ducking, occlusion, compressed audio, a voice cap |
| `amadeo-ui` | Layout, text, focus (drawn), theme, pausing. Missing: pointer navigation — **and ADR 0063's plan for it does not work, read Q36** |
| `amadeo-anim` | A clip animates a *reflected field* (ADR 0066); `amadeo anim` reports why nothing is moving. Missing: **skeletal animation and skinning**, blending, a state machine |
| Save/load | Works end to end in `games/atrium`, and **survives a patch** (ADR 0069). Missing: real per-version migrations, which nothing needs yet, and **where a save file lives — Q38** |

| Module | State |
|---|---|
| `amadeo-character` | Movement, ground, jump, slopes. Missing: crouch, coyote time, pushing dynamic bodies |
| `amadeo-camera` | Third **and** first person, separate components sharing one aiming system |
| `amadeo-interaction` | Look at a thing, use the thing. **`games/atrium`'s brass key is its first user**, and found a real defect on the first try |
| `amadeo-behaviour` | AI as a state machine over named facts (ADR 0068). `games/atrium` has a watcher |
| `amadeo-inventory` | Items, stacks, containers (ADR 0070). An item is an **entity**; storing it removes its `Transform` |

### What to do next — and as of session 20 this IS an order

**The gate order in `docs/12-the-bar.md` overrides the list below.** Nothing proceeds until the
critic passes the current part:

| | Part | State |
|---|---|---|
| 1 | **Design the game** — `docs/11-the-warren.md` | Five rounds of critique, each finding something real. **Not yet passed** |
| 2 | **Change and improve the engine** to the AA-indie bar | Not started |
| 3 | **Add what the engine is missing** | Not started |
| 4 | **Build the game** | Not started |

The list below is still accurate about *what exists*, and is now a description of the ground rather
than a plan. Everything in it is subject to the design document, which supersedes several of its
assumptions — the key and the door are being removed, and the level generator is being rewritten
mission-first.

**Two things are already known to be first in part 2**, from the critic's baseline review and from
the bar's own audit:

- **A mesh authoring path.** `amadeo-gltf` is a reader with no writer and the scene format has no
  raw-geometry line, so a Rust binary can emit a texture, a sound and an environment map but **not a
  model**. That is the single largest gap between this engine and the requirement that Claude can
  author a game's assets.
- **Skeletal animation**, which `docs/04` §14 and ADR 0066 §5 record as blocked on a rigged model and
  which `docs/12-the-bar.md` reclassifies as the engine's problem to solve — three of the nine target
  games are impossible without it. Open as **Q41** since session 21.

**Every named M3 subsystem and all five named genre modules now exist.** What is left in the
milestone is mostly the exit gate itself.

1. **M3's exit gate** — `games/warren` is first person, generated, and has a loop you can win and
   lose. That is **gate items 1** (a playable loop with a win and a lose state), **2** (bounded
   procedural interiors) and **3** (a pursuer with distinct AI states, driven by `mod-behaviour`).
   Still to come:
   - ~~Bounded procedural interiors~~ — **done.** `cargo run -p warren --bin layout` writes a level:
     a seeded room graph over eleven prefab pieces, always connected, always looped, with a place to
     wake up, a torch one door away, a key off the shortest route, a door set into an outer wall and
     a warden half way along. The game boots into it and `you_can_walk_out_of_the_generated_level`
     plays it through. **What is left here is content, not mechanism**: one room shell, one wall, one
     doorway, and a level that reads as a grid because it is one. Rotation of pieces is the obvious
     next step and ADR 0071 deliberately left it out.
   - ~~A title screen~~ and ~~the rest of item 1's shell~~ — **done.** Five screens: title, playing,
     paused, ended, quitting, all authored in `hud.scene` and all driven by `Menu { screen }` so
     that a fourth menu is a scene edit with no Rust. Save and resume work, and so does starting
     over, which restores the world exactly as it loaded.
   - ~~Audio~~ (item 6) — **done.** Six sounds from `cargo run -p warren --bin sounds`, a spatial
     breath on the warden, footsteps, a chime and two stings. **Placeholders**: drop a real `.wav`
     in with the same id and nothing else changes.
   - **Atmosphere** (item 5) — **most of the way there.** Fog landed with **ADR 0073**: a forward
     term on the surface shader rather than a post-process, which is why it never needed the depth
     buffer ADR 0034 said it was waiting for. Off by default and byte-identical when off, pinned
     three ways. And the Warren has a real environment map (`cargo run -p warren --bin gloom`), so
     an indirect surface is lit rather than exactly black — the `sky ""` gap this box has recorded
     twice is closed.
     **What is left is volumetric light shafts**, which is the biggest remaining visual step for
     this game: the torch beam is not visible in the air, and that is most of what a horror
     flashlight *is*. ADR 0073 records why it was not paid for now, and that it raymarches through
     exactly the fog this added.
   - ~~A HUD~~ — **done**: two lines authored in the scene, saying what is in reach and how the run
     ended. **Its pixels are unverified in this game**, and that is worth knowing: the content is
     tested headlessly and `amadeo-ui` has its own draw tests, but nothing has captured this HUD on
     a screen, because a capture cannot stand the player in front of a door. Session 18's own lesson
     says a green suite is not a picture.
   - **Atmosphere** (item 5), which is where the lighting numbers below stop being placeholders.
2. **A runtime-driven aim.** ~~An interactor sweeps horizontally, so an item on the floor cannot be
   reached.~~ **Checked, and that was wrong** — an authored pitch reaches the floor with nothing
   built, because an interactor is an ordinary entity and the sweep follows its forward
   (`aiming_down` in the module pins it). What is actually missing is a pitch **driven at runtime**,
   which the horror slice gets for free by putting the `Interactor` on a first-person camera. Not a
   task of its own any more.
3. **Skeletal animation**, the largest unbuilt piece of a named subsystem. ADR 0066 §5 says where the
   reflected-field design deliberately stops: a read-patch-write per bone per tick is hopeless at a
   few hundred bones, so skinning gets its own typed path. **Blocked on an asset** — the repository
   has no rigged glTF model.
4. **Pointer navigation** — read **Q36** first; the replacement design is written up there.
5. **Q38: where a save file lives**, and whether a redirect file ships with the build rather than
   sitting beside the save. Small, and nothing depends on it.

### The Warren's eyeball numbers, all waiting on Justin

The lighting and prop numbers now live in `games/warren/assets/pieces/`, which is the point of them
being pieces: changing `room_lamp.scene` changes every room of every level ever generated. The rest
are constants in `src/lib.rs`. All one line each, all only roughly tuned, and judged with
`amadeo capture -p warren --ticks 5`.

Four are new this session and are the ones most worth a look:

- **`LAMPS_WORKING`, 0.45** — the chance a room other than the start has a working lamp, and the
  single number that decides how dark the Warren is. Too high and the torch is pointless; too low
  and the level is a black maze before you have found it. Rooms are lit or not, rather than
  everywhere being dimly lit, because a dark room next to a working one reads as lighting that has
  failed in patches, which is a place — uniform gloom reads as a renderer setting.
- **`GENERATED_ROOMS`, 14, and `CELL`, 12 m** — so the shipped level is about 170 m across at its
  widest. It reads as large. Fewer rooms would make it tighter and more claustrophobic, which is
  probably the horror answer, and it is one constant plus a re-run of `--bin layout`.
- **`PROP_OFFSET`, 3.2 m** — how far from a room's centre a crate stands.
- **The seed itself, `GENERATED_SEED` = 20250815.** Generating twenty and keeping the good one is
  exactly what ADR 0071 §1 says a file-based generator buys, and nobody has done it yet.

The older ones, unchanged:

- **`LEVEL` = 5.0 in `src/bin/gloom.rs`** — how bright the Warren's environment map is, and
  therefore how much you can see with the torch off. **The single most important atmosphere number
  in the game**: too high and the torch is pointless, too low and the level is a black maze before
  you have found it. Tuned by eye across three captures; 1.0 was nearly unnavigable and 5.0 lets
  walls read while doorways stay black. `ABOVE` and `BELOW` beside it are the *ratio* — cool from
  the ceiling, a warm carpet bounce from below — and are what stop untextured geometry reading as
  cardboard.
- **The fog: `density 0.055`, `start 1.5`, colour `0.006 0.008 0.011`** in `warren.environment`.
  Roughly a corridor that has closed in by about twenty metres. Density is the knob; the colour is
  deliberately *darker* than the darkest surface, so distance swallows things rather than glowing,
  which is the horror read rather than the mist one.
- **`spill`, a `DirectionalLight` at 0.06** (`pieces/spill.scene`). **Now redundant-ish and left
  in deliberately**: the environment map does the ambient's job properly, and this is left only for
  the bit of directional shape it gives. Turning it to zero is a one-line experiment worth doing.
  It has `shadows Off`, which is what stops it giving away every wall it passes through.
- **The lamp at intensity 14, range 8** (`pieces/room_lamp.scene`), which is what a lit room is
  actually lit by. Range 8 in a 12 m room means the corners fall off, which is doing real work.
- **The torch beam**: `BEAM_INTENSITY` 30 in `lib.rs`, 11°/26° cone, 18 m range, **and it casts** —
  the shadows work, and a flashlight that casts is most of the atmosphere in a game like this.
- **Movement**: 2.6 m/s and no jump, which is a horror-pace guess rather than a measured one.
- **The warden**: sight 9 m, speed 1.9 m/s, reach 0.9 m, and five seconds of searching before it
  gives up. Speed is the one that matters — it must stay under the player's, and a test reads the
  player's authored speed out of the scene rather than repeating it.
- **The handcrafted room**: 12 × 16 × 3 m, one lamp, two crates. No longer what the game boots into,
  but still what the rule tests play and still where the pieces were cut from.

**The sounds, all new and all placeholders** — the descriptions are a table of frequencies at the top
of `games/warren/src/bin/sounds.rs`, and re-running it rewrites the `.wav` files:

- **`warden_breath`, peak 0.55, four seconds** — the one that matters. A slow low pulse with breath
  over it, spatial, so distance and direction tell you where the warden is. **Four seconds so the
  pulse is slow**; a shorter loop reads as machinery. If a chase feels unfair, this is the knob.
- **`warren_tone`, peak 0.14** — the room, lower and emptier than the Atrium's, non-spatial.
- **`footstep`, peak 0.4, and `STRIDE` 0.95 m** — how far you walk between steps. Set by arithmetic
  against the authored 2.6 m/s rather than by ear, which is the honest description of it.
- **`caught` at 0.75 and `escaped` at 0.5** — the two endings. `caught` is a tritone and is the one
  sound in the game allowed to be unpleasant.
- **`taken`, peak 0.45** — picking something up.

### One caution that stands, and one that is retired

- **The watcher in `games/atrium` has no collider and walks through pillars.** Deliberate and stated
  in `move_the_watcher`: giving it one would mean building a second character controller to prove a
  decision about AI. Do not "fix" it without deciding you want that.
- ~~`modules/amadeo-interaction` has no game using it.~~ **Retired.** The brass key is its first
  user, and the review was worth having immediately — it found the `body_of` defect described above,
  which no existing test could have caught. `modules/amadeo-inventory` was written with that user in
  the same session, ADR 0068's pattern rather than session 17's.

### Three things noted rather than fixed

- **Nothing can ask how wide a label will be.** `FontCache::shape` returns the width and nothing
  surfaces it, so a panel behind a label is sized by hand — the Atrium's title plate *and* its pause
  panel were both authored, looked at, and corrected by eye. **Two occurrences now**, which was the
  bar session 16 set for closing it.
- **`padding` is uniform.** Asymmetric padding needs a child's margin. A four-token version is
  additive and nothing written today would change.
- **There is no scrim token.** A pause menu over a bright scene wants a dimming layer, and the
  palette has no name for one — `Paint::Custom` would work and is precisely what ADR 0064 says not to
  use for chrome. The Atrium does without and reads fine, because the room is dim. An eighth token is
  the answer if a second game wants one.

> **On the text decision, because it is a calibration signal worth keeping.** I recommended the
> lighter option (rasterise a TTF into a glyph atlas) and listed `cosmic-text` as the heavier
> alternative. **Justin chose `cosmic-text`**, and he was right: the only argument for the lighter
> one was *scope*, and `CLAUDE.md` §5 has said since session 6 that he would rather have a complete
> engine than one that accumulates problems. When the sole case for the smaller option is effort,
> recommend the complete one.
>
> **Session 17 has the matching one in the other direction.** I recommended building pointer
> navigation and **Justin chose to defer it** for `amadeo-anim`. He was right there too, and it is
> the same rule read the other way: the *analysis* was the valuable part and it is written down in
> Q36, while the code would have served nothing in M3. "Prefer the complete option" means not
> leaving problems behind — not building everything immediately.
>
> **And a third, later the same session.** Offered four AI architectures with the research behind
> each, Justin took the recommendation — which is the pattern `CLAUDE.md` §5 already names, and the
> reason the burden is on the recommendation to have been *earned* rather than on the menu to have
> been offered. The recommendation there rested on a specific claim (the sequencer is the cheap half;
> the boundary is the expensive one and is identical for all four), and that claim is what made it
> safe to be wrong.
>
> Both readings together: **do the thinking, then ask what to build with it.**

### Q12 did not bite, and that is a finding rather than a non-event

Five sessions of notes predicted `kira::AudioManager` would be the first thing unable to satisfy
`Service: Send + Sync`. **It satisfies it fine** — manager and every handle — so no `LocalService`, no
`Mutex`, no relaxed bound. Checked by compiling the bound with a control case that fails, and pinned
by `the_backend_fits_in_a_service_without_a_mutex_or_a_local_store`.

**The reason is the reusable part**: kira's desktop backend hands the `cpal` stream to its own thread
and keeps a controller, and a library that already owns a thread has usually had to become
`Send + Sync` in order to. So aim the suspicion at libraries that want to be driven from *your*
thread — a script VM, a `wgpu` surface tied to a window — not at libraries that feel low level. Q12
stays open with kira struck off; see ADR 0060 §3.

### Eyeball calls waiting on Justin

None is blocking; all are cheap to change and all were tuned by looking (or listening) rather than
derived. **The theme ones are now a single file edit** — that is what ADR 0064 bought.

- **The watcher's numbers** — sight at 11 m, speed at 2.6 m/s (deliberately slower than the player's
  5, so a chase is something you can win), and four seconds of searching before it gives up. All in
  `games/atrium/src/lib.rs` and `atrium.scene`, all set by eye, all one line to change.
- **The Signage theme's numbers** — `games/atrium/assets/looks/signage.theme`, and the built-in copy
  in `Theme::signage`. The spacing scale is 4/8/16/28 and the type scale 52/26/19/13, both set by
  eye against one capture. **If the interface feels too airy or too cramped, this is the one place to
  change it**, and nothing else needs touching.
- **The title plate's size**, 218×72 in `atrium.scene`. Authored by hand because nothing can measure
  a label yet, and corrected once already by looking at a capture.
- **The three generated sounds** — a 60 Hz lamp hum at peak 0.5, a 55 Hz room tone at 0.16, and a
  180 ms footstep thud at 0.45, all from `cargo run -p atrium --bin tone`. Placeholders, meant to be
  replaced: drop a real `.wav` in with the same id and nothing else changes. The levels are a guess.
- **`STRIDE`, 1.9 m** — how far you walk between footsteps. Decides whether the gait feels like
  walking or like jogging, and it was set by arithmetic against a 5 m/s walk rather than by ear.
- **Camera `height`** — Scarp 1.6, Atrium 1.5. It now means *the point the camera aims at*, so it
  decides what sits mid-screen.
- **The Atrium's two demo lights** — a warm `PointLight` at intensity 22 and a shadow-casting
  `SpotLight` at 38. The lantern was at 90 first and blew the room out.
- **`shadow_distance`** — still 70 on the Scarp. Cascades decoupled near quality from it, so it could
  go much further now.

### The habit worth keeping, because it caught a real one

Session 15 shipped a shader that compiled locally and failed every GPU test on CI. **Run this before
pushing any `.wgsl` change** (also in CLAUDE.md §4b):

```
WGPU_BACKEND=dx12 WGPU_DX12_COMPILER=fxc cargo test -p amadeo-render --all-features --test capture
```

Windows CI has no GPU, uses WARP, and compiles through FXC, which is far stricter than the DXC or
Vulkan path a real GPU takes. **Ubuntu CI is no help** — with no software fallback it skips every GPU
test and passes regardless, so a green Ubuntu job says nothing about a shader.

---

## Session 18 — a save survives a patch, a key goes in a pocket, and a level generates itself

**Both decisions that were waiting on Justin, decided and built — and then a third.** Sixteen
commits, three ADRs (0069–0071), one new module, one new game.

Three questions resolved (**Q37**, **Q40**), one withdrawn as a misdiagnosis (**Q39**), and one
opened (**Q38**).

### What landed

1. **The measurement that reframed Q37.** The question recorded the expected fix as "restore
   leniently: default the missing fields", and claimed that alone would make a save survive an added
   field. It does not — leniency gets past the first error into a second, because a defaulted field
   is still hashed and the world is rebuilt correctly and *then* rejected. Pinned as a test before
   any decision was taken, which is what made the options honest.
2. **ADR 0069 — a save is a snapshot read leniently.** One format, two entry points. The integrity
   check becomes **conditional on a layout fingerprint** rather than dropped, so the common case — a
   player who has not updated — keeps the full check, and the strict path stays exercised by every
   ordinary load. The fingerprint **recurses through every type a field names**, because a component
   whose own field list is unchanged over a nested struct that grew still hashes differently.
   Missing fields come from the **field's** type; an enum is refused rather than guessed at; renames
   are a text file (Unreal's `CoreRedirects`, not migration code); everything is reported.
   Per-component `version` is written and read by nothing, so real migrations stay additive.
3. **ADR 0070 — an item is an entity.** Decided by *reading three passes rather than reasoning about
   them*: `collect_meshes`, `step_physics` and `propagate_transforms` all require a `Transform`, so
   taking a thing out of the world is removing one component and the audit that was this option's
   main cost does not exist.
4. **`games/atrium` has a brass key.** Walk up to it, press F, it goes in your pocket and leaves the
   world; it survives a save; you can drop it again. It is the first user of **two** modules.

### What the first user found, which is the point of having one

- **`amadeo-interaction` ignored the wrong entity.** It ignored the interactor; an interactor is
  normally a *child* with no collider, so the sweep started inside the parent's and returned it at
  `fraction: 0.0`. `Looking::at` was `None` for ever, which is indistinguishable from being too far
  away. Every existing test put the interactor on a lone entity with no collider anywhere — **the
  arrangement the module's docs called usual was the one nothing covered.** Fixed with `body_of`,
  and three tests added that fail when the old behaviour is put back.
- **Reach is a band around the interactor's forward line**, and whatever an object rests on blocks
  the sweep to it. That is why the Atrium's interactor is a child above the plinth top and why the
  key stands upright.
- **And then a claim I published turned out to be wrong, which is the more useful entry.** I wrote
  that an item on the floor was unreachable and that looking down was unbuilt, and put it in three
  documents before checking. An interactor is an **ordinary entity with an ordinary `Transform`**, so
  an authored pitch aims the sweep down and reaches a key on the floor with nothing built — pinned at
  −20° by `aiming_down`, with level and −35° pinned as misses so the angle reads as a tuning number
  rather than a switch. Only a *runtime-driven* pitch is missing. **When a component composes out of
  `Transform` and `Parent`, check whether the thing you are calling unbuilt is already authorable.**

5. **`games/warren`** — M3's exit gate, and the third module-with-no-user retired:
   `FirstPersonCamera` had existed since session 17 with no game behind it. It now has a **playable
   loop** (torch → key → door, with a warden that catches you — gate items 1 and 3) and a **HUD**
   saying what is in reach and how the run ended.
6. **ADR 0071 and a working level generator.** Q40's real question was not which algorithm but what
   the generator *produces*: I1 makes a seed-only level unauthorable, so it writes a **scene file**.
   Justin chose the room graph. Built in three layers, each tested on its own — `Socket` (authored,
   facing is the mechanism), `lay_out` (bounded, connected, always looped, 64 seeds), and `to_scene`
   (every entity a prefab instance, every piece declared). `cargo run -p warren --bin layout` writes
   one, `amadeo check` passes it, and it loads and draws.

### Three habits that paid, again

- **Break the fix and check the test fails.** Twice: the fingerprint's recursion (exactly one test
  failed, and the failure was the predicted one — a *good* save hard-refused) and `body_of` (exactly
  the three new tests failed and no old one, proving the gap was real).
- **Write the test expecting the wrong answer.** `contents` on a despawned container was expected to
  come back empty; it does not, and should not.
- **Read the code rather than estimating the cost.** ADR 0070's whole shape came from three query
  signatures, and the estimate they replaced was much worse than the truth.
- **Look at the output — and be careful what you conclude from it.** This one paid three times in
  one session, and cost once.
  - **Q39**: a black screen found by capturing and looking, against a fully green suite. But the
    *diagnosis* was wrong twice over and got filed as a P0 against two renderer faults that do not
    exist. Both errors had one shape — an observation promoted to a claim about the engine with no
    isolating test. The isolating test took ten minutes and disproved both. **A capture tells you
    something is wrong, never what.**
  - **The HUD**: three green tests asserted what its lines *said*, and the screen had no words on
    it — the font was never declared in the scene's `assets` block, so every line shaped to nothing,
    silently and by design. **Whatever a test asserts is not the thing the player sees.**
  - **The generator**: `amadeo check` reported `ok` on a scene that then refused to load, because a
    prefab instance needs `override Transform` rather than a bare one (ADR 0029). Schema-valid and
    wrong. **`check` is not a load** — `amadeo capture --ticks 1` is the cheapest one there is.
  - And the standing corollary: a capture needs a scene that **differs** from the ones that already
    work. Every scene this renderer had ever drawn had a sun in it, which is why nothing caught the
    one configuration the exit gate actually needs.

---

## Session 17 — the room stops, and a field it has never heard of moves

**Seventeen commits, four ADRs (0065–0068), three new crates.** Every named M3 subsystem now exists,
the exit gate's save-and-resume loop works, and **all four** named genre modules are built.

### What landed

1. **The focus is drawn** — the last line of ADR 0063, left undone. A focused `Panel` resolves to
   `Accent` and text inside it to `OnAccent`, **substituted in the draw pass** rather than written
   into a component, because `Focus` is hashed and an appearance must not be. The rule comes from the
   palette rather than from taste: ADR 0064 already documents `Accent` as meaning focus, so a menu
   authored knowing nothing about focus highlights correctly. Verified by rendering two identical
   panels with one focused and reading the pixels back.
2. **ADR 0065 — pausing is a per-system opt-in**, resolving Q35. See below; it is the session's
   expensive decision.
3. **A pause menu in `games/atrium`**, which is the first thing to use layout, text, focus, theme and
   pausing at once. Three buttons authored in `atrium.scene`, each carrying a `MenuButton` saying
   what it means.
4. **Q36 filed** — pointer navigation deferred, because ADR 0063's plan for it turned out not to
   work. See below.
5. **ADR 0066 — `amadeo-anim`**, from nothing. A clip animates a *reflected field*.
6. **ADR 0067 — a list item may have named fields**, which is the scene-format gap the first `.anim`
   file fell into.
7. **`anim.describe`**, built *before* the hole it closes had a chance to bite. ADR 0066 created two
   failure reports and nothing could read either — which is exactly the hole ADR 0060 had while it
   was being written, and which cost session 16 a follow-up commit. Served as `amadeo anim` too.
8. **Save and load**, which is M3 exit gate item 1's "save → quit → resume from save". The Atrium's
   pause menu has both; a resumed game and one that never stopped are proven to be the same game.
   **Building it found two defects immediately** — see below.
9. **A first-person camera rig.** `modules/amadeo-camera` had only the third-person half, and M3's
   exit gate is a first-person slice; `docs/05` names the rig as its own module precisely so neither
   perspective is privileged. Separate component, shared aiming system.
10. **`ShapeHit::entity`** — a cast said *where* it stopped and not *what* it stopped against, which
    serves two camera sweeps and nothing else.
11. **`modules/amadeo-interaction`**, which that unblocked: look at a thing, use the thing. M3 exit
    gate item 4.
12. **ADR 0068 — `modules/amadeo-behaviour`**, the last named genre module, plus a **watcher in
    `games/atrium`** that notices you, chases, searches and gives up.
13. **`StableHash for BTreeMap`**, which the above needed and which had never existed. Hashes in key
    order, and there is deliberately **no `HashMap` impl** — a component holding one now fails to
    compile rather than reproducing intermittently, which is trap 2 enforced instead of remembered.

### The engine wrote snapshots it could not read back

**An empty list had no spelling.** `inline_value` joined a list's elements with spaces, and joining
nothing gives the empty string — so an empty `Vec` anywhere in a value wrote as a field name with a
trailing space and *no value*, which this format does not have and which parses back as `Unit`.

Every registered event queue holds two empty lists at rest. So `amadeo snapshot` followed by
`amadeo status --from` **failed on the engine's own demo game**, and had done since events were first
registered in session 16. Nothing noticed because nothing had ever restored one.

An empty list is now `[]`, checked by name on the way in, exactly as `Unit` is `()`. The format
already had that shape; an empty list had simply been left out of it.

> **The reusable part:** any encoding with a "join the parts" path has this waiting in it for the
> empty case, and a round-trip test written against hand-made values will happily never contain one.
> It was found by building the feature that uses the format end to end.

### And `PhysicsBackend::reset` was unreachable — then turned out not to do what it says

Documented since ADR 0036 as the thing that makes a physics game snapshot-able, and **no game could
call it**: the backend is private on purpose, so the only callers were tests holding one directly.
`Physics::reset` is the pass-through. That is the *third* instance this session of a mechanism built,
documented as load-bearing, and unreachable from where it is needed.

Then the more interesting half. Its stated purpose is that a solver carrying another world's contact
caches simulates differently after a restore. **Measured against a settled, sleeping stack of six
dynamic bodies, a warm solver matches a cold one exactly.** That is ADR 0036's own contract paying
off rather than a surprise — `step` is handed the complete input and a backend may keep no state
which cannot be rebuilt from the bodies it is given, so the decision that makes physics deterministic
is what removes the hazard.

`reset` is still right to call: it drops **static geometry**, so a game that streams terrain does not
keep the ground of the level it just left. The docs now say what was measured, and the test *reports*
the contact-cache result rather than asserting it — a claim about somebody else's solver at a pinned
version is not something to fail a build over.

### The empty-value hole had three instances, and the third was found by looking

`inline_value` joins a value's parts, and **joining nothing gives the empty string** — so anything
empty wrote as a field name with a trailing space and no value, which this format does not have and
which reads back as `Unit`.

It bit three times in one session, in order of discovery: an **empty list** (every registered event
queue holds two, so every snapshot of `games/atrium` was unrestorable), a **one-element list** (`value
22.0` is one token and layer 1 has no schema to tell it from a list of one), and an **empty map**
(`Facts` in the new behaviour module starts empty, so a monster that had never perceived anything
could not be saved).

Three explicit markers now — `[]`, `{}` and the pre-existing `()` — and one rule: **a field with no
value is not something this format has.**

> **The transferable part is what to do on finding the first one.** Any encoding with a
> "just join the parts" path has this waiting in it for every empty case, and a round-trip test
> written against hand-made values will happily never contain one. Finding one instance is a reason
> to go looking for its siblings, not to close the ticket. The second and third were found by
> building things that used the format rather than by reasoning about it.

### A habit that paid four times today

**Break the fix and check the test fails.** It caught a test that proved nothing (the first version
of the physics one restored into a *fresh* app, where there were no stale caches to carry, so it
passed with the reset commented out). It confirmed the empty-list regression tests, and it confirmed
the `+ 1` offset on a packed entity — where without it the *first entity a world spawns* reports as
scenery, and that is usually the floor, so "what am I standing on" would have been the one question
with a wrong answer.

Three minutes each. Worth it every time, and the one that mattered most is the one where the test was
wrong rather than the code.

### Three engine-level mechanisms turned out to be unreachable from where they were needed

Not a coincidence, and worth stating as a check rather than three anecdotes:

| Mechanism | Documented as | Reachable by |
|---|---|---|
| `PhysicsBackend::reset` | "what makes a physics game snapshot-able" (ADR 0036) | only tests holding a backend directly — **no game** |
| `ClipCache::failures`, `Animatable::missing` | "the whole diagnosis" (ADR 0066) | nothing, until `anim.describe` |
| `ShapeHit`'s entity | — | it did not exist; a cast said *where* and not *what* |

**When you write a doc comment saying a thing is load-bearing, check that the thing can be reached
from where it is needed.** The comment is not the mechanism. All three were found by trying to build
something that needed them.

And the follow-up, which is the more interesting half: **when you finally measure what a mechanism is
worth, the answer may not be what the comment says.** `reset`'s contact-cache argument does not
actually bite — a warm solver matches a cold one exactly — because ADR 0036's own contract already
prevents the hazard it was written for. The comment now says what was measured.

### The two defects worth remembering, because they are the same defect

**A flag on a parent changes what its children mean, and every reader has to walk up.**
`UiNode::visible` is a field rather than a despawn so that toggling a menu does not move entities
between archetypes. The consequence is that `layout_ui` skips a hidden node *and its descendants*,
never overwriting the rectangles they had while visible — and every node inside a hidden menu still
says `visible: true`, because it is.

It bit twice, an hour apart:

- **the draw pass** kept drawing a closed menu's buttons off stale rectangles;
- **`focusable_in_order`** let the focus land inside a closed menu, so the next `confirm` would
  activate a button nobody could see — **which is the bug ADR 0063 names in its own consequences**,
  sitting in the code that named it.

Found by building a pause menu, which is what a demo is for. One shared upward walk answers both now.

### Q36: an ADR can be right about a hazard and still name a sink that reintroduces it

ADR 0063's consequences say pointer navigation belongs in "a presentation-side system that writes
through the same `Focus` resource", and that "a replay records the resulting focus moves rather than
the pointer that caused them". **Neither half works.** `Focus` is hashed, so a `Render`-stage system
writing it puts the pointer and the window size into the state hash — the exact I3 break the ADR
exists to prevent. And `InputChange` is `Button` and `Axis`; nothing in it can record a focus move.

Worth keeping as a *shape* rather than an incident: a decision can be completely right about where a
hazard comes from and still leave a door open into it, in the section nobody re-reads.

The replacement is written up in Q36 — the lockstep-RTS answer, which is that the interface sits
outside the simulation and the pointer resolves to a **command** that is what gets recorded.

### ADR 0066's surprise: animation is simulation

The reflex from `GlobalTransform` and `ComputedRect` says a computed value should be derived and
outside the state hash. **Animation is not.** A clip that moves a `Transform` is a moving platform you
stand on: physics reads it the same tick, a save restores it, and `docs/04` §14 requires hitboxes on
frames to reproduce. So the clock is hashed, what it writes is hashed, and `animate` runs in
`Simulation`.

Its consequence is sharper than it looks: **a missing clip changes the state hash**, which no other
missing asset in this engine does. A missing texture draws magenta and a missing sound is silence;
a missing clip means a platform does not move. `ClipCache` therefore has no placeholder, `load_clips`
installs itself, and `ClipCache::failures` plus `Animatable::missing` are the diagnosis.

### `amadeo check` paid for itself twice in one afternoon

Writing the first two `.anim` files found two format problems, and the validator named both against
the real schema rather than leaving them as symptoms:

- **a list of structs was unspellable** — ADR 0032's one missing shape, which the writer already knew
  about and had a `Debug`-form fallback for. Now ADR 0067;
- **a one-element list was unspellable** — `value 22.0` is one token and layer 1 has no schema, so a
  scalar track could not be authored at all. Fixed where it belongs: `Vec<T>::from_value` accepts a
  single value, the type resolving an ambiguity the text genuinely has.

The symptom of the second was "a lamp that did not flicker". The message was
`list<f32>: expected list, found 64-bit float`.

### Eyeball calls added this session

- **The pause panel's 300×194**, and the 36-pixel buttons in it. Authored by hand because nothing can
  measure a label, and corrected once by looking at a capture — the first version ran eight pixels
  past the last button and had its heading in `Dim`, which is the token for things you skim past.
- **The lantern's sweep** — 12 seconds, ±26° of pitch and a half-turn of yaw at 9s — and **the lamp's
  flicker**, 2.6 seconds between 19.5 and 23.5. Both set by eye against one capture, and both are one
  line in a `.anim` file to change.

---

## Session 16 — the engine makes a sound, and then grows an interface

**Sixteen commits, five ADRs (0060–0064), all green.** Two subsystems went from barely-started to
working: `amadeo-audio` was a trait and a null backend and is now a game you can hear, and
`amadeo-ui` did not exist and now draws a themed title screen you can navigate.

The twelve sections below are in the order they happened. The short version:

| | |
|---|---|
| **0060** | A missing sound is silence — there is no audible magenta — and the kira backend's only test is a person |
| **0061** | A one-shot is an event carrying a place, played once per event *sequence* |
| **0062** | Game UI is anchors plus flow; text is properly shaped with `cosmic-text` |
| **0063** | Focus is an **authored** order — a spatial one would put the window size in the state hash |
| **0064** | A theme is named tokens; the default look is **Signage** |

**Q12 was answered twice in the negative**, and the reasoning generalised into a prior for the next
candidate. **Four bugs are recorded below with their symptoms**, three found by a test failing and
one — the only one no test could have caught — found by looking at a picture.

### The audio half, and the wall Q12 predicted was not there

**`KiraAudio` is written and works**, behind a `kira` feature that is off by default like `gpu` and
`rapier`. `games/atrium` turns it on, on the same trade it already makes for rapier: a demo of the
audio system that makes no sound is not a demo.

The mechanical part was as advertised — `VoiceTracker::reconcile` hands back
`stopped`/`started`/`updated` and applying them is a loop each. What took the thinking was three
things ADR 0060 records, none of which is about kira.

### 1. Q12 was wrong about kira, and the reason is worth more than the answer

Five sessions of notes said `kira::AudioManager` would be the first thing unable to satisfy
`Service: Send + Sync`, and offered three ways to cope. **It satisfies the bound**, along with
`StaticSoundHandle`, `TrackHandle`, `SpatialTrackHandle` and `ListenerHandle`.

Checked by compiling the bound rather than by reading kira's source — **and with a control case,
because a probe that cannot fail proves nothing.** The control (`Cell<u32>`) failed and nothing else
did.

The reason: kira's desktop backend does not hold the `cpal` stream. It hands it to a stream-manager
thread and keeps a controller. **A library that already owns a thread has usually had to become
`Send + Sync` in order to** — which points the suspicion, for the next candidate, at libraries that
expect to be driven from *your* thread rather than at libraries that feel low level. `mlua::Lua` and
`wasmtime::Store`, the two that genuinely failed in the Q1 spike, are both the former.

Q12 stays open with one example struck off. Deciding it now would be deciding it speculatively, which
its own entry has warned against since it was written.

### 2. There is no placeholder sound, and there must not be one

ADR 0021 wants a **visible stand-in plus a structured report** for a missing asset, and `TextureCache`
ends its fallback chain in a magenta check built in code so the last resort cannot itself be missing.
`SoundCache` implements only the second half.

Magenta works because **nobody ships magenta** — it is unmistakably not content. Nothing audible has
that property. A beep, a tone, a click: each is indistinguishable from something a game might
legitimately play, and unlike magenta it would *repeat*, at the volume and in the position the
missing asset would have had. A placeholder sound turns a broken asset into a design choice.

So a sound that will not load is silent and `SoundCache::failures` is the whole diagnosis. That is a
real weakening — a silent game is less obviously broken than a magenta one — taken because the
alternative is worse in the case that matters. ADR 0060 states it as the general rule rather than as
an audio exception: **ADR 0021 wants a stand-in that is legible as a stand-in, and where no such
thing exists, the report is the whole answer.**

### 3. When nothing can test it, commit the procedure instead of the intention

This is the first subsystem in the engine whose output leaves the process. The tempting move is a
test that *looks* like it covers the backend — submit a frame, assert no error, call it `sound_plays`
— and that is worse than nothing, because it turns "unverified" into "verified" for whoever reads the
test list next.

Two things instead, and both generalise:

- **The judgement lives where CI can reach it.** `VoiceTracker` was pulled out in session 15 for
  exactly this, and `kira_backend.rs` is left with "start this, stop that, set the other". **Do not
  move reconciliation back in.** When adding an audio feature, ask which of the two files it belongs
  in; the answer is almost always the tracker.
- **The listening procedure is committed**, as two `#[ignore]`d tests in
  `crates/amadeo-audio/tests/you_can_hear_it.rs`. They open a real device, play for a few seconds,
  and print the acceptance criteria before starting. `#[ignore]` keeps them out of CI; being in the
  repository rather than in a shell history is the point. They still assert everything up to the
  speaker, and where a claim they watch for *can* be checked one layer down, it is —
  `the_tracker_agrees_with_what_that_procedure_expects` is not ignored, and if it is red there is no
  point listening.

Every one of those files says plainly that the last step is a person's. That is the load-bearing
part, not modesty.

### 4. `audio.describe`, because ADR 0060 had a hole in it while it was being written

ADR 0060 decided a sound that will not load is **silent**, with `SoundCache::failures` as the whole
diagnosis. That is only true if something can *read* the report — and nothing outside Rust could.
`assets.list` reports *load* failures; nothing has ever reported a *decode* failure, for textures
either.

So `audio.describe` is `render.describe`'s counterpart, served over RPC and as `amadeo audio`. The
asymmetry between them is the point: **a blank screen has an obvious symptom and silence has none.**
Nobody notices a quiet game, and every cause is invisible from outside.

Three things about it worth keeping:

- **It reads the world, not the last frame.** `NullAudio` remembers what it was given and a real
  backend does not, so reading back from a backend would work headlessly and answer nothing about the
  game somebody is actually playing.
- **`collect_audio` and `describe_audio` share one frame builder.** Two copies of "what should be
  audible" is the fifth instance of this project's recurring failure, and here it reports a game
  playing something it is not — worse than no answer. `describing_agrees_with_what_was_actually_
  submitted` is what says so rather than the comment.
- **`silent_because` orders its causes deliberately.** No listener is reported *before* no voices,
  because a world with no ears submits none and the second is the symptom. The null backend is
  reported **last**, because it is almost always true of a headless run and putting it first would
  bury a real fault.

`amadeo audio --package atrium --ticks 5` now prints both voices, the listener, and "SILENT — the
null backend is installed, which makes no sound by design."

**And it turned up the same hole one layer over, which is now closed too.** `TextureCache::failures`
has existed since M1 and **nothing outside Rust ever read it**: `assets.list` reports a file that
would not *load*, and a file that loaded fine and would not *decode* had no channel at all. So "why
is this magenta" was answerable only by reading source — exactly the gap invariant I5 exists to
prevent, sitting in the oldest introspection method in the engine. `render.describe` now carries
`texture_failures`, omitted when there is nothing wrong.

Worth noticing *how* that was found: not by auditing, but by building the audio equivalent and
noticing the shape was missing next door. **The general form is in `docs/07` now** — an introspection
method must share the code it describes, and a structured report with no reader is not a report.

### 5. One-shots, which closes the gap ADR 0059 named and refused to guess at

ADR 0061. A footstep is not a state, so it is a **`SoundPlayed` event** — and the reason that works
is that the boundary the problem has is already the boundary the mechanism has. `Event` requires
`StableHash` because *"queued events are part of simulation state at a tick boundary"*, so **deciding
a footstep happened is in the state hash and reproduces in a replay**, while playing it is a service
and is not. Nothing had to be invented.

Three things to know:

- **It carries a place, not an entity.** A `Voice` is keyed on the entity making it because a voice
  continues; a one-shot has no identity on purpose, because an identity invites a backend to decide a
  footstep is "still playing" and decline the next one. Following is refused for now with the
  reasoning written down — every major engine offers both and defaults to the fixed position.
- **Played once per event *sequence*, not once per frame.** `collect_audio` runs in `Render`, buffers
  swap per *tick*, and the loop renders uncapped — so one footstep sits in the readable buffer across
  every frame drawn that tick. Read naively it plays five overlapping copies at 300 fps and one at
  60, which is a bug whose symptom depends on the frame rate.
- **`Stride` is a hashed resource, not a service**, so a save restores you mid-gait rather than
  resetting it.

`games/atrium` owns `Stride` and `play_footsteps`; `modules/amadeo-character` knows nothing about
them. The module knows how to move, the game knows what moving sounds like.

> **A bug worth not re-introducing, and it is now in `docs/07`.** The first version tracked "the
> highest sequence already played", initialised to 0, filtered with `>`. `EventClock` starts at
> **zero**, so event zero — the first sound a world ever makes — was dropped forever, and everything
> after it worked. Nearly undiagnosable by ear. The fix is the half-open bound (`next`, `>=`), which
> has no special case at zero, and
> `the_very_first_one_shot_a_world_ever_sends_is_heard` is the regression test.

### 6. `amadeo-ui` exists, and its layout works

ADR 0062 settled the two decisions `docs/04` §13 had carried since M0, and the layout half is built:
anchors, flow, `grow`, and a `ComputedRect` per node, with eighteen tests.

**Anchors *and* flow, because there are two problems.** A HUD is placement — a health bar belongs at
a screen corner regardless of what else exists. A menu is flow — the buttons take their positions
from each other. Unity, Unreal and Godot all ship the pair, independently, and that convergence is
the argument.

Three things worth knowing before touching it:

- **A flow node has no size of its own.** There is no intrinsic sizing, deliberately, so a bare
  `UiNode { flow: Column }` is a 0×0 box whose children are centred in *nothing* and land at negative
  coordinates. `UiNode::column`/`row` therefore also set `Anchor::fill()`. **Found by a failing test
  rather than by reasoning**, which is why the constructor now bundles the three decisions.
- **Screen space here is +Y down, origin top-left** — the opposite of ADR 0018's world convention, on
  purpose, because "twenty pixels from the top" is what a person means. It is the seam most likely to
  be got wrong and a layout with the flip backwards is plausible and upside down;
  `each_corner_anchors_where_its_name_says` is what catches it.
- **`ComputedRect` is `DERIVED`**, and it matters more here than for most derived data: layout depends
  on the *window size*, so a game at 1920×1080 and the same game at 1280×720 must not be two
  different worlds.

Written here rather than taken from `taffy`, and the reason is not invented-here: flexbox is a
*document* layout spec, most of it is machinery a HUD never touches, and adopting it would put a model
we did not design at the centre of the UI system. The subset that matters is one top-down pass with
no measure step — which is what makes it followable, and `CLAUDE.md` §6 makes that a real constraint.

### 7. Text is shaped, and the test font is generated

`FontCache` wraps `cosmic-text`. **Measuring a string is shaping it** — there is no cheap
`width_of(text)`, and that is the honest consequence of choosing real text layout over a glyph atlas.

Two things about it are decisions rather than details:

- **`default-features = false` is load-bearing.** cosmic-text's defaults read the *operating
  system's* font database, and a game that falls back to whatever happens to be installed looks
  different on every machine — and correct on the developer's. `FontCache::new` starts with an empty
  database, so a game ships its fonts and a missing one is reported.
- **A missing font shapes to nothing, never to a substitute.** ADR 0060's rule a third time: a wrong
  typeface quietly standing in for the right one is how a game's look drifts without anyone noticing.

**The test font is built in code** (`test_font.rs`), which is `pix` and `tone` applied once more —
no licence, no binary blob, no dependency on what is installed. Writing a valid TrueType file turned
up two traps worth knowing, and both present identically as *"the font does not load"* with no
further detail:

- **OS/2 version 4 is exactly 96 bytes.** Subscript and superscript are **four** fields each, not
  five. Five made the table 100 bytes and shifted everything after it.
- **`fontdb` requires a PostScript name** (name id 6) as well as a family, and returns "unnamed font"
  without it.

`the_generated_font_parses` is what caught both, and it earns its place for a specific reason: with a
malformed font, every shaping test passes *vacuously* by producing no glyphs — which is exactly what
a missing font produces and indistinguishable from it.

### 8. The glyph atlas, and the renderer needed nothing new

`GlyphAtlas` rasterises each glyph once into a shared 1024-square texture. **A glyph is a tilesheet
tile**: `Sprite::region` has existed since ADR 0023 and batching is already on
`(sort order, texture)`, so a page of text is *one* draw call and no new pipeline was required.

- **White RGB with the coverage mask in alpha.** A rasterised glyph says how much of each pixel the
  outline covers and nothing about what colour the text is — so `Sprite::color` tints it, and one
  atlas serves every colour of text in the game. Baking colour in would need an entry per colour per
  glyph.
- **A pixel of padding is not optional.** Filtering samples just outside a region, so glyphs packed
  flush against each other show a faint sliver of the neighbouring letter — an artefact that gets
  blamed on the font.
- **Shaping rasterises**, deliberately. Splitting them would need `PositionedGlyph` to carry the
  shaper's own cache key, which is exactly the foreign type ADR 0036 §4 keeps out, and a glyph that
  is measured is very nearly always a glyph that is drawn.
- **Shelf packing**, chosen for legibility over skyline or MaxRects. Text at menu sizes fills a few
  percent of the atlas, so better packing buys nothing; if one is ever exhausted the answer is a
  second page, which callers would not notice.

`the_atlas_really_contains_the_glyph_and_not_just_a_reservation` samples the middle of a rasterised
box and asserts it is nearly opaque. Everything else in that file would pass against a packer that
allocated regions and copied no pixels — and the symptom of *that* is invisible text, which is
indistinguishable from a missing font, a wrong colour, or a layout bug.

### 9. The draw pass, and the seam it needed

`Panel` and `Text` are components; `collect_ui` turns laid-out nodes into a `View`. Two decisions,
both flagged here because they were made without asking:

- **The renderer owns the slot and `amadeo-ui` fills it.** `amadeo-ui` sits *above* `amadeo-render`
  (I6), so `render_quads` cannot go looking for a `UiNode`. `amadeo-render` grew an `Overlay`
  service holding `Vec<View>`; `render_quads` **drains** it and merges by camera order. This is
  `TextureCache`/`MeshCache`/`SkyCache`'s inversion a fourth time, so it is the established idiom
  rather than a new one. Draining rather than reading is deliberate: a stale interface frozen over
  the game is worse than no interface, and it would look like a rendering bug rather than a missing
  system.
- **The UI camera is synthesised, not authored.** Orthographic, height = the screen height, eye at
  `(w/2, -h/2)`, so `x ∈ 0..w`, `y ∈ -h..0` and **a pixel is a unit**. A game does not choose
  whether its interface is in screen space, and an authored UI camera would be one more thing every
  game had to remember and update on resize.

**Nothing spawns an entity, and that is not tidiness.** Entities are simulation state, so a
paragraph of text would move the state hash — and move it *differently at two window sizes*.

Screen-to-world is one line (`[sx, -sy]`) in one file, because a flip applied twice, or in half the
cases, gives a layout that is plausible and upside down.

**And then somebody looked at it.** `tests/it_draws.rs` renders the interface offscreen and reads the
pixels back. That was written because every other assertion about UI is about *numbers*, and the
numbers and the picture are joined by a projection, a camera and a shader that no unit test touches —
which is exactly how a voxel mesher kept correct normals and inside-out winding for two sessions with
a green suite.

It passed first run, which is worth recording rather than glossing: the y-flip, the pixel-is-a-unit
projection, the overlay merge and `Panel::order` were all right the first time. **Opposite corners
are asserted in two separate tests**, because a single corner passes against a picture flipped on the
other axis.

### 10. And then text was drawn, which found a hole in the agent's eyes

Justin supplied **Bebas Neue** (SIL OFL, © Dharma Type) — a condensed display face with real
character, and about as far from `CLAUDE.md` §6's forbidden defaults as a typeface gets. It lives in
`games/atrium/assets/fonts/` **with its licence beside it**, which is what the OFL requires of a
redistributed font.

`games/atrium` now shows a title plate and "THE ATRIUM" over the 3D room, and **the whole HUD is
authored in `atrium.scene`** — so ADR 0062's claim that a menu is a scene file is cashed rather than
asserted.

Two things came out of actually looking:

- **`render.capture` could not see the interface at all.** `capture_to_png` called `render_quads`
  directly, so it skipped every other `Render`-stage system — including the one that fills the
  overlay. The agent's eyes were blind to anything a game contributes to a frame beyond its own
  cameras. It now runs the stage, and only calls `render_quads` itself when the game did not (which
  it checks, because a second pass would draw a frame whose overlay had already been drained and the
  interface would vanish *only in captures*).
- **The first capture had the plate running 100 px past the text.** Fixed by looking, not by
  reasoning — and it is the first time the "no intrinsic sizing" trade has actually been felt. A
  container cannot hug its text, so the width is authored. That is the deliberate design (`layout.rs`
  says so), and it means **there is no way to ask the engine how wide a label will be**. Worth
  closing if it bites again: `FontCache::shape` already returns the width and nothing surfaces it.

### 11. Focus navigation, and the reason it is not what a tutorial would do — ADR 0063

A menu you can move around in and choose from. The interesting part is what it **refuses** to do.

The obvious design — find the widget under the pointer, highlight it, act on click — **cannot be part
of a deterministic simulation here**. Hit-testing reads a `ComputedRect`, ADR 0062 made layout depend
on the window size, and `ComputedRect` is `DERIVED` precisely so two resolutions are not two
different worlds. So "which button is under the pointer" answers differently at 1920×1080 and
1280×720, and the moment that reaches the state hash, invariant I3 is gone for every menu in every
game built on this engine.

So **`Focusable::order` is a number somebody writes in a scene file**. Navigation reads no rectangle,
no pointer and no screen size, which buys three things:

- identical at every resolution — the property that makes a menu part of the simulation at all;
- driven by **named actions**, which `InputState` already hashes and replays already record, so **a
  menu replays with nothing new and no change to the replay format**. The alternative was recording
  pointer positions, which would have made replays resolution-dependent;
- works with no cursor, on a controller, which a console-facing menu needs anyway.

`Focus` is a hashed **resource** — the one hashed thing in `amadeo-ui`, and correct rather than
inconsistent: where the highlight sits is gameplay, it moves only through recorded input, and a save
should restore it. `UiActivated` carries the entity, because the engine does not know what a button
*means* (I4, one level up — the same split footsteps use).

**Pointer and spatial navigation are still possible**, and ADR 0063 says where they go: a
presentation-side system, outside the deterministic zone, writing through the same `Focus`.
`ComputedRect::contains` is the right primitive — it was the *placement* of the logic that would have
been wrong, not the function.

> **Six tests failed at once and the code was right.** `just_pressed` is edge-triggered, and the test
> helper never released the key — so every press after the first read as *held*. The fix was in the
> helper, and it turned up a real property worth pinning:
> `holding_a_direction_moves_once_rather_than_scrolling`. There is no key repeat, deliberately:
> repeat is a *timing* feature, and timing is what a fixed tick expresses worst.

### 12. Theming — ADR 0064, and both halves were Justin's

Four directions were **mocked up rather than described**, because choosing a look from prose is a bad
way to choose a look. He picked **Signage**: bone on near-black, safety orange, zero rounding, tight
leading — wayfinding rather than software, and built for the Bebas Neue the engine already ships.

He also picked the deepest of three theming depths: **named tokens for colour, type *and* spacing**.

- A widget says `paint Accent`, `scale Title`, `padding Snug`. Nothing in `atrium.scene` states a
  colour or a size any more.
- **Padding and gap are density; margin is placement.** That is why `UiEdges` survives for margin
  alone — before the theme they were one type, which made a density knob and a coordinate look
  identical in a file.
- Seven colours, four type steps, five spacing steps. Deliberately few: a palette nobody can hold in
  their head is one whose greys drift apart.
- The default is **built in code**, `TextureCache`'s argument a third time — a last resort that is
  itself a file cannot cover the case where files are the problem.

`Theme` is one type that is both a `Component` (so a `.theme` file can hold it, exactly as
`.material` and `.environment` do) and a `Service` (so it is outside the state hash — two players
with different themes must simulate identically).

Two things worth keeping:

- **Colours are written in sRGB and converted once.** `0.0044` does not read as "near-black" to
  anyone, and the conversion is not linear: sRGB `0x80` is **0.216**, not 0.5. A theme that assumed
  otherwise would be visibly washed out beside a texture of the same value, which is pinned.
- **Four layout tests broke and the fix improved them.** They asserted literal pixels that had come
  from literal padding. They now ask the theme what `Snug` means — which is correct regardless, since
  otherwise retuning the spacing scale breaks the suite and cheap retuning is the entire point.

`App::read_component_assets` became public on the way: `amadeo-ui` sits below `amadeo-scene` and
cannot parse its own asset (I6), the same bind `amadeo-render` was already in for `.material`. A
component-shaped asset is a general idea rather than a renderer one.

### What else landed

- **`SoundCache`** — id → bytes → samples, `TextureCache`'s third instance. This is what was missing
  for a `.wav` to get off disk: the decoder existed and nothing called it from the asset system.
  Decoding is lazy for the same reason texture decoding is, and a failure is remembered so a broken
  file costs one decode rather than one per frame forever.
- **The Atrium hears things.** A `lamp_hum` on the lamp (spatial) and a `room_tone` from nowhere
  (non-spatial) — **the backend's two code paths, chosen deliberately because no test can tell them
  apart.** Ears on the **camera** rather than the character: third person, so the viewer hears what
  they can see. `the_room_is_heard.rs` pins that choice, since moving it would be legitimate and
  would sound different.
- **`cargo run -p atrium --bin tone`** generates both `.wav` files from a table of frequencies —
  `games/vault`'s `pix` argument applied to audio, since a `.wav` is not diffable either. It uses
  `amadeo_core::sin_cos_degrees` rather than `f32::sin`, **not** for ADR 0044's reason (nothing here
  reaches the state hash) but for the mundane one that a generator whose output can differ from
  itself is not a build step. Every partial completes a whole number of cycles, which is what stops
  the loop clicking; the generator refuses a frequency that would not, naming the one to change.
- **CI installs `libasound2-dev` on Linux.** `cpal` links ALSA there, and feature unification means
  the whole workspace build wants it. A build requirement, not a runtime one — the runner still has
  no device.
- **`WavError` gained `Clone`**, because `SoundFailure` holds one and a remembered failure is handed
  out to whoever asks why a sound is silent.

### One process note worth not repeating

The `walking_away_from_the_lamp` test's threshold was checked against measured numbers rather than
guessed — session 15's lesson about capture thresholds, applied. It is 11.56 → 13.81 metres against a
1.0 metre margin, and the margin is now written next to the assertion.

Getting those numbers cost a self-inflicted wound: reading the file back through PowerShell's
`Set-Content` to patch the assertion silently mangled every em-dash in it. **Use the Edit tool for
files with non-ASCII text**; this is at least the second time.

---

## Session 15 — the camera fixed, and a determinism hole nobody knew was open

**Justin played the Scarp and reported two things about the camera. Both were real, and the second
one turned out to be sitting on top of an invariant violation.**

1. **"Any movement in any direction makes the camera flicker close or far."** The upward sweep to the
   camera's pivot starts at the parent's origin — which is the middle of **the parent's own capsule
   collider** — and never said to ignore it. Rapier reads that penetration as a slope too steep to
   stand on, reports `sliding_down_slope`, and cancels the motion, *intermittently*, because whether
   it resolves that way depends on the contact normal and the normal moves as you walk. So on about
   one tick in ten the pivot collapsed to the player's feet, the arm snapped to its 1.2 m minimum, and
   the ease-out at 0.1 m/tick never covered the 5.8 m back before being knocked down again. **The
   camera had never once reached its authored distance while moving.** One missing `.ignoring()` —
   the call `modules/amadeo-character` had always made, one crate away, with a comment explaining why.

2. **"Pointing the camera down means looking at the ground; up means looking up from where the camera
   is."** Not a wrong line — a missing concept. The camera's position was the constant
   `[0, height, distance]` and pitch reached it nowhere, so tilting spun the camera on the spot. It is
   now an **arm**: pitch is an angle *around the pivot*, so tilting down lifts the camera over the top
   to look down at the player and tilting up drops it. Unreal's spring arm and Cinemachine's orbital
   rigs both work this way and it is what "the subject stays framed" means.

**And the orbit is what found ADR 0053.** Placing something at an angle needs `sin`/`cos`, and the
result lands in a **hashed** `Transform`, which ADR 0044 forbids. Looking for where to put a
deterministic version turned up the fact that **the camera was already violating it**:
`keep_camera_clear` built a matrix from the parent's rotation via `Mat4::from_euler_degrees`, which
used `f32::sin_cos`, and wrote the projected result straight into hashed state.

`crates/amadeo-transform/src/matrix.rs` **had described that exact route in its own header** as the
"side door" back into the state hash — and guarded the lesser risk (SIMD) while leaving it open. See
the general lesson in `docs/07`; the short version is that a documented hazard is not a mitigated one.

> ### The scope decision, and it was Justin's
>
> Put to him as camera-only (safe, no pixel can move) versus engine-wide (`Mat4` adopts it, every
> matrix in the engine shifts by about a bit, all 23 GPU capture tests at risk). **He chose
> engine-wide**, and it is the better answer because the camera was not special — any system that
> reads a `GlobalTransform` and writes it back into a `Transform` reopens the hole, and asking every
> future caller to remember is the arrangement that had just failed.
>
> **The risk did not materialise.** The whole suite passed unchanged on the first run: all 23 capture
> tests, the pinned rapier state hash, every golden replay. Rotations in those fixtures are zero or
> quarter turns, where the two agree exactly — and where the new one is *exactly* right and the old
> one was not.

**Three things to know before touching the camera again:**

- **`height` changed meaning.** It is now *the point the camera aims at*, not how high the camera
  floats — so it decides what is in the middle of the screen. Both games came down (Scarp 3.0 → 1.6,
  Atrium 2.8 → 1.5) to stop aiming a metre above the character's head. **Tuned by eye against a
  capture, not derived.** Worth a look and a re-tune.
- **`CameraArm` is a new component** and both scenes author it (Q32 churn, and honest churn). It holds
  the smoothed arm length, which must survive to the next tick and cannot be recovered from the
  transform once the arm leans — local `z` is `distance × cos(pitch)`, close enough to a distance to
  pass a tolerance and wrong enough to make a test mean something else.
- **One old test was passing because of the bug.** `the_camera_is_pulled_in_when_the_sweep_hits_
  something` forced its obstruction with a four-metre probe sphere, which centred on a pivot three
  metres up **contains the player's capsule** — so it hit the player, not the ground, and broke the
  moment the sweep started ignoring the parent. It now forces the case geometrically instead.

**Both new tests were watched failing**, and the messages are the reported symptoms almost verbatim:
*"got 1.2 — the arm is being knocked down faster than it can ease out"* and *"tilting down must lift
the camera above where it was (2.9999995 to 2.9999995)"*.

### Then Justin played it again, and the third report closed Q34

**"Pointing the camera upwards, the view always ends up showing the skybox."** With a screenshot: a
dark mass filling the frame over a pale band. That is the camera **under the terrain**, looking at its
unlit underside (ADR 0052) with sky past the edge — the third time this session a camera in the wrong
place read as a rendering fault.

**It was the projection workaround failing on its own terms**, and it is a good worked example.
Session 14 fixed the camera's first flicker by projecting the swept travel onto the arm, because
`move_shape` *slides* and a sideways slide was counting as progress. That correction cannot survive
the case where the slide goes **along** the query direction: tilted up, the arm points down and back,
it hits the ground, slides *backward*, and backward is 0.87 of the arm. Measured — the arm shortened
from 7.0 to 6.86 for a shape that had gone nowhere, and the camera was placed 0.057 m **below** the
surface where its own 0.35 m probe radius should have held it clear.

**ADR 0054 gives `PhysicsBackend` its fourth operation, `cast_shape`, and closes Q34.** Sweep a shape
along a line, get the fraction travelled, the position — *on the line by construction* — and the
surface normal. `None` means clear. The camera uses it for both sweeps and has **no workarounds left**:
the projection is gone, and `.ignoring()` is now a plain statement about which body the sweep starts
inside rather than a dodge around an unstable answer.

Same case after the fix: **0.371 m above the ground**, which is the probe radius plus the skin — the
sphere resting exactly on the surface. Captured and looked at, and it is an ordinary low-angle
third-person shot.

> **The rule worth carrying: two corrections on one call means the question is wrong.** By the end
> that sweep carried both a projection and an exclusion filter. The general form is in `docs/07`.

### And then shadow cascades landed — ADR 0055, which completes ADR 0045's tier 1

**The last item on the renderer list is done.** `ShadowMode::Cascaded { blend }` ships, `games/scarp`
uses it, and the plan below is now a record of what was built rather than what to build.

Everything the plan predicted held, including the trap it named: **the bias has to be per cascade**,
because it lives in clip depth and a ten-metre box and a seventy-metre box turn the same authored
offset into very different numbers. `fit_cascade` dividing through each box's own range gives that
for free, and a test pins it.

**Three things the plan did not predict**, all worth knowing:

1. **The blend became a payload on the variant rather than a field on `DirectionalLight`** — which
   ADR 0032's enum payloads make spellable in a scene file. That means **no `.scene` that did not opt
   into cascades changed at all**, so Q32 did not bite a fourth time. First time the *shape* of a
   change has dodged it rather than the churn being absorbed.
2. **A shadow map is now always a texture array**, one layer when `Orthogonal`. That is what keeps
   this to one shader and one pipeline — `texture_depth_2d` and `texture_depth_2d_array` are
   different binding types. The layer count lives *inside* `TargetFormat::ShadowMap32`, so the
   transient pool keeps a one-layer and a four-layer map apart with no new code.
3. **Measured, both ways, on the same scene:** 71.7 µs → 113.7 µs of GPU time, about 1.6× and still
   0.7% of a 60 Hz budget. The three extra shadow passes each cost *less* than the first, because
   they draw the same casters into smaller boxes. Full table in `docs/10-frame-budget.md`.

> ### ⚠️ The bug it shipped through, and it is the fourth of its kind
>
> The first capture with cascades on came back with **a huge dark wedge across the horizon**. Nothing
> failed to compile, nothing failed wgpu validation, every headless test passed.
>
> `mesh.wgsl` and `sky.wgsl` read **the same uniform buffer at the same binding** and each declared
> its own copy of the struct. Making `light_view_projection` an array of four grew that struct by 192
> bytes in one copy and not the other, so the sky shader read its direction vectors from the wrong
> offsets and drew the sky facing somewhere else.
>
> `view.wgsl` now holds the declaration once and is prepended to both at pipeline creation. **One
> copy is left and cannot be removed this way**: `GpuMeshView` in Rust. A `#[repr(C)]` struct and a
> WGSL struct are two statements of one layout in two languages, and only a wrong picture says they
> disagree — so add a field to both, in the same position, and then capture something and look.
>
> This is the same shape as the winding/normals pair, the two-sided apron, and `format_float` being
> borrowed rather than copied. **Two copies of one fact drift, and a comment saying "keep these in
> step" is not a mechanism.**

### Then bloom, because it was authorable and read by nothing — ADR 0056

`Environment::Bloom` has had two authored fields, a schema and a line in every `.environment` file
this repo ships **since ADR 0034**, and the renderer ignored it completely. A scene could ask for
bloom and get nothing, with no error and a `describe` that reported the field as meaningful. Same
defect shape as Q32's silent asset and Q31's forgotten `color_space`: **the file format promising
something the engine does not deliver.**

Three graph passes now — bright, blur x, blur y, at half resolution — composited by the post pass
**between exposure and tonemapping**, which is what the HDR scene target has existed for since ADR
0034 and what nothing had exercised. A glow added after tonemapping is a grey wash; added before it,
it is light.

**Off by default and byte-identical when off**, pinned as bytes rather than "close", because close is
also what an accidental extra full-screen pass looks like.

> **The Scarp does not use it, and that is the finding.** Its daylight scene has nothing above the
> threshold after exposure, so bloom at a sensible threshold changes **not one byte** — and at a
> threshold low enough to catch something, it washes the whole picture out. Both were captured and
> looked at. Turning on an effect that either does nothing or makes the picture worse is not an
> improvement, so `scarp.environment` keeps `intensity 0.0`.
>
> What bloom is *for* is M3's exit gate — a dark corridor with a moving flashlight, a few genuinely
> bright sources in a mostly dark frame. **This engine does not have a scene like that yet**, which
> is the honest reason the feature ships untested by a game.

**Known limitation, named rather than discovered later:** nine taps at half resolution reach eight
full-resolution pixels — a tight bright halo, not a broad haze. Widening it is a **downsample chain**,
not a bigger kernel, and it is a change to those three passes alone.

### And then lights at a place — ADR 0057, the last structural gap before M3's gate

The engine had **exactly one kind of light** since M2: directional, with no position, so nothing in a
scene could be lit *from somewhere*. M3's renderer exam is a dark corridor with a moving flashlight,
which is a spot light. `PointLight` and `SpotLight` now exist as authored components.

**Put to Justin as a scope question and he chose lights first, shadows after** — so the component
shape, which lives in scene files and is the expensive-to-change part, gets settled and used before
shadows complicate it.

Decisions I made and flagged rather than asked, all behind `RenderBackend` and cheap to revisit:
**forward with a fixed array of eight**, not clustered (worth its machinery north of a few dozen
lights, and a lit room is under eight) and **not deferred** — deferred fights ADR 0051's MSAA, which
ADR 0050's low-poly direction depends on. Over eight, the nearest win, measured to the light's
*reach* so a big distant lamp beats a small near one.

> **They cast no shadows.** Everything a lamp lights, it lights through walls. That is the honest
> state of it and it is visible in the Atrium's capture as plainly as the light is. A spot's shadow is
> a second map with a perspective projection; a point's is six faces of a cube. Both want an atlas,
> and `TargetFormat::ShadowMap32`'s layer count already has the shape for one.

Two things worth knowing: a point light is stored as a **spot whose cone is the whole sphere**, so the
shader has no branch on kind — but they stay two *components*, because an author should not be typing
`inner_angle` on a bulb. And the Cook-Torrance BRDF moved out of `fs_main` into `direct_light`,
because a sun and a torch differ in exactly two things and two copies would drift into a material that
looks right under one and wrong under the other.

`games/atrium` gained a warm lamp, on the rule that a feature nothing uses is a feature nobody has
looked at.

> **A test caught me being sloppy, which is worth recording.** The first spot-light test asserted the
> floor outside the cone was `< 40`. It failed at 94 — and 94 is what **ambient light alone** gives,
> because a camera naming no environment still gets the neutral cube map. So the assertion was
> unsatisfiable, *and* the companion point-light assertion (`> 90`) had been trivially true and would
> have passed against a light the renderer ignored entirely. Both now compare against a captured
> unlit baseline. **An absolute pixel threshold in a lit scene is almost always measuring the
> ambient.**

### And then spot lights started casting — ADR 0058, which finishes the flashlight

The other half of the scope decision. A spot light's shadow is **a layer of the same texture array
the cascades use**, because all four bind groups are already spoken for — view, shadow, material,
environment — and a second shadow texture would have nowhere to bind.

`View::shadow_atlas` is the single place that decides how many layers a view needs and how big they
are, so the graph's declaration, the backend's layer arithmetic and the shader's indexing cannot
drift apart. The cost is a **shared resolution**: `SpotLight::shadow_resolution` is a request and the
largest wins, because a texture array has one size.

The fitting is trivial where a cascade's is not — a spot light *is* a camera, so its matrix is
`perspective(2 × outer_angle) × look_along` with no fitting at all. **Two things differ from the
cascaded path and would be wrong if copied**: the perspective divide is real here (a cascade's
projection is orthographic, so `mesh.wgsl` skips it), and the bias divides through the *range* rather
than the depth span, because perspective clip depth is compressed towards the far plane.

> ### ⚠️ The bug it shipped through, and it looked like nothing at all
>
> `shadow_casters` was culled to the **directional** light's box alone. A scene lit only by a torch
> therefore produced an **empty caster list** — every shadow pass cleared its layer, drew nothing, and
> every surface came out fully lit.
>
> **A shadow map with nothing in it does not look broken. It looks like no shadows** — which is
> precisely the thing the feature exists to change, so "it isn't working" and "it isn't wired up" are
> the same picture. It is now the union of every shadow volume, deliberately loose: a pass whose own
> light cannot see a mesh clips it anyway, so a generous list costs a few vertices where a tight one
> costs a missing shadow.

Point lights still cast nothing — a cube shadow is six faces and six passes, and it is not what the
gate needs. `games/atrium` gained a shadow-casting lantern beside the warm lamp; the pool and the
pillar shadow inside it are in the capture.

### A red build, and the check that came out of it

**The spot-shadow commit went 3/5.** Every GPU capture test failed on Windows at once — the signature
of a shader that will not compile rather than a wrong picture.

`textureSampleCompare` picks its mip level from the **implicit derivatives** of its coordinates, which
makes it a gradient instruction, and HLSL forbids those in non-uniform control flow. The punctual-light
loop is bounded by a uniform, so FXC called it varying, tried to unroll it, and failed at
8 lights × 9 PCF samples = 72 iterations. Two error codes, one cause: 1479 × `X3570` and one `X3511`.
`textureSampleCompareLevel` names mip zero and takes no derivatives; a shadow map has one mip level,
so the result is identical.

> ### ⚠️ Half the CI matrix was silent about shaders, and that is the real finding
>
> A real GPU compiles through DXC or Vulkan. **Windows CI has no GPU**, falls back to WARP, and
> compiles through **FXC** — much stricter. And **Ubuntu CI has no software fallback at all**, so it
> skips every GPU test and passes regardless.
>
> So a green Ubuntu job says *nothing* about whether a shader is valid, and the only signal was a red
> Windows job discovered by tripping it. That is now a command, in CLAUDE.md §4b as a conditional
> fifth check and in `docs/07` with the general rule:
>
> ```
> WGPU_BACKEND=dx12 WGPU_DX12_COMPILER=fxc \
>     cargo test -p amadeo-render --all-features --test capture
> ```
>
> **Verified both ways** — it fails on the bad shader and passes on the good one — so it is a check
> that has been watched failing, which is the standard this codebase holds its tests to and had not
> held its own build steps to.

### `amadeo-audio` exists — ADR 0059, and kira is decided

Justin chose **kira behind the trait**, settling the ⚠️ that `docs/04` §12 has carried since M0. The
deciding argument: audio is *outside the state hash*, so unlike physics there is no determinism reason
to own the mixing — ADR 0036 owns rapier's interface because a solver's results reach a replay, and
nothing kira does can. ADR 0036 §4's rule applies unchanged: no kira type may cross `AudioBackend`.

What landed is the **engine-owned half**: the trait, `NullAudio`, buses, the components, the collection
pass, and sixteen tests. Third instance of the same shape as rendering and physics, which is what makes
invariant I7 hold — the engine runs with no window, no GPU **and no sound card**.

Two things worth knowing before touching it:

- **An `AudioFrame` is a state, not commands.** *"These are the sounds that should be audible now"*, so
  a backend diffs it and a hum stops because its entity stopped existing. That is what makes
  `AudioSource` authorable, visible to `describe`, and correct after a snapshot restore.
- **A one-shot has no home yet, and the tempting fix is wrong.** A footstep is an event, not a state.
  The obvious `play_once` flag plus a system that clears it would put a write into *gameplay state* for
  something that must not be in the state hash at all. Events are what `amadeo-events` is for.

### Two things starting the kira backend found, without writing it

Both are exactly what building the interface before the dependency was for.

**A `Voice` had no identity.** An `AudioFrame` is a state a backend diffs, but two entities playing
one sound were indistinguishable — so a backend could not tell "still playing" from "started again",
and the only available behaviour was restarting every sound every frame. **A stutter at sixty hertz
rather than a hum.** The entity is the identity that already exists and is stable across frames, so a
voice handle would have been a second name for the same thing.

Worth knowing *why rendering never needed this*: a renderer redraws from scratch, so a triangle drawn
this frame has nothing to do with last frame's. A sound is the opposite — it continues.

**And the reconciliation had no testable home.** A kira backend is the one piece of this engine
neither CI nor a headless run can verify, so the logic that goes silently wrong was pulled into
`VoiceTracker`, where eight tests cover the cases that are inaudible until they are not:

- an unchanged frame must produce **no work at all** — re-applying an identical gain every frame is a
  sound that never settles, and it gets blamed on the library
- a source swapping its clip is a **stop and a start**, not an update; it looks like an update and
  treating it as one leaves the old clip running and silently ignores the new one
- a position that moves in its last bits is **not** a change — positions come from transforms
  recomputed every tick, so without a tolerance every spatial sound is re-positioned sixty times a
  second forever

**`decode_wav` closes the last gap that was reachable without a device.** Hand-written, no dependency,
following `amadeo-image` exactly — PNG comes from a crate, PPM is written out, and WAV is the one
worth writing out. 16-bit PCM, 24-bit PCM, 32-bit float, mono or stereo, plus
`WAVE_FORMAT_EXTENSIBLE`, which is what a Windows tool emits above 16 bits. Compressed audio is
refused **by name** rather than as "unsupported", because the latter tells nobody which converter
setting to change.

> The trap in it is 24-bit. It has no Rust type, so the sign has to be extended by hand — and getting
> that wrong turns every negative sample into a very large positive one, which is heard as **loud
> noise** rather than as a quiet mistake. `twenty_four_bit_pcm_is_sign_extended` pins it.

**Next:** see the 📬 box at the top of this file.

## 📋 The plan cascades were built from — kept as a record

## M2.5 is complete, and M3's renderer work is two items in

**M2.5's four exit gates are all met.** A generated world you walk on and dig into
(`cargo run -p scarp`), reproducing at every thread count, drawing only what can be seen, at 61 µs of
GPU time.

**Session 14 was long and mostly renderer.** ADR 0045's tier 1 is **complete** — mipmaps, normal
mapping (0047), PBR (0048), image-based lighting (0049), the sky, and anti-aliasing (0051) — plus the
two items ADR 0050 added when the art direction became low-poly, and three bug fixes that came out of
Justin actually playing it.

**Four questions closed: Q27, Q28, Q29 and Q33.** One opened: **Q34**. Q32 is **raised to P1** and
half-addressed — its silence is fixed, its actual decision is not.

**The one remaining tier-1 item is shadow cascades.** Its split scheme and fitting are built and
tested; the GPU half is planned in detail below and is the obvious next session's work.

**Twenty-three crates, two modules and four games.** `modules/amadeo-camera` is new: the follow
camera moved out of `games/scarp` when `games/atrium` wanted it, which is the first time this
project's "promote on the second caller" rule has actually been acted on.

> ### ⚠️ The three bugs Justin found by playing, and what they have in common
>
> None was found by a test, and two were misdiagnosed before a screenshot settled them. All three are
> written up where they belong, but the shared shape is worth carrying:
>
> 1. **"Digging down shows the sky."** Diagnosed twice wrongly — first as the dig radius (which
>    *had* doubled and *was* a real bug), then as the camera being underground (true, and a
>    different statement). It was **ADR 0052**: terrain is an open surface with no underside, so
>    culling made it invisible from below and the sky pass filled the gap.
> 2. **The camera flickering near and far.** `move_shape` *slides*, so measuring straight-line
>    distance counted a sideways slide as progress. Now projected onto the axis asked for, and eased
>    outward. **Q34** records what is actually missing: a pure shape cast.
> 3. **The sky's lower hemisphere 2.5× too bright.** Its branch in `bin/sky` returned before the
>    scale the sky above it got. It read as *the terrain* being pale, and survived two looks —
>    switching the ground to flat colour is what exposed it, because there was no texture variation
>    left to blame.
>
> **The common thread is that each looked like a different subsystem's fault than it was**, and the
> thing that resolved all three was a picture plus somebody saying what they actually saw.

**Two things about the sky worth knowing before changing it.** It is *content* and was tuned by eye —
`bin/sky.rs`'s `SKY_SCALE` exists because the Scarp's sun intensity was tuned against the old `0.12`
ambient, and raising the sky to physical brightness without retuning the sun would change two things
at once. And `games/atrium` and `games/vault` still name no sky, so they shade exactly as they always
did; the Atrium is an open-topped room that would benefit from one.

Three things from normal mapping are worth knowing before touching the renderer:

1. **A mesh now has three independent properties, not two.** `docs/07` paired *normals* and *winding*
   after session 13's inside-out mesher; **tangents** are the third, and they fail the same silent
   way. The nastiest case passes every validity check: an orthonormal frame rotated 90° is perfectly
   valid and slides the normal map sideways across the surface. Any new `MeshData` producer needs all
   three checks.
2. **`PixelFormat` now distinguishes colour from data**, and a normal map is data. A `.png` cannot
   say which it holds — the bytes are identical — so the `.ama-meta` sidecar declares it with
   `color_space = "linear"`. **Forgetting it is silent and subtly wrong (Q31)**, which is the sharpest
   edge this feature ships with.
3. **Terrain is the one surface that did not really get normal mapping.** Its planar UV projection
   gives a vertical face zero UV area, so those tangents fall back to an arbitrary axis — valid rather
   than `NaN`, and wrong rather than right. Triplanar mapping is the fix for that *and* for the UV
   stretching that shares its cause, and it needs no vertex tangents at all.

The session 13 detail below is still worth reading before touching the renderer or the terrain
crate — five engine defects, one sharp edge that is still open (Q30), and the graphics direction
settled in ADR 0045.

## Where M2.5 got to

**Exit gates 1 and 2 are met.** `cargo run -p scarp` is a generated world you walk on, streamed in
chunks, with collision and shadows, and its replay reproduces at every thread count. What is left is
gate 3 (frustum culling) and gate 4 (frame budget with GPU time). **Q26 blocked gate 3 and is now
closed**, so gate 3's baseline is measured and waiting: the Scarp reports 50 meshes drawn, 20 visible,
30 off-screen. Historical note follows — **Q26 turned out to block
gate 3**, because `render.describe` cannot see meshes and the gate says to measure through it.

### Building the demo found five engine defects, and one of them had been wrong since session 12

Every piece of streaming was tested and none of it had ever carried a player. The bet that a real
game finds what tests do not paid for the third milestone running.

1. **Every surface-nets mesh was inside out.** All three axes passed the wrong `flip` to `push_quad`,
   so every quad ever emitted was wound against its own normal. **It hid behind a gap between two
   true things**: the mesher's tests check *normals*, which come from the field's gradient and were
   always right, and the GPU decides which face you see from the *winding*, which nothing checked.
   Nothing had ever drawn one either — a collider has no winding, so physics was correct throughout.
   The symptom is the worst part: a heightfield that is inside out is **invisible from above** and
   faintly visible at the horizon, which reads as *chunks that failed to stream in*. Found by
   `amadeo capture` and a photograph, not by reasoning. `triangles_are_wound_to_match_their_own_normals`
   reported **2316 of 2316** triangles wrong on its first run.
2. **Digging changed the collider and not the picture.** A dug chunk re-meshes under the same asset
   id; the renderer asked `has_mesh`, got yes, and skipped the upload forever. The player walks into a
   tunnel that still looks like solid rock, and no simulation test can see it because the simulation
   is right. `MeshCache` now carries a version per entry.
3. **Nothing ever freed a chunk's geometry.** `stream_terrain`'s own docs said `removed` drops the
   cache entry; the code despawned the entity, removed the collider and left the mesh. Both the cache
   and video memory grow for as long as you walk in one direction, with **no wrong picture** — the
   frame stays correct until the allocation fails. `RenderBackend::remove_mesh` closes it.
4. **A character on terrain would not start.** `amadeo_character::install` and
   `amadeo_terrain::install` both registered `step_physics`, and duplicate labels are a hard error —
   so the ordinary open-world case failed with `DuplicateLabel`. Each now asks `App::has_system`
   first. Checking rather than making `add` idempotent, because a collision between two *different*
   systems is still a real bug.

Plus one that is the Atrium's session-9 defect from a new direction: **`App::load_materials` only
scans `Mesh` components that exist when a scene finishes loading**, so anything spawned at *runtime*
gets `Material::default()`. Terrain drew plain white over an otherwise correct world.
`App::load_material` loads one by id, and `amadeo_terrain::install` calls it.

### CI found a fifth, and it is the third time delivery timing has decided something

`4ea7eae` went **3/5** — both `test (ubuntu-latest)` and the determinism job failed
`walking_brings_new_ground_in_and_lets_old_ground_go` with *"terrain/0/-1/0/2 was despawned but its
geometry is still cached"*.

**A real leak, not a flaky test.** Collection of finished meshes was gated on the `data` residency
set, which is `visual` grown by one ring — the apron, which exists so meshing can read a neighbour's
samples and which is never submitted, drawn or given an entity. So a chunk that had *left* the drawn
region could still be delivered while it sat in that ring, after `removed` had told the caller to drop
it. The caller re-cached geometry for an entity that no longer existed and nothing ever named that key
again.

Whether it happened depended on **when a job finished**: same tick as the removal and
`stream_terrain`'s insert-then-remove ordering hid it; a tick later and the entry was orphaned for
good. Gate is now `visual`, the set that actually has consumers, so delivery and residency agree by
construction. **`Residency::data` now has no runtime consumer at all** — it is the statement of the
apron constraint a test enforces, and Q25 is what will need it.

Two regression tests, both watched failing against the old gate. The first **reproduces it
deterministically by controlling when jobs land** — submit, move the viewer, and only *then* wait —
rather than hoping for a slow machine, which is what made the game-level test a coin flip.

> This is the third variant of one shape and `docs/07` now carries all three. The first two were about
> *filtering* an output by what the caller already has; this one is about *admitting* a result the
> caller has no consumer for. **When several sets are in scope, the correct one is the narrowest that
> covers every consumer, not the widest that contains the key.**

### Materials can carry a texture now, and what still stands between that and good-looking terrain

`Material::base_colour_texture` had existed since ADR 0033 and was read by **nothing** — not the
frame builder, not the shader — so every 3D surface in the engine was one flat colour. Four things
stood between the field and a pixel: `decode_frame_textures` returned early whenever there were no
*sprite* batches (true of every 3D-only scene), the upload path walked sprite batches only, mesh
draws were grouped by mesh id alone so a bind group could not vary, and the backend's only sampler is
clamped and unfiltered — correct for a sprite region inside a sheet, wrong for a surface whose UVs
run past 1.0. Surfaces now get their own repeating, filtered sampler and a second bind group per
texture.

An untextured material binds a 1×1 **white** placeholder, because white is the identity of the
multiply — so one pipeline serves both, and it is deliberately not the magenta "asset missing"
placeholder.

**And the bigger question that prompted, now decided — ADR 0045.** Justin asked whether the basic
look means the engine should eventually write Vulkan directly rather than going through wgpu. The
researched answer is **no, and the reason matters**: wgpu's native feature set is Vulkan's in practice
(bindless, multi-draw indirect, subgroups, ray query, mesh shaders, BC/ASTC), **Tiny Glade** shipped
on Bevy/wgpu in 2024 and is praised specifically for how it looks, and **not one item on the list of
things that would improve Amadeo's picture is blocked by the API.** The renderer is six features deep
where a shipping one is forty. ADR 0045 orders that work by visual return and names the only real
trigger for going native: a **console** target.

**Three things still stand between this and terrain that looks good, and none is small:**

| | |
|---|---|
| ~~Mipmaps~~ | ✅ **Done — M3's first renderer item.** `amadeo_image::mip_chain`, averaged in linear light, with 16× anisotropic filtering on surfaces and sprites pinned to level 0. The terrain tile went from 8 m to **4 m** as a direct result |
| ~~Normal mapping~~ | ✅ **Done — M3's second (ADR 0047).** Tangents read from glTF when the file has them, generated at load when it does not. **Terrain is the exception and still needs triplanar**, below |
| ~~Metallic-roughness PBR~~ | ✅ **Done — M3's third (ADR 0048).** Cook-Torrance/GGX, and a glTF-packed metallic-roughness map. **Changed the picture almost not at all**, because every material in the repo is a rough dielectric — which is exactly where a full BRDF and Lambert agree. Scaffolding for the next item rather than a win on its own |
| ~~Sky and image-based lighting~~ | ✅ **Done — M3's fourth (ADR 0049), and it closes Q28.** The ambient constant is gone. Shadows are filled by the sky, metals reflect their surroundings. **Does not draw the sky** — that is a separate pass and is now the largest visual gap |
| ~~Drawing the sky~~ | ✅ **Done.** One oversized triangle at the far plane, depth-tested with depth-write off, drawn **only when a camera names a sky** — naming none means "do not draw one", which is what keeps the 2D games' backgrounds intact |
| ~~Anti-aliasing~~ | ✅ **Done — 4× MSAA (ADR 0051).** MSAA over a post filter on purpose: it fixes geometry edges and leaves flat colour alone, where FXAA would smear exactly the facets low-poly depends on. All 21 existing capture tests passed with it *off*, which is why the new one scans across a silhouette |
| ~~Flat shading~~ | ✅ **Done — Q33 closed.** `MeshData::flat_shade`, `GltfPart::flat` to ask for it, and `Terrain::flat_shaded` for a generated world. Runs **before** tangent generation, which is load-bearing |
| ~~Two-sided rendering~~ | ✅ **Done — ADR 0052.** What "digging down showed the sky" actually was: terrain is an open surface with no underside, so culling made it vanish from below. Byte-identical from above; only the broken views changed |
| ~~The camera in the ground~~ | ✅ **Done — Q27 closed, and it is now `modules/amadeo-camera`.** A swept sphere pulls the follow camera in, snapping inward and easing outward, plus right-click mouse look. Used by the Scarp *and* the Atrium |
| ~~Silent asset failures~~ | ✅ **Done — half of Q32.** An asset naming a component it cannot build now says which asset, which component and which field. The optionality question stays open |
| ~~Shadow cascades~~ | ✅ **Done — ADR 0055, and it completes ADR 0045's tier 1.** Four concentric cascades in four layers of one depth array. The near texel goes from ~7 cm to ~1 cm of ground, for 71.7 -> 113.7 µs of GPU time |
| **Triplanar mapping** | Terrain UVs are a planar projection from world x/z, so anything steep stretches — *and* has zero UV area, so its tangent frame falls back to an arbitrary axis. One fix for both. Wants a `Material` field to opt in, which is another schema change to every `.material` file (**Q32**) |
| **Ambient / sky light** | Still the hardcoded `0.12` constant (**Q28**). No ambient occlusion, no bounce, no sky colour. Flat lighting is the other half of why a scene reads as a prototype, and no amount of texture fixes it |

The visual gap is now overwhelmingly **shading**, not geometry.

### 🎨 The art direction is low-poly — ADR 0050

**Decided in session 14.** Amadeo's own demos and assets are low-poly. The honest reason is that no
art can be authored here in the conventional sense — no modelling, sculpting, texture painting or
photogrammetry — but **generators** can be written, and the project already has three (`bin/pix`,
`bin/turf`, `bin/sky`). Low-poly's quality lives in form, silhouette and colour, which is exactly what
code can express; photoreal's lives in scanned surface detail, which it cannot.

**The renderer does *not* narrow.** `CLAUDE.md` trap 8 forbids baking an art style into it, and the
target games still span stylised-realistic, low-poly and dark interiors. Nothing is removed — what
changes is the order of what is left:

- **Anti-aliasing moves to the top.** Hard silhouette edges everywhere is what low-poly *is*.
- **Image-based lighting was more important than it looked**, and is already built. Flat facets each
  catching a different part of the sky is what makes low-poly read as solid rather than flat.
- **Normal mapping matters much less** for this content. ADR 0047 is not wasted — imported art uses
  it, and the other targets need it — but nothing further should be built on it now.

**And one gap it opens: Q33, raised to P1.** Low-poly needs per-face normals. `BoxMesh` tessellates
twenty-four vertices rather than eight so each face carries its own; **nothing else does**, so a glTF
exported smooth imports smooth and shades as a blob. Note it interacts with ADR 0047: splitting
vertices per face changes the tangent frame, so it has to run *before* `generate_tangents`.

### 📋 Shadow cascades — the plan they were built from, kept as a record

**✅ Built in session 15 — ADR 0055.** Kept because the plan turned out to be accurate, including the
trap it named, and because that is worth knowing next time something is planned before it is begun.
Every "will" below is now a "did", with two exceptions noted in the session-15 entry above: the blend
became a payload on the variant rather than a field on `DirectionalLight`, and the shadow map became
a texture array in every mode rather than only the cascaded one.

**The last item in ADR 0045's tier 1.** Deliberately not begun rather than begun badly: it touches
the shadow fitting, the render graph's transient pool, the backend and the shader at once, and half
of it would be worse than none. What follows is the plan, so the next session starts on code.

**The problem.** `games/scarp` fits one 2048² map over a 70 m box, so a shadow-map texel is about
7 cm and edges are visibly blocky. Cascades split the camera's range into a few slices and give each
its own map, so near geometry gets a fine one and distant geometry a coarse one.

**Three decisions, all cheap to reverse, so taken here and flagged rather than asked:**

1. **Four cascades, fixed, not authored.** Four is what nearly everything ships. Making the count
   authored means a variable-length texture array and a variable shader loop bound to buy flexibility
   nobody has asked for. **Additive later**: a `cascade_count` field defaulting to four changes no
   existing file.
2. **The split scheme is the "practical" one, with an authored blend.** Uniform splits waste
   resolution near the camera; logarithmic splits waste it far away. The standard fix (Fournier /
   NVIDIA's parallel-split work) interpolates between them by a weight, conventionally `lambda`, with
   `0.5` the usual default. That weight *is* worth authoring — it is the one number whose right value
   depends on the scene — so `DirectionalLight` gains one field, not four.
3. **`Orthogonal` stays** alongside `Cascaded`. An indoor scene does not need cascades and one map is
   cheaper — and M3's exit gate is indoor. `games/vault` and `games/atrium` keep what they have.

**The five touchpoints, in the order they have to happen:**

| | |
|---|---|
| `mesh.rs` | `ShadowMode::Cascaded`, and a `cascade_blend` field on `DirectionalLight`. **Adds a field to a component, so every `.scene` naming a `DirectionalLight` changes — see docs/07's entry on exactly that, and Q32** |
| `lib.rs` | `fit_shadow` becomes *fit four*. Split the camera's near-to-`shadow_distance` range by the practical scheme, fit a box per slice, keep the existing world-origin snapping per cascade — **the snapping is what stops edges crawling and it must be per cascade, since each has its own texel size** |
| `backend.rs` | `ShadowData` becomes up to four, and `View` carries the split distances the shader selects by |
| `graph.rs` + `gpu.rs` | The shadow transient becomes a **depth texture array of four layers**. The transient pool matches on format, so a layered depth target is a new `TargetFormat` variant rather than a flag — the same reasoning `ShadowMap32` already carries |
| `mesh.wgsl` | Pick a cascade from the fragment's view depth, then the existing lookup unchanged. **The bias must scale per cascade**: a fixed world-space bias tuned for the near map is far too small for the far one, which is what makes distant shadows acne up |

**The trap worth knowing before starting.** A fragment near a split boundary can pick a different
cascade than its neighbour, which shows as a hard line across the ground where resolution changes.
The standard fix is to blend over a small band either side. Worth building *without* it first and
looking, because the seam may be invisible at these distances and blending costs a second sample.

Sources: [NVIDIA GPU Gems 3, ch. 10 — parallel-split shadow maps](https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-10-parallel-split-shadow-maps-programmable-gpus),
[MJP, A Sampling of Shadow Techniques](https://therealmjp.github.io/posts/shadow-maps/)

### Session 14: normal mapping, and the dependency it did not need — ADR 0047

A normal map stores which way a surface leans, per pixel, so a flat wall lights as though it had
grooves in it. ADR 0045 ranked it second by visual return, after mipmaps.

**The decision was about tangents, and it turned on something the codebase was throwing away.** A
normal map's directions are relative to *the surface*, which is what lets one image tile across a
curved wall — but converting one to a world direction needs to know which way "left" points at each
vertex. The normal does not say: it fixes which way is *out* and leaves the surface free to spin
around it. That missing piece is the tangent.

The industry standard is **MikkTSpace**, and it matters for a concrete reason rather than a purist
one: a normal map is *baked* against a particular frame, and a renderer computing a different one
lights every bump slightly wrong. The reference implementation is ~1900 lines of dense C. Bevy vendors
a Rust port and has since added a **faster approximate path beside it**, because the real one is slow
enough to be a filed bug.

**Amadeo needed neither, because glTF can carry `TANGENT` directly and `amadeo-gltf` was discarding
it.** The case where MikkTSpace correctness actually matters is imported art with a baked map — which
is exactly the case where *the file already holds the right answer*. So: read the file's tangents when
it has them, generate a Gram-Schmidt frame when it does not, and take no dependency. Generation then
only ever runs on procedural shapes, whose UVs are flat and axis-aligned and where the two algorithms
agree exactly.

**The test that matters is not the obvious one.** A tangent frame can be wrong in three ways and only
the first is loud: zero-length gives a `NaN` and a black surface; not-perpendicular shears the frame;
and **orthonormal but rotated 90° passes every validity check** and slides the map sideways across the
surface. So `a_tangent_points_the_way_the_texture_grows` compares against a direction worked out by
hand — a plane's `u` axis runs along +x, so its tangent must too — and was watched failing against a
deliberately flipped tangent.

**Verified on a real GPU, not only headlessly**, which is session 13's lesson applied rather than
recited: a surface leaning 45° off the light is measurably darker than one facing it square, and
`normal_strength 0.0` is byte-identical to naming no map at all — which is what attributes the
difference to *the map* rather than to anything else the textured path does. Both games were also
captured and looked at, through a vertex layout change that touched every mesh producer.

**Two supporting decisions, both cheap and both in the ADR.** `PixelFormat` gained `Rgba8Unorm`,
because a normal map's bytes are directions rather than light and the sRGB curve bends every one of
them — declared in the sidecar, which needed **no format change** because its settings were already a
free-form map. This is precisely the extension ADR 0026 described before it existed, and it cost one
variant and one match arm. And bind group 2 became a *combination* of textures rather than one
texture: the obvious alternative, a fourth bind group, works today and dead-ends immediately, since
wgpu guarantees four and three are spoken for.

**Two things it ships knowing about**, both now open questions rather than notes: a sidecar that
forgets `color_space` is silently wrong and nothing warns (**Q31**), and every field added to
`Material` rewrites every `.material` file (**Q32**).

### Session 14 also landed PBR — ADR 0048 — and it barely changed the picture

`metallic` and `roughness` had been on `Material` since ADR 0033 and the shader read **neither**. It
now runs Cook-Torrance with GGX, Smith visibility and Schlick Fresnel — glTF 2.0's model, which
ADR 0033 had already committed to — plus a glTF-packed metallic-roughness texture.

**The honest result: the picture is almost unchanged, and that is the finding.** Every material in the
repository is a rough dielectric (`metallic 0.0`, roughness 0.6–0.9), which is exactly the case where a
full BRDF and plain Lambert nearly agree. The Scarp is essentially identical; the Atrium picked up a
faint sheen where its floor is seen at a shallow angle and the Fresnel term rises.

**The number that says the implementation is right.** A red box lit head-on used to have green and
blue near zero, because Lambert reflects only the surface colour. A dielectric's highlight is *white*
— that is what makes plastic look like plastic — and the maths predicts about 0.15 in linear light at
the default roughness of 0.5. Measured: 111 in sRGB, which is 0.15. Predicted before it was measured,
and they agreed to two decimal places.

**One existing test changed its premise**, which is worth knowing rather than skimming:
`a_mesh_actually_reaches_the_pixels` asserted green and blue stayed below 60, and that encoded *"there
is no specular"* as though it were a property of the renderer rather than a missing feature.

**Two limitations shipped on purpose, both named in ADR 0048.** Metals are unusable until Q28 closes,
and the test that pins that is written so closing Q28 breaks it. And a physically-correct highlight
needs a tonemapper the default `Environment` deliberately does not apply.

### And one sharp edge that is NOT fixed — Q30

**Writing a `Transform` to move a physics body does nothing, silently.** `step_physics` prefers
`GlobalTransform`, and `propagate_transforms` runs at the *end* of the tick, so the write is read back
stale and physics puts the body straight back. A test that teleported a character to check streaming
spent a debug cycle on it. Preferring `GlobalTransform` is *correct* for a parented body, so this is a
missing capability rather than an inversion — there is no supported teleport, and respawns and fast
travel both need one.

### ADR 0044 — a terrain generator may not use `sin`

The demo needed hills, and the obvious way to write them would have been a **latent I3 violation with
no symptom on any one machine**. Rust documents `f32::sin`, `f32::cos` and `f32::powf` as having
non-deterministic precision — varying by platform, by Rust version, *and between two calls in one
execution* — while `sqrt` is guaranteed exact, because IEEE 754 requires correct rounding only for
`+ - * / sqrt` and lists the transcendentals as recommended.

ADR 0043 made a chunk's collider gameplay state and a `TerrainSource` decides where it is, so a sine
in a generator puts Windows and Linux on different ground. The report would read *"the replay does not
reproduce on Linux"* and point at physics, the scheduler and the job pool long before anyone suspected
trigonometry. **This is a fifth entry for trap 2, and the least visible of the five.**

`amadeo-noise` is built from `+ - * /`, `floor` and integer hashing. **Justin was given two decisions
and took the recommendation on both**: its own crate (so a 2D heightmap does not depend on a 3D
mesher — trap 9), and a per-entry version number for mesh updates.

Its literal-hash test **has already earned its keep**: `SCALE_2D` was written as `1.414_213_6` and
replaced with `std::f32::consts::SQRT_2`, which reads better and looks like the same number. It is
not — they differ in the last bit, every 2D sample moved, and that assertion was the only thing in the
workspace that noticed.

### What `games/scarp` is, in one paragraph

`cargo run -p scarp`. WASD to walk, Q/E to turn, **hold right-click to steer the view**, Space to
jump, **F to dig**. Since session 14 it is also **low-poly**: eight two-metre cells per chunk, flat
shaded, flat colour, under a generated sky. Nothing is authored but
the player, the camera and the sun — the ground is a function of the seed, sampled into chunks as you
approach, dropped as you leave, meshed on a job pool, made solid on the tick you need it. Gate 2 is
`a_walk_reproduces_at_every_thread_count`: five worlds at 1, 2, 3, 5 and 8 workers, advanced **in
lockstep** a tick at a time, state hashes compared every tick for 480 ticks, over a walk with a turn
and a dig in it. It was **watched failing** — spawning chunk entities from mesh arrival instead of
from residency diverges it on tick 1.

That test is worth more than the two it appears to duplicate. Those drive a `Viewer` along a straight
line *by setting its coordinate*, so a divergence in the terrain cannot feed back into where the
viewer goes next. Here it can: a collider differing by one chunk moves the character, which moves the
viewer, which loads different chunks, and the worlds separate for good.

**The spawn height is authored in the scene**, which looked like it had to break I1 on a generated
world. It does not: gradient noise is exactly zero at every lattice point whatever the seed, and the
origin is one for both octaves — so the ground there is the base height on the nose for every seed.
A test holds that in place.

---

`docs/05-roadmap.md` has the milestone in full. Built so far:

| | |
|---|---|
| ✅ | **Threading model** — ADR 0041, which resolved **Q9**, the oldest open question |
| ✅ | **`amadeo-jobs`** — worker pool + `Inbox` draining in key order. No dependencies at all |
| ✅ | **`par_for_each_mut`** — parallel iteration whose closure cannot be written unsafely |
| ✅ | **Background asset loading** — byte-identical to the sequential path |
| ✅ | **`amadeo-voxel`** — surface-nets meshing, and **ADR 0042** for the terrain data model |
| ✅ | **Chunk residency** — `ChunkKey`, `Viewer`, `Residency`. Integer boxes, **ADR 0043** |
| ✅ | **The terrain source** — ADR 0042's generated base plus sparse edits, and per-chunk meshing |
| ✅ | **Static trimesh colliders** — geometry reaches the solver by **id**, not as a component |
| ✅ | **The streaming core** — `amadeo-terrain`. Colliders meshed **inline**, meshes on the job pool |
| ✅ | **The ECS layer** — `TerrainViewer`, `TerrainChunk`, `stream_terrain`, `install`. Behind the `engine` feature |
| ✅ | **Digging** — `TerrainStreamer::edit`. Invalidates up to eight chunks; jobs carry an edit version |
| ✅ | **`amadeo-noise`** — deterministic gradient noise, **ADR 0044**. No transcendentals, literal hash pinned on both platforms |
| ✅ | **`games/scarp`** — a generated world you walk on and dig into. **Exit gates 1 and 2 met** |
| ✅ | **Where edits live — Q29 closed (ADR 0046).** `TerrainEdits` is a hashed resource; the streamer is a cache of it. A dug world saves and reloads dug |
| ✅ | **`render.describe` sees 3D — Q26 closed.** Real perspective projection, a `Mesh` kind, and the eye widened to three components |
| ✅ | **Frustum culling — exit gate 3 MET.** 50 meshes exist, 20 in view, **20 submitted**, and the picture is byte-identical |
| ✅ | **GPU timestamp queries — exit gate 4 MET.** The Scarp at 640×360 costs **61 µs of GPU time**, 0.4% of a 60 Hz budget |
| | LOD (**Q25**), `amadeo-math` over glam, GPU timestamp queries (gate 4) |
| | More than one light, textures on materials |

**Session 13 assembled the pieces and the assembly is what found the bugs.** Everything below the
demo existed and was tested; putting a player on it exposed four defects and one missing capability,
listed above. The most valuable single act of the session was **taking a photograph** — `amadeo
capture` plus actually looking at the PNG — which is how a two-session-old winding inversion surfaced
after every headless test had passed.

**ADR 0041 §2 is still the rule it must obey**, and it is now half-enforced by types rather than by
memory. A chunk has two products: its **mesh** is drawn and nothing else, so it may arrive whenever;
its **collider** is gameplay, because a character stands on it, so *when* it arrives changes where the
character ends up. `Residency` carries separate `visual` and `collision` sets for exactly that reason.

### ⚠️ ADR 0042 described **half** the apron, and the other half cracks every chunk

This is the single most important thing session 12 found, and it was found by meshing two adjacent
chunks and looking — not by reading the ADR, which was written before anything meshed two chunks.

- **Vertices** need the *high* apron: a cell's vertex comes from its eight corners, so a chunk's last
  cell needs the next chunk's first sample. This is what ADR 0042 §2 says, and it is correct.
- **Quads** need a *low* apron too. `surface_nets` emits a quad by looking at the four cells around a
  grid edge, and at a chunk's **low** face two of them belong to the previous chunk. So the quads
  *bridging* two chunks were emitted by neither, and every chunk had a one-cell gap around it.

So a chunk of `n` cells fills an **`n + 2`** sample grid covering `n + 1` cells, starting one cell
*below* its own origin. Every quad in the world is then emitted exactly once, by the chunk on its high
side — no gaps and no duplicates. **ADR 0043 §4 amends ADR 0042 §2**; `ChunkShape::samples_per_axis`
is the number, and `two_adjacent_chunks_have_no_gap_between_them` holds it.

Verified by watching it fail: reverting to the high-apron-only scheme prints *"the right chunk does
not reach back to the join at x = 8; its low apron is missing and the bridging quads belong to
nobody"*.

**And the apron is no longer something to remember.** `Residency` carries three nested sets —
`collision ⊆ visual ⊆ data` — where `data` is `visual` grown by one chunk, so a drawn chunk always has
loaded neighbours to mesh against by construction. Breaking it fails a test that names the chunk and
the neighbour it is missing.

**The agent can see.** `amadeo capture shot.png` launches a game headless, renders it on an offscreen
GPU and writes a PNG. That closes ADR 0021's "agent's eyes" and gave the GPU path its first automated
coverage, which `STATUS.md` had carried as a known gap through three milestones.

**M1 closed — all five exit gates tested, four met and one refuted.**
Reflection, the scene format, the agent's read layer, the agent protocol and a working `amadeo` CLI,
the whole asset layer, the sprite batcher, textured sprites on the GPU, invariant I8, snapshots,
prefabs, and **`games/vault` — a complete small 2D game** have all landed.

**Gate 4 was tested and found false** — `describe` is a schema, not a manual — which is a result
rather than an omission. **ADR 0030 settles what the protocol is for** and fixes the three parts of
that finding that were genuine holes; the API half stays in `docs/07` by invariant I5. **ADR 0029
closes Q7** with prefabs, and **ADR 0032 closes Q21** by letting a scene file nest values at all.
Q3, Q4, Q7, Q10, Q13, Q14, Q16, Q17, Q19, Q21 and Q22 are all closed.

**Q9 closed in session 11 — ADR 0041 — which was the oldest one open**, raised in session 2 and
answered the way it asked to be: before the first background task rather than after.

**Nothing is blocked.** Seven questions are open and every one of them is a *decision waiting for its
first real case*, which is the state this project deliberately keeps them in:

| | | |
|---|---|---|
| ~~Q29~~ | — | **Closed in session 13 by ADR 0046.** Terrain edits are a hashed resource; the streamer is a cache of them, and a dug world saves and reloads dug |
| **Q32** | **P1** | **Raised from P2 — it has now bitten five times in three sessions.** A field added to a component invalidates every file that spells it out, and the failure is silent: the file is skipped, the lookup comes back empty, and the error surfaces layers away as a *missing service*. The churn is not the problem; the reporting is |
| ~~Q34~~ | — | **Closed in session 15 by ADR 0054.** `PhysicsBackend::cast_shape` — sweep a shape along a line, get the fraction, a position *on the line*, and the normal. Closed because the workaround produced a visible bug: two corrections on one call was the signal |
| **Q31** | P2 | Nothing warns when a normal map's sidecar forgets `color_space = "linear"`. Silent, subtly wrong, and it becomes blocking the moment authored art ships |
| ~~Q33~~ | — | **Closed in session 14.** `MeshData::flat_shade`, opted into per glTF part and per terrain |
| ~~Q27~~ | — | **Closed in session 14.** The follow camera sweeps a sphere and pulls in; snaps inward, eases outward |
| **Q30** | P2 | **No way to move a physics body from outside the tick.** Writing a `Transform` is silently reverted — `step_physics` prefers `GlobalTransform` and propagation runs last. Blocks respawns and fast travel |
| **Q25** | P1 | LOD across chunks — **better posed** by ADR 0043 and still open: may a chunk's mesh depend on its neighbours' resolutions? |
| **Q23** | P1 | One environment per frame, when a world may hold several cameras |
| **Q15** | P1 | Modding, and whether ADR 0011 still holds |
| **Q12** | P1 | `Service: Send + Sync` — ADR 0041 changed the argument without closing it |
| ~~Q26~~ | — | **Closed in session 13.** `render.describe` sees meshes through a real perspective projection |
| ~~Q27~~ | — | **Closed in session 14.** The follow camera sweeps a sphere and pulls in, and the mouse steers it |
| ~~Q28~~ | — | **Closed in session 14 by ADR 0049.** The sky is a light source; the ambient constant is gone |
| **Q6, Q8, Q11, Q18, Q20** | P2 | Editor process model, entity relations, netcode introspection, unreadable `ActionId`, gate 4's stronger test |

**Remote:** `origin → https://github.com/justinbs/amadeo.git`. **The repository is public now**, so
Actions minutes are free and unlimited — the Windows→Ubuntu cost optimisation discussed in session 13
is no longer needed and should not be built. Green on every job.

**Session 15 opened by checking CI and it was clean:** after a `git fetch`, `origin/main..HEAD` was
empty and the last five runs were green 5/5, so session 14's fourteen commits are genuinely on the
remote. The habit below is what makes that a fact rather than an assumption, and it stays.

**Session 14 opened by checking CI and it was clean:** both of session 13's trailing commits
(`6d6993f` mipmaps, `cea8ae4` docs) had landed, `origin/main..HEAD` was empty after a fetch, and the
last five runs were 5/5 green on both platforms.

> **Session 13 opened by checking CI and it was clean:** `cc32d7a` went 5/5 green on both platforms
> and `origin/main..HEAD` was empty, so session 12's work is genuinely on the remote. The habit below
> is what made that a fact rather than an assumption, and it stays.
>
> **Session 12 opened by checking CI and found the previous session's push had not landed.**
> `2aa232f`, `df6f245` and `c26601a` were believed pushed and were not: after a `git fetch`,
> `origin/main` was still at `7dceed8`, and `gh run list` agreed — the newest CI run was on
> `7dceed8`, green 5/5. So there is **no CI evidence at all** for background asset loading, surface
> nets, or session 11's handoff docs, and none for session 12's two commits either.
>
> This is the mirror image of the correction the previous version of this file carried, which claimed
> two commits were waiting that were already pushed. Both mistakes are the same mistake: **a claim
> about the remote written from memory instead of from the remote.** `git log --oneline
> origin/main..HEAD` after a `git fetch` is the only thing worth believing here.

> ### ⚠️ Working rules — one of these changed in session 14
>
> 1. **Pushing is allowed now.** This reverses the session-7 rule. Run the four checks, commit, push,
>    then verify with `gh run list`. The gate existed when Actions minutes were scarce; the repository
>    is public and CI is free.
> 2. **Sole authorship, and no co-authorship of any kind.** No `Co-Authored-By` trailer, no
>    "generated by" line, no attribution in a comment or doc. Commits are in Justin's name alone.
> 3. **Consult him on anything hard to reverse** — the test is cost-to-undo, not visibility. An
>    internal mechanism nobody would read still warrants asking if ripping it out later means
>    rewriting a lot. Unchanged, and *not* widened by rule 1. All three are in `CLAUDE.md` §5.

---

## Where we are

Sessions 1–2 established scope, stack, and architecture. Session 3 built M0. Session 4 closed it by
resolving Q1. Session 5 built most of M1's foundations. Session 6 resolved six open questions and
built the whole agent transport and CLI. Session 7 finished `amadeo-assets`, audited the earlier
work, took the target list from three games to eight, built the sprite batcher, and then chased its
cost down through two layers of the ECS. **Session 8 put sprites on the screen** — a decoder crate, a
texture cache, and the wgpu texture path — **closed invariant I8**, making `Reflect` a
compiler-enforced bound on resources and events and shipping `world.resources`, **shipped snapshots**,
**built `games/vault` and closed M1**, and then **settled Q7 with prefabs**. ADRs 0022–0029.

**Twenty-two crates, two modules, and four games**, all tested: `amadeo-derive`, `amadeo-image`,
`amadeo-core`, `amadeo-reflect`, `amadeo-ecs`, `amadeo-transform`, `amadeo-events`, `amadeo-assets`,
`amadeo-input`, `amadeo-render`, `amadeo-physics`, `amadeo-scene`, `amadeo-snapshot`, `amadeo-agent`,
`amadeo-gltf`, `amadeo-jobs`, `amadeo-noise`, `amadeo-voxel`, `amadeo-terrain`,
`amadeo-app`, `amadeo-cli`, plus **`modules/amadeo-character`** — the first occupant of a layer
reserved since session 1 — and **`modules/amadeo-camera`**, the second, and
`games/quad-demo`, `games/vault`, `games/atrium` and `games/scarp`.
**1237 tests passing with `--all-features`** (23 of them GPU capture tests, 6 the follow camera,
7 rapier, 9 character,
7 shadow fitting, 12 the Atrium, 15 the Scarp, 9 deterministic noise, 7 frustum culling, 6 mip
chains, 13 glTF, 5 profiling, 8 jobs, 6 parallel iteration, 4 parallel loading, 10 surface nets,
13 chunk residency, 10 the terrain source, 9 static trimesh colliders, 18 the terrain streamer,
5 terrain edits, 8 streaming into a world);
fmt, clippy `-D warnings`, and rustdoc all clean. CI runs on Windows and Linux with a dedicated
determinism job.

Thirty things work end to end today:

- **The renderer only draws what can be seen.** `Frustum` extracts six planes from a view-projection
  matrix — Gribb–Hartmann, and the depth convention matters: wgpu clips z to `0..w`, so the near
  plane is one row rather than a sum, and the OpenGL form would cull things just in front of the
  camera. In the Scarp, **50 meshes exist, 20 are in view, and 20 are submitted**, with the rendered
  PNG byte-identical to the pre-culling one.
  **One implementation, two callers** — the collection pass and `render.describe` — so what is culled
  and what is reported cannot disagree, which matters because the gate is measured *through*
  `describe`. And **two lists, not one**: the colour pass draws what the camera sees, the shadow pass
  what the light sees. A single list holding the union is correct and culls nothing useful, because a
  shadow box is `shadow_distance` in every direction and the Scarp's is wider than its whole world.

- **There is a world, and it is not in any file.** `cargo run -p scarp` — rolling hills to the
  horizon, streamed in chunks around the player, solid underfoot, casting and receiving shadows, and
  diggable with F. The only authored entities are the player, the camera and the sun; the ground is a
  pure function of the seed, generated by `amadeo-noise` from arithmetic IEEE 754 specifies exactly
  (ADR 0044), so two machines agree about it bit for bit. **A replay of a walk through it reproduces
  at 1, 2, 3, 5 and 8 meshing threads**, compared every tick for 480 ticks — which is ADR 0041's
  central claim proved rather than assumed, in the one place where the answer could feed back into
  itself. Building it found four engine defects and one missing capability.

- **The engine runs.** `cargo run -p quad-demo` opens a window with a quad you steer with WASD.
  Deterministic at a fixed 60 Hz, records to a hand-editable `.replay` file, and replays against
  checkpoint state hashes in CI.
- **A text file builds a world.** A `.scene` file (ADR 0014) parses, formats byte-stably, and
  instantiates into a `World` using the engine's real components, hierarchy included.
- **The engine describes itself.** `amadeo_agent::describe` emits the full component schema as
  JSON — names, types, docs, units, ranges, replication — generated from the code, so never stale.
- **The CLI talks to a running game.** `amadeo describe Velocity` describes a type defined in
  `games/quad-demo`, answered over JSON-RPC by the game binary the CLI launched. Also `query`,
  `entity`, `schedule`, `status`, `call`, `check`, `replay`, and a standalone `fmt`.
- **A replay reproduces in a fresh process.** `amadeo replay games/quad-demo/replays/wander.replay`
  launches the game, plays a hand-written recording, and asserts four checkpoint hashes. This is the
  stronger half of the golden-replay claim — the in-process test proves a recording survives a
  rebuild, this proves it survives a new process — and it runs in CI.
- **Assets are named, found, and loaded.** `amadeo assets` lists every declared id with the file
  behind it and whether its bytes are resident; `amadeo import` gives a new file a sidecar with an id
  from its filename. A scene declares what it needs in an `assets` block, and `amadeo check` refuses
  one naming an id that does not exist — with "did you mean" when it is close.
- **Loading cannot move a replay.** quad-demo loads a real file at startup and `wander.replay` still
  matches all four checkpoints, because ADR 0009's `Service` split keeps asset state out of the hash
  structurally rather than by convention.
- **Sprites batch into draw calls.** 20,000 fully interleaved sprites collapse to 32 batches in
  2.58 ms (15.5% of a 60 Hz frame); 50,000 tiles on one sheet are a single draw call.
- **A scene file nests.** An indented block is a list if its lines start with `- ` and named fields
  otherwise (ADR 0032), so `Material` with a `base_colour` inside it, a map, and
  `projection Orthographic` with `height 8.0` beneath it all write and read back byte-identically.
- **The view is part of the level.** `entity eye "Camera"` in a scene file decides what is drawn and
  from where. A world may hold any number (ADR 0031) -- each with a projection, a target that is the
  window or a texture, a viewport rectangle, and an order -- and a camera parented to a character
  *is* a follow camera, with no special case.
- **A frame is a declared plan, not a hardcoded sequence.** The render graph (ADR 0034) names each
  pass and what it reads and writes, and derives the order from that: `view 0` and `view 1` write the
  `scene` transient, `present` reads it and writes the destination. It knows nothing about wgpu, so
  `NullBackend` compiles the same graph and reports the resolved order — a pass-ordering bug is
  catchable with no GPU. Composing the frame off-screen is also **what gave the windowed backend
  `capture`**, which this file had listed as waiting on post-processing.
- **Bodies collide, and the hash is pinned across platforms.** A ball dropped on a floor lands and
  rests; 200 bodies reproduce bit-identically; and one test asserts a **literal state-hash number**
  that CI runs on Windows and Linux, so ADR 0036's cross-platform promise is checked rather than
  believed. Behind `--features rapier`, off by default. Verified the collision tests are evidence by
  pointing them at `NullPhysics`: the ball falls to −72 instead of resting at 0.5.
- **A signed-distance field becomes smooth terrain.** `amadeo-voxel` meshes a field with naive
  surface nets -- the **fourth producer of mesh data**, and nothing above the loader can tell it from
  a box or a glTF primitive. A sphere meshes onto its radius, normals point outward, and the same
  field meshes byte-identically every time, which is what a terrain *collider* being gameplay state
  requires (ADR 0041). ADR 0042 settles the data model: a generated base plus sparse hashed edits, so
  a save file is a seed and a diff.
- **Assets load across threads, and the result is byte-identical.** `load_all_in_parallel` reads
  files on a job pool and fills the store in **key order** at a barrier, so it produces exactly what
  loading them one at a time produces — failure messages included. ADR 0021 forbidding gameplay from
  asking "has this loaded?" is what made this safe, three milestones before anything needed it.
- **The engine says where the frame goes.** `amadeo call profile.frame --package atrium --ticks 600`
  reports every system's mean and worst run against the 16.67 ms budget — which exists because an
  agent cannot *feel* a frame-rate problem and can only be told about one. The `Profiler` is a
  service, so the wall clock it reads inside the tick is structurally outside the state hash
  (ADR 0040), and two worlds — one profiled, one not — are asserted to reach the identical hash.
- **A model becomes a level.** `amadeo import-gltf level.glb` writes a `.scene` for the node
  hierarchy, a `.material` per material, and a `.mesh` per primitive — each a four-line pointer, not
  vertex data. The geometry stays in the `.glb` as source art, exactly as a `.png` already does
  (ADR 0039). What comes out is canonical scene text, because the importer runs its own output
  through the canonical writer rather than trying to match it by hand. Nothing above the loader can
  tell a glTF primitive from a `BoxMesh`, which is ADR 0035's bet paying off three milestones later.
- **There is a 3D room you can walk around in.** `cargo run -p atrium` — a floor, four walls, four
  pillars, a plinth, a sun casting shadows, and an amber character steered with WASD, Q/E and Space.
  Lighting, the character controller and shadow maps in one place for the first time. Everything in
  it is text, including the six meshes, the three materials and the look; the follow camera is a
  child entity of the player and nothing else. **Building it found five real defects** — see below.
- **Things cast shadows.** A block above a floor darkens the floor beneath it — a real shadow map,
  drawn from the sun's point of view, sampled with hardware comparison filtering and softened over
  nine taps. Off by default; `ShadowMode::Orthogonal` on a `DirectionalLight` turns it on, and
  cascades are a third variant of that enum rather than a rewrite (ADR 0038). The box follows the
  camera and is snapped to a world grid so edges do not crawl. **Measured, not assumed:** 46 in
  shadow against 235 in light, and 235 in both with shadows off.
- **Something walks around, and walls stop it.** A capsule driven by named input actions accelerates
  to speed, turns, jumps only when it is standing on something, slides along a wall instead of
  stopping dead, and lands back on the floor. `CharacterController` and `CharacterMotion` are
  reflected and **hashed**, so a character-driven game is snapshot-able and replayable for nothing.
  The geometry is `PhysicsBackend::move_shape` in `amadeo-physics`, which knows nothing about
  characters; the character is `modules/amadeo-character`, which is what I4 and trap 10 ask for
  (ADR 0037). **Every collision claim is asserted twice** — against rapier where it must hold, and
  against `NullPhysics` where it must fail.
- **Bodies fall, and the world is the record of it.** `RigidBody`, `Collider`, `Velocity` and
  `Gravity` are reflected, **hashed** data, so a physics-driven game is snapshot-able and replayable
  with nothing extra built (ADR 0036). `NullPhysics` integrates velocity and gravity for real — it is
  a backend without a *solver*, not a stub — so a determinism test runs in milliseconds with no
  rapier. Collision detection is what is missing.
- **There is 3D on the screen.** A `BoxMesh` from a text file, lit by a `DirectionalLight`, drawn
  through a perspective camera with a depth buffer so nearer faces hide further ones and back faces
  are culled. Three GPU tests each check something the others cannot: that geometry reaches the
  pixels, that the nearer face wins the depth test, and that a surface angled away from the light is
  darker than one square to it — which is what proves the lighting reads normals rather than
  painting every face flat.
- **A shape written as three numbers becomes geometry.** `games/vault/assets/meshes/wall_panel.mesh`
  is six lines of text carrying `BoxMesh { size 1.0 2.5 0.2 }`, and it comes out as a tessellated box
  of exactly that size — no toolchain, no binary, no import step (ADR 0035). `amadeo check` validates
  it and `amadeo fmt --check` finds it already canonical. **Nothing draws it yet**; the mesh pass is
  the next thing to build, and the data it will read is done.
- **A camera has a look, and the look is a file.** `environment "corridor_dark"` on a camera, and the
  file behind it is a scene document with one `Environment` — exposure, tonemap, grade, vignette, in
  an order the engine fixes because the order is arithmetic rather than taste (ADR 0034). The
  cameras draw into an **HDR** target so tonemapping has something to compress. `amadeo fmt --check`
  found the new file already canonical, which is "no new format" being true rather than asserted.
  **The default look is a byte-identical no-op** — the Vault's capture is the same PNG, byte for
  byte, as before post-processing existed.
- **The agent can see.** `amadeo capture --package vault --ticks 200 shot.png` launches the game with
  no window, renders it offscreen on the GPU, and writes a PNG — walls, sigils, wardens, the player,
  the score readout. ADR 0021 called capture the agent.s eyes; this is them. `WgpuBackend::offscreen`
  is the mechanism, and it also gives the GPU path its first automated tests: a red quad reaches the
  middle of the target and does *not* fill the corners.
- **Sprites are on the screen.** `cargo run -p quad-demo` shows a strip of textured floor tiles, each
  reading a different cell of one 2×2 texture through its `region` — one texture, one draw call — plus
  one sprite deliberately asking for an id that does not exist, which draws the magenta placeholder.
  A file becomes an id becomes bytes becomes pixels becomes a GPU texture, with a decoder crate
  (`amadeo-image`), a `TextureCache`, and the wgpu texture path in between. ADR 0026.
- **A missing texture is visible and explainable, not fatal.** Three-step fallback ending in an image
  built in code, so the last resort cannot itself be a missing file, and `TextureCache::failures()`
  says which ids fell back and why.
- **The engine describes its whole state, not just half of it.** `amadeo call world.resources`
  reports `Camera2d`, `InputState` and `SimRng` with live values from a running game. Entities carry
  components and everything else is a resource; before ADR 0027 the second half was invisible.
- **And its whole schema, with nothing dangling.** `amadeo describe` has four sections —
  `components`, `resources`, `types`, and a `manual` pointer. `types` is the transitive closure, so a
  field reported as a `Phase` can actually be looked up and its variants read. Before ADR 0030 the
  schema named types it could not describe, and resources were absent entirely.
- **The engine shows you how to spell something.** `amadeo describe Run --package vault --example`
  emits a minimal valid instance in both the scene and JSON spellings, generated from one value so
  they cannot disagree. It teaches that `phase` takes a **bare word** — `phase "Playing"` parses and
  then fails to load, and no schema could ever have said so, because bare-versus-quoted is grammar
  rather than type information. Tested by pasting the output into a scene and loading it, for every
  component in the engine.
- **A moment can be saved and returned to.** `amadeo snapshot --ticks 600 mid.snapshot` captures a
  whole world to a readable text file; `amadeo status --from mid.snapshot` gets back to tick 600 by
  *reading the file* rather than simulating 600 ticks. Verified across separate processes, hashes
  matching exactly. This is the answer ADR 0011 named to the one problem its spike actually found —
  re-simulation, not compilation, is what degrades the agent's loop.
- **There is a game.** `cargo run -p vault` — collect six sigils in a walled arena without touching
  a patrolling warden. Player movement, wall collision, enemy patrols, a sprite-digit score, and a
  win and a lose state. The level is a `.scene` file; the sprites come from hand-written `.pix` text.
  **It was built and debugged without ever being looked at**, which is the whole point: 22 tests
  drive it headlessly, and `render.describe` caught a layout bug — the score readout overlapping the
  top wall — that no simulation test could have seen.
- **Repeated content is written once.** `entity s1 "Sigil" from sigil_pickup` plus a two-line
  `override Transform` is a whole sigil (ADR 0029). The prefab is an asset like any other, so
  `amadeo check` validates the reference and offers "did you mean" on a typo. A prefab may instance
  another; a cycle is reported with its chain rather than expanded forever.
- **A prefab that changed cannot silently lose an override.** If an override names a component the
  prefab no longer has, loading refuses and says which entity, which component, which prefab. That is
  the deliberate opposite of Unity, where the value quietly reverts and you find out months later.

**M0 exit gate: 4 of 4, nothing carried.** Gate item 2's "separate process" half — open since M0
because it needed `amadeo-cli` — closed in session 6: `amadeo replay` plays
`games/quad-demo/replays/wander.replay` through the real game binary in a fresh process, four
checkpoints asserted, and CI runs it in the determinism job.

**M1 exit gate: all five tested.** Gate 1 (a complete small 2D game) is `games/vault`. Gate 2
(verify it through the CLI and RPC without looking) is `tests/verified_without_eyes.rs`, and it found
a real layout bug. Gate 3 (scene round-trip byte-identical) has held since session 5. Gate 5 (golden
replays still pass) holds, including through the prefab conversion. **Gate 4 was tested and found
false**, which is a result rather than an omission — see below.

**No blockers of any kind.** Q14, Q13, Q4, and two thirds of Q3 all closed in session 6 — every one
of them except Q4 built the same session it was decided.

## ✅ The CI crash is fixed and confirmed — nothing to do here

`baefb1f` went **5/5 green** on both platforms, so the wgpu fix worked. This section stays only
because the shape of the bug is worth keeping; there is no outstanding action.

**What it was:**

1. The job's release step ran `cargo test --workspace --release` **without** `--test-threads=1`,
   where the three debug runs three steps earlier had it. Same code, same job, one flag apart.
2. It ran the GPU tests at all only because of **feature unification** — `quad-demo` and `vault`
   enable `amadeo-render/gpu`, so `--workspace` turns it on for everything. A comment in `ci.yml`
   asserted the opposite and had never been checked.
3. Parallel GPU tests each drop a headless device while others are alive, which is
   [`gfx-rs/wgpu#6571`](https://github.com/gfx-rs/wgpu/issues/6571) — reported against precisely this
   situation.

**Fixed in two places on purpose**: `tests/capture.rs` takes a `static` mutex for each device's whole
lifetime, so a developer running `cargo test --all-features` is safe too, and `ci.yml` passes
`--test-threads=1` on the release step so it matches the debug ones.

**The caveat that was here is now discharged.** This machine has a real GPU and never reproduced the
crash, so the fix was reasoned from the known wgpu bug rather than confirmed against the failure —
and then CI confirmed it. Worth remembering that "it passes on my machine" proved nothing here, and
the only evidence available was a push.

## The single most important thing to do next

**M2.5 is complete, and so is Q29.** A dug world now saves and reloads dug (ADR 0046). **M3 has
started**, with the renderer work ADR 0045 ordered — **mipmaps and anisotropic filtering are done**,
and **normal mapping landed in session 14 (ADR 0047)**.

## ✅ Q28 is closed — the sky is a light source (ADR 0049)

**`let ambient = 0.12;` is gone.** One number added to every surface regardless of which way it faced
is now a real environment: decoded from a Radiance `.hdr`, projected onto a cube, convolved twice at
load, and read by the shader as irradiance for diffuse and a GGX-prefiltered chain for specular.

**Justin was given four options with costs and chose full image-based lighting** over the cheaper
hemisphere gradient that was recommended — the right call, because M3's exit gate is an indoor scene a
sky gradient does nothing for, and because the gradient would have been thrown away rather than
extended.

**The visible result** is clearest in `games/scarp`'s shadow: it was a flat dark hole and it is now
**blue**, because the sky fills it. Surfaces facing up pick up sky colour; a metal reflects its
surroundings instead of rendering black, which is what ADR 0048 shipped without.

**And the sky is drawn too**, in a follow-up commit: the background was a flat clear colour until
then, so the sun lighting the world and casting its shadow was invisible.

**Two defects in the sky, both found by looking, and the second is a rule rather than a slip.** The
sun was *below the horizon* — a direction vector written by hand with the sign of Y inverted, so
light travelled upward and the sky had no sun in it, silently. It is now derived from the same Euler
angles the scene uses, through the engine's own transform code. Then fixing that blew the whole demo
out to near-white, because **the scene has a `DirectionalLight` for the sun and the sky had a disc for
the same sun** — every surface received it twice. Irradiance weighs a direction by its solid angle, so
a 5° disc at 250× is half a percent of the sky and out-contributes all the rest of it. `docs/07`
carries the rule: keep a direct light's *energy* in the light, and only a token of it in the
environment.

**Two defects, both found by looking at the picture rather than by reasoning** — the third session
running that this has been the method:

1. **The whole feature rendered nothing.** Every game installs `TextureCache` by hand, so `SkyCache`
   followed that precedent and *nothing installed it* — so every frame silently fell back to the
   neutral sky, with no error, no failing test, and a capture identical to the one before. It now
   installs itself beside `EnvironmentCache`. **`docs/07` carries this as a fourth sibling of the
   background-work rule: a capability may not depend on a caller remembering to enable it.**
2. **Adding `sky` to `Environment` broke every `.environment` file**, and the symptom was not a parse
   error — the file was skipped in silence and the failure surfaced three layers away as a *missing
   service*. That is **Q32** arriving one session after it was filed as theoretical.

**And one test was rewritten rather than kept.**
`a_metal_is_black_under_ambient_because_there_is_no_sky_yet` was written last session so that closing
Q28 would break it. It did not break — it kept passing for a reason that had stopped being true, which
is worse than no test. It now checks the real claim: a metal under a blue sky reads blue, under a red
sky reads red.

**Sharp edges it ships with**, both in ADR 0049: prefiltering costs **seconds at load** for a real
environment map (an import pipeline is the answer when that stops being acceptable, which ADR 0026
already anticipates); and **the sun in a generated sky and the sun in a scene must agree by hand** —
`games/scarp`'s `bin/sky.rs` carries a `SUN_DIRECTION` matching its `DirectionalLight`, and nothing
holds them together.

---

**What made it the one that mattered:** it replaced the hardcoded `0.12` ambient (**Q28**). ADR 0045 called it "probably the single biggest step
towards looking like a real engine", and building PBR has made that *more* true rather than less.

**Three things now converge on it, which is why it is unambiguously next:**

1. **Metals are unusable without it.** A metal has no diffuse, so one lit by a constant ambient is
   black — correctly, since a metal with nothing to reflect is black. The sky is what it should be
   reflecting. `a_metal_is_black_under_ambient_because_there_is_no_sky_yet` is written so that closing
   Q28 **breaks it**, which is deliberate.
2. **Shadowed areas are still flat-filled**, which is the other half of why a scene reads as a
   prototype, and no amount of texture or BRDF work touches it.
3. **PBR's payoff is mostly locked behind it.** A real BRDF with nothing in the environment to reflect
   is a very good specular model of a single sun.

**One thing to decide near it: whether the default `Environment` should tonemap.** ADR 0034 made the
default a byte-identical no-op, which was right when nothing produced values above 1.0 — and PBR is
what makes it produce them. A near-mirror facing a light is genuinely a hundred times brighter than
white, and the HDR target carries that correctly right up until the default look clips it. Worth
settling *with* sky lighting rather than on its own.

**And Q32 is now concrete rather than theoretical.** Every field added to `Material` rewrites every
`.material` file. Normal mapping added two, PBR added one, and image-based lighting will want more.

That is where "it looks like a prototype" stops being true, and M3's exit gate — a dark corridor with
a moving flashlight that reads as genuinely atmospheric — was always the renderer's real exam.

Worth saying plainly at the milestone boundary: **M2.5 was about worlds that scale, and it scaled
them.** It was never about how they look, and the demo looks accordingly. ADR 0045 is the evidence
that this is a feature-set gap rather than a backend one.

**`par_for_each_mut` is built, and the `rayon` question is answered by measurement rather than
argument** — no dependency. `std::thread::scope` spawns threads per call and that cost is real, but
the numbers say it does not matter: 1.29× at 2,048 rows, **3.35× at 16,384**, **5.42× at 131,072**,
on 8 threads. A persistent pool would only help the small end — which is the case where this should
not be used at all, since the whole simulation tick is 8.3 µs. `amadeo-jobs` already has a persistent
pool for genuinely coarse work.

**Background asset loading is built** — `AssetStore::load_all_in_parallel`, the first real consumer
of `amadeo-jobs`. It produces a store **byte-identical** to the sequential path, failure messages
included, and `AssetStore` derives `PartialEq` so the test compares the whole store rather than a
sample of it. Reading files parallelises better than arithmetic does, because it is mostly waiting on
the operating system.

**Surface-nets meshing is built** -- `amadeo-voxel`, no dependencies, a pure function from a
signed-distance `Field` to a mesh. **ADR 0042 settles the data model**, which is the half that
matters: terrain is a **generated base plus a sparse overlay of edits, and only the edits are
hashed**. An untouched world costs nothing to store or hash; a dug tunnel costs the samples that were
dug; and a save file is a seed plus a diff rather than gigabytes of voxels.

**Next: chunked streaming** -- where ADR 0041's visual/gameplay split gets its first real exercise,
and where the apron constraint bites.

**Before that, one loose end worth closing early: export something from Blender and import it.** The
whole glTF path is built and tested from both ends, but only against `.glb` fixtures constructed in
the tests. Nothing has yet been through a real digital content creation tool, and that is exactly the
sort of gap that turns out to contain a surprise. `amadeo import-gltf <file.glb>` is the command.

Then, in order:

1. **A real Blender round-trip**, as above.
2. **Textures on imported materials.** A generated `.material` carries colours only, so a textured
   model imports untextured. ADR 0026's decode path already exists, so this is wiring rather than
   design.
3. **Collision events into `amadeo-events`**, which turns a sensor into a gameplay trigger. The
   Vault's sigils are exactly this shape, done by hand today. This is also what would let the
   character report what it bumped into: `move_shape` already receives per-collision callbacks from
   rapier and deliberately throws them away, because nothing consumes them yet.
4. **PBR**, a normal matrix per instance, more than one light, transparency. All cheap, all isolated
   behind `RenderBackend`, none blocking a gate.

Two things the Atrium turned up are now written down as **Q28** (authored ambient / sky light) and
**Q27** (camera collision for a third-person rig), so they are decisions waiting rather than notes
buried in a status file.

## Chunk streaming's foundation — ADR 0043, session 12

**Justin was given two decisions and took the recommendation on both.**

**1. Colliders exist only at the finest detail level.** Distant chunks are drawn and are not solid.
The reason is not cost: if a collider changed resolution, and resolution depends on viewer position,
the ground under a character would change shape because *another player* walked toward it — gameplay
state moving for a rendering reason, which is close to what ADR 0041 exists to prevent. Pinning it has
a consequence worth having: **the seam question becomes purely visual** and leaves the state hash
entirely. The cost is named rather than hidden — anything needing to interact with terrain far from a
viewer falls through it, and that gets its own answer when a game needs one.

**2. One resolution now, LOD decided against a running system.** Q25 stays open, which is what it
asked for. What is *not* deferred is the level itself: `ChunkKey` carries `lod`, because a chunk's
resolution is part of its **identity** — two chunks over the same volume at different resolutions are
different chunks with different meshes, jobs and collider ids. Adding it later would change the key
type that storage, jobs, colliders and residency are all built on. Same move as ADR 0038's
`ShadowMode`.

**Residency is integer arithmetic, and concentric boxes rather than an octree.** `godot_voxel` — the
closest production analogue — migrated away from an octree to exactly this, for reasons that apply
here: predictable loading, and several viewers without split/merge logic reconciling them. Six of the
eight target games are co-op or multiplayer (ADR 0006). There is exactly **one** floating-point step
in the whole module, turning a world position into a chunk coordinate, and it uses only division,
`floor` and a saturating cast.

**Research killed two of Q25's four options**, so they do not get re-investigated: Transvoxel is for
marching cubes, a *primal* method, and surface nets is *dual*; and the seam-octree approach needs
adaptive leaf nodes where `amadeo-voxel` is a uniform grid. What is left would be derived here rather
than ported.

### Terrain collision could not be a component, and that is the sixth time

`Shape` is `Copy` and `StableHash`. A triangle mesh is neither cheap to copy nor something ADR 0042
will allow into the state hash — the whole point of a generated base plus sparse edits is that an
untouched world costs nothing to hash. It cannot go through `step` either: `BodyState` is handed over
in full every tick, and a chunk is thousands of triangles.

So it travels **the way a texture travels to the GPU: by id, uploaded once.**
`PhysicsBackend::insert_static_mesh` / `remove_static_mesh` / `static_mesh_count`. The geometry is
derived, so ADR 0019 puts it outside the hash; what *is* hashed is the seed and the edits that made
it. And it knows nothing about terrain — a static trimesh is equally an imported level's collision, a
bridge, or scenery too concave for a box. Same shape as `move_shape` knowing nothing about characters.

**Three things in it worth not rediscovering:**

- **Empty is the common case, not an edge case.** Most chunks of a real world are entirely air or
  entirely rock and both mesh into nothing, and `ColliderBuilder::trimesh` returns a `Result` that
  refuses no triangles. Rejected explicitly by **both** backends with the same error — a null backend
  that accepted more than the real one would hide a missing filter until somebody enabled rapier.
- **Inserting a known id replaces rather than accumulates.** Digging into a chunk re-meshes it under
  the same id; leaving the old surface behind makes the tunnel you just dug still solid.
- **Removing wakes the bodies resting on it.** Taking the ground from under a sleeping crate without
  waking it leaves the crate hanging in mid-air until something else disturbs it, which reads as a
  physics bug and is bookkeeping.

`reset()` drops static geometry deliberately, per ADR 0028: restoring a snapshot must not leave the
old world's ground standing in the new one. Terrain is derived, so rebuilding costs nothing.

**Every collision claim is asserted twice**, per ADR 0037 §5. Against rapier a ball dropped on a
two-triangle floor rests at 0.5; against `NullPhysics` the same ball falls through to below −1. **The
control half is not gated on the feature**, so it is never the one that gets skipped — without it, a
ball "resting" at 0.5 could just as well be a ball that never moved.

### The streaming core is built, and breaking it exposed a weak test

`amadeo-terrain` — `TerrainStreamer::update(viewers) -> TerrainUpdate`. Visual chunks go to a
`JobPool` and are collected from an `Inbox` draining in key order; **collision chunks are meshed
inline, on the calling thread, before the tick continues.** That asymmetry is the API:
`TerrainUpdate::colliders` is complete every tick and gameplay may rely on it, `TerrainUpdate::meshes`
is whatever finished and gameplay may not look at it.

The crate depends on `amadeo-voxel` and `amadeo-jobs` and **nothing else** — no `World`, no renderer,
no solver — so M2.5's exit gate 2 is testable with no engine at all. A streamed chunk also needs **no
renderer change**: `MeshCache` is keyed by a plain string, so `chunk_mesh_id` names the geometry and
everything above the loader is untouched. ADR 0035's bet, paying off a fourth time.

**The finding worth keeping.** Breaking it on purpose — routing colliders through the pool, exactly
what ADR 0041 §2 forbids — made two tests fail. But
`the_thread_count_cannot_reach_the_colliders`, **the test named after the exit gate, did not.** It ran
each worker count to completion separately, and a streamer running alone gets all the wall clock it
needs, so even the slow configuration finished in time and the divergence never appeared. It now
walks five streamers east **in lockstep** at 1, 2, 3, 5 and 8 workers, and the same break fails it
with *"5 workers produced different colliders than 1 worker"*.

So: **"watch it fail" is not only how you check the implementation — it is how you find out whether a
test measures what its name claims.** That one would have sat in the suite looking like exit-gate
evidence and proving nothing.

**Then CI found a second class of the same bug, twice.** `colliders` came out with the right contents
in the **wrong order**, and the order followed worker count: it was built as "meshed this tick" then
"already known", and which group a chunk fell into depended on what the job pool had finished.
Fixing it exposed the same shape in `removed`, which was filtered by *"was the caller ever told about
this chunk"* — and what the caller has been told is exactly what delivery timing decides.

> **The general form, worth carrying forward:** *anything filtered by "what does the caller already
> have" inherits the nondeterminism of delivery.*

Both looked like bookkeeping and both were ADR 0041 §2 violations. All four `TerrainUpdate` lists are
now **structurally** deterministic or structurally not — three are `BTreeSet` differences over
residency, `meshes` is the inbox drain and is timing-dependent by design.

### The ECS layer — `stream_terrain`

`TerrainViewer` on an entity loads terrain around it; `TerrainChunk` marks each streamed chunk and
**carries its key on the entity rather than in a map on the service**, because a service is not
restored by a snapshot and a lost map would leave chunks nothing could despawn (ADR 0028).

Two things in it worth not rediscovering:

- **Entities are spawned from `visible_added`, never from mesh arrival.** An entity is world state,
  so spawning on arrival would make the entity allocator — and the state hash — follow machine speed.
  A chunk whose mesh has not landed is an entity that draws nothing, which is correct and invisible.
- **The collider path must fill the mesh cache too.** A collision chunk is meshed inline and recorded
  as known, so the pool never touches it and it never appears in `meshes`. Miss this and the one
  piece of terrain that is invisible is the ground you are standing on.

Behind an `engine` feature, off by default, so the core keeps its no-engine-dependencies property and
ADR 0041's claim stays testable with no `World` in the build. **CI runs `--all-features` in the `test`
job** (both platforms), and the determinism job deliberately does not — which is the right split,
since the determinism-critical core is not feature-gated. That was **checked in `ci.yml` rather than
assumed**, because a feature-gated test nobody runs is this project's most-repeated defect.

### Digging works, and one race in it was worth closing carefully

`TerrainStreamer::edit` changes a sample. Two things in it that would have been found the hard way:

- **An edit invalidates up to eight chunks, not one.** A chunk of `n` cells covers samples
  `[k*n - 1, k*n + n]`, overlapping its neighbours at both ends because of the two-sided apron. Mark
  only the chunk that "owns" the sample and the neighbours keep geometry that disagrees with it —
  so the crack opens exactly where somebody has been digging.
- **A job that started before an edit finishes after it**, carrying geometry for a world that no
  longer exists. Delivering it refills the hole milliseconds after it was dug: timing-dependent, and
  close to unreproducible from a bug report. Every job now carries the **edit version** it was
  submitted under and stale deliveries are discarded. Edits are copy-on-write behind an `Arc`, so
  running jobs finish against the old data safely rather than being raced.

### Four judgement calls made alone this session, flagged rather than buried

All four are cheap to undo — `CLAUDE.md` §5 allows deciding those alone and saying so. If any looks
wrong, say so and it changes.

1. **`amadeo-terrain` is one crate with an `engine` feature**, rather than two crates. Precedent is
   `gpu` and `rapier`. The consequence worth knowing: the crate's *position in the dependency graph*
   is feature-dependent — above `amadeo-app` with it on, just above `amadeo-voxel` with it off.
   Nothing depends on `amadeo-terrain`, so neither position can cycle. Undoing it is a file move.
2. **`Physics` gained two narrow pass-throughs** (`insert_static_mesh`, `remove_static_mesh`) rather
   than an exposed `backend_mut()`. Handing out `&mut dyn PhysicsBackend` would let any caller drive
   `step` directly, and `step` must be fed from the world's components because they are the source of
   truth (ADR 0036). Static geometry is the one thing that genuinely cannot travel that way, so it
   gets its own door and nothing else does.
3. **`removed` is keyed on the *visual* set, not the data set.** The data set is one ring wider and
   exists so meshing can read into neighbours; since a chunk is sampled from the source on demand
   rather than stored, nothing is held for it to release. The apron therefore has no runtime consumer
   right now — it is enforced by a test and by `ChunkShape::samples_per_axis`, which is honest rather
   than dead.
4. **UVs on terrain are a flat planar projection from x and z.** There is no artist to author them.
   It stretches on vertical faces, which is what triplanar mapping fixes and belongs with the
   material work, not here.

## Q9 is resolved and M2.5 exists — ADR 0041

**The oldest open architectural question is closed**, and it was closed the way it asked to be:
*"Decide before adding the first background task, not after."* Justin asked for multithreaded asset
loading and parallel ECS queries, which are the first background tasks.

**The engine had no threading at all** — grepping for `std::thread` found only
`thread::current().id()` inside error messages.

**What the research found, and it is specific.** Three ways parallelism destroys determinism, all
demonstrated in shipping engines: Bevy's own `ParallelCommands` docs admit command order depends on
thread count; floating-point addition is not associative so parallel reduction changes results (the
same problem ADR 0036 hit); and **whether a job finished by tick N depends on the wall clock**, which
diverges a replay even though every computation was correct. Avian reaches rapier's conclusion —
disable parallelism for strict determinism.

**But ECS is a viable deterministic concurrency model, on a precise condition**: each parallel unit
must write only its own entity's components, with no shared accumulator and no cross-entity reads.
That is enforceable by API shape rather than by documentation.

**And one measurement set the priority.** Gate 4 says the whole simulation tick is 8.3 µs — 0.05% of
a frame. Parallel *system execution* would optimise something that costs nothing. What is expensive
is asset loading and chunk meshing, and neither is a gameplay system: both are jobs.

So: **parallelism is allowed only where determinism is structural, and the unsafe shapes are made
unspellable** rather than discouraged — the same move as `Component: Reflect` and the
Resource/Service split.

`amadeo-jobs` is built, with **no dependencies at all**. A job owns its inputs so it cannot borrow
the world, and there are exactly two ways an answer returns: wait at a barrier, or deliver into a
Service gameplay cannot observe. An `Inbox` drains in **key order, never completion order** —
`the_same_work_drains_identically_however_many_workers_run_it` runs the same jobs on a pool of 1 and
a pool of 8 and requires identical output.

**`par_for_each_mut` is built too**, and its signature is the safety argument: the closure is
`Fn + Sync`, so a captured accumulator will not compile — which matters because float addition is
not associative and a parallel sum genuinely gives a different number. No `&World` and no `Commands`
either, so no cross-entity reads and no spawning. `the_thread_count_cannot_reach_the_answer` runs the
same work at 1, 2, 3, 5 and 8 threads and requires identical output; the odd counts are there because
an off-by-one in chunk slicing hides completely when the rows divide evenly.

**And the `rayon` question is closed by measurement rather than argument.** On 8 threads: 1.29× at
2,048 rows, 3.35× at 16,384, 5.42× at 131,072. `std::thread::scope`'s per-call thread spawning is
what the small end pays, and a persistent pool would only help there — which is the case where this
should not be used at all, since the whole simulation tick is 8.3 µs. No dependency taken.

### The rule most likely to be got wrong later

**A streamed terrain chunk has two products and they have different rules.** Its *mesh* is drawn and
nothing else, so it goes in a Service and may arrive whenever. Its *collider* is gameplay — a
character stands on it — so when it arrives changes where the character is.

Which chunks are active is therefore decided **deterministically** from the player's position, and
the simulation **blocks** on colliders it needs. A slow machine gets a frame hitch and keeps its
replay. ADR 0021 already established half of this for assets: gameplay may not ask "has this finished
loading?" The generalisation is that gameplay may not observe *any* completion timing.

## Gate 4 closed, and M2 with it — ADR 0040

**Numbers, measured and written down**, in `docs/10-frame-budget.md`. Regenerate with
`cargo test -p atrium --release --test frame_budget -- --nocapture`.

| | Release | Share of a 60 Hz frame |
|---|---:|---:|
| One simulation tick, `games/atrium` | **8.3 µs** | 0.05% |
| CPU-side frame preparation, 1280×720 | **125.5 µs** | 0.75% |
| Simulation at 211 bodies (gate 3's case) | **450.2 µs** | 2.70% |
| Simulation at 811 bodies | 1914.3 µs | 11.49% |

Growth is roughly linear and slightly worse — 13.3× the bodies costs 16.3× the time, which is what
contact-heavy physics looks like. Single-threaded permanently, by ADR 0036.

**The engine had no profiler at all**, so this needed one, and that was the decision: measuring per
system means the tick loop reads a clock, and `CLAUDE.md` trap 2 names exactly that as a
nondeterminism leak. Justin was given three options and took the recommendation — **a `Profiler`
service, always on**.

**ADR 0009's split is what makes it safe**, and it turns out to have been built for exactly this: a
resource is simulation state and is hashed, a service is machinery and is structurally excluded. So
nothing the profiler records *can* reach a replay — `World::state_hash` cannot see the service store.
`profiling_does_not_move_the_state_hash` runs two worlds 180 ticks, one profiled and one not, and
they agree exactly. **The residual risk is named rather than hidden:** a gameplay system could read
the service and branch on a duration, and only the golden replays would catch it.

**Always on rather than feature-gated**, because an agent cannot *feel* a frame-rate problem — which
is `docs/04` §18's own stated reason for wanting `profile.frame`. A profiler compiled out of the
shipped build would report on a build nobody runs.

### Three things worth keeping

- **`docs/04` §18 marked `profile.frame` ✅ and it did not exist.** `docs/protocol/v1.md` had it right
  and listed it as pending. Same class of error as the CI comment that asserted the GPU tests did not
  run — a claim written into a doc and never executed. Both docs are now correct.
- **The worst run matters as much as the mean, and `SystemTiming` keeps both.** `step_physics`
  averages 4.4 µs and its worst single run was 102.8 µs — 23× its own average. Still only 0.6% of a
  frame, so it is a fact rather than a problem, but an average alone would have hidden it entirely.
- **A timing test went red in session 12, and the claim above was the reason it could.** The
  frame-preparation test reused `CEILING_FRACTION` — a constant whose doc comment argues it is
  "deliberately enormous… the only timing claim a shared CI runner can support". That argument is
  true for the *tick* test it was written for (8.3 µs against an 8333 µs ceiling, 1000× headroom) and
  **false** for frame preparation, which measures 130 µs on real hardware and **8764 µs** on a
  runner's software adapter. One constant, two measurements three orders of magnitude apart, and
  nobody checked which. Fixed by making the engine able to say what answered —
  `WgpuBackend::adapter().software` — and asserting only on real hardware. Same class as the three
  findings below: **a claim written down and never executed under the conditions it claims to cover.**
- **Nothing fails CI on a timing regression, deliberately.** §6 forbids wall-clock tests, CI runners
  are variable, and a flaky performance gate is one people learn to ignore — which is worse than not
  having one. What *is* asserted: scene complexity, run counts, one enormous ceiling at half a frame,
  and a loose scaling ratio across four body counts an order of magnitude apart, which can tell a
  slow constant from a bad complexity class where a single measurement cannot. Same split
  `sprite_throughput.rs` settled.

### What the gate does not answer, stated rather than papered over

**GPU execution time is not measured.** The profiler covers systems and CPU-side frame preparation is
timed separately, but how long the GPU takes to run the commands it is handed needs **timestamp
queries** the wgpu backend does not have. On a scene this small the GPU is almost certainly idle —
and "almost certainly" is not a measurement. Also unmeasured: a scene with real art in it, sustained
frame time in a real window with vsync, and memory.

## glTF import landed — ADR 0039

**M2's exit gate 1 is now complete on every part.** `amadeo import-gltf level.glb` turns a model into
engine text.

**The framing was about data again, and this time it was about which data.** "Import a glTF" is
ambiguous, because a glTF is *not a model* — it is a whole scene, with a node hierarchy, materials
and meshes. So the question was never how to read one. It was **which parts become engine text and
which stay art**, because I1 says text files are the only source of truth.

Half of that was already settled by precedent: a `.png` is opaque bytes and ADR 0026 accepted it,
because a PNG is source art rather than authored engine data. A `.glb` from Blender is the same kind
of thing. The other half is what Godot, Unity and Unreal all do — convert the hierarchy into the
engine's own scene format, leave geometry as a resource. Nobody serialises vertex arrays into their
human-editable format, and nobody leaves the layout locked inside the interchange file either.

Justin was given three options and took the recommendation.

**What one command writes:** a `.scene` for the hierarchy, a `.material` per material, a `.mesh` per
**primitive** (a four-line pointer, not vertex data), and a sidecar for the `.glb`. What people and
agents author — where the wall goes, what colour it is, what is parented to what — is text. Vertex
positions are not.

### Four things in it worth not rediscovering

- **A glTF *primitive* is what corresponds to an Amadeo mesh, not a glTF *mesh*.** A mesh holds one
  primitive per material and a `Mesh` component draws one thing with one material, so getting this
  backwards silently loses every material but the first — which looks like an art problem.
- **The indirection is a `GltfPart` component inside the `.mesh` asset**, not a compound id string
  and not a field on `Mesh`. A string like `"level#3"` hides structure inside a name, which is the
  exact defect ADR 0030 called out; a field on `Mesh` would make every existing scene file grow
  something meaningless to a procedural shape. A `.mesh` file already *is* the indirection from a
  name to a shape.
- **Generated text is parsed and re-emitted through the canonical writer** rather than written
  canonically by hand. I2 says `amadeo fmt` is the single authority, and a generator reimplementing
  the rules is a second one — they disagreed over a trailing blank line the first time this ran.
  Parsing its own output also turns a generator bug into a failure at import time that names the file.
- **The scene format needed no change at all** to express an imported hierarchy, because nested
  entities already meant parenting. ADR 0014 and ADR 0032 paying off rather than luck.

### And ADR 0035's bet paid, three milestones later

It was written before any of this existed, specifically so the importer would be an **addition**
rather than a change to the mesh component, the cache, the batcher and every test that asserts on a
mesh. `GltfPart` is simply a third producer of `MeshData` alongside `BoxMesh` and `PlaneMesh`, and
`a_gltf_mesh_is_still_just_a_mesh_to_everything_above_the_loader` pins it: both load through one
call, into one cache, with no caller distinguishing them.

**Tested from both ends, because either half alone would pass its own tests while the pair was
broken.** `amadeo-cli` asserts the importer writes valid canonical scene text; `amadeo-app` asserts
that text actually loads and produces the glTF's vertices rather than an empty mesh nobody notices.
The `.glb` fixtures are **built in the tests rather than committed**, so the format is written down
and reviewable instead of hidden in a binary.

**What it does not import:** textures (a generated material carries colours only, so a textured model
imports untextured), animations, skins, and cameras. And **nothing has been through Blender yet** —
only fixtures built in the tests.

## `games/atrium` — M2's demo, and what building it found

Three of gate 1's four parts had been built and **none of them had ever been seen together**:
lighting (session 9), the character controller and shadows (session 10). Each was proved by headless
tests and single-purpose GPU captures, and by nothing else.

`cargo run -p atrium` is a lit 3D room — floor, four walls, four pillars, a plinth, a sun casting
shadows, and an amber character steered with WASD, Q/E and Space. The room, its six meshes, its three
materials and its look are **all text**, and `amadeo check --package atrium` validates every one of
them. The follow camera is a **child entity of the player** in the scene file and nothing else, which
is ADR 0031's "a camera parented to a character *is* a follow camera" cashed rather than repeated.

**The bet paid, exactly as it did in M1.** Five things it found, none of which any test had:

1. **Three materials were silently ignored and the whole room rendered default white.** They were
   missing a required field, and `load_materials` skips an asset that will not parse — so eleven
   meshes drew with the default material and the room looked like an untextured grey box. **`amadeo
   check` names the missing field and the file exactly**; it had simply never been pointed at them,
   because only the *level* was in CI's list. Both the level and every asset file are checked now.
   A validator nobody runs is a validator that does not exist.
2. **Contrast above 1.0 crushes shadows to pure black.** The grade is
   `(colour - 0.5) * contrast + 0.5`, which drives near-black values *negative*, and they clamp to
   zero. A contrast of 1.05 was enough to turn every shadowed pixel into a hole. Nothing in a scene
   was ever that dark before shadows existed, so this could not have surfaced earlier.
3. **The 0.03 ambient term was far too dark once anything could be in shadow**, and is now 0.12.
   Before shadow maps the only ambient-only pixels were faces turned away from the light — small, and
   fine as near-black. With shadows, whole areas of *floor* are ambient-only. **The real fix is an
   authored sky colour on `Environment`**; the constant is a stand-in and now says so.
4. **A GPU test was comparing against a clipped white.** Raising the ambient made
   `a_face_turned_away_from_the_light_is_darker_than_one_facing_it` fail at a difference of 13,
   because its square-on reading was already 255 and had no room left to move. Fixed by testing a
   mid-grey box, which makes the assertion about the lighting rather than about where the clip lands.
5. **`amadeo_character::install` must be called before `load_scene`**, since it is what registers
   `CharacterController` and `CharacterMotion`. Written the wrong way round first — and the error
   said so, including *"if it belongs to a module, that module may not be loaded"*, which is that
   message earning its keep. Now documented on `install`.

**Two limitations it also found, both real and neither fixed:**

- **The third-person camera clips through walls.** It is a child entity at a fixed offset, so backing
  the character into a wall pushes the camera outside the room. The fix is camera collision — a
  spring arm that shortens the offset when something is in the way — and `PhysicsBackend::move_shape`
  is already the right tool, since ADR 0037 explicitly names "a camera that must not clip through a
  wall" as something the query describes.
- **`render.describe` cannot see meshes.** Asked what the Atrium was drawing, it reported a default
  orthographic camera and zero entities, because it only knows the 2D path. That made it useless for
  the one debugging job it was reached for, and it is the agent's main way of seeing a frame without
  a GPU. Worth fixing before the editor needs it in M4.

## Shadows landed — ADR 0038

**The first thing in this engine that reads a depth texture rather than only writing one**, which
`STATUS.md` had carried as a known wrinkle since the mesh pass.

**The framing was about data again, for the sixth time in this subsystem.** `RenderBackend` isolates
the shader completely, so *how* shadows are computed was never the expensive part. The field on
`DirectionalLight` is: it is authored, it is hashed, and scene files carry it.

**The research is what settled the scope.** Godot ships single-map ("Orthogonal") as a real supported
mode alongside 2 and 4 splits — not as a stepping stone — and Unity and Unreal both expose cascade
count as an ordinary setting. Nobody treats one-versus-many as an architectural fork, because it is
not one. So `ShadowMode` ships `Off | Orthogonal` and cascades become a third variant: **one map now
is a value of a field that has to exist anyway, not a shortcut to undo.** Same argument `PixelFormat`
shipped with under ADR 0026.

Justin was given three options and took the recommendation.

**What is built:** the shadow pass, a sampleable depth format, hardware comparison sampling, 3×3 PCF
softening, slope-scaled bias, front-face culling in the shadow pass, and world-anchored texel
snapping. Off by default, so a game that never asks pays a 1×1 placeholder texture and a uniform
branch.

**Its honest limitation**, which is the mode's rather than a defect: one map stretched over a large
outdoor scene gives every shadow-map pixel a lot of ground, and edges go blocky. That is what
cascades fix, and where they will go.

### Four things in it worth not rediscovering

- **The snap grid must be anchored at the world origin, not the camera.** Got this wrong once while
  deriving it. Snapping the box relative to the camera is snapping to something that moves, which is
  no snapping at all — and without snapping every shadow edge crawls and fizzes as the player walks,
  with nothing in the scene moving. `a_shadow_box_moves_in_whole_texels` pins it.
- **A shadow map is its own `TargetFormat` variant, not a flag on `Depth32`.** They are the same wgpu
  format and differ in what they are *for*: one needs `TEXTURE_BINDING` and the other must not ask
  for it, and `assign_transients` matches on `(width, height, format)` — so without distinct tags the
  two could be handed the same texture and one would be missing the usage it needs.
- **`PooledTexture::bind_group` stays an `Option`.** The old note predicted shadows would be what
  finally sampled a depth texture and the `Option` would go away. Half right: there turned out to be
  *two* kinds of depth texture, and only one is sampled. The scene depth buffer still gets none.
- **Front-face culling in the shadow pass is the cheapest acne fix there is.** Recording the far side
  of each object moves the stored depth away from the surface being lit, so a lit surface stops
  shadowing itself. Paired with a **slope-scaled** bias, because a surface seen edge-on by the light
  spans far more depth per texel than one facing it square — one flat bias forces a choice between
  acne on slopes and peter-panning on flat ground.

### And it was verified by watching it fail

The floor under a floating block reads **46** with shadows on and **235** with them off, against a
lit floor of **235** in both. Measured before believing the green.
`the_same_scene_without_shadows_is_evenly_lit` keeps that control in the suite rather than as
something done once by hand — the same discipline session 9 arrived at the expensive way, and the
second session running where it is built in rather than restated.

## The character controller landed — ADR 0037

**Gate 1's character controller is built, and it created the `modules/` layer.** Justin was given
three options and took the recommendation: **a move-and-slide query on `PhysicsBackend`**.

**The framing was, for the fifth time in this project, about the data rather than the mechanism.**
`PhysicsBackend` had exactly one operation — `step(bodies, gravity)` — and for a `Kinematic` body
that means *put it exactly where gameplay said*, walls included. So a character built on the existing
trait would walk through the level. The question was never "how do we write a controller"; it was
"what second question does a solver have to be able to answer".

**What the research found**, and it decided the answer: Unity, Unreal, Godot and PhysX all ship an
explicit kinematic controller as the *primary* answer and treat a dynamic-body character as the
alternative. The line that settled it for this engine was not about feel — the recurring advice is to
choose an explicit controller **when deterministic replay and network sync are priorities**, which is
I3 and ADR 0006 rather than a preference.

**The split, which is what `modules/` is for:**

- **`amadeo-physics` owns the geometry.** `PhysicsBackend::move_shape` sweeps a shape, slides along
  what it hits, and reports where it ended up and whether it landed on something. It has no concept
  of a character — it describes a lift, a projectile, or a camera that must not clip through a wall.
- **`modules/amadeo-character` owns the character.** `CharacterController` (speed, acceleration,
  jump, turn, slope limit, step height) and `CharacterMotion` (velocity, grounded), driven by named
  input actions. Trap 10 in full: the engine must not assume a game has a character, and one of the
  eight target games does not.

**It cost less than expected and adds no determinism surface.** Rapier's `as_query_pipeline` is a
*borrowed view* over sets `RapierPhysics` already owns — it allocates nothing that outlives the call
and caches nothing between calls, so there is nothing new for `PhysicsBackend::reset` to clear.
Checked in rapier's source before the ADR was written rather than assumed.

**One ordering is load-bearing and would not have been noticed.** `move_shape` answers from a spatial
index `step` builds, so the character system must run **after** `step_physics`. The other way round it
queries an *empty* index on tick 1 — the character passes through the level once, at startup, and
behaves perfectly forever after. `install` sets `.after(STEP_PHYSICS)` so no game has to remember, and
a game that registers by hand and forgets gets a schedule that refuses to resolve and names the
missing label.

### The bug it produced, which is worth reading before touching the controller

The first version pressed the character gently downward while grounded — the usual trick for staying
attached to the floor. **It sank about 0.07 units per second and would eventually have fallen
through.**

Ground detection holds a character a **skin width** (0.01 units) above the surface; the downward bias
moved it 0.0167 units in one tick. Moving further than the gap left the capsule exactly touching the
floor, which is the degenerate case for a shape cast — **rapier's own penetration-fixing routine is
commented out in its source** — so it sank again next tick. Slow enough to look like tuning, fast
enough to lose a level.

Fixed by not pressing down at all: vertical speed is exactly zero while grounded, and staying attached
is `snap_distance`'s job, which pulls *down to* the surface after the move rather than aiming below it.
`a_resting_character_does_not_sink_into_the_floor` pins it over ten seconds.

**Generalise this:** when something moves a small amount per tick and something else holds a small
tolerance, compare the two numbers. Movement exceeding the tolerance will tunnel, slowly enough to be
mistaken for a feel problem.

**Found by tracing, not by reasoning.** Twelve ticks of printed height and grounded flags showed the
ratchet immediately — ticks 6, 7, 10 and 11 each dropping by exactly `1.0 * FIXED_DT`. The arithmetic
was in the output; no amount of re-reading the code would have been faster.

### And the session-9 lesson was applied rather than restated

**Every collision claim in `modules/amadeo-character/tests/walks_around.rs` is made twice** — once
against `RapierPhysics`, where it must hold, and once against `NullPhysics`, where it must fail. A
character that walks through a wall and falls through a floor is an asserted test, not a known
limitation. That is "a test is not evidence until you have watched it fail" built into the suite
rather than done once by hand.

## Where M2's exit gate actually stands

**Gate 1** — "an imported glTF level, dynamic lighting, shadows, and a physics-driven character
controller you can walk around with". Lighting ✅ (session 9), character controller ✅ and shadows ✅
(session 10), glTF import ✅ (session 10). All four parts are built, and the first three are in one
place and running as `games/atrium`. **The honest caveat: nothing has been through Blender yet** —
the glTF path is tested against `.glb` fixtures built in the tests, so a real digital-content-creation
round trip is the remaining unknown.

**Gate 2** — a 2D scene from M1 still renders unchanged. ✅ Holds: `games/vault` is untouched and its
tests, replays and capture all still pass.

**Gate 3** — a physics-heavy replay of 200+ bodies reproducing bit-identically across runs and
processes. ✅ `crates/amadeo-physics/tests/rapier_determinism.rs` pins a **literal state hash** that CI
runs on Windows *and* Linux, so a cross-platform divergence turns CI red rather than going unnoticed.

**Gate 4** — frame time within a declared budget at a declared scene complexity, numbers written
down. ✅ `docs/10-frame-budget.md`, regenerated by
`cargo test -p atrium --release --test frame_budget -- --nocapture`. Measured knowing physics uses
one core, as ADR 0036 requires — and it is nowhere near the limit: gate 3's 200-body case costs 2.7%
of a frame. **GPU execution time is the piece this does not answer**; it needs timestamp queries the
backend does not have.

`PhysicsBackend::reset` exists for ADR 0028's reason rather than a physics one — see its doc comment.

### The renderer: what landed, and what is left

ADR 0035's data half, the whole CPU side, and the mesh pass are all built. 3D renders.

- ✅ **The maths.** `Mat4::perspective` (WebGPU's 0..1 depth range, not OpenGL's −1..1),
  `Mat4::inverse_rigid` — which is what a view matrix is — and `project_point`, which refuses a point
  behind the camera rather than folding it back onto the screen mirrored.
- ✅ **Collection.** A `Mesh` with loaded geometry becomes a `MeshInstance` carrying its model matrix
  and its **resolved** material; a `DirectionalLight` becomes a direction and a pre-multiplied
  colour. `View` carries both, plus `eye_matrix`.
- ✅ **A perspective camera is no longer skipped**, and a camera's projection now selects which pass
  it feeds — orthographic gets quads and sprites, perspective gets meshes, neither built on the
  other (ADR 0031).

- ✅ **The depth buffer.** `TargetFormat::Depth32`, declared **only when a frame holds a perspective
  camera** — a full-screen depth texture for every 2D game would be a real cost paid for nothing.
  Depth is its own field on a `Pass` rather than an entry in `writes`, because it is state a pass
  tests against rather than an image any later pass reads. Verified on a real device, not just in
  the plan.

- ✅ **The mesh pass. 3D is on the screen.** The first pipeline here with a real vertex buffer —
  every other pass builds its geometry from the vertex index alone. Indexed drawing, per-instance
  model matrices and materials, depth testing, back-face culling, and one directional light.
  Geometry travels by id and is uploaded once, like a texture; a material travels by value in the
  frame, because five numbers are not worth an upload path.

**What remains:**

1. **PBR.** Shading is diffuse `N·L` today. The material already carries the metallic-roughness
   fields and nothing reads them yet — deliberately, so that getting geometry, depth, projection and
   lighting onto the screen was one problem rather than tangled with reflectance. `RenderBackend`
   isolates the shader, so this is the cheap change four ADRs have found it to be.
2. **A normal matrix per instance.** Normals are currently rotated by the model's basis, which is
   correct for uniform scale and wrong for non-uniform. The fix belongs in the instance data, not
   the shader, and nothing authors a non-uniformly scaled mesh yet. Commented where it lives.
3. **More than one light**, which needs either a loop in the shader or a pass per light — picking
   between those before anything wants two would be guessing.
4. **Transparent meshes**, which need back-to-front sorting within a `SortOrder` (ADR 0018 says so).
   The mesh pipeline is opaque-only until there is something transparent to sort.
3. **A shader.** Start with diffuse `N·L` against the material's base colour to prove the path, then
   PBR — which is a shader change and therefore cheap, per four ADRs.
4. **The projection**, built in the backend from `eye_matrix` and the target's aspect ratio, because
   only the backend knows the target size. `View` deliberately carries the camera's transform rather
   than a finished view-projection for that reason.

#### The wrinkle this section predicted, and how it actually resolved

The previous version predicted that **shadow maps or fog would be what finally sampled a depth
texture**, and that when they did, `PooledTexture::bind_group`'s `Option` would force the compiler to
ask about every place that assumed it could. Shadows landed in session 10, and that is half right —
worth recording, because the half that was wrong is the interesting one.

**The `Option` survives rather than going away.** There turned out to be *two* kinds of depth
texture: the scene depth buffer, which is only ever attached, and a shadow map, which is attached and
then sampled. They are the same wgpu format and want different usages, so they became **two
`TargetFormat` variants** rather than one with a flag. The shadow map gets a bind group built against
a comparison layout; the scene depth buffer still gets none, exactly as before.

The related note — that `assign_transients` matches on `(width, height, format)` so a depth transient
can never be handed a colour texture — turned out to be **load-bearing rather than lucky**. It is
what keeps a shadow map and a scene depth buffer of the same size from sharing a texture, which would
leave one of them missing the usage it needs. `a_shadow_map_and_the_scene_depth_buffer_never_share_a_texture`
pins it.

The other open question here — where depth fits in the graph's vocabulary — was settled when the pass
was written: `Pass::depth: Option<String>`, cleared by the first view pass and loaded by later ones.
A shadow pass is the exception that proves it useful, being the only pass with a depth attachment and
**no colour attachment at all**.

Fog and volumetrics are what still want the depth buffer, and M3's exit gate 5 depends on them.
Culling and glTF import are the rest — the last of which ADR 0035 made additive.

**Also still open in the renderer, in rough order after the mesh pass:**

- **Bloom's blur passes.** Its fields exist in the schema and are inert (`intensity` defaults to
  zero). This is the first effect needing more than one pass, so it is what will finally give
  `assign_transients` two same-shaped transients to reuse a texture between — `Lifetime::overlaps` is
  tested and has never had a real decision to make.
- **Render targets on a camera**, which ADR 0031 shipped as a field and nothing implements. Q23 says
  per-camera post-processing is the same work, so do them together.

### A gap found by running a claim instead of repeating it

ADRs 0033, 0034 and 0035 all say the same thing: a material, an environment and a mesh are scene
files, so `amadeo check` validates them **for nothing**. Pointed at the environment file the Vault
already ships, it refused — *no component named `Environment` is registered*.

The loader reads the type directly through `Reflect::from_value` and never consults the
`ComponentRegistry`, so **loading worked while validation did not**, and the two disagreed about what
counts as valid. Fixed where it belongs — a game that ships an asset registers the type it holds —
and both files are now checked in CI, which is what would have caught it.

**Worth generalising**: "the existing toolchain applies for nothing" is a claim about *other* code,
which makes it exactly the kind that gets written into an ADR and never executed. Run it.

### Physics: Q24 is closed, and the crate is built — ADR 0036

Raised by Justin asking whether the engine needs a physics engine. It does, and in **this
milestone**: two of M2's four exit gates depend on it. Both are now met or half met.

**Rapier gives exactly what gate 3 asks for** — bit-level cross-platform determinism, same bytes on
different CPUs and operating systems — through its `enhanced-determinism` feature. **But that feature
cannot be enabled alongside `parallel` or `simd-*`.** Determinism and fast physics are mutually
exclusive, and there was no "take both and decide later". It also pins the rapier version, because an
upgrade may legitimately change results and invalidate every replay containing physics.

**Determinism won**, permanently: I3 is non-negotiable and gate 3 is written against it. So the
physics layer is single-threaded and scalar *by design*. `CLAUDE.md`'s trap list puts retrofitting
determinism first, which is why this was decided before the crate existed rather than after.

**The consequence to remember for gate 4:** physics uses one core, and that is not negotiable. If the
frame-time budget is missed because of physics, the answers are fewer bodies, better culling, or
sleeping inactive bodies — not relaxing this.

**Nothing is blocked and nothing is undecided.** All six of M2's expensive decisions are made and
built. What is left is build work.

### The render-graph decision is closed — ADR 0034

The question this file carried into session 9 — *is the graph a public, extensible surface or an
internal detail?* — was researched and put to Justin, who took the recommendation: **internal**.

**The framing needed correcting for the fourth time in this subsystem**, and this time the confusion
was in the vocabulary rather than the emphasis. "Render graph" names two independent things: a frame
scheduler that derives pass order and allocates transients, and an extension point where a game
inserts a pass. The roadmap line asks for the first; the worry recorded here was about the second.
Only the second is an API decision — and most of the first is already done by wgpu, which tracks
resource state and inserts barriers itself.

**"Configurable post-process stack" also does not say what it looks like it says.** Tunable and
extensible are different things, and `docs/00-vision.md` asks only that the renderer not bake in a
look. Godot, Unity and Unreal all ship the tunable stack as the primary answer and put the extension
point behind an advanced, much later, much harder door. Bevy is the one engine that made its graph
public, and it is evidence *against*: it walked back from resource dependencies (graph slots removed
as boilerplate-heavy) and its public graph has been rewritten repeatedly, most recently as
render-graph-as-systems in 0.19.

The deciding argument for data over code was **I5 and I7**, not anything about rendering:
configuration made of data is authorable, describable, checkable and visible headless for nothing,
and a pass supplied as code is none of those. Same shape of argument that settled ADR 0030.

### The rest of M2

- **Mesh rendering, and with it the `Material` field list.** ADR 0033 settled *where* a material
  lives — an asset with an id, its file a scene file with one root — and deliberately left what it
  *holds* to arrive with PBR, since adding a field to a reflected type is the cheap change the schema
  exists for.

### Gate 4's result, and what closed it — ADR 0030

`describe` is a **schema, not a manual**, and the gate asked it to be both. Justin was given three
options and chose the most complete one. The decision splits along a line the gate had blurred:

**The API half stays out of the protocol, and `describe` says where it lives.** How to declare a
component, register one, write a system, query the world — that is API knowledge, and **invariant I5
is what settles it**: anything the editor can do, the CLI and RPC can do, and the editor will *never*
declare a new Rust component type, because that means editing the game crate and recompiling. So the
gate was asking the protocol for something the project's own invariants do not ask of it. `describe`
now carries a `manual` key naming `docs/07-working-with-the-code.md` — a pointer rather than the
prose, because prose copied into a protocol reply is documentation nothing recompiles.

**The schema half was a genuine hole, and fixing it properly found two more.** Resources were
missing — `Run`, which holds the Vault's entire outcome, appeared nowhere. Beyond that: the schema
was **not closed** (`Run.phase` reported `"type": "Phase"` and nothing could look `Phase` up, so
nothing could know the legal values were `Playing`, `Won`, `Lost`), and a fixed array's **length
existed only inside its name**, so anything needing the count had to parse `"array<f32, 2>"` apart.
Both are editor blockers that would not otherwise have surfaced until M4.

**And `describe <Type> --example` now emits something that loads** — a minimal valid instance in the
scene spelling and the JSON spelling, generated from one value so they cannot disagree. The clearest
justification for it: `phase Playing` is a **bare word**, and `phase "Playing"` parses and then fails
to load. Bare-versus-quoted is scene-format grammar rather than type information, so no amount of
schema would ever have said so.

`games/vault/tests/gate_four.rs` pins all of it, in the game that found the gap. The write-up
`docs/09-gate-4-describe-is-not-enough.md` keeps its honest caveat: the experiment was run by an
agent that had already read the engine source, so the gaps are ones it *noticed* rather than ones it
was stopped by, and the stronger test — hand the JSON to a reader with no prior exposure — has still
not been run.

### What building a real game found

The roadmap's bet was that a game would find what the engine is awkward at faster than reasoning
would. It did, and these are the findings rather than a list of chores:

- **The scene format is impractical for repeated content, and prefabs are what fix it.** A sigil cost
  fourteen lines of scene text and there are six of them. **This is what got Q7 settled** — ADR 0029,
  same session, decided from use rather than from theory. A sigil is three lines now and the scene
  went from 223 lines to 142 with `collect-three.replay` matching all four checkpoints unchanged.
  **But prefabs did not fix the walls, and should not.** Forty-four tiles as instances is 176 lines
  against a seven-line picture of the level, so they stay in `MAP`. Prefabs fix repeated *designed*
  content; a grid wants a tilemap (M7). "Prefabs will fix the walls" was the obvious expectation and
  it was wrong — which is itself the finding.
- **No game had ever loaded a scene file.** `markers.scene` had existed since session 5 and nothing
  read it. The reason was a papercut with teeth: `instantiate` needs the world mutably and the
  registry shared, `App` owns both, and the borrow checker refuses the obvious spelling — so every
  game would have had to rediscover the workaround. `App::load_scene` fixes it.
- **A game with two binaries breaks every CLI command against it** unless it sets `default-run`.
  `amadeo` launches games with `cargo run -p <package>` (ADR 0016), which is ambiguous the moment a
  package has a tool binary alongside the game. The failure is a cargo error with nothing to do with
  the engine, which makes it slow to diagnose.
- **PPM cannot express a sprite.** It has no alpha, so anything drawn over the floor would be an
  opaque rectangle. The Vault's art is therefore PNG, generated from hand-written `.pix` text files
  by a small tool in the game's own directory — which is a miniature of the import pipeline ADR 0026
  defers, with the same shape: hand-authorable input, machine-readable output, one command between.

### Then, in rough order

- **`snapshot.diff`** — comparing two snapshots. The format is text and diffable already, so this is
  polish rather than capability.

**The sprite path has been confirmed on screen** — Justin ran `cargo run -p quad-demo` at the end of
session 8 and the screenshot checks out against the world coordinates: nine floor tiles alternating
light/dark (so each is reading a *different texel* of one shared texture through its `region`, which
is the tilesheet mechanism), the 4×4 magenta placeholder where the deliberately-missing sprite is,
markers and player where their transforms put them, and texture colours matching the literal values
in the `.ppm` (so the sRGB texture format and sRGB surface agree rather than double-converting).

**One thing that is still unexercised: the vertical flip in `sprite.wgsl`.** The UV calculation does
`1.0 - corner.y` because world space has +Y up and texture space has v = 0 on the top row. With a
2-row test texture and `region.height = 0.5`, every sample lands in the top row whichever way v runs
— so a flipped image would look identical. **The first time a real photograph or a tall sprite sheet
goes in, check it is not upside down**, and if it is, that one line is the suspect.

**One more thing waiting on a trigger rather than on a decision:** the **import pipeline**, for when
a target game wants compressed textures or mip levels. ADR 0026 sets out exactly what changes and
what does not; the short version is that nothing above `TextureCache` is affected.

### Also worth knowing

**Q15** — modding versus ADR 0011, raised by the target list growing — blocks nothing today but
should not be discovered late. The other question raised alongside it, the **ADR 0014 / ADR 0020
disagreement about `from`**, is closed: ADR 0029 says an asset id and supersedes 0014's grammar.

### `amadeo-assets` and the sprite batcher — done, session 7

### `amadeo-assets` — done, in the order STATUS.md previously listed

1. ✅ **A directory scan** producing a catalogue. Sorted walk into a `BTreeMap` (I3), duplicate ids
   refused naming both files, and every problem reported at once rather than the first.
2. ✅ **A missing sidecar generated on import**, id defaulting to the filename stem. Prepare-then-apply,
   so a dry run is the same code path as a real one and nothing is written if anything would fail.
3. ✅ **`assets.list` and `amadeo assets`** — the ADR 0020 requirement, in place before ids became the
   reference syntax. Also `amadeo import`, and `--check` on it so it can gate a commit.
4. ✅ **Loading**, to ADR 0021's rule, plus the barrier and the `assets` block a scene declares in.
5. ✅ **`amadeo check` verifies asset ids**, with `similar_to` giving "did you mean".
6. ✅ **The sprite batcher and ADR 0023**, settling Q3's last third against a measurement.

**One decision came up that STATUS.md had said would not** — see ADR 0022 below.

The list that followed it here — `Resource: Reflect`, then snapshots, then Q7 — is kept current at
**The single most important thing to do next** near the top of this file. The first item is done as
of session 8 (ADR 0027); snapshots are now next.

### ADR 0022, and a correction to what this file said

The previous version of this section claimed the loading half had **no open decisions left in it**.
That was wrong on one point, found immediately on starting the work: a game names its asset directory
with a *relative* path, and the working directory differs in all four ways a game gets started — the
CLI sets it to the project root, `cargo run` from a subdirectory does not, and a packaged binary
could be anywhere.

Researched rather than guessed, per the standing instruction. Bevy answers with an environment-variable
chain (`BEVY_ASSET_ROOT` → `CARGO_MANIFEST_DIR` → executable directory); Godot anchors on a marker
file, defining `res://` as the directory holding `project.godot`. **ADR 0022 takes Godot's approach**,
because this project already has a marker file and `amadeo-cli` already walks up for it — resolving
the game side by a different rule would invent a disagreement about which project we are in. It also
needs no shared code, which matters because `amadeo-cli` deliberately does not depend on `amadeo-app`.

Worth knowing for next time: "no open decisions left" is a claim that should be checked, not trusted.

*(This paragraph was written in session 7 and named two open questions. The current list is the table
near the top of this file — there are seven now, and `docs/06-open-questions.md` is authoritative.
The struck-through two below are kept because their reasoning is still worth reading.)*

- ~~**Q3 (the last third) — which render pipeline shape.**~~ **Resolved in session 7 — ADR 0023.**
  Sprites batch by `(sort order, texture)`. Decided against measurements, as the question demanded:
  20,000 interleaved sprites collapse to exactly 32 batches, and a whole tilesheet is one draw call.
  The measurement also found that the pipeline shape is *not* currently the limiting factor — Q16 is
  — which is the opposite of what the question expected.
- ~~**Q7 — prefab override semantics.**~~ **Resolved in session 8 — ADR 0029**, and built the same
  session. `from` holds an asset id (superseding ADR 0014's grammar); an override is a top-level
  patch on the instance **root** and can reach nothing inside it, which is what makes nesting
  structurally safe rather than carefully handled; a dangling override refuses to load. The Unity and
  Godot failure modes the question told us to study are what decided the middle one — both come from
  overrides reaching *inward*, so here they cannot.
- **Q12 — `Service: Send + Sync`.** Not moot: a `kira` audio manager, an asset loader holding a file
  watcher, and a `wgpu` surface all hit it in M3. Decide when the first real offender lands.
- **Q15 — modding, and whether ADR 0011 still holds.** New in session 7, raised by the target list
  growing. ADR 0011 decided game logic is plain Rust, by measurement — but it measured *iteration
  speed for the developer*, and a mod author cannot rebuild the engine at any speed. The reserved
  WASM hatch is probably the right answer (the Q1 spike measured it bit-identical to native at 1.24×,
  and sandboxed by construction), but the trigger ADR 0011 recorded does not cover this reason.
  **Decide before the module system hardens in M2–M3**, since "what can a mod do" is the same
  question as "what is the module boundary". Nothing today depends on it.

Prefab instancing, which this paragraph used to describe as unbuilt-rather-than-undecided, is now
both decided and built: `App::prefab_library` resolves each `from` id through the asset catalogue and
hands the parsed documents to `instantiate_with`.

## Q1 is resolved — ADR 0011

**Game logic is Rust systems in the game crate.** No scripting layer, no dynamic reload,
no `amadeo-script`.

Four candidates were prototyped and measured against one shared benchmark (a three-state enemy AI
over 64 entities, 1800 ticks). Everything is in `spikes/q1-game-logic/`, re-runnable via
`measure.ps1`.

| | edit → observe | state survives | hash vs native Rust | µs/tick |
|---|---|---|---|---|
| **A** pure Rust | 0.95 s (2.1 s in the real game) | no | reference | 4.6 |
| **B** cdylib | 0.69 s | yes | ✅ identical | 4.6 |
| **C** Luau | 0.4 ms | yes | ❌ **differs** | 109.7 |
| **D** WASM | 0.63 s | yes | ✅ identical | 5.7 |

**The recorded Luau prior was refuted, and it is worth knowing why.** Luau is not nondeterministic —
it reproduces perfectly across processes. But its numbers are `f64` and components are `f32`, so it
computes something *different* from the Rust reference: the two agree at tick 1 and diverge at tick 2.
That kills the prior's central mechanism specifically, because "graduate hot logic from Luau into
Rust" changes behaviour and invalidates every golden replay taken before the move.

**The premise behind the whole question was also wrong at this scale.** Q1 was written to avoid a
feared 30-second rebuild. Measured: **0.9 s** for a gameplay edit, **2.0 s** for `quad-demo` (which
links wgpu and winit), **3.2 s** for an engine-crate edit rebuilding everything downstream. There was
no crisis to solve, so the decision is to not pay a permanent architectural cost for it.

**WASM is reserved, not rejected.** It is bit-identical to native Rust (verified across two
optimisation levels) at 1.24× runtime cost, and it is the same artefact M5's web export needs. ADR
0011 names it as the escape hatch behind a measured threshold — a gameplay rebuild sustaining above
5 s. Check by re-running the spike, not by impression.

### Decided
- Name: **Amadeo**.
- Unified 2D **and** 3D from the start (not 2D-first, and not 3D-only). Restated in session 6: the
  three 3D target games order the work, they do not narrow the engine. `CLAUDE.md` §7 trap 9.
- Native desktop first, Windows as the primary target. Web export deferred to M5.
- Graphical editor **and** full text/code/headless parity are both first-class requirements.
- Stack: Rust + wgpu + winit + glam + rapier + egui. See `docs/adr/0002`.
- Scene tree is the authoring model; ECS is the runtime model. See `docs/adr/0004`.
- Text files are the only source of truth. See `docs/adr/0003`.
- Determinism is a hard invariant, designed in from tick zero. See `docs/adr/0005`.
- **Code must stay legible to a Rust-learning human.** Justin intends to read, debug, and fix the
  codebase himself. Boring Rust over clever Rust; accepted cost in verbosity. `CLAUDE.md` §6.
- **Target games: eight of them, extended from three in session 7.** Palworld, Schedule I, Inside the
  Backrooms, **Minecraft, Terraria, Project Zomboid, RimWorld, Stellaris**. Deliberately different
  genres, dimensions, scales, and art directions — used as a prioritisation signal. The intersection
  defines the core; the divergence defines what must stay pluggable. See `docs/00-vision.md`
  § Target games for what the five additions changed; the short version is that 2D became a
  requirement rather than a principle, destructible chunked worlds became a real subsystem, ECS
  throughput and dense UI both moved up sharply, and **modding put ADR 0011 under real pressure
  (Q15)**.
- **Renderer must not bake in an art style.** Configurable post-process stack, flexible dynamic
  lighting, fog/volumetrics. The three targets span stylised-realistic outdoors, low-poly, and dark
  atmospheric interiors.
- **Camera rig is separate from the character controller** — the targets are a mix of first- and
  third-person.
- **Multiplayer is no longer a non-goal.** All three targets are co-op. Client-server with server
  authority and client prediction (*not* deterministic lockstep). Hooks reserved during M0–M2, netcode
  built at M6. See `docs/adr/0006`.
- **First game to finish: single-player first-person atmospheric horror slice** at M3 — smallest
  genuinely finishable complete game, and the hardest test of the renderer.
- **Game logic is plain Rust in the game crate.** No scripting layer, no hot reload. WASM reserved as
  a pre-selected escape hatch behind a measured threshold. See `docs/adr/0011`.
- **`spikes/` exists** for prototypes that answer a question with a measurement. Separate cargo
  workspaces, frozen once their ADR is written. See `spikes/README.md`.

- **Q4 resolved — an asset is named by a declared `id` in its sidecar**, not its path and not a GUID.
  Defaults to the filename stem on import, so it reads like a path and survives a move. ADR 0020.
- **Q13 resolved — `ComponentId` is the hash of a component's canonical name**, not its Rust path.
  Moving a component between crates is free; renaming one is a deliberate, visible change. ADR 0017.
- **Q3 resolved, two thirds of it — one 3D `Transform`, and an explicit `SortOrder`.** 2D is the
  degenerate case rather than a separate type; rotation is Euler degrees so it stays hand-writable.
  The pipeline shape is deliberately still open. ADR 0018.
- **Q14 resolved — the game binary hosts the agent; the CLI launches it.** One-shot JSON-RPC over
  stdio, hand-written parser, `App` owns the `ComponentRegistry`. See `docs/adr/0016`.

### How Justin wants to work — stated in session 6, and load-bearing

These are not preferences to weigh; they are instructions. Full versions in `CLAUDE.md` §5 and §6.

- **Research before asking, not instead of asking.** He has no game-engine-development background
  and says he tends to take whichever option is recommended. So a menu of options I have not
  researched is not sharing a decision — it looks like collaboration and is not. When the codebase
  alone cannot settle a trade-off, go read how real engines solve it. He explicitly endorsed the
  time. ADR 0021 is the worked example: the research changed the answer.
- **Pros *and* cons for every option**, including the recommended one.
- **Plain language**, with the vocabulary defined at the point it affects a choice he has to make.
- **Prefer the more complete option over the faster one.** His words: he would rather have a
  complete engine than one that accumulates problems, and does not mind more steps or more time.
  Do not quietly narrow scope to save effort — that is not the trade he is asking for.
- **No `Co-Authored-By: Claude` trailer on commits.** Personal project; he knows. End the message at
  the last line of the body.

### Not yet decided (blocking)

Nothing is blocking. Q14, the last P0, closed in session 6.

## Environment

Verified on this machine (2026-07-30):

| | |
|---|---|
| OS | Windows 11 Pro 26200 |
| CPU | AMD Ryzen 7 5700X3D (8C/16T) |
| GPU | NVIDIA RTX 4060 Ti — Vulkan and DX12 capable, fine for wgpu |
| RAM | 40 GB |
| Installed | Node 24.16, npm 11.13, git 2.53, Java 25 |
| **Rust** | ✅ rustup + rustc 1.97.1 + cargo 1.97.1, target `stable-x86_64-pc-windows-msvc`, in `%USERPROFILE%\.cargo\bin` |
| **MSVC build tools** | ✅ VS Build Tools 2022 17.14.37, MSVC 14.44.35207. Verified 2026-07-30: `cargo build` compiles **and links**, and the binary runs. |
| Editor | ✅ VS Code + rust-analyzer v0.3.2989 |
| **Toolchain status** | ✅ **No blockers.** Compiles, links, runs, tests. |
| Also missing | Python, cmake. Neither is needed. |
| Gotcha — PATH | Installers update the persistent PATH but not running processes. VS Code's integrated terminal needs **VS Code itself** restarted, not just a new tab. |
| **Gotcha — `gh`** | The GitHub CLI is installed but **not on PATH** for tool invocations, the same as `cargo`. It lives at `C:\Program Files\GitHub CLI\gh.exe`; prefix with `$env:PATH = "C:\Program Files\GitHub CLI;$env:PATH"`. Worth knowing because checking CI yourself after a push is faster than waiting to be told it is red. |
| Smart App Control | **Resolved.** It was blocking every binary this project builds — confirmed via event log (3118, policy `{0283ac0f-…}`). Justin disabled it (one-way change on Win11). If a future machine hits `os error 4551`, this is why; see `docs/07-working-with-the-code.md` §5. |
| Gotcha — winget | `winget install` on an already-installed package attempts an *upgrade* and silently ignores `--override`, so it cannot add a workload. Use the VS Installer to modify an existing install. |
| Gotcha — wgpu | This project is on **wgpu 30**, which differs from most material online. Read the crate source under `~/.cargo/registry/src/*/wgpu-30.0.0/src/api/` rather than trusting search results. `docs/07` records the three changes that cost the most time. |
| **Gotcha — GPU tests in parallel** | Dropping a headless wgpu device **while another is alive** is `gfx-rs/wgpu#6571`: `STATUS_ACCESS_VIOLATION` on Windows, reported against exactly this (parallel tests, headless adapters). Cargo runs tests in parallel by default. `tests/capture.rs` takes a `static` mutex for each device's whole lifetime; CI passes `--test-threads=1` as well. **It only ever failed in CI, never locally** — a real GPU tolerates it and the runner's software adapter does not, so "it passes on my machine" proves nothing here. |
| **Gotcha — feature unification** | `cargo test --workspace` builds *every* member, so a feature any one of them enables is on for the whole build. `quad-demo` and `vault` enable `amadeo-render/gpu`, which means **the GPU tests run even without `--all-features`** — a CI comment claimed otherwise for months and was wrong. The same rule is why ADR 0036 says physics determinism cannot be a per-game choice. |
| **Gotcha — rapier 0.34 uses glam** | Not nalgebra. `Rotation` is a `glam::Quat`, and rapier's own `vector![]` macro still builds an **nalgebra** vector its API will not accept. Use `Vector::new`. Both are "a vector" and only the compiler notices. |
| **Gotcha — a `Field` is sized in samples** | `Field::new(n)` holds `(n+1)³` samples and meshes `n³` cells. A chunk that fills only its own volume **cracks at every seam**, and the symptom points at the renderer rather than at the data. |
| **Gotcha — a chunk needs TWO aprons, not one** | The line above is about *vertices* and is only half the story. `surface_nets` also emits a quad by looking at the four cells around a grid edge, and at a chunk's **low** face two of them belong to the previous chunk — so the bridging quads are emitted by nobody and every chunk has a one-cell gap around it. A chunk of `n` cells fills **`n + 2`** samples covering `n + 1` cells, starting one cell *below* its origin. `ChunkShape::samples_per_axis` is the number; ADR 0043 §4 amends ADR 0042 §2. Use `mesh_chunk`, which gets it right, rather than calling `surface_nets` on a hand-built field. |
| **Gotcha — an empty chunk is not an error** | Most chunks of a real world are entirely air or entirely rock and mesh into nothing. `ColliderBuilder::trimesh` returns a `Result` and refuses no triangles, so filter with `StaticMesh::is_empty` before inserting. Both backends reject it identically on purpose. |
| **Gotcha — CI has no GPU, so `offscreen` gets a *software* adapter** | `WgpuBackend::offscreen` asks for an adapter with no compatible surface, which is what lets a CPU rasteriser (WARP on Windows, lavapipe on Linux) answer where there is no hardware — that is how CI captures images at all. It is **dozens of times slower, not slightly**: frame preparation measures 130 µs on this machine and **8764 µs** on a runner. Any test asserting a time bound must check `WgpuBackend::adapter().software` first and report rather than fail. Turned CI red once, in session 12. |
| **Gotcha — PowerShell `$?` after a pipe is not the exit code** | `cargo clippy ... \| Select-String "error"` sets `$?` from **`Select-String`**, which reports failure when it finds nothing — so a clean run looks like a failed one. Use `cmd *>$null; $LASTEXITCODE` when what you want is whether cargo succeeded. Cost real confusion in session 12. |
| **Gotcha — an asset that will not parse is skipped in silence** | By design (ADR 0021): a missing asset must be survivable. So a `.material` with one field missing produces no error and a uniformly white scene. `amadeo check <file>` names the field — but only if you point it at the *asset* files, not just the level. CI does both games now. |
| **Gotcha — `Grade::contrast` above 1.0 crushes shadows to black** | `(colour - 0.5) * contrast + 0.5` drives near-black values negative and they clamp to zero. Inherent to a pivot with no toe. Invisible before shadows existed, because nothing in a scene was ever that dark. |
| **Gotcha — `amadeo check` and `fmt` need `--package`** | Both validate against a *game's* registered components (ADR 0016). Run without `--package` and they check against whatever `amadeo.toml` names, which will report every component of the game you meant as unregistered. |
| **Gotcha — line endings** | `core.autocrlf` is **true** by default on Windows and on GitHub's windows-latest runners. It rewrites committed LF into CRLF on checkout, breaking byte comparisons of `.replay` and `.scene` fixtures — invariant I2. `.gitattributes` pins `eol=lf`; **do not remove it**. This machine has `core.autocrlf=false` set locally, which is why it reproduced nowhere here. Tell: only the *Windows* CI jobs fail, because Linux checkout does no conversion. |

## CI

Green as of session 6. Five jobs: `check` (fmt + clippy), `test` on windows-latest and
ubuntu-latest, `determinism` (the suite three times serially, then release, then a separate-process
replay), and `docs`.

**The first push, in session 6, went red 3/5 and stayed red for four commits.** Not a determinism
failure despite looking exactly like one — see the line-endings gotcha above. Worth knowing that the
run before the fix failed *with identical state hashes on both sides of the assertion*; the
simulation was never wrong.

Older commits still show red on GitHub. That is correct and needs no action: CI ran against trees
that had no `.gitattributes`, so re-running them would fail identically. The code in them is fine —
in every red run, `golden_file_replays_to_its_recorded_hashes` (the test that actually asserts state
hashes) passed.

## Next actions

**M0 is under way and unblocked.** Done so far, in the order it was built:
- ✅ Cargo workspace, workspace lints (`unsafe_code = "forbid"`), toolchain pinned
- ✅ Q5 resolved: 60 Hz fixed timestep (ADR 0007)
- ✅ ECS storage strategy decided: safe archetype columns, no unsafe (ADR 0008)
- ✅ `amadeo-core`: `Tick`, `FIXED_DT`, hand-written PCG32 `Rng` with stream forking, hand-written
  FNV-1a `StableHasher` (cross-checked against an independent implementation), `StableId` / `NetId` /
  `Authority` (the ADR 0006 hooks).
- ✅ `amadeo-ecs`: generational `Entity` handles, `ComponentId` derived from type *name* (not
  `TypeId`, which is not build-stable), type-erased-but-safe archetype columns, archetype migration
  on component add/remove, `iter` / `for_each_mut` / `for_each_pair_mut` queries, per-row change
  ticks, and `World::state_hash`.
- ✅ CI: fmt, clippy `-D warnings`, tests on Windows + Linux, a **determinism job** that runs the
  suite three times in separate processes plus a release build, and a rustdoc job.

- ✅ `Resource` (simulation state, hashed) and `Service` (engine machinery, **not** hashed) as two
  separate stores on `World`, with the distinction enforced by trait bounds — ADR 0009. Found by a
  failing determinism test rather than by design foresight.
- ✅ `amadeo-events`: typed double-buffered queues, a shared `EventClock` giving a total order across
  event types, and a `WorldEvents` extension trait. Events written on tick N are readable on N+1.
- ✅ `amadeo-app`: `Stage`, `Schedule` with `before`/`after` constraints resolved by topological sort
  with **alphabetical tie-breaking** (so registration order cannot influence results), the
  fixed-timestep loop with both `run_ticks` (deterministic, ignores wall time) and
  `advance_real_time` (accumulator, capped at 8 ticks/frame to prevent a catch-up spiral), and
  `SimRng`.
- ✅ Determinism integration suite (`crates/amadeo-app/tests/determinism.rs`) — 14 tests covering
  repeat-run agreement, per-checkpoint agreement, seed divergence, headless-vs-windowed equivalence,
  real-time-vs-exact-tick equivalence, stall recovery, and event ordering.

- ✅ `amadeo-input`: `ActionId` (gameplay reads named actions, never keys), `InputState` with
  `just_pressed`/`just_released` edge detection, `InputSource` implementations (null, scripted,
  replay), and a `Recorder` that writes change-only recordings.
- ✅ **The replay file format** — the project's first authored text format, built to the rules every
  later format must follow (I1/I2): hand-writable, line-oriented, canonically ordered, byte-stable
  round-trip, LF endings, and parse errors carrying line numbers. Rejects a tick-rate mismatch rather
  than replaying it wrong (ADR 0007).
- ✅ **Golden replay harness** with a committed fixture at
  `crates/amadeo-app/tests/golden/walk_and_jump.replay`. A recording made once is replayed by every
  later build and asserted against checkpoint state hashes. Regenerate deliberately with
  `UPDATE_GOLDEN=1 cargo test -p amadeo-app --test golden_replay`.

- ✅ **Deferred commands** — `Commands` service with `despawn`, `insert`, `remove`, `spawn_with`, and
  a `queue` escape hatch. Systems can now change structure from inside a query. The app flushes after
  every stage, so a change requested in `PreSimulation` is visible in `Simulation`. Commands queued
  *during* a flush wait for the next one, deliberately — an unbounded loop inside one flush would
  hang, which is far worse to diagnose than a one-stage delay.

- ✅ `amadeo-render` **abstraction and null backend** — `Transform`, `Quad`, `Camera2d`, the
  `RenderBackend` trait, `NullBackend` (records what *would* have been drawn, so draw calls are
  assertable with no GPU), and the `render_quads` collection pass. Draw order is by explicit
  `Quad::layer` with a stable sort, never by iteration order.
- ✅ `World::iter_pair` — a read-only two-component query, added because the renderer needed one:
  the mutable version would mark every drawn entity as changed each frame and make change detection
  worthless.

- ✅ **The Q1 spike** (session 4) — four candidates for game-logic authoring and hot reload,
  prototyped against one shared benchmark and measured. Resolved by ADR 0011: **plain Rust**.
  Prototypes and numbers in `spikes/q1-game-logic/`; re-run with `measure.ps1`. Established the
  `spikes/` convention (separate workspaces, frozen after their ADR).

**M0 is complete.** Nothing remains.

### M1 so far (session 5)

- ✅ **Three-component ECS queries** — `iter_triple` and `for_each_triple_mut` (writes two, reads
  one). Added because the Q1 benchmark needed exactly that shape and had to work around it.
- ✅ **`amadeo-reflect`** — the `Value` tree (struct fields sorted by construction, so I2 does not
  depend on anyone remembering), `TypeInfo` schema, `TypeRegistry` (BTreeMap, so anything generated
  from it is diffable), and the metadata vocabulary including ADR 0006's replication annotations.
  ADR 0012.
- ✅ **`amadeo-derive`** — `#[derive(Reflect)]` and `#[derive(StableHash)]`. The second matters more
  than it looks: a hand-written `stable_hash` that forgets a field still compiles and still produces
  a plausible number, while silently excluding part of the simulation from every replay assertion.
- ✅ Two gaps closed in `amadeo-core` found while building the above: `stable_hash_of` was `pub` but
  never re-exported, and `[T; N]` had no `StableHash` impl.
- ✅ **`Component: Reflect`** (ADR 0013) — invariant I8 is now enforced by the compiler rather than by
  remembering. An unreflectable type cannot be a component. Every existing component converted to
  `#[derive(StableHash, Reflect)]`, hand-written hash impls deleted, and `Transform`/`Quad`/
  `Camera2d` annotated with units, ranges, and ADR 0006 replication policies.
- ✅ **Q2 resolved and `amadeo-scene` layer 1 built** (ADR 0014). Justin chose a custom,
  indentation-based format from four hand-written candidates in `spikes/q2-scene-format/`. Parser
  with line-numbered actionable errors, canonical byte-stable writer, and the round-trip test that
  satisfies **M1 exit gate 3**. The ADR's worked example is asserted byte-identical to the
  formatter's output, so the spec cannot drift from the implementation.

- ✅ **Scene layer 2** — `ComponentRegistry` in `amadeo-ecs` builds a component from a *name* and a
  `Value`, using monomorphised function pointers rather than a trait object (ADR 0012 chose a
  non-object-safe `Reflect` deliberately, and this is the way back). It owns the `TypeRegistry`, so
  one `register::<T>()` call satisfies I8 with no way to register the constructor and forget the
  schema. Then `amadeo_scene::instantiate` turns a document into entities **atomically** — any
  failure despawns everything it created, because a half-loaded scene looks like it worked.
- ✅ Numeric leniency in `Reflect`: a scene's `intensity 3` arrives as an integer because the parser
  has no schema, and must still fill an `f32` field. Floats accept any numeric value; integers stay
  strict, since an out-of-range integer is a mistake rather than an approximation.

- ✅ **`amadeo-transform`** (ADR 0015) — a new crate holding `Transform` (moved out of
  `amadeo-render`) and `Parent`. Resolves a straight contradiction between `CLAUDE.md` §4 and
  `docs/04` §3 about where hierarchy lives; the `CLAUDE.md` note was a dependency-direction error,
  since render, physics, and animation all sit *below* `amadeo-scene` and all need transforms.
  Scenes now materialise their nesting as real `Parent` components instead of just recording it.

- ✅ **`amadeo-agent`, read half** — `describe` renders the registry as JSON (Pillar 2: "what can I
  do?"), `entity` and `query` render the live world (Pillar 3: "what did I just do?"), on a
  hand-written JSON writer whose objects are sorted so a dump is diffable. `ComponentRegistry` gained
  a type-erased *reader* to match its inserter, and `World::entities()` lists live entities in a
  stable order so introspection does not show churn that did not happen. All read-only, so looking at
  a world cannot perturb it.

### M1 continued (session 6)

- ✅ **Q14 resolved — ADR 0016**, then built the same session. See the session log below for what
  reading the code changed about the question.
- ✅ **A JSON reader** in `amadeo-agent`, beside the writer that was already there, with a round-trip
  test pinning the two together. Strict — no trailing commas, comments, or `NaN` — plus two
  strictnesses past the spec, each because the alternative hides a bug: **duplicate object keys are
  an error** rather than a silent last-one-wins overwrite into a `BTreeMap`, and **nesting is capped**
  so a few thousand `[` arriving from a pipe is a message rather than a stack overflow.
- ✅ **`App` owns a `ComponentRegistry`**, with `App::register_component::<T>()`. This was the gap
  ADR 0016 found by reading code rather than docs: the registry was built ad hoc in tests and nowhere
  else, and `quad-demo` registered nothing, so `describe` against a real game would have reported an
  empty schema for the game's own types the first time anyone tried it.
- ✅ **The protocol** (`amadeo-agent`) and **the host** (`amadeo-app`), split where I6 forces it —
  `amadeo-agent` sits above `amadeo-app`, so it cannot reach down for `App`. It owns the JSON-RPC
  envelope and the methods needing only a world; `amadeo-app` owns the stdin loop and the methods
  needing the schedule or the tick count. A client never sees the seam. Spec in `docs/protocol/v1.md`.
- ✅ **`quad-demo` hands over in one line**, sharing `build_simulation()` with the windowed path so an
  answer about the inspected world is an answer about the game that actually runs (I7).
- ✅ **`amadeo-cli`** — `describe`, `query`, `entity`, `schedule`, `status`, `call`, `check`,
  `replay`, and `fmt`. The ADR 0016 split is visible in `--help`: `fmt` runs in the CLI and never
  builds anything; everything else launches the game through `cargo run`, so a stale binary is
  rebuilt rather than answering for code that no longer exists.
- ✅ **`amadeo replay`** — the separate-process half of the golden-replay mechanism, and the last
  thing carried over from M0's exit gate. `--replay` and `--seed` are *launch* arguments rather than
  methods, because a recording must be installed before the first tick and `App::with_seed` fixes the
  seed at construction — before the handover is even reached. So a game reads
  `amadeo_app::requested_seed()` before building; one that does not gets a clear seed-mismatch error
  instead of a divergence that looks like a regression. Reports every failing checkpoint, not the
  first. Fixture at `games/quad-demo/replays/wander.replay`, hand-written and then filled in from the
  mismatch report — which is the intended way to author one.
- ✅ **`GlobalTransform` and `propagate_transforms`** (ADR 0019) — waiting since ADR 0015, unblocked
  by ADR 0018 settling what a transform is. Walks up the parent chain per entity rather than keeping
  a depth-sorted work list, because that list is a cache with an invalidation story and hierarchies
  are shallow. A `Parent` cycle falls back to the local transform rather than hanging.

  **`GlobalTransform` is `DERIVED`, so it is excluded from the state hash** — Justin decided this
  directly, and it is the reason matrix arithmetic cannot move a replay. Proven rather than asserted:
  `quad-demo` now carries a `GlobalTransform` on every entity and **both replay fixtures are
  byte-unchanged**. Two tests guard each other — one that propagation does not move the hash, one
  that a real change still does, so neither can pass because hashing quietly broke.

  Also a scalar `Mat4` in `amadeo-transform` rather than creating `amadeo-math` or taking glam:
  propagation needs compose-and-multiply and nothing else, and designing a maths surface backwards
  from its first caller is how a wrong abstraction gets locked in.
- ✅ **`amadeo check`** — validates scene files against the game's *real* schema, which is precisely
  what a standalone tool cannot do. Reports **every** problem in one pass rather than the first:
  `instantiate` stops at the first error because that is right for loading and wrong for checking, so
  `amadeo_scene::validate` collects instead, on a new `ComponentRegistry::validate` that answers
  "would this build?" with no `World` to build into. Diagnostics come back naming an entity id; the
  CLI turns that into `file:line` because it is the side that still has the text. One launch covers
  every file named, since a build per scene would make checking a directory unusable.
- ✅ **Q13 resolved — ADR 0017.** `ComponentId` now hashes a component's canonical name rather than
  its Rust path, so moving a type between crates stopped being a silent replay-invalidating change.
  Cost: two components sharing a canonical name now collide. The registry already refuses that;
  `World::insert` gained a **debug-build guard** for anything unregistered.
- ✅ **Q3 resolved, two thirds — ADR 0018.** One 3D `Transform` (2D is its degenerate case,
  `Transform2d` retired), rotation as **Euler degrees** so it stays hand-writable, and `SortOrder`
  replacing `Quad::layer`. The pipeline shape is deliberately still open and dropped to P2.
- ✅ **`GlobalTransform` and `propagate_transforms`** (ADR 0019) — waiting since ADR 0015, unblocked
  by ADR 0018 settling what a transform is. Walks up the parent chain per entity rather than keeping
  a depth-sorted work list, because that list is a cache with an invalidation story and hierarchies
  are shallow. A `Parent` cycle falls back to the local transform rather than hanging.

  **`GlobalTransform` is `DERIVED`, so it is excluded from the state hash** — Justin decided this
  directly, and it is the reason matrix arithmetic cannot move a replay. Proven rather than asserted:
  `quad-demo` carries a `GlobalTransform` on every entity and **both replay fixtures are
  byte-unchanged**. Two tests guard each other — one that propagation does not move the hash, one
  that a real change still does — so neither can pass because hashing quietly broke.

  Also a scalar `Mat4` in `amadeo-transform` rather than creating `amadeo-math` or taking glam:
  propagation needs compose-and-multiply and nothing else, and designing a maths surface backwards
  from its first caller is how a wrong abstraction gets locked in.
- ✅ **The renderer reads `GlobalTransform`**, so hierarchy reaches the screen. Scale and rotation
  come back out of the **composed matrix**, not the local transform — a matrix's columns are its
  scaled axes, so a column's length is that axis's total scale and its angle the total rotation.
  Without that a parent's turn would move a child but not rotate it.
- ✅ **`.gitattributes`** — the fix for the CI failure, see the CI section above.
- ✅ **Q4 resolved — ADR 0020**, and **ADR 0021** on top of it. Asset identity is a declared `id` in
  a sidecar; the simulation never observes asset *state*.
- 🟡 **`amadeo-assets`, first slice** — the `.ama-meta` sidecar format and the `AssetCatalogue`
  mapping id to file, with duplicate ids refused naming both files. Loading, handles, the import
  pipeline and hot-reload are still to come, to ADR 0021's rule.

### M1 continued (session 7)

- ✅ **`amadeo-assets`, the loading half** — all five steps listed above. The scan reports what it
  could *not* catalogue (unimported files, orphaned sidecars), because ADR 0020 predicted that exact
  confusion by name: asking for `wall` is refused while `wall.png` sits right there in the tree.
  Stored paths are normalised to forward slashes, since they go over the protocol and
  `textures\wall.png` against `textures/wall.png` would need a special case in every cross-platform
  assertion. Dotfiles are not assets, which is the *only* rule about what counts as one — an
  extension allowlist would be genre knowledge and I4 forbids it.
- ✅ **ADR 0022** — the asset root is found by walking up for `amadeo.toml`. See the correction above.
- ✅ **The load barrier**, and the `assets` block a scene declares its requirements in. A missing
  asset is recorded and survivable rather than fatal, per ADR 0021. **Proven, not asserted:**
  quad-demo now loads a real 700-byte file at startup and `wander.replay` still matches all four
  checkpoints, because `Assets` is a `Service` and ADR 0009 excludes those by trait bound.
- ✅ **`amadeo check` validates asset ids**, with near-miss suggestions.
- ✅ **A PCG32 reference cross-check** — see the audit below.
- ✅ **The sprite batcher — ADR 0023, resolving Q3's last third.** A `Sprite` component holding a
  texture *id* (ADR 0020) plus a `region`, so a tilesheet is one texture and one batch. Batches are
  `(sort order, texture)` pairs: layering is never violated, and within one order the relative order
  of *different* textures is explicitly not guaranteed — that is the trade, and `SortOrder` is the
  mechanism for controlling it.

  Decided against numbers, as the question demanded. 20,000 fully interleaved sprites collapse to
  exactly **32** batches — the theoretical minimum that preserves layering — and 50,000 tiles on one
  sheet are **one** draw call. Batch counts are asserted (a pure function of the world, no clock);
  times are printed, with only an algorithmic-collapse ceiling asserted.

  Two things the measurement changed. The first version sorted by `(order, &str)` and was 55% slower;
  keying on an index into a sorted texture table made the sort integer-only. And `SpriteInstance`
  carries the transform's **axes** rather than a size and an angle, which removes a round trip
  through trigonometry on both the CPU and the shader — and is strictly more expressive, since a
  size-and-angle pair cannot represent a sheared or non-uniformly-scaled-then-rotated sprite.

- ✅ **Component ids are compile-time constants now — ADR 0024, resolving Q16.** `Reflect` gained
  `STATIC_NAME` (filled in by the derive) and `STATIC_NAME_HASH` (a `const fn` FNV-1a over it), so
  `ComponentId::of::<T>()` is a constant load rather than a `String` allocation plus a hash on
  **every** component access.

  This is an engine-wide win, not a rendering one — `World::get`, `World::insert`, and every query
  pay it. Sprite collection went **5.13 ms → 3.32 ms** at 20,000 sprites (31% → 20% of a frame), and
  the 50,000-tile case **11.55 ms → 6.77 ms**. Ids are byte-identical: both golden replays and the
  separate-process `amadeo replay` pass unchanged, which is the assertion that matters, since a
  different hash would have invalidated every committed replay at once.

- ✅ **Queries are tuples of terms, and a term may be optional — ADR 0025, resolving Q17.**
  `world.query::<(&Transform, &Sprite, Option<&SortOrder>, Option<&GlobalTransform>)>()`. Each column
  is resolved **once per archetype** instead of once per entity, which is the structural reason
  archetype ECSs are fast and the thing Amadeo's hand-written query methods could not express.

  **Justin chose this**, over hand-writing every shape or a lower-level per-archetype accessor, after
  the trade was put to him with the legibility cost stated. It is the one deliberate piece of clever
  Rust in the ECS — a trait with an associated type plus a macro writing the tuple impls — and the
  module docs explain each part of the machinery next to the code rather than only in the ADR.

  Read-only on purpose: a generic *mutable* query cannot prove two type parameters are different
  columns, so Bevy uses `unsafe` for it, this crate forbids `unsafe`, and the measured problem was
  entirely on the read side. `for_each_pair_mut` and friends are untouched, and a test asserts the
  old and new paths see the same world.

  Sprite collection: **3.32 ms → 2.58 ms** at 20,000 sprites, and **5.13 → 2.58 ms** across ADRs 0024
  and 0025 together — 15.5% of a 60 Hz frame, from 31%.

### M1 continued (session 8) — sprites reach the screen

- ✅ **`amadeo-image`, a new crate at the bottom of the graph** — ADR 0026. Decodes PNG and PPM into
  `TextureData { width, height, format, pixels }`. Depends on **no engine crate at all**, so it sits
  beside `amadeo-derive` below even `amadeo-core`.

  **The format tag is the load-bearing part.** `PixelFormat` has exactly one variant today, and it is
  there so that adding GPU-compressed textures later is a new variant plus a new producer rather than
  a change to the loader, the cache, the backend, and every test that asserts on pixels. That is the
  one genuinely expensive-to-retrofit piece of this design, and it costs nothing now.

  Format is chosen by **sniffing the leading bytes, not the extension** — which matters more here
  than in most engines, because ADR 0020 makes the path bookkeeping an author may freely change.

- ✅ **`TextureCache` in `amadeo-render`** — id → bytes → pixels, held. `get` **never fails**: a
  three-step fallback ends in a magenta check built in code, because a placeholder that is itself a
  file cannot cover the case where files are the problem. Every fallback is reported, since a frame
  that silently draws magenta is a frame an agent cannot diagnose.

- ✅ **The wgpu backend draws sprites.** Texture upload, one nearest-neighbour sampler, one bind group
  per texture built once at upload rather than per frame, and a second pipeline sharing the camera
  bind group with the quad one. Every batch's instances go into **one** buffer and each batch draws
  its own slice, so there is one buffer write per frame regardless of batch count — the batches only
  decide how often the texture binding changes, which is the cost ADR 0023 is actually about.

- ✅ **quad-demo shows it.** A nine-tile floor strip, each tile reading a different `region` of one 2×2
  texture, plus one sprite deliberately naming an id that does not exist so the placeholder path is
  visible in the running game rather than only in a test.

**`wander.replay` was regenerated, and the diagnosis was verified rather than assumed.** All four
checkpoints moved. With *only the ten new sprite entities* removed — but `TextureCache` installed,
`Sprite` registered, and a second asset loaded — every checkpoint matched its committed value
exactly. So none of the new machinery touches the state hash; the divergence is authored content
changing, which is what a replay should catch. The diff is four checkpoint lines and a byte-identical
input stream.

**One real error found by writing the shader.** `SpriteInstance::axes` documented itself as carrying
*half*-extent axes and gave a corner formula multiplying by two, while `instance_for` has always
produced full-extent axes and `SpriteInstance::size()` has always read them back as full extents.
The code was consistent; the contract was wrong, and the shader would have been written to it. Fixed,
and the doc now names `QuadInstance` as the convention it shares.

### Invariant I8 closed — ADR 0027, session 8

The other half of I8, deferred by ADR 0013 because it was **not yet possible**: two of the four
resources could not reflect at all. `SimRng` wraps an `Rng` whose state is private to `amadeo-core`,
which sits *below* `amadeo-reflect` and so cannot implement the trait (I6); and `InputState` is two
maps, which the value tree could not represent.

- ✅ **`Resource: Reflect` and `Event: Reflect`**, both compiler-enforced. Events were not in the
  original scope and were added once the work started — `Events<T>` is a `Resource`, so it hit the
  bound transitively, and the argument turns out to be *stronger* for events: the event log is how an
  agent answers "what did I just do?".
- ✅ **`Value::Map`, with string keys.** Kept distinct from `Value::Struct` even though they hold the
  same shape, because a struct's fields are fixed and a map's keys are data — which is what lets
  `from_value` be strict about one and permissive about the other. **Justin chose string keys** over
  Bevy's and Godot's arbitrary-key maps, after the trade was researched: `Value` holds floats and so
  has no total order to sort arbitrary keys by, and a struct-as-a-key has no hand-writable syntax.
- ✅ **`Rng::state()` / `from_state()`**, serving three things that all need to *observe* a generator
  rather than draw from it: reflection, hashing, and snapshots. And **`Reflect for Tick` written
  inside `amadeo-reflect`** — the simpler answer when the state is already public, since the impl can
  go where the trait lives rather than where the type does.
- ✅ **`world.resources`**, the concrete payoff. `amadeo call world.resources` reports a real game's
  `Camera2d`, `InputState` and `SimRng` with live values. Blocked in `docs/protocol/v1.md` on exactly
  this bound; a resource behind a trait object had thrown away everything about its type but a hash.

**Both replays were regenerated, and the diagnosis was verified rather than assumed.** `SimRng` used
to hash `format!("{:?}", rng)` — which made every committed replay depend on the exact text of a
`Debug` impl, so renaming a private field would have invalidated all of them for a reason nobody
would connect to the failure. Justin chose to pay the regeneration now rather than leave it armed.
Reverting *only* that hash — with `Resource: Reflect` in force and five types newly reflected —
restored both replays exactly, proving the reflection work is invisible to the state hash.

**One gap created rather than found, and recorded as Q18.** `InputState` reflects faithfully and
unreadably: its keys are `ActionId`s, which are hashes whose names are not kept, so the protocol
reports `"8831028638596390904"` instead of `"move_x"`. Only visible once `world.resources` existed
and could be pointed at a running game. Nothing is blocked, and the fix belongs at the presentation
layer rather than in the type.

**Verified green: 698 tests passing; clippy, fmt, and rustdoc all clean under `-D warnings`.**

### The sprite work, session 8

**Verified green: 669 tests at that point; all four commands clean.**

**And verified on screen.** Justin ran the demo and the screenshot matches the world coordinates
exactly — tile positions, marker positions, sprite widths at the window's aspect ratio, the
alternating tile colours proving `region` picks a different texel per tile, and texture colours
coming back as the literal values in the file. The one thing it does *not* exercise is the vertical
flip; see "The single most important thing to do next".

### Session 7's work

**Verified green at the end of session 7: 610 tests passing; clippy, fmt, and rustdoc all clean.**

### The audit Justin asked for, session 7

He asked for the earlier work to be re-checked, since everything before the last two additions was
built on whichever option was recommended. What was checked, and what it found:

**The invariants hold, and two of them hold better than the docs claim.**

- **I3 (determinism).** There is **no `HashMap` or `HashSet` anywhere in the engine** — the only
  occurrences are comments explaining why a `BTreeMap` is used instead. No `Instant::now` or
  `SystemTime` in any engine crate. Transcendental functions (`sin_cos`, `atan2`, `hypot`) appear in
  exactly **two** places, and both are outside the hashed path: `amadeo-transform`'s matrix build,
  which feeds `GlobalTransform` (`DERIVED`, excluded by ADR 0019), and `amadeo-render`'s matrix
  decomposition, which is render-side. That matters more than it looks — IEEE 754 does not specify
  transcendental functions, so `sin` can differ in the last bit between platforms. **ADR 0019's
  decision is load-bearing for cross-platform determinism in a way the ADR does not state.**
- **The safety net is real.** The `test` CI job runs on **both** Windows and Linux and includes the
  golden-replay test, which asserts *committed* hashes. So a hashed path growing a `sin` call would
  fail CI on one platform. That is a genuine cross-platform determinism check and the docs undersell it.
- **I6 (dependency DAG).** Verified crate by crate. Every edge points the right way; no cycles.
- **`World::state_hash` is sound.** Entities sorted by index and generation, components in sorted id
  order, resources in `BTreeMap` order, tick included, services excluded. `DERIVED` components skip
  **their id as well as their value**, which is the subtle half — writing the id would mean adding a
  `GlobalTransform` still moved the hash. The sorted-`component_ids` invariant it relies on is
  enforced by `debug_assert`.
- **The golden replay is not vacuous.** Four distinct checkpoint hashes, with a paired `assert_ne`
  guarding against the hash being constant.

**One real gap, now closed. `Rng` had no known-answer test.** Every existing test was a
*self-consistency* property — same seed gives same sequence, different seeds diverge, outputs in
range. All of them would still pass if the algorithm were subtly wrong (shift by 17 instead of 18),
because a wrong generator is still a perfectly deterministic one: I3 would hold and the statistical
quality PCG was chosen for would be silently gone. `StableHasher` *was* cross-checked against an
independent FNV-1a when written; the generator was going on the claim in its own doc comment.

Closed by `crates/amadeo-core/tests/pcg_reference_vector.rs`. **The result: `Rng` reproduces the
official PCG32 demo output exactly** — seeded `(42, 54)` it emits `a15c02b7, 7b47f409, ba1d3330,
83d2f293, bfa4784b, cbed606e`. So the implementation is genuinely PCG32 XSH-RR 64/32, confirmed
against a published vector rather than against a transcription that could share a mistake with it.
FNV-1a's constants and its xor-then-multiply ordering were checked too, and are correct.

**Smaller things found and fixed in passing:**

- `amadeo-agent`'s lib docs still said there was no JSON-RPC server and no JSON parser. Session 6
  built both.
- `quad-demo`'s `build_simulation` doc comment had been detached from it by a `const` inserted
  between them, so the function was undocumented and `DEFAULT_SEED` was documented as a colour palette.
- `docs/protocol/v1.md` listed `assets.list` as not implemented. Now specified.

**Smaller things found and left alone, deliberately:**

- Three `expect()` calls in `amadeo-app/src/schedule.rs` technically breach the "no `unwrap`/`expect`
  in engine crates" convention. All three are provably unreachable local invariants established a few
  lines above, each with an explanatory message; rewriting them would add unreachable error paths.
  Every other occurrence in the engine is inside a doc-comment example, which is fine.
- `amadeo-app` lists `amadeo-input` in both `[dependencies]` and `[dev-dependencies]`. Harmless.

Two things found by running it rather than by thinking about it:

- **PowerShell's pipe prepends a UTF-8 BOM**, and rejecting it produced an error pointing at an
  invisible character — the least actionable message that parser could produce. A leading U+FEFF is
  now skipped, and only a leading one.
- **`state_hash` goes over the wire as a hex string**, not a number. It is a `u64`, JSON numbers are
  `f64`, and above 2^53 a client silently reads a different value — which would break replay
  assertions in the least visible way available.

### Session 5 detail

**The golden replay did not need regenerating**, which was not guaranteed. The derive sorts fields by
name, so any component whose fields were not already alphabetical changes fingerprint. The committed
fixture happens to use only `Position { x, y }` and `Velocity { x, y }` — alphabetical, scalar, no
arrays — so its hashes are byte-identical. `Transform`, `Quad`, and `Camera2d` *did* change, and
nothing asserts on them. Reasoning in ADR 0013 so nobody re-derives it from scratch.

Carried into M1 rather than counted as done — **now closed, in session 6:**
- A **separate-process** replay check. The golden test replays in-process against a committed
  fixture, which covers "separate build" but not "separate process". `amadeo replay` closes it:
  `games/quad-demo/replays/wander.replay` is played by the real game binary in a fresh process, with
  four checkpoints asserted, and CI runs it in the determinism job. **M0's exit gate is now 4 of 4
  with nothing carried.**

Known gaps deliberately left for later:
- No bundle/spawn-with-components API, so building an entity with N components costs N archetype
  migrations. Correct but wasteful; optimise when it shows up in a profile.
- Query shapes reach three components (`iter_triple`, `for_each_triple_mut` — writes two, reads one),
  added in session 5 because the Q1 benchmark needed exactly that and had to work around it. Four or
  more, or a different mutability split, still needs collect-and-write-back. Extend on demand.
- **`Service` requires `Send + Sync`**, which excludes any non-`Sync` runtime from living in the
  world — found when neither script VM in the Q1 spike could be stored there. Harmless today, will
  bite the audio mixer and asset loader in M3. Filed as **Q12**.
- Events cannot be sent from inside a query closure (the world is already borrowed). Workaround is to
  collect then send, as `bounce` does in the determinism tests. Deferred commands solve the same
  problem for structural changes; an equivalent for events has not been built.
- No parallel system execution. ADR 0005 permits it only where access is provably disjoint, and the
  scheduler does not yet track access patterns.
- `SimRng`'s `StableHash` goes through its `Debug` output, which works but is inelegant. Revisit when
  the reflection registry lands in M1 and can expose the state fields directly.

## Open risks

| Risk | Mitigation |
|---|---|
| Scope is genuinely very large (unified 2D/3D + editor + AI layer ≈ rebuilding Godot). | Vertical slices with hard exit gates. Reuse proven crates for solved problems instead of writing them. Ruthless non-goals list in `docs/00-vision.md`. |
| Rust compile times degrade the agent iteration loop. | **Measured, session 4:** 0.9 s for a gameplay edit, 3.2 s for a full downstream rebuild — not currently a problem (ADR 0011). Now depends on keeping the crate graph small and shallow, which has become load-bearing rather than hygiene. Re-run `spikes/q1-game-logic/measure.ps1` when the engine has grown; WASM is the pre-selected answer if the threshold is crossed. |
| **Re-simulation cost, not compile time, degrades the loop.** Getting back to the moment of interest grows linearly with session length (~21 µs/tick; 382 ms to reach 5 simulated minutes). | Snapshot/restore, promoted to an M1 priority by ADR 0011. |
| Determinism erodes silently as features land. | Golden-replay tests in CI from M0. Every subsystem PR adds one. |
| Editor drifts into being the source of truth. | I1/I5 enforced by making the editor an RPC client with no privileged path. Round-trip byte-stability test in CI. |

## Reading order for a fresh session

If you are starting cold, this is the shortest path to being useful:

1. `CLAUDE.md` — invariants (§2), what exists (§4), how to verify (§4b), **how to put a choice to
   Justin (§5)**, and the traps (§7).
2. This file: **How Justin wants to work**, **The single most important thing to do next**, and
   **CI**. Those three are the whole handoff; everything else here is background.
3. `docs/07-working-with-the-code.md` — the Rust patterns this engine uses and why, the everyday
   `amadeo` commands, and the golden-replay mechanism. Skip if you already know the codebase.
4. `docs/adr/` — 28 of them now, so read by need rather than in order:
   - **0023** and **0026** before touching the renderer, **0024** and **0025** before touching
     `amadeo-ecs`. 0026 in particular if you are about to add an asset kind or wonder why the engine
     has a dependency that is not `thiserror`.
     0025 in particular: `world.query` is the API every read path should use, and its module docs
     explain the one piece of deliberately non-boring Rust in the engine.
   - **0013** and **0027** before adding a component, resource, or event — all three require
     `Reflect` by trait bound, and 0027 covers the one awkward case (a type whose state is private to
     a crate below `amadeo-reflect`) plus how maps work.
   - **0005** (determinism), **0008** (ECS storage), **0009** (resource vs service) and **0019**
     (derived components) before touching `amadeo-ecs` or anything that reaches `state_hash`.
   - **0003** and **0004** before touching scenes or the editor; **0014** for the scene format.
   - **0011** before proposing a scripting language or hot reload — decided by *measurement*, so
     reopening it needs numbers, not arguments.
   - **0016** plus `docs/protocol/v1.md` before touching the CLI, the agent, or process boundaries.
   - **0028** before touching snapshots — and before assuming a state-hash comparison proves a
     restore is correct, because it does not.
   - **0028** before touching snapshots — and before assuming a state-hash comparison proves a
     restore is correct, because it does not.
   - **0017** before moving or renaming a component (moving is free now; renaming is not).
   - **0018** before touching transforms or draw order; **0020** and **0021** before assets.
5. `docs/06-open-questions.md` — before assuming anything undecided. Ten remain, none blocking.
   **Q15** (modding vs ADR 0011) and the **`from` conflict inside Q7** are the two that were raised
   in session 7 and deliberately left for Justin. **Q18** is new in session 8 and is the smallest of
   the three: a reflected `ActionId` is a hash nobody can read.

Then `git log --oneline -25`. Commit messages explain *why*, deliberately, and session 6's are long
on purpose — several record a diagnosis that took a while to reach.

**Things that will bite a cold session specifically:**

- **`cargo` is not on PATH for tool invocations.** Prefix with
  `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"`.
- **`gh` is not on PATH either.** It is at `C:\Program Files\GitHub CLI\gh.exe`.
- **Windows PowerShell 5.1 reads UTF-8 as ANSI and writes back a BOM.** If you script a file edit,
  use .NET APIs with `UTF8Encoding($false)`, or every em-dash in the repo is silently corrupted.
  Console *display* of em-dashes as mojibake is harmless; a `git diff --stat` showing the whole file
  changed is not.
- **PowerShell here-strings break `git commit -m`** when the message contains quotes — the message
  gets split into pathspecs and the commit fails confusingly. Write the message to a file with the
  Write tool (which emits UTF-8 with no BOM) and use `git commit -F <file>`.
- **Do not push.** See the box at the top of this file.

## Session log

- **S1 (2026-07-30):** Scope, stack, and architecture decided. Planning docs and ADRs 0001–0005
  written. Repo initialized. No code.
- **S2 (2026-07-30):** Target games captured (Palworld / Schedule I / Inside the Backrooms), module
  priorities reordered toward 3D, and the renderer required to stay art-direction-agnostic.
  **Multiplayer promoted from non-goal to planned M6 with hooks reserved in M0–M2 (ADR 0006)** — the
  largest plan change so far. M3's exit gate set to a horror slice with concrete criteria.
  Human-legibility requirement added to `CLAUDE.md` §6 and `docs/07-working-with-the-code.md`
  created. GitHub remote added (personal account; the *global* git identity on this machine is a
  work account, so this repo carries a local override — do not remove it). Rust verified installed,
  MSVC build tools confirmed missing and blocking, rust-analyzer installed; Smart App Control found
  blocking and disabled by Justin. No engine code.
- **S3 (2026-07-30):** M0 implementation, essentially complete. In order: workspace + CI + `amadeo-core` (ADR 0007 fixed
  timestep, ADR 0008 ECS storage); `amadeo-ecs` archetype storage; `amadeo-events` +
  `amadeo-app` schedules and loop + the resource/service split (ADR 0009, found by a failing test);
  `amadeo-input` + the `.replay` text format + golden replay harness; deferred commands;
  `amadeo-render` abstraction and null backend; the wgpu backend behind an opt-in `gpu` feature; and
  `games/quad-demo`, whose window Justin confirmed working. 228 tests. ADRs 0007-0010 written.
  Visual-design preference recorded in `CLAUDE.md` §6. **Remaining in M0: the Q1 spike only.**
- **S4 (2026-07-31):** **M0 closed.** The Q1 spike, run as a measurement rather than an argument:
  four candidates (pure Rust, hot-reloaded cdylib, embedded Luau, WASM) implementing one shared
  benchmark — a three-state enemy AI over 64 entities — with agreement between them tested by state
  hash rather than by inspection. **ADR 0011: game logic is plain Rust in the game crate**, WASM
  reserved as an escape hatch behind a measured threshold.

  The recorded Luau prior was refuted, and specifically: Luau is perfectly deterministic but its
  `f64` arithmetic computes something *different* from `f32` components, diverging at tick 2. That
  breaks the prior's own central mechanism — graduating a system from Luau to Rust would change its
  behaviour and invalidate every golden replay taken before the move. Luau was also 24× slower, of
  which ~78% turned out to be the marshalling binding rather than the language.

  The question's premise was also wrong at this scale: the feared 30-second rebuild measured at
  0.9–3.2 s. Two engine gaps surfaced along the way — `Service: Send + Sync` excludes any non-`Sync`
  runtime (filed as Q12), and the two-component query limit is now confirmed as a real constraint
  rather than a speculative one. Established the `spikes/` convention. No engine code changed;
  still 228 tests.
- **S5 (2026-07-31):** **M1 begins.** Three-component ECS queries first, closing the gap the Q1 spike
  had exposed. Then the M1 keystone: `amadeo-reflect` and `amadeo-derive`, settling the four
  decisions `docs/04-subsystems.md` §8 flagged as needing to be made before writing any of it — a
  value tree rather than dynamic field access, struct fields sorted by construction so I2 is
  structural rather than remembered, the metadata vocabulary (including ADR 0006's replication
  annotations), and a derived `StableHash` so a forgotten field cannot silently drop simulation state
  out of every replay assertion. ADR 0012. Two latent `amadeo-core` gaps closed on the way. Then
  **ADR 0013: `Component: Reflect`**, turning invariant I8 from a convention into a compiler-enforced
  bound and converting every existing component — the same move ADR 0009 made for
  `Resource: StableHash`, and cheapest at eight components. The golden replay survived, for a reason
  worth reading in ADR 0013 rather than assuming. Finally **Q2**: four scene syntaxes hand-written
  and diffed (`spikes/q2-scene-format/`), where the prescribed criterion turned out not to
  discriminate — diffs are identical in all four — so the spike narrowed it to two and Justin chose
  the custom format. `amadeo-scene` built to it (**ADR 0014**) — parser, canonical writer, and then
  layer 2: `ComponentRegistry` and `instantiate`, so a scene file now loads into a `World` using the
  engine's real components. That surfaced a contradiction between two docs about where hierarchy
  components live, resolved by **ADR 0015** with a new `amadeo-transform` crate — and a second trap
  found on the way, filed as Q13: a component's id is the hash of its *fully-qualified path*, so
  moving a type between crates silently invalidates every state hash containing it. Finished with
  the read half of `amadeo-agent` — `describe`, `entity`, `query`, and a deterministic JSON writer —
  which made Pillar 2 real and surfaced **Q14**: under ADR 0011 a standalone CLI cannot know a
  game's components, so the roadmap's `amadeo-cli` shape needs rethinking before it is written.
  392 tests.
- **S6 (2026-07-31):** **Q14 resolved — ADR 0016 — and then built.** The decision came first and
  alone, deliberately: it fixes the shape of `amadeo-cli` and most of what remained in M1, and was
  worth settling before writing the CLI rather than during.

  Reading the code rather than the roadmap changed the framing twice. First, **option 1 was never a
  competing option** — the game binary is the only process holding the registry, the world, and the
  systems at once, which is the same argument ADR 0010 used to put the event loop there, so hosting
  the agent in the game is the substrate all three options are built on and the only live question is
  what wraps it. Second, **the registry has no home**: `ComponentRegistry` is built ad hoc in tests
  and nowhere else, and `quad-demo` registers nothing, so `describe` would today report an empty
  schema for a real game's own components. ADR 0016 puts the registry on `App` for the same reason
  ADR 0013 made `Component: Reflect` a compiler-enforced bound — registering in one place and
  spawning in another is how a component ends up invisible to the agent.

  Two sub-decisions the question had not asked. **One-shot batch before a live session**: each CLI
  invocation is a fresh deterministic run that exits, which is *more* reproducible than attaching and
  covers M1's exit gate; `sim.step` and the mutating calls wait for M4's editor to actually need a
  connection that outlives one question. And **the JSON parser is hand-written**, joining the writer
  already in `amadeo-agent` — `serde_json` was considered and rejected as the first real dependency
  beyond `thiserror` in a workspace that has hand-rolled PCG32, FNV-1a, and two text formats on
  legibility grounds.

  Then **built all of it**: the JSON reader, the registry on `App`, the protocol in `amadeo-agent`,
  the host in `amadeo-app`, the one-line handover in `quad-demo`, and `amadeo-cli` itself. The thing
  that now works is the point of the whole milestone — `amadeo describe Velocity` describes a type
  defined in `games/quad-demo`, answered over JSON-RPC by a game binary that a CLI which has never
  linked it went and launched. Two bugs were found by running it rather than by reasoning about it: a
  UTF-8 BOM from PowerShell's own pipe producing an error that pointed at an invisible character, and
  `state_hash` needing to be a hex string because a `u64` above 2^53 does not survive JSON's `f64`
  numbers.

  Then **`amadeo check`** on top of it — the first command that could not exist in a standalone CLI
  at all, since validating a component name means knowing which names exist. It needed
  `amadeo_scene::validate`, which collects *every* problem rather than stopping at the first the way
  `instantiate` does: stopping is right for loading and wrong for checking, because an agent fixing a
  file cannot ask a follow-up question and one error per round trip is a functional defect.

  Finally **`amadeo replay`**, which closes the separate-process replay gate carried since M0 —
  the last outstanding item from that milestone. The seed problem it raised turned out to have a
  boring answer: the game asks `requested_seed()` *before* building, rather than the host re-seeding
  afterwards, because a world whose construction consumed randomness would then differ from the one
  recorded and the divergence would look like a real regression.

  Then two decisions that had been waiting, each built the same session it was made. **Q13**
  (ADR 0017): `ComponentId` now hashes a component's canonical name rather than its Rust path, so
  moving a type between crates stopped being a silent replay-invalidating change. Both replays were
  regenerated, and confirmed the diagnosis rather than merely obeying it — only the checkpoint lines
  moved, with byte-identical input streams.

  Then **Q3**, which turned out to be three decisions wearing one question. Reading the code showed
  the framing everyone uses — one pipeline or two — is the *cheapest* of the three to reverse, since
  `RenderBackend` isolates it entirely, while the two expensive ones are about data: what a transform
  is, and what decides draw order. So a three-pipeline spike would have measured the wrong thing.
  **ADR 0018** settles the data half: one 3D `Transform` with 2D as its degenerate case, rotation as
  Euler degrees so it stays hand-writable, and `SortOrder` replacing `Quad::layer`. The pipeline
  choice is deliberately deferred to when the sprite batcher exists and can be measured.

  Then **`GlobalTransform` and `propagate_transforms`** (ADR 0019), waiting since ADR 0015. Justin
  decided directly that a derived component stays **out of the state hash**, which needed a mechanism
  the ECS did not have: `Component::DERIVED`, carried through the type erasure by `Column`, honoured
  by `state_hash`. Named `DERIVED` rather than `HASHED` on purpose — the first states what must be
  *true* so the rule follows from the name, the second describes what it does and invites anyone
  wanting a quieter diff to reach for it. Proven, not asserted: `quad-demo` now carries a
  `GlobalTransform` on every entity and both replay fixtures are byte-unchanged.

  **Then CI, which had been red since the first push and was not what it looked like.** The failing
  assertion had *identical checkpoint hashes on both sides* and differed only in `\n` versus `\r\n`.
  `core.autocrlf` is true by default on GitHub's Windows runners; with no `.gitattributes` it
  rewrote every committed LF on checkout. This machine has it set to `false` locally, which is why
  it reproduced nowhere here across seven different reproductions of CI's exact commands. Fixed with
  `.gitattributes`, verified by two fresh clones under `autocrlf=true` (17 CR bytes before, 0
  after). Worth recording that the toolchain-pin commit immediately before it was a real fix for a
  real defect — `channel = "stable"` was not pinning anything despite its comment promising exactly
  the reproducibility I3 needs — but it was **not** this bug, and I presented it with more adjacency
  to the failure than it earned.

  Finally **Q4 (ADR 0020)** and **ADR 0021**, plus the first slice of `amadeo-assets`. Q4 asked what
  names an asset; the answer follows Q13 one layer up — a path is a *location*, so identity is a
  declared `id` in a sidecar, defaulting to the filename stem so it reads like a path but survives a
  move. ADR 0021 then settled how loading avoids breaking I3, and this one was **researched rather
  than reasoned about** (Justin's standing instruction): the industry pattern is a loading barrier,
  but Bevy chooses it for user experience and tolerates mid-game loads, so adopting it for
  determinism would give the right shape for the wrong reason and would not hold the first time
  someone streams a chunk. The invariant is stronger: gameplay holds an id and never observes asset
  *state*, so there is nothing to branch on.

  519 tests, all five CI jobs green, seventeen commits.
- **S7 (2026-08-02):** **`amadeo-assets` finished** — all five steps the previous handoff listed, in
  its order. The scan, sidecar generation on import, `assets.list` plus `amadeo assets` and
  `amadeo import`, the load barrier with a scene-declared `assets` block, and asset ids validated by
  `amadeo check`.

  **The handoff's claim that the loading half had "no open decisions left in it" was wrong**, and it
  surfaced in the first hour: a relative asset path has to resolve against *something*, and the
  working directory differs in all four ways a game starts. Researched rather than guessed — Bevy
  uses an environment-variable chain, Godot anchors on a marker file — and **ADR 0022** took Godot's
  approach, because this project already has `amadeo.toml` and the CLI already walks up for it.

  Then **the audit Justin asked for**, which is written up in its own section above. Headline: the
  invariants hold, and I3 holds better than the docs claim — there is no `HashMap` anywhere in the
  engine, and all transcendental maths is confined to non-hashed paths, which makes ADR 0019's
  derived-component decision quietly load-bearing for cross-platform determinism. One real gap:
  `Rng` had only self-consistency tests, which would all pass on a subtly wrong generator. Now
  cross-checked, and it **reproduces the official PCG32 demo vector exactly**.

  One unresolved conflict found and deliberately *not* decided alone: ADR 0014 and ADR 0020 disagree
  about whether `from` holds a path or an asset id. Filed under Q7, since it has to be settled before
  prefab instancing.

  578 tests, all four verification commands green, `wander.replay` unchanged.

  **Then the target list grew from three games to eight** — Minecraft, Terraria, Project Zomboid,
  RimWorld, Stellaris added to Palworld, Schedule I, and Inside the Backrooms. Written up in
  `docs/00-vision.md`, and it is a larger change than a list edit: the original three were all 3D,
  all action-paced, all co-op, all rendering-led, and the five additions break every one of those.
  Six consequences, of which two matter most. **2D stopped being a principle being defended and
  became a requirement** — three of the eight are 2D or isometric, which lands the same week the
  sprite batcher does. And **modding became a target-driven requirement**, which puts ADR 0011 under
  a kind of pressure Q1 never evaluated: it decided by measuring the developer's iteration speed, and
  a mod author cannot rebuild the engine at any speed. Filed as **Q15**, deliberately not decided.

  **Then the sprite batcher (ADR 0023), which closed Q3 and then kept going.** The batching rule is
  `(sort order, texture)`: layering exact across orders, grouped by texture within one. 20,000
  interleaved sprites collapse to exactly 32 batches, and a whole tilesheet is one draw call.

  What made the rest of the session was that **the measurement did not agree with the theory.**
  Collecting 20,000 sprites took 5.1 ms, and removing the batcher's own trigonometry moved it by 4% —
  which ruled out the obvious suspect and pointed into the ECS twice over:

  - **ADR 0024** — `ComponentId::of` was allocating a `String` and hashing it *on every call*, on the
    hot path of every component access. Now a compile-time constant via two new `Reflect` consts.
    5.13 → 3.32 ms, and it made the whole engine faster, not just rendering.
  - **ADR 0025** — the ECS could not express an *optional* component in a query, so the renderer fell
    back to `world.get` per entity: 40,000 lookups a frame, which is exactly what archetype storage
    exists to avoid. `world.query::<(&A, &B, Option<&C>)>()` now resolves each column once per
    archetype. 3.32 → 2.58 ms. **Justin chose this design** from three options after research.

  Two near-misses worth keeping. A `static` cache inside a generic function is shared across
  monomorphisations, not per-type — it collapsed every component onto one id and the archetype tests
  caught it instantly. And the throughput fixture gave no entity a `GlobalTransform`, so it was
  measuring a fallback path no shipped game takes; fixing it changed the final number materially.

  **610 tests, all four verification commands green, both replays unchanged throughout** — which
  mattered most for ADR 0024, where a wrong hash would have invalidated every committed replay at
  once.
- **S8 (2026-08-03):** **Sprites reach the screen**, which is what STATUS.md had named the single
  most important thing to do next.

  **The decision turned out to be bigger than the handoff framed it.** The handoff asked where the
  decoder should live. Reading `docs/04-subsystems.md` §5 found something else: an import pipeline
  was already recorded there as **decided** — "the runtime never parses source formats" — with a ✅
  beside it, no ADR behind it, and code doing the opposite. A decision that exists on paper,
  contradicts reality, and has never met real work is worth re-deriving rather than obeying.

  Researched rather than reasoned about. **The reason an import pipeline is eventually mandatory is
  concrete**: GPU-compressed formats like BC7 are deliberately asymmetric, cheap to decode and
  minutes-to-hours per texture to *encode*, so compression can only ever happen offline. Godot, Unity
  and Unreal all import for that reason; Bevy is the outlier, and this project has already declined
  Bevy's answer twice on its merits.

  **What dissolved the tension was noticing that the expensive part is the type, not the pipeline.**
  Give the runtime an explicit `PixelFormat` on day one and the import step becomes a later
  *addition* rather than a later rewrite, because everything above the decoder already speaks
  `TextureData` and cannot tell where one came from. Building the pipeline now would mean a compiled
  file format, a cache, and cache invalidation — which §5 still lists as unsolved — carrying nothing
  but the same RGBA the decoder produces anyway.

  **The dependency question was measured, not argued**, since it breaks a pattern the project has
  held to deliberately: `png` costs 9 crates and a 3.2 s clean release build, `image` costs 15 and
  14.5 s, and both are one-time. What justifies the break is that PNG data is DEFLATE-compressed, so
  hand-writing it means hand-writing inflate — ~800 lines whose failure mode is *slightly corrupt
  pixels* rather than a wrong known answer. PCG32 and FNV-1a were worth hand-rolling; this is not the
  same kind of thing. Justin chose all three recommendations after the trade was put to him.

  Then built it: `amadeo-image`, `TextureCache`, the wgpu texture path, and a demo that shows it.
  Two things worth keeping. **The contract for `SpriteInstance::axes` was wrong** — it claimed
  half-extents where the code has always produced full extents — and it was only caught because the
  shader had to be written from it, which is a good argument for writing the consumer of a doc
  comment rather than trusting it. And **`wander.replay`'s regeneration was diagnosed rather than
  obeyed**: removing only the ten new entities restored all four committed hashes exactly, proving
  the new machinery is invisible to the simulation and the content change is the whole cause.

  669 tests, all four verification commands green, and **confirmed on screen** — Justin ran the demo
  and the screenshot checks out against the world coordinates, including the alternating tile colours
  that prove `region` is picking a different texel per tile.

  **Then invariant I8 was closed — ADR 0027.** `Resource: Reflect` had been the oldest unpaid debt in
  the engine, deferred by ADR 0013 because it was genuinely *not yet possible*: `SimRng` wraps an
  `Rng` whose state is private to a crate sitting below `amadeo-reflect`, and `InputState` is two
  maps, which the value tree could not represent. Both had to be solved before the bound could exist.

  **Two decisions went to Justin and he took both recommendations.** Maps in the value tree get
  **string keys** rather than Bevy's and Godot's arbitrary ones — researched, and the deciding facts
  were that `Value` holds floats and so has no total order to sort arbitrary keys by, and that a
  struct-as-a-key has no hand-writable syntax in an indentation-based format. And **`SimRng`'s
  `Debug`-based hash was retired now** rather than left, at the cost of regenerating both replays.

  The scope grew twice in ways worth recording. **Events joined the bound** — `Events<T>` is a
  `Resource`, so it hit it transitively, and the argument turns out to be stronger for events than
  for resources, since the event log is how an agent answers "what did I just do?". And
  **`world.resources` was built on top**, because the bound alone is invisible: it exists so that
  something can be *shown*, and the protocol doc had listed that method as blocked on exactly this.

  Two things worth keeping. **A type below `amadeo-reflect` has two different answers** depending on
  whether its state is public: `Tick`'s impl was written *inside* `amadeo-reflect` (legal, since the
  impl can live where the trait does), while `Rng` had to expose `state()` and be reflected a layer
  up. And **the replay regeneration was diagnosed, not obeyed** — reverting only the `SimRng` hash,
  with five types newly reflected and the bound in force, restored both replays exactly, proving the
  reflection work never touched the state hash.

  One gap was **created** rather than found, and only became visible once `world.resources` could be
  pointed at a running game: `InputState`'s keys are `ActionId`s, which are hashes whose names are
  not kept, so it reports `"8831028638596390904"` instead of `"move_x"`. Filed as **Q18** with the
  recommendation deliberately withheld until a second instance shows what the general shape should be.

  698 tests, all four verification commands green, both replays regenerated and passing.

  **Then snapshots — ADR 0028 — which ADR 0027 had just unblocked.** Two parts of this were forced
  by earlier decisions rather than chosen, and neither was obvious until traced back. A snapshot must
  be a **file**, because ADR 0016 makes every CLI invocation a fresh process that exits, so an
  in-memory one would die with the process that took it. And a snapshot must capture the **entity
  allocator**, because `state_hash` excludes the free list — so a snapshot of only the live entities
  would restore a world that hashed identically and then handed out different entity handles on the
  next `spawn`. That second one is the whole subject of the ADR: it means **hash equality after a
  restore is necessary and not sufficient**, so correctness is tested by running the world *on*
  afterwards. Delete `free_slots` from the format and exactly one test fails.

  **Justin chose text over binary, and a separate crate over a module.** The speed objection to text
  does not survive the numbers already in STATUS — re-simulation is ~21 µs/tick and writing a few
  dozen entities is well under a millisecond. Reusing the `.scene` format was rejected outright as
  trap 4. The consequence of the separate crate was handled rather than accepted: `amadeo-snapshot`
  *borrows* `amadeo-scene`'s scalar encoding, because `format_float` is subtle in three different
  ways and two copies would drift.

  **Running it against the real game found a defect no unit test had.** `InputState` is two maps, and
  the format had no nesting — so its value fell out in `Display` form, which no parser reads back.
  Since `InputState` is a resource in *every* game, a snapshot of anything real would capture and
  then refuse to restore: broken in the way that looks like it worked until you need it. The format
  gained proper nesting, which it should have had from the start.

  747 tests, all four verification commands green, `wander.replay` unchanged.

  **Then M1's exit gate: `games/vault`, a complete small 2D game.** Collect six sigils in a walled
  arena without touching a patrolling warden. All five things the gate asks for — player moves,
  enemies patrol, collision, a score, a win state.

  **Three engine gaps had to close first**, and finding them was worth as much as the game. Gate 2
  names `render.describe` as the verification channel and **it did not exist**. Gate 1 says the game
  is authored via text files, but **no game had ever loaded a scene file** — `markers.scene` had sat
  unread since session 5, because `instantiate` needs the world mutably and the registry shared and
  `App` owns both, so every game would have had to rediscover the workaround. And the roadmap's
  snapshot acceptance test had never been run: measured at **22× faster than re-simulating** in
  debug, which is the profile the agent's loop actually uses.

  **The game was built and debugged without ever being looked at.** That is the milestone's whole
  thesis and it held: the win circuit was authored blind, by reasoning about distances and speeds,
  and passed first time. `render.describe` then caught a real layout bug — the score readout
  overlapping the top wall by 0.15 units — which no simulation test could have seen and which was
  fixed before anyone opened a window.

  **What the game found about the engine is written up above**, and the headline is that **the scene
  format is impractical for repeated content**: forty-four wall tiles would be four hundred lines of
  near-identical text. That is what prefabs are for, and prefabs were blocked on Q7 — which is what
  got Q7 settled later the same session. The argument arrived from use rather than from theory.

  795 tests, all four verification commands green, and a new replay fixture asserted by CI in a
  separate process.

  **Then exit gate 4, tested — and its claim is false.** The gate says `describe` output should be
  sufficient to write a new component and system without reading engine source. Tested by doing it:
  `Trap` and `spring_traps`, shipped in the Vault. `describe` turned out to be **sufficient to author
  content and silent about the API** — every field carries type, unit, range and meaning, which is
  what made `vault.scene` writable, and nothing in it says how to declare a component, register one,
  write a system, or query a world. **Resources are absent from it entirely**, so `Run` — the very
  resource `spring_traps` exists to change — appears nowhere. Written up in
  `docs/09-gate-4-describe-is-not-enough.md`, with an honest caveat about the confound: the
  experiment was run by an agent that had already read the engine source, so the gaps are ones it
  *noticed*, not ones it was stopped by. Three options for closing it are in that document and the
  choice is Justin's, because it decides whether the protocol is a schema or a manual.

  **Then Q7 — prefabs — ADR 0029**, chosen over the roadmap's next item because the Vault had just
  run straight into it and nothing would ever be better informed. Both halves settled:

  - **`from` holds an asset id**, superseding ADR 0014's path grammar. The whole asset toolchain then
    applies to a prefab for nothing — `amadeo check` validates the reference and offers "did you
    mean", ADR 0021's barrier makes it resident before the first tick, `amadeo assets` lists it.
  - **An override is a top-level patch on the instance root and reaches nothing inside it.** This is
    the half the research decided. Unity's overrides evaporate under nesting because an override
    names something *inside* a prefab and then has to track it across every future edit of that
    prefab; Godot's editable children can write back to the source scene and to every other instance.
    Both failures come from overrides reaching inward. Here there is no syntax that can, so there is
    nothing to lose track of — nesting is **structurally** safe rather than carefully handled, and
    `nesting_is_safe_because_overrides_cannot_reach_inside` is a passing test rather than a hope.
  - **A dangling override refuses to load**, naming the entity, the component and the prefab. The
    direct counter to Unity's worst behaviour: the failure arrives when the prefab changed, not
    months later as a value that mysteriously reverted. `override Foo` on a component the prefab
    lacks is an error, and a bare `Foo` on one it *has* is an error too, because the author meant
    `override` and silently picking one would hide it.

  **Proof it is behaviour-preserving:** the Vault's six sigils and two traps became prefab instances,
  the scene went from **223 lines to 142**, each sigil from fourteen lines to three — and
  `collect-three.replay` matched **all four checkpoints unchanged**. The same world, authored
  differently.

  Two costs, both recorded rather than swept up. A prefab shares one id namespace with every other
  asset, and the Vault hit that immediately: `sigil.scene` collided with the `sigil` texture, fixed
  by renaming to `sigil_pickup`. And **`amadeo import` cannot import a prefab** — a bootstrapping
  deadlock, since `import` launches the game and the game refuses to start while a prefab it needs
  has no sidecar. The Vault's two sidecars were written by hand; the deadlock is filed as **Q19**.

  **What prefabs deliberately do not fix: the wall grid.** As instances the forty-four tiles would be
  176 lines of scene text against a seven-line picture of the level, so they stay in `MAP`. Prefabs
  fix repeated *designed* content; a grid wants a tilemap, which is `mod-tilemap` in M7. Worth
  stating because "prefabs will fix the walls" was the obvious expectation and it is wrong.

  **And a bonus find while writing the prefabs up:** `amadeo fmt --check` had never been pointed at
  a scene file, and **all four in the repo were non-canonical** — components out of sorted order,
  written by hand and never run through the formatter. Invariant I2 applies to hand-written scene
  files exactly as `cargo fmt` applies to code and nothing was enforcing it. Reformatted, and CI now
  checks all four; `collect-three.replay` still matched all four checkpoints afterwards, which is
  also a small proof that component order within an entity does not reach the state hash.

  817 tests, all four verification commands green, `amadeo check` passing on the scene and on both
  prefabs.

  **Then gate 4's decision, which had been left for Justin — ADR 0030.** Three options were put to
  him, from "leave it failed and say so" to extending the protocol; he took the most complete one.
  The reframe that made it tractable: the five gaps gate 4 found are **two different kinds of
  thing**, and treating them as one question is what made it look hard.

  **Four of them are API knowledge and stay out of the protocol.** The argument is **I5**: anything
  the editor can do, the CLI and RPC can do — and the editor will never declare a new Rust component
  type, since that means editing the game crate and recompiling. So the gate was asking the protocol
  for something the project's own invariants do not ask of it. `describe` gained a `manual` key
  naming the file instead. Rejected outright: putting the recipe *in* the reply, because prose inside
  a protocol reply is documentation nothing recompiles. MCP has exactly that field — servers may
  return an `instructions` string at handshake — and the spec calls it a hint, and most servers do
  not set it.

  **The fifth was a real hole, and looking at it properly found two more the gate had missed.**
  Resources were absent from `describe` entirely. The schema was also **not closed** — `Run.phase`
  reported `"type": "Phase"` and nothing could look `Phase` up, so nothing could know its legal
  values. And a fixed array's **length lived only inside its name**, so anything needing the count
  had to parse `"array<f32, 2>"` back apart. Both of those are editor blockers that would not have
  surfaced until M4.

  Bevy's remote protocol is the closest analogue and it went the same way twice: resources were added
  to BRP after the fact, and a third-party crate added `discover_format` because the schema alone
  "doesn't show the actual JSON format needed" — leaving people reverse-engineering shapes out of
  error messages. That is what `describe.example` is, built in rather than bolted on.

  **`describe <Type> --example` emits a minimal valid instance** in both the scene and JSON
  spellings, from one value so they cannot disagree. Its clearest justification is a single line:
  `phase Playing` is a bare word, and `phase "Playing"` parses and *then* fails to load — grammar
  rather than type information, so no schema would ever have said it. The testable property is that
  the emitted example **loads**, and that is the test, for every component the engine has.

  Two things went in underneath to make it possible, both defensible on their own: `Reflect` gained a
  derive-generated `register_dependencies`, so registering a type registers everything it names
  (inserted before recursing, so a self-referential type terminates — that is a test, not a hope);
  and `TypeKind::List` gained a `length`.

  827 tests, all four verification commands green, both replays matching all eight checkpoints
  unchanged.

  **Then M2 opened with its ADR, which the roadmap requires before any code — ADR 0031.** The
  interesting part is that the question was pointed at the wrong thing, and it was the *second* time.
  `docs/04` §4 calls the pipeline shape "the real decision of this subsystem"; ADR 0018 had already
  corrected that framing once, noting that Q3 emphasised the pipeline while the expensive decisions
  were data. ADR 0023 then recorded outright that the pipeline is cheap, because `RenderBackend`
  isolates it so completely that no file and no hash can observe it.

  So the pipeline was a consequence rather than a choice: **two passes in one render graph**, neither
  built on the other. Option (a), one unified orthographic pipeline, was not actually available —
  ADR 0023 had already rejected depth-buffering sprites because transparent sprites erase what is
  behind them, so "one pipeline" would have meant a 3D pipeline with depth switched off for sprites.
  Two pipelines with the honesty removed. Option (c), compositing 2D over 3D, forecloses a 3D object
  drawn in *front* of a 2D layer, and is the arrangement Godot needs a plane-mesh-and-SubViewport
  workaround to escape. Bevy runs separate `Core2d` and `Core3d` subgraphs in one graph, which is
  where this lands too.

  **The expensive decision hiding inside it was the camera model, and nothing had framed it as a
  question.** A camera is reflected data — it lives in the schema, it can live in a scene file, and
  today it lives in the state hash. `Camera2d` is a *resource*, so a world can hold exactly one,
  forever. Justin chose to make it an entity now rather than later, taking the full version with
  render targets and viewport rectangles.

  Three things forced it. **M4's editor needs a camera the game does not own**, and invariant I1 puts
  it in the world rather than in private editor state — so deferring would have made M4 a migration
  moving the scene format, the schema, the state hash and a new GUI at once. **Render-to-texture** is
  a target setting and impossible with one camera; Backrooms and Schedule I want security monitors,
  RimWorld and Zomboid want minimaps. And **Project Zomboid is isometric**, which is neither cleanly
  2D nor cleanly 3D — an orthographic projection feeding sprite drawing with Y-sorting, which only
  works if the projection belongs to the camera rather than to a pipeline. Bevy migrated to
  camera-driven rendering the same way, which is evidence both that the shape is right and that
  retrofitting it is expensive.

  **And designing the component found a real hole in the scene format.** Probing it directly rather
  than assuming: a nested struct emits `{height: 8}`, a Rust `Debug` form nothing parses; an enum with
  a payload does the same; and `Option::None` writes a bare field name that the parser refuses
  outright. Never hit before, because every component in the engine is scalars and flat lists. It is
  why ADR 0031's camera is flat — a fieldless `projection` enum beside plain `height`, `fov`, `near`
  and `far` — when `Projection::Orthographic { height }` is the obvious design and the better type.
  Accepted rather than solved, and filed as **Q21 at P1**, because fixing it is a change to ADR 0014's
  grammar and deserves its own decision. It has to be settled before M2's material model, where the
  same problem arrives at a type nobody would want to flatten.

  **No code yet.** The ADR is committed on its own, which is what "decided before code" means.

  **Then ADR 0031, built.** A `Camera` component replaces the `Camera2d` resource; `FrameData`
  becomes a list of `View`s, one per active camera, already in draw order; the wgpu backend runs one
  pass per view with a dynamic-offset uniform buffer, only the first clearing, so a HUD camera
  composes over a world camera rather than erasing it; and `render.describe` answers for the camera
  that draws first to the window, with `describe_frame_through` for any other. **Both games now
  author their camera in their scene file**, which is invariant I1 reaching a subsystem it had not
  reached — the view is part of the level.

  **A world with no camera draws nothing**, where it used to fall back to a default. That was the
  right answer when there could only ever be one camera and is the wrong one now: inventing a view
  nobody authored would draw a picture nobody asked for. The screen is still cleared, so "no camera"
  looks empty rather than frozen.

  **Both replays moved, and the isolation was unusually clean.** `docs/07` says find out why before
  regenerating, and the answer here is a three-row table: HEAD reproduces; HEAD with *only* the
  camera's data placement changed gives `950455d547a4adf9` at tick 300; the whole refactor with that
  same change gives **the identical value**. So the entire render restructuring contributes nothing
  to simulation state, and every bit of the movement is the deliberate data move. Regenerated on that
  basis.

  **Building the control turned up Q22.** The stand-in resource had the same canonical name and the
  same fields and hashed *differently* — because `ResourceId` hashes the **Rust path** while
  `ComponentId` hashes the canonical name (ADR 0017). Opposite rules for the two, which means moving
  a resource between crates silently invalidates every golden replay. Nothing is broken today; the
  crate graph is still moving, so it is worth deciding.

  839 tests, all four verification commands green, both replays passing on their new hashes, and
  `amadeo check` and `amadeo fmt --check` clean on all four scene files.

  **Then Q21 — ADR 0032 — because the camera's flat fields were a symptom.** The grammar already had
  the slot: a field with no inline value already opened an indented block, it just only accepted
  `- ` items. So the whole extension is one rule, and it is **YAML's** rule — a block is a list if
  its lines start with `- ` and named fields otherwise. No schema is consulted, which matters,
  because layer 1 deliberately has none. Nested structs, maps and enum payloads all fall out of it,
  and **maps became scene-expressible as a side effect**, closing ADR 0027's recorded gap.

  Purely additive, so every scene file valid before is valid after.

  `Option::None` was left unsolved on purpose. `none` collides with an enum variant of that name; a
  sigil would be this format's first punctuation, having chosen indentation over punctuation
  throughout; and omitting the field destroys ADR 0014's distinction between "explicitly nothing" and
  "whoever wrote this forgot". Nothing has an `Option` field, so it waits for a real case.

  **`Projection` was un-flattened immediately** — `Orthographic { height }` and
  `Perspective { fov, near, far }`, each carrying only what it needs, with `Projection::height()`
  returning `None` for a perspective camera rather than a fallback. Done now rather than later
  because the replays had just been regenerated, so it was the cheapest moment it will ever be.

  **Three things fell out of doing it, all found by use rather than reasoning:**

  The derive was **silently dropping `min`, `max` and `unit` on enum variant fields** — so a field
  lost its declared range simply by moving into a variant, which is precisely what this ADR
  encourages. The struct and variant paths now share one function.

  `amadeo-snapshot` **could not write a payload enum**: it came out as `Orthographic({height: 8})`,
  Rust's `Debug`, which nothing reads back. That is the *second* time that exact defect has been
  found in that crate — the first was maps, earlier this session — and both times by snapshotting a
  real game and reading the file. It now has a test that builds a world holding every awkward shape
  and asserts the restored state hash matches.

  And **quad-demo had been drawing nothing since the previous commit.** ADR 0031's camera went into
  `scenes/markers.scene`, and quad-demo *does not load its scene file* — it never has. Nothing caught
  it: its replay still passed, because a camera the world never had cannot move the state hash, and
  quad-demo has no `render.describe` test the way the Vault does. It now spawns its camera in code
  beside everything else, and has two tests — one that it has a camera, one that something is
  actually on screen.

  Both replays regenerated again, and the vault's cause was proven by snapshot diff first: the only
  difference between the two worlds was the camera's four flat fields collapsing into the
  projection's payload. Nothing else moved.

  853 tests, all four verification commands green.

  **Then Q19, which prefabs had opened.** `amadeo import` writes the `.ama-meta` sidecar an asset
  needs before a game will start — and it learned the asset directory by *launching the game*, which
  refuses to start while a sidecar is missing. The tool that fixes the problem could not run.

  `amadeo import --assets <dir>` names the directory instead. Asking the game stays the default,
  because the path is a constant in the game's own source and so nothing can disagree with it; the
  flag is the escape hatch for exactly the case where the game will not start.

  **The first attempt was wrong and worth recording.** It put `assets = "..."` in `amadeo.toml`, and
  a manifest is per-*project* while an asset directory is per-*game* — in this repo, with two games,
  the key could only describe one. The Vault, the case that motivated the question, runs under
  `--package vault` and would have fallen straight back to launching the game. Caught by asking
  whether the fix reached the motivating case, which it did not.

  **Verified by reproducing the deadlock**: deleted both prefab sidecars, watched
  `amadeo import --package vault` fail exactly as before, then `amadeo import --assets
  games/vault/assets` wrote them with nothing launched — **byte-identical** to the hand-written ones.

  855 tests, all four verification commands green.

  **Then Q22, which turned out not to be a question.** A resource's identity in the state hash was
  `std::any::type_name` — the Rust path — where a component's is its canonical name, so moving a
  resource between crates silently invalidated every golden replay.

  **ADR 0017 had already decided it and deferred only the timing:** *"resources get this treatment
  when `Resource: Reflect` lands"*. ADR 0027 landed that bound earlier the same session, so the
  trigger had fired and been missed. Worth noticing as a *class* of mistake rather than a one-off —
  a deferred obligation inside an accepted ADR has nothing watching it, and only surfaces when
  something trips over the inconsistency. ADR 0017 even argued the timing: it rejected deferring
  because "the cost of this decision grows with every recorded replay".

  Services keep the Rust path, permanently, for the reason that ADR gave: not reflected, not in any
  hash, named by no file.

  **Three replays regenerated** — including `walk_and_jump.replay`, the in-process one, which had not
  moved all session. The signature was exactly what ADR 0017 recorded for an identity change: input
  streams byte-identical, only checkpoint lines moved. Confirmed independently by snapshot diff, the
  world before and after being byte-identical apart from the `state-hash` line.

  857 tests, all four verification commands green.

  **Every open question this session raised was closed in it** — Q7, Q19, Q21, Q22 — along with Q3,
  Q10 and M1's gate 4. What is left is build work rather than decisions.

  **Then the GPU path got its first automated coverage, ever.** `STATUS.md` carried "no automated
  coverage at all" as a known gap through three milestones: `render.describe` checks what *should* be
  drawn, computed from the world, and nothing checked what the GPU actually produced. Every claim
  about the wgpu backend rested on somebody opening a window and looking.

  `WgpuBackend::offscreen(width, height)` renders into a texture it owns rather than a window's
  swapchain, and `RenderBackend::capture` reads it back. **The two backends differ in where the frame
  lands and in nothing else** — same shaders, same pipelines, same passes — which is what makes a
  captured image evidence about the renderer that ships rather than about a second one written to be
  testable. It is also the path agent mode needs, since ADR 0016 launches a game with no window.

  Four tests: the clear colour is the dark non-black it is supposed to be, a red `Quad` reaches the
  middle pixels *and does not fill the corners*, two cameras over one world produce different images,
  and a backend that cannot capture says so while naming what answers the same question instead.
  That third one is the interesting one — it catches a projection wired up wrongly, which
  `render.describe` structurally cannot, because `describe` *computes* the same projection rather
  than observing it.

  They skip and pass on a machine with no adapter, which is honest rather than convenient: a missing
  GPU is a fact about the machine. CI runs them as their own step, since `cargo test --workspace`
  does not enable the `gpu` feature.

  861 tests plus the 4 GPU ones, all four verification commands green.

  **Then `render.capture` over the protocol — the agent has eyes.** `amadeo capture shot.png`
  launches a game headless, opens an offscreen GPU, renders the world, encodes a PNG and writes it.
  Run against the Vault it produces the arena: walls, six sigils, two wardens, two traps, the player,
  and the score readout — and captured at two different ticks the wardens have moved along their
  patrol routes, so it is live simulation state rather than a static picture.

  **Justin chose PNG** over the PPM this engine already reads. The deciding argument is that the
  point of a capture is that a *human opens it*, and nothing opens a PPM — not a file browser, not a
  chat client, not a pull request. The `png` crate was already a dependency for decoding, so encoding
  cost no new one, and the same reasoning that kept DEFLATE out of the hand-rolled column applies
  identically to writing it.

  **The image goes to a file rather than into the reply**, and the *game* writes it: a screenshot is
  hundreds of kilobytes, and base64 in a JSON-RPC line would make a transcript unreadable for no
  gain. The reply carries the path, the size, the tick, and the number of drawable entities — that
  last one because "the file is small and the world is empty" and "the file is small and something is
  wrong" look identical otherwise.

  Capture creates an offscreen device, uses it, and drops it. That costs a device creation per call,
  which is the right trade for an introspection method nobody calls in a loop — the alternative is
  holding a GPU open for every headless run, including the thousands that never capture anything. It
  is behind a `gpu` feature on `amadeo-app`, off by default, so a dedicated server does not link a
  graphics stack it will never use (I7). Without it the refusal names `render.describe`.

  CI now runs both halves: the unit tests, and the whole path end to end through a real game binary.

  864 tests plus the 4 GPU ones, all four verification commands green.

  **Then ADR 0033, the material and shader model — decided before its code, like ADR 0031.** And
  `docs/04` §4 had the emphasis wrong for the **third** time: it asks about shaders, which
  `RenderBackend` isolates completely, while the hard-to-reverse decision was where a material's
  *data* lives. That is now a pattern worth naming rather than three coincidences.

  **A material is an asset with an id**, Justin's call, on three arguments. It is shared by
  construction — the Vault's forty-four walls use one, so inline data would be forty-four copies in
  every state hash and every snapshot. ADR 0023's batching rule extends to `(sort order, material)`
  and comparing an id is a string compare where comparing a struct is a deep one, on the path the
  batcher exists to keep cheap. And the whole ADR 0020/0029 toolchain — validation, "did you mean",
  the load barrier, `amadeo assets`, `amadeo import` — applies for nothing.

  Its file *is* a scene file with a single root, exactly as a prefab is, so the parser, the canonical
  writer, `amadeo fmt` and ADR 0032's nested values all work on it the day it exists.

  **This was blocked until earlier the same session.** The inline alternative was unrepresentable
  before ADR 0032 gave the format nested values, and deciding against a format that could not hold
  the alternative would have prejudged the answer.

  Shaders: hand-written WGSL with `#include`, `#ifdef` and a pipeline cache keyed by the defines —
  Bevy's shape, reached after they hit the variant problem for real. **No material graph**: that is
  an editor-sized project before the first triangle, and if ever wanted it is additive, since a graph
  emits WGSL rather than replacing it. Decided alone and flagged, since `RenderBackend` isolates it.

  What a `Material` *holds* is deliberately not decided — that depends on the PBR model and arrives
  with meshes, because adding a field to a reflected type is the cheap change the schema exists for.
- **S9 (2026-08-04):** **The render graph — decided, then built**, which is what this file had named
  the single most important thing to do next.

  **The framing was wrong for the fourth time in this subsystem, and this time it was the
  vocabulary.** ADR 0018, 0031 and 0033 each found `docs/04` §4 asking about the pipeline while the
  expensive decision was the data beside it. Here the trouble was that "render graph" names two
  independent things — a frame scheduler that derives pass order and allocates transients, and an
  extension point where a game inserts a pass. The roadmap line asks for the first and the worry
  recorded here was about the second. Separating them is what made the question answerable, and it
  also revealed that **most of the first is already done**: wgpu tracks resource state and inserts
  barriers itself, which was half of what Frostbite's FrameGraph existed for.

  **The requirement also does not say what it looks like it says.** "Configurable post-process stack"
  can mean tunable or extensible, and `docs/00-vision.md` asks only that the renderer not bake in a
  look. Godot, Unity and Unreal all ship the tunable stack as the primary answer and put the
  extension point behind an advanced, later, harder door — Godot's `CompositorEffect` arrived in 4.3
  and its own docs call it an advanced feature working on only two of three renderers.

  **Bevy is the one engine that made its graph public, and it is evidence against.** It walked back
  from resource dependencies — graph slots removed as boilerplate-heavy, data moved into ECS
  components with the graph doing ordering only — and making it public turned it into a permanent
  migration surface, rewritten as render-graph-as-systems as recently as 0.19.

  **Justin took both recommendations.** The graph is internal; a look is an `Environment` asset held
  by the camera, its file a scene file with one root exactly as a material's is. The deciding
  argument for data over code was **I5 and I7** rather than anything about rendering — configuration
  made of data is authorable, describable, checkable and visible headless for nothing, and a pass
  supplied as code is none of those. Same shape as ADR 0030. Recorded honestly in the ADR: 0033's
  *decisive* argument does not apply here, since a world has one to three cameras rather than
  forty-four walls, so the asset form rests on a look being the thing that gets tuned and swapped.

  Then built it. The graph is a plan that knows nothing about wgpu, so `NullBackend` compiles it too
  and reports the resolved pass order — a pass-ordering bug is now catchable on a machine with no
  GPU, which would have been impossible had the graph lived inside the wgpu backend. Ordering is
  write-before-read, then declaration order between two writers of one image, then declaration order
  for anything unordered — deliberately the opposite of `Schedule`'s alphabetical tie-break, because
  a schedule's registration order is accidental while a graph's is the order the frame is composed
  in.

  **Two findings, both from writing the tests rather than the code.**

  The `scene` transient is always RGBA and deliberately does **not** inherit the destination's
  format. A window surface is commonly BGRA — the adapter picks, not the engine — so a transient that
  copied it would hold the finished picture with red and blue swapped on the windowed path and not
  the offscreen one, and every capture would have to know which.

  And **the first version of `capture` was wrong**, caught only because the new orientation test was
  checked against a deliberately broken shader and *passed*. It read the transient on both paths,
  which meant a capture no longer observed the present pass at all — so the one shader that now runs
  on every frame had no coverage, and the screen could have been upside down with every test green.
  That is the exact gap session 8 closed and it had been quietly reopened. Fixed by having an
  offscreen backend read its **destination**, after the present pass, so the path CI and agent mode
  both use covers the whole pipeline; a windowed one reads the transient and is everything except the
  final copy. Broke the shader again afterwards to confirm the test fails. **Worth generalising: a
  new test is not evidence until it has been seen to fail.**

  **And the windowed backend can capture**, which this file had listed as waiting on post-processing.
  It was never really waiting on the effects — only on the off-screen target they need, which the
  graph brought with it.

  Verified end to end: the Vault captures through the new graph with walls, sigils, wardens, traps,
  player and score readout, right way up and unchanged in colour.

  879 tests plus 5 GPU capture tests, all four verification commands green.

  **Then post-processing, which is what the graph was built for.** An `Environment` asset the camera
  names by id, holding exposure, tonemap, grade and vignette; the cameras draw into an **HDR** target
  and a post pass brings it down, because on an 8-bit target bloom has nothing above the display
  range to isolate and tonemapping has nothing to compress. `TargetFormat` gained `Hdr16` and it cost
  a match arm — ADR 0026's format-tag argument coming true a second time.

  **The dependency direction decided where the loading lives, not preference.** An environment's file
  is a *scene* file and `amadeo-scene` sits **above** `amadeo-render`, so by I6 the renderer cannot
  parse its own asset. It owns the type and the cache; `App::load_environments` reads. The same split
  `TextureCache` already had, arrived at the same way.

  **A real defect in the state hash, found by accident and worth the detour.** Adding `environment`
  to `Camera` should have moved every golden replay and moved *none* of them. `StableHash for str`
  wrote the bytes with **no length prefix**, so an empty string contributed nothing — and worse, two
  adjacent string fields hashed as their concatenation. `Camera { target: "", environment: "x" }` and
  `Camera { target: "x", environment: "" }` are different worlds that hashed **identically**.
  Reachable from content, in shipped code, in the one mechanism the whole determinism story rests on.

  The fix goes in the `StableHash` impl rather than in `write_str` — which is exactly where the `[T]`
  impl writes its own length, and which leaves the name-hashing path and every `ComponentId` alone.
  **Diagnosed rather than obeyed**, and the control was already in hand: with the `Camera` field
  added and *before* the fix, both replays matched exactly, so all of the movement is the fix.
  `walk_and_jump.replay`, whose world holds no string fields, did not move at all — which is what a
  string-hashing change predicts and nothing else does. Both regenerated files changed four lines
  each, the checkpoints, with input streams byte-identical: ADR 0017's recorded signature for an
  identity change.

  **Two findings worth keeping beyond the fix.** *Adding a field to a reflected component breaks
  every existing scene file that authors it* — `vault.scene` had to gain `environment ""` by hand,
  because the format is strict about missing fields by design (ADR 0014's "explicitly nothing" versus
  "whoever wrote this forgot"). That cost is small now and will recur with every `Material` field;
  worth knowing before it is a surprise. And *a test is not evidence until it has been seen to fail* —
  the same lesson as the capture bug earlier in the session, learned twice in one day.

  **The Vault ships `corridor_dark.environment` and deliberately does not use it.** Its appearance is
  what M1's exit gate was judged against, and pointing the camera at a look would move
  `collect-three.replay` for a cosmetic reason — a content decision that is Justin's rather than
  something to slip in. It is a worked example and the fixture for `a_look_is_a_file.rs`, which
  drives the whole chain against real files: sidecar, catalogue, load barrier, parser, reflection,
  cache.

  Recorded as **Q23**: one environment per frame, from the camera that draws first. ADR 0031 has
  every camera compose into one image, so per-camera post needs per-camera targets — the same work as
  `Camera::target`, so the two belong together.

  900 tests plus 7 GPU capture tests, all four verification commands green, both golden replays
  regenerated and passing in separate processes.
