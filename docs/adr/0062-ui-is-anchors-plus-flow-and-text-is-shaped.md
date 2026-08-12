# ADR 0062 — Game UI is anchors plus flow, and text is properly shaped

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0014, ADR 0018, ADR 0023, ADR 0031 ·
**Settles:** `docs/04` §13's layout and text questions

## Context

`amadeo-ui` is the largest unbuilt thing between the engine and M3's exit gate. Gate item 1 is
*"title screen → playable loop → lose state → win state → pause → save → quit → resume"*, and four of
those seven are user interface.

`docs/04` §13 carried four ⚠️ marks: the layout model, retained versus immediate mode, focus
navigation, and text rendering. Two of them decide the shape of everything else and are expensive to
undo, so they were put to Justin with the alternatives researched.

**Retained versus immediate is not among them**, because it was already settled by an earlier
argument rather than by preference: an immediate-mode widget exists only for the duration of a
function call, so it is invisible to introspection — which breaks invariant I5 and the whole
observability story — and it cannot be authored in a scene file, which breaks I1. Game UI is
**retained**. `egui` remains the *editor's* UI and is not this.

## Decision

### 1. Layout is anchors for placement, plus flow containers for children

**Anchors** place a node within its parent: which edges it is pinned to, with what margin, so
"top-right with a 20 px inset, scaling with the screen" is a property rather than arithmetic.

**Flow containers** — row, column, grid — lay out a variable number of children automatically, so a
menu with five buttons and a menu with six are the same authored thing.

**This is not a compromise between two models; it is the recognition that there are two problems.**
A HUD overlay is placement — a health bar belongs at a screen corner regardless of what else exists.
A menu is flow — the buttons take their positions from each other. A single model does one of these
well and the other badly.

The evidence is convergence: **Unity** ships `RectTransform` anchors *and* Layout Groups; **Unreal**
ships a Canvas Panel with anchor slots *and* Box/Grid panels; **Godot** ships anchors on every
`Control` *and* Containers. Three engines with unrelated lineages arrived at the same pair.

**Written here rather than taken from `taffy`.** Taffy is a good crate and Bevy, Zed and Dioxus use
it, but flexbox is a *document* layout specification and most of it is machinery a HUD never touches.
Adopting it would put a model we did not design at the centre of the UI system and require a
translation layer to keep its types off our side of the boundary (ADR 0036 §4). The subset that
matters here — main axis, cross alignment, gap, grow — is small enough to write, to read, and to
explain in `docs/07`, which the legibility requirement in `CLAUDE.md` §6 makes a real consideration
rather than a preference.

**Layout output is derived and stays out of the state hash**, exactly as `GlobalTransform` does
(ADR 0019). What is authored is the style; where a widget ended up is computed from it and the
screen size, and a game whose window is a different size must not have a different state hash.

### 2. Text is shaped properly, with `cosmic-text`

Full shaping: glyph shaping, bidirectional text, line breaking, and font fallback.

**This overrides the recommendation put to Justin, and the override is the interesting part.** The
recommendation was the lighter option — rasterise a `.ttf` into a glyph atlas with `fontdue` or
`ab_glyph` and draw quads through the existing sprite batcher — on the grounds that M3's exit gate is
an English horror slice and complex-script shaping is a later problem.

He chose the complete option, and he was right to, because the argument for the lighter one was
*scope* rather than *engineering*. `CLAUDE.md` §5 has said since session 6 that he would rather have
a complete engine than one that accumulates problems, and "we will do localisation later" is exactly
the deferred problem that shape describes. Retro-fitting shaping means revisiting every place that
measures, wraps, or positions a string.

The practical consequence is that **text stops being a font-rendering feature and becomes a text
*layout* feature**, which is the honest description: a line break in Thai is not where a space is, an
Arabic glyph's form depends on its neighbours, and a mixed Hebrew–English line is not laid out left
to right. A system that got any of those wrong would be one that could only ever draw English.

`cosmic-text`'s types do not cross the boundary — ADR 0036 §4 again, third application — so the
choice stays reversible.

## Consequences

**`amadeo-ui` sits above `amadeo-render` and below `amadeo-scene`**, which the dependency order in
`CLAUDE.md` §4 already reserves for it. It needs the renderer to draw and the transform layer to
place; a scene file authoring a menu is `amadeo-scene`'s business, one level up.

**A widget is an entity with components**, like everything else. That is what makes `world.query`,
`describe`, and a scene file work on UI for free, and it is ADR 0031's argument for the camera
applied again.

**Two coordinate spaces meet here and the seam is a real hazard.** World space has +Y up (ADR 0018);
screen space has +Y down with the origin top-left, which `render.describe` already flips once
deliberately. UI is authored in screen space. Getting the flip wrong produces layouts that are
plausible and upside down.

**Drawing reuses the sprite path.** A glyph is a textured quad and a panel is a quad, so `SortOrder`
already stacks UI over the world (ADR 0018 anticipated this — UI over the world is a higher sort
order, not a separate hierarchy). No new pipeline.

**Focus navigation is still open** and is deliberately not decided here. It is the third ⚠️ in
`docs/04` §13, it is always painful to add later, and it depends on the layout tree existing first.

**A font becomes an asset**, with the whole `.ama-meta` toolchain applying to it for nothing. It is
the first asset whose *decoded* form is not pixels or samples.

## Alternatives rejected

**Anchors only, flow later.** Smallest step, and it fails immediately: a menu with a variable number
of buttons is M3's exit gate, and hand-computed positions are exactly what a layout system exists to
remove. Retro-fitting flow into an anchors-only tree means revisiting every widget.

**A constraint solver (Cassowary), as iOS AutoLayout uses.** The most expressive option and
essentially no game engine does it. Constraints go over- or under-determined and the failure mode is
an unreadable solver dump rather than a wrong pixel — and it is hard to hand-author in text, which
invariant I1 requires.

**A hand-authored bitmap font**, in the style of `games/vault`'s `.pix` sprites. No dependency and
fully diffable, but one fixed size and one style, and `CLAUDE.md` §6 asks for a typeface picked
deliberately — which a programmer's hand-drawn character set is the opposite of.

**Immediate mode.** Covered above: invisible to introspection, unauthorable in a file. Not a
close call, and it is worth writing down because immediate mode is what `egui` is and the editor
uses `egui`, so the temptation to reuse it will recur.
