# ADR 0063 — Focus is an authored order, driven by named actions

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0005, ADR 0009, ADR 0019, ADR 0062 ·
**Settles:** `docs/04` §13's focus-navigation question

## Context

ADR 0062 built the interface: layout, text, and drawing. None of it is *interactive*. `docs/04` §13
has carried focus navigation as a ⚠️ since M0 with the note that it is "always an afterthought,
always painful to add later", and M3's exit gate needs a title screen and a pause menu — both of
which are things you move around in and choose from.

The obvious design is the one every tutorial shows: find the widget under the pointer, highlight it,
act when it is clicked.

## Decision

**Focus is an authored order. Navigation is driven by named input actions. Choosing raises an
event.**

### 1. Why the obvious design cannot work here

Hit-testing reads a `ComputedRect`, and ADR 0062 made layout depend on the **window size** —
deliberately, and `ComputedRect` is `DERIVED` precisely so that a game at 1920×1080 and the same game
at 1280×720 are not two different worlds.

So "which button is under the pointer" has a **different answer at different resolutions**. The
moment that answer reaches the state hash, the same inputs produce different worlds on two machines
and invariant I3 is gone — not subtly, and not in one game, but for every menu in every game built on
this engine.

That is the whole reason this ADR exists. The naive design is not merely inelegant here; it is
incompatible with the property the project is built around.

### 2. So `Focusable::order` is a number somebody writes

Navigation walks an authored order and reads no rectangle, no pointer and no screen size. Three
things follow, and all three are worth more than the convenience given up:

- **Identical at every resolution**, which is what makes a menu part of a deterministic simulation
  at all.
- **Driven by named actions** — `ui_next`, `ui_previous`, `ui_confirm` — which `InputState` already
  hashes and `amadeo-input` already records and replays. **A menu replays with no new machinery and
  no change to the replay format.** That is not a small thing: the alternative was recording pointer
  positions, which are resolution-dependent and would have made replays non-portable.
- **It works with no cursor**, on a controller or a keyboard, which is what a console-facing menu
  needs regardless.

It is also what a designer wants. Reading order and tab order are not the same thing — a "Back"
button at the top-left should usually come *last* — and a spatial rule cannot express that without a
special case.

### 3. `Focus` is hashed; everything else in `amadeo-ui` is not

The one hashed thing in the crate, and that is correct rather than inconsistent. Where the focus sits
is gameplay state: it changes only through recorded input, it does not depend on the window size, and
a save should restore the highlighted option. Layout and draw data stay outside the hash exactly as
before.

### 4. Choosing raises `UiActivated`, carrying the entity

The engine does not know what a button *means*. A game reads the event and decides — invariant I4 one
level up, the same split `games/atrium` uses for footsteps: the module knows how a menu moves, the
game knows what its buttons are for.

An event rather than a callback, for ADR 0016's reason and `amadeo-events`' own: a callback runs
another system's code at a moment nobody chose, which is hostile to reasoning about a tick.

### 5. Navigation is edge-triggered, and there is no key repeat

A held direction moves once. Key repeat — move, pause, then accelerate — is a *timing* feature, and
timing is the thing a fixed-tick deterministic system is worst at expressing: the obvious
implementation counts ticks, which is fine, and the obvious *tuning* is in milliseconds, which is
not. Left out until something needs it, and
`holding_a_direction_moves_once_rather_than_scrolling` records that as deliberate.

## Consequences

**Spatial navigation is still possible and belongs outside the deterministic zone.** "The button
visually below this one" is what a d-pad feels like, and a pointer is what a mouse is. Both are
resolution-dependent, so both belong in a *presentation-side* system that writes through the same
`Focus` resource — the same way the `Overlay` inversion works. The deterministic path underneath does
not change, and a replay records the resulting focus moves rather than the pointer that caused them.

**A game must register the event and the resource.** `App::register_event::<UiActivated>()` and a
`Focus` resource, the same wiring `SoundPlayed` needs (ADR 0061). The engine does not install them
centrally because a game with no menu should not pay for them.

**Focus falls off anything that stops being focusable** — despawned, hidden, or disabled. A stale
focus is how "confirm" activates a button that is no longer on screen, which is the pause-menu bug
where closing a menu and pressing a key triggers whatever was highlighted underneath.

**Nothing is drawn differently yet.** A focused item looks like an unfocused one until a theme says
otherwise, which is the next open question in `docs/04` §13.

## Alternatives rejected

**Hit-testing against `ComputedRect`, in the simulation.** Covered above: it puts the window size
into the state hash. `ComputedRect::contains` exists and is the right primitive for the
presentation-side layer described in the consequences — it is the *placement* of the logic that was
wrong, not the function.

**Deriving the order from the layout tree** — depth-first through the children. Resolution-independent
and tempting, and it fails the same way a spatial rule does for the "Back button last" case, while
being much harder to override: you would have to reorder the scene file to reorder the menu, which
couples navigation to visual grouping.

**A callback on the button.** Rejected in `amadeo-events` already: a send that runs another system's
code makes execution order implicit and allows reentrancy, and both are hostile to determinism and to
reading a tick.

**Recording pointer positions in the replay format.** The honest version of the naive design. It makes
replays resolution-dependent, which would mean a recording made at one window size cannot be trusted
at another — and replay-as-test is the property this whole engine is arranged around.
