# ADR 0064 — A theme is named tokens, and the default look is Signage

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0021, ADR 0033, ADR 0034, ADR 0062 ·
**Settles:** `docs/04` §13's theming question, and `CLAUDE.md` §6 for the interface

## Context

ADR 0062 built the interface and ADR 0063 made it navigable. Nothing styled it: a focused item looked
exactly like an unfocused one, and every colour and size was a literal typed into a scene file.

`CLAUDE.md` §6 constrains what the default may look like more tightly than any other part of this
project — it names the references (Blender, Houdini, Ableton, Reaper, Nuke), lists the defaults to
avoid, and asks for "committed choices, not hedged neutrals". So both halves of this were put to
Justin, with four directions mocked up rather than described.

## Decision

### 1. Widgets name tokens; the theme decides what they mean

A widget says `paint: Accent` and `scale: Title` and `padding: Snug`. It does not say
`[0.81, 0.08, 0.01, 1.0]`, `52.0` and `8.0`.

That indirection is the whole of what makes a theme a theme:

- **one file changes the whole look**, rather than a find-and-replace across every scene;
- a game reskins the engine's interface without touching a widget;
- and **density can be retuned**, which matters because §6 asks for "information density over
  whitespace" and that is a judgement nobody gets right first time. It is unfixable if every gap is
  a number somebody typed.

Three token sets, deliberately small: **seven colours**, **four type steps**, **five spacing steps**.
A palette large enough to express every shade is one nobody can hold in their head, and the result is
a UI whose greys drift apart.

**One escape hatch**, because a rule with no exception gets worked around: `Paint::Custom` carries a
literal. It is for the genuine one-off — a faction colour, a damage flash — and using it for ordinary
chrome is how a theme stops working.

### 2. Padding and gap are tokens; margin is not

**Padding and gap are *density*. Margin is *placement*.**

How tight an interface feels is decided by the space inside things and between them, and that is the
knob worth being able to turn in one file. Where a HUD element sits relative to a screen corner is a
specific position, and a token would only get in the way of saying it.

This split is why `UiEdges` survives for margin alone. Before the theme they were the same type,
which made a density knob and a coordinate look identical in a scene file.

Padding is uniform on all four sides. Asymmetric padding is expressible with a child's margin, and a
four-token version is additive later.

### 3. The default look is **Signage**, and it is built in code

Bone on near-black, safety orange, **zero corner rounding anywhere**, wide letterspacing, tight
leading. The references are wayfinding and industrial signage rather than software.

Chosen from four mocked-up directions. The reasoning that decided it:

- A theme is a **file a game overrides**, so the default's job is to be *good*, not inoffensive. §6
  says as much — "allowed to look like *something* rather than like nothing".
- It is built for **Bebas Neue**, the face the engine already ships (ADR 0062), which is a condensed
  display face made for exactly this.
- The rejected-for-being-safe option ("Instrument": cool near-black, hairline rules, amber accent)
  was the most reusable and the least characterful, and "safe" is the direction §6 warns about by
  name.

**Built in code, not shipped as a file.** `TextureCache`'s argument, third instance: the last resort
must not itself be a file, because a file cannot cover the case where files are the problem. A game
with no `.theme` asset gets a complete, deliberate look rather than a placeholder.

### 4. `Theme` is one type that is both a `Component` and a `Service`

A `.theme` file is a scene file holding one `Theme`, exactly as a `.material` and an `.environment`
are (ADR 0033, ADR 0034). The live theme is a service.

`Environment` and `EnvironmentCache` are two types because the cache holds *many* by id. There is one
active theme, so a wrapper would be a layer with nothing in it.

**A `Service`, so a theme is outside the state hash** — which is worth stating rather than assuming:
two players running the same game with different themes must simulate identically. A theme changes
what a menu looks like and nothing else.

## Consequences

**`App::read_component_assets` became public.** `amadeo-ui` sits below `amadeo-scene` and cannot
parse its own asset (I6), the same bind `amadeo-render` is in for `.material` and `.environment`.
That helper already existed privately for those; a component-shaped asset is a general idea rather
than a renderer one, so a game — or a future asset kind the engine has never heard of — can use it.

**Colours are written in sRGB and converted once.** Every colour the engine draws with is linear, and
every colour a person picks is sRGB. Writing the linear values directly would make the built-in theme
a wall of numbers nobody could recognise, and `0.0044` does not read as "near-black" to anyone.
`srgb_converts_through_the_real_curve_rather_than_a_guess` pins the transfer function, because sRGB
`0x80` is **0.216** linear rather than 0.5 and a theme that assumed otherwise would be visibly washed
out beside a texture of the same value.

**Tests assert through the theme, not against pixel counts.** Four layout tests broke on the way in
because they checked literal numbers that had come from literal padding. They now ask the theme what
`Snug` means — which is the correct form regardless, since otherwise retuning the spacing scale
breaks the suite and the whole point of ADR 0064 is that retuning is cheap.

**Nothing yet draws a focused item differently.** The theme makes it *possible* — `Paint::Accent`
exists and the focus is known (ADR 0063) — and no widget consults the focus. That is the next piece,
and it is small.

## Alternatives rejected

**Colour tokens only**, leaving spacing and sizes literal. Smaller, and it gives up the half that
matters most for §6: density is what a retune is *for*, and it would have needed redoing.

**Defaults only, no tokens** — the theme supplies values a widget may override. Barely a theme: the
moment a widget states a colour it stops following the theme, so a reskin is a find-and-replace
across scene files.

**"Low Light" as the default** — warm black, dim amber, sickly green. The most atmospheric of the four
and the best fit for M3's horror slice specifically, and **wrong as an engine default**: it is
strongly one genre, and its low contrast is a real legibility problem. It is the right thing for a
game to override *with*.

**"Slate"**, a light theme. Genuinely contrarian and excellent for dense information, and a light
panel over a dark 3D scene is a bright rectangle in the player's eye — which is why games avoid it,
and actively wrong for the slice M3 is judged on.
