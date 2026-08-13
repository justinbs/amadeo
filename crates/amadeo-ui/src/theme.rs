//! The theme: what `accent`, `body` and `snug` actually mean — ADR 0064.
//!
//! # Why widgets name tokens rather than values
//!
//! A widget says `paint: Accent` and `scale: Heading`. It does not say `[0.81, 0.08, 0.01, 1.0]` and
//! `28.0`. That indirection is the whole of what makes a theme a theme:
//!
//! - **one file changes the whole look**, rather than a find-and-replace across every scene;
//! - a game can reskin the engine's interface without touching a single widget;
//! - and density can be *retuned* — `CLAUDE.md` §6 asks for "information density over whitespace",
//!   which is a judgement nobody gets right first time and which is unfixable if every padding is a
//!   number typed into a scene file.
//!
//! An escape hatch exists, because a rule with no exception gets worked around: [`Paint::Custom`]
//! carries a literal colour. It is there for the one-off that genuinely is not part of the palette —
//! a faction colour, a damage flash — and using it for ordinary chrome is how a theme stops working.
//!
//! # The default is built in code, and cannot be missing
//!
//! `TextureCache`'s argument, third instance: the last resort must not itself be a file. A game with
//! no `.theme` asset gets [`Theme::signage`], which is a complete, deliberate look rather than a
//! placeholder — so an interface always draws as *something* somebody chose.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, Service};
use amadeo_reflect::Reflect;

/// Which colour from the palette.
///
/// Seven names, which is deliberately few. A palette large enough to express every shade is a
/// palette nobody can hold in their head, and the result is a UI whose greys drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
pub enum Paint {
    /// The base a panel sits on. The darkest thing in a dark theme.
    #[default]
    Surface,
    /// A panel one step up from the surface — a card, a dialogue box, a header bar.
    Raised,
    /// Body text and anything that must be read.
    Ink,
    /// Secondary text: captions, disabled options, the things you skim past.
    Dim,
    /// Focus, selection, and the one thing on screen asking to be looked at.
    ///
    /// **One accent, not a set.** A second accent is a second meaning nobody has defined, and the
    /// first thing that happens is that two unrelated states end up the same colour.
    Accent,
    /// Text drawn *on* the accent, where `Ink` would be unreadable.
    OnAccent,
    /// Dividers and borders.
    Rule,
    /// A literal colour, for the one-off that is genuinely not part of the palette.
    ///
    /// Linear RGBA, like every other colour in the engine.
    Custom {
        /// The colour itself.
        rgba: [f32; 4],
    },
}

/// Which step of the type scale.
///
/// A scale rather than free numbers, for the reason typographers give: sizes chosen independently
/// drift towards each other until nothing is clearly bigger than anything else, and the page loses
/// its hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum TypeScale {
    /// A game's name on a title screen. Used once per screen, or not at all.
    Title,
    /// A section: "Paused", "Options".
    Heading,
    /// Everything you actually read, and what a menu item is.
    #[default]
    Body,
    /// Small print: a hint, a version string, a key prompt.
    Caption,
}

/// Which step of the spacing scale.
///
/// **This is the density control.** Retuning these four numbers makes the whole interface tighter or
/// airier at once, which is exactly the judgement `CLAUDE.md` §6 cares about and exactly the one that
/// cannot be revisited if every gap is a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Spacing {
    /// No space at all.
    #[default]
    None,
    /// Between things that belong together — a label and its value.
    Tight,
    /// Between items in a list.
    Snug,
    /// Between groups.
    Normal,
    /// Between a group and the edge of the screen.
    Loose,
}

/// One step of the type scale: how big, and how far apart the lines are.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct TypeStep {
    /// Height in pixels.
    #[reflect(min = 1.0, max = 512.0)]
    pub size: f32,
    /// Distance between baselines, in pixels.
    ///
    /// Held beside the size rather than derived from it by a ratio, because the right ratio is not
    /// constant: a title wants tighter leading than body text, and a single multiplier gives one of
    /// them the wrong answer.
    #[reflect(min = 1.0, max = 1024.0)]
    pub line_height: f32,
}

impl TypeStep {
    /// A step at a size, with leading a given multiple of it.
    #[must_use]
    pub fn new(size: f32, leading: f32) -> Self {
        Self {
            size,
            line_height: size * leading,
        }
    }
}

/// What every token in the interface resolves to.
///
/// A [`Service`], so it is outside the state hash (ADR 0009) — which is correct and worth stating:
/// **two players running the same game with different themes must simulate identically.** A theme
/// changes what a menu looks like and nothing else.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Theme {
    /// The base a panel sits on.
    pub surface: [f32; 4],
    /// A panel one step up.
    pub raised: [f32; 4],
    /// Body text.
    pub ink: [f32; 4],
    /// Secondary text.
    pub dim: [f32; 4],
    /// Focus and selection.
    pub accent: [f32; 4],
    /// Text drawn on the accent.
    pub on_accent: [f32; 4],
    /// Dividers and borders.
    pub rule: [f32; 4],

    /// A game's name.
    pub title: TypeStep,
    /// A section heading.
    pub heading: TypeStep,
    /// Body text and menu items.
    pub body: TypeStep,
    /// Small print.
    pub caption: TypeStep,

    /// Between things that belong together, in pixels.
    pub tight: f32,
    /// Between items in a list.
    pub snug: f32,
    /// Between groups.
    pub normal: f32,
    /// Between a group and the edge.
    pub loose: f32,
}

impl Service for Theme {}

/// Also a [`Component`], which is what lets a `.theme` file exist.
///
/// # Why one type rather than two
///
/// `Environment` and `EnvironmentCache` are separate because the cache holds *many* environments by
/// id. There is one active theme, so a wrapper would be a layer with nothing in it.
///
/// So the same type is the thing a file holds and the thing the world holds: a `.theme` asset is a
/// scene file with one `Theme` in it, loaded and inserted as a service. Nothing ever puts one on an
/// entity, and if something did, layout and drawing would simply not look at it.
impl Component for Theme {}

impl Default for Theme {
    fn default() -> Self {
        Self::signage()
    }
}

impl Theme {
    /// The engine's default look: bone on black, safety orange, nothing rounded.
    ///
    /// # Why this one
    ///
    /// Chosen by Justin from four directions. `CLAUDE.md` §6 asks for "committed choices, not hedged
    /// neutrals" and for something that "looks like *something* rather than like nothing", and a
    /// theme is a *file* a game overrides — so the default's job is to be good rather than
    /// inoffensive. It is also built for Bebas Neue, the face the engine ships with, and an
    /// institutional-signage look is close to the M3 exit gate's subject matter by accident.
    ///
    /// The references are wayfinding and industrial signage rather than software: high contrast,
    /// heavy horizontals, wide letterspacing, and **zero corner rounding anywhere**.
    #[must_use]
    pub fn signage() -> Theme {
        Theme {
            // Near-black rather than black: a true zero gives the eye nothing to judge the other
            // darks against, and looks like a hole rather than a surface.
            surface: srgb(0x0e, 0x0e, 0x0e, 1.0),
            raised: srgb(0x1a, 0x1a, 0x19, 1.0),
            // Bone rather than white. Pure white on near-black is a contrast a person cannot read
            // for long, and every printed sign that has to be read in a hurry avoids it.
            ink: srgb(0xe8, 0xe2, 0xd6, 1.0),
            dim: srgb(0x6f, 0x6a, 0x60, 1.0),
            // Safety orange. The one saturated thing on screen, so it can only mean one thing.
            accent: srgb(0xe8, 0x50, 0x1f, 1.0),
            on_accent: srgb(0x0e, 0x0e, 0x0e, 1.0),
            rule: srgb(0x33, 0x31, 0x2c, 1.0),

            // Tight leading throughout, because Bebas Neue is a condensed display face with no
            // descenders to speak of — the airy leading body text wants makes it look loose.
            title: TypeStep::new(52.0, 1.0),
            heading: TypeStep::new(26.0, 1.1),
            body: TypeStep::new(19.0, 1.25),
            caption: TypeStep::new(13.0, 1.3),

            // A rough fourth-power scale. Dense on purpose: §6 asks for information density, and
            // signage is set tight.
            tight: 4.0,
            snug: 8.0,
            normal: 16.0,
            loose: 28.0,
        }
    }

    /// The colour a [`Paint`] means.
    #[must_use]
    pub fn paint(&self, paint: Paint) -> [f32; 4] {
        match paint {
            Paint::Surface => self.surface,
            Paint::Raised => self.raised,
            Paint::Ink => self.ink,
            Paint::Dim => self.dim,
            Paint::Accent => self.accent,
            Paint::OnAccent => self.on_accent,
            Paint::Rule => self.rule,
            Paint::Custom { rgba } => rgba,
        }
    }

    /// The size and leading a [`TypeScale`] means.
    #[must_use]
    pub fn scale(&self, scale: TypeScale) -> TypeStep {
        match scale {
            TypeScale::Title => self.title,
            TypeScale::Heading => self.heading,
            TypeScale::Body => self.body,
            TypeScale::Caption => self.caption,
        }
    }

    /// The number of pixels a [`Spacing`] means.
    #[must_use]
    pub fn space(&self, spacing: Spacing) -> f32 {
        match spacing {
            Spacing::None => 0.0,
            Spacing::Tight => self.tight,
            Spacing::Snug => self.snug,
            Spacing::Normal => self.normal,
            Spacing::Loose => self.loose,
        }
    }
}

/// An sRGB byte colour as the linear RGBA the engine draws with.
///
/// # Why the conversion is here rather than in the numbers
///
/// Every colour in this engine is **linear** (`Quad::color` says so), and every colour a person
/// picks — from a palette, an eyedropper, a hex code — is **sRGB**. Writing the linear values
/// directly would make the built-in theme a wall of numbers nobody could recognise or adjust, and
/// `0.0044` does not read as "near-black" to anyone.
///
/// So the theme is written in the numbers a designer would use and converted once, here. The
/// exponent is the sRGB transfer function's, not a guess: `amadeo-image` decodes textures through
/// the same curve, and getting it wrong makes UI colours that do not match a texture of the same
/// value.
fn srgb(r: u8, g: u8, b: u8, alpha: f32) -> [f32; 4] {
    [to_linear(r), to_linear(g), to_linear(b), alpha]
}

/// One channel, sRGB byte to linear.
///
/// `powf` is a transcendental and ADR 0044 bans those where they decide gameplay. This is safe for
/// `amadeo-image`'s reason and more strongly: a theme is a `Service`, so its output cannot reach the
/// state hash even in principle.
fn to_linear(byte: u8) -> f32 {
    let value = f32::from(byte) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_resolves_to_something() {
        // A token that fell through to a default would be an invisible widget, and the failure would
        // look like a layout bug rather than a missing colour.
        let theme = Theme::signage();
        for paint in [
            Paint::Surface,
            Paint::Raised,
            Paint::Ink,
            Paint::Dim,
            Paint::Accent,
            Paint::OnAccent,
            Paint::Rule,
        ] {
            let colour = theme.paint(paint);
            assert!(colour[3] > 0.0, "{paint:?} resolved to something invisible");
        }
    }

    #[test]
    fn a_custom_paint_passes_its_colour_straight_through() {
        let theme = Theme::signage();
        let odd = [0.2, 0.9, 0.4, 0.5];
        assert_eq!(theme.paint(Paint::Custom { rgba: odd }), odd);
    }

    #[test]
    fn the_scale_actually_has_a_hierarchy() {
        // Sizes chosen independently drift together until nothing is clearly bigger than anything
        // else. This is what says the scale is still a scale after somebody has retuned it.
        let theme = Theme::signage();
        assert!(theme.title.size > theme.heading.size);
        assert!(theme.heading.size > theme.body.size);
        assert!(theme.body.size > theme.caption.size);
    }

    #[test]
    fn the_spacing_steps_increase_and_start_at_nothing() {
        let theme = Theme::signage();
        assert_eq!(theme.space(Spacing::None), 0.0);
        assert!(theme.space(Spacing::Tight) < theme.space(Spacing::Snug));
        assert!(theme.space(Spacing::Snug) < theme.space(Spacing::Normal));
        assert!(theme.space(Spacing::Normal) < theme.space(Spacing::Loose));
    }

    #[test]
    fn srgb_converts_through_the_real_curve_rather_than_a_guess() {
        // Mid-grey is the value that catches a linear pass-through: sRGB 0x80 is *not* 0.5 linear,
        // it is about 0.216, and a theme that treated it as 0.5 would be visibly washed out beside
        // a texture of the same colour.
        let grey = srgb(0x80, 0x80, 0x80, 1.0);
        assert!(
            (grey[0] - 0.2158).abs() < 1e-3,
            "sRGB 0x80 should be ~0.216 linear, got {}",
            grey[0]
        );

        // The ends are exact, which the piecewise curve makes easy to get wrong.
        assert_eq!(srgb(0x00, 0x00, 0x00, 1.0)[0], 0.0);
        assert!((srgb(0xff, 0xff, 0xff, 1.0)[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_default_is_the_chosen_look_rather_than_a_placeholder() {
        // `TextureCache`'s argument, third instance: the last resort must not itself be a file. A
        // game with no `.theme` asset still gets something somebody designed.
        assert_eq!(Theme::default(), Theme::signage());
        // Signage means high contrast: the ink has to be far from the surface or nothing is legible.
        let theme = Theme::signage();
        assert!(theme.ink[0] - theme.surface[0] > 0.5);
    }

    #[test]
    fn the_theme_round_trips_through_the_value_tree() {
        // Invariant I8, and the thing that makes a `.theme` file possible at all.
        let mut registry = amadeo_reflect::TypeRegistry::new();
        registry.register::<Theme>().expect("registers");

        let theme = Theme::signage();
        assert_eq!(
            Theme::from_value(&theme.to_value()).expect("round trips"),
            theme
        );
    }
}
