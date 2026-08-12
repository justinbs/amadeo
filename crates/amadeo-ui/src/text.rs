//! Text: from a string and a font id to positioned glyphs — ADR 0062.
//!
//! # Why this is a *layout* problem and not a font-rendering one
//!
//! The tempting shape is "rasterise a `.ttf` into an atlas and draw quads", and it works right up
//! until the text is not English. A line break in Thai is not where a space is. An Arabic glyph's
//! form depends on its neighbours. A line mixing Hebrew and English is not laid out left to right,
//! and the *same* string reversed is not the same line. None of that is a rendering detail; all of
//! it decides where a glyph goes.
//!
//! ADR 0062 chose `cosmic-text` for exactly this, and the consequence is that **measuring a string
//! is shaping it**. There is no cheap `width_of(text)`; there is a shaped buffer, and its width.
//!
//! # A game ships its fonts
//!
//! `cosmic-text`'s default features read the operating system's font database. That is turned off in
//! `Cargo.toml`, deliberately: a game whose text falls back to whatever happens to be installed looks
//! different on every machine, and on a machine with nothing suitable it looks like nothing at all.
//! A font is an asset with an id, like a texture or a sound.
//!
//! # No cosmic-text type crosses this module
//!
//! ADR 0036 §4, for the fourth time. [`ShapedText`] and [`PositionedGlyph`] are plain data, so the
//! choice of shaper stays reversible and nothing above here learns a foreign vocabulary.

use amadeo_assets::Assets;
use amadeo_ecs::Service;
use std::collections::BTreeMap;

/// Why an id could not be turned into a usable font.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FontFailure {
    /// The asset system has no bytes for this id.
    #[error(
        "no bytes are loaded for font `{id}`, so nothing using it can be drawn.\n\
         Check that a scene declares it in its `assets` block, and run `amadeo assets` to see \
         whether the file behind it was readable"
    )]
    NotLoaded {
        /// The id that was asked for.
        id: String,
    },

    /// The bytes are there but no font could be read from them.
    #[error("font `{id}` could not be read: {reason}")]
    Unreadable {
        /// The id that was asked for.
        id: String,
        /// What went wrong.
        reason: String,
    },
}

/// One glyph, placed.
///
/// Coordinates are in **pixels relative to the top-left of the text box**, so a caller adds the
/// box's own position and nothing here needs to know where on the screen it ended up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    /// The shaper's identifier for this glyph within its font. **Not a character** — one character
    /// can become several glyphs and several characters can become one, which is the whole reason
    /// shaping exists.
    pub glyph: u16,
    /// Which font it came from, as an index into [`ShapedText::fonts`].
    ///
    /// Not always the font that was asked for: fallback picks another when the requested one has no
    /// glyph for a character, which is what stops missing characters becoming empty boxes.
    pub font: usize,
    /// Left edge, in pixels from the left of the text box.
    pub left: f32,
    /// Baseline, in pixels down from the top of the text box.
    ///
    /// **The baseline, not the top of the glyph.** Aligning glyphs by their tops makes every letter
    /// with a descender sit wrong, which reads as text that wobbles.
    pub baseline: f32,
    /// The size this glyph was shaped at, in pixels.
    pub size: f32,
    /// Where its pixels are in the glyph atlas, and how they sit against the pen.
    ///
    /// `None` for a glyph with nothing to draw — a space — and also when the atlas is full, which
    /// [`FontCache::atlas_is_full`] reports.
    pub image: Option<crate::GlyphImage>,
}

/// A string, shaped and laid out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapedText {
    /// Every glyph, in visual order.
    pub glyphs: Vec<PositionedGlyph>,
    /// How wide the longest line came out, in pixels.
    pub width: f32,
    /// How tall the whole block came out, in pixels.
    pub height: f32,
    /// How many lines it broke into. One for a string with no wrapping and no newlines.
    pub lines: usize,
    /// The fonts actually used, by asset id, in the order [`PositionedGlyph::font`] indexes them.
    ///
    /// Usually one entry. More than one means fallback happened, which is worth being able to see:
    /// a game whose text is silently coming from a different font is a game whose look drifts.
    pub fonts: Vec<String>,
}

/// Loaded fonts, and the shaper that owns them.
///
/// A [`Service`]: text machinery, never simulation state. Shaping is a pure function of the string,
/// the font and the width, so nothing it does can reach the state hash — but it holds caches, and
/// ADR 0009 is what keeps those structurally out of the way.
pub struct FontCache {
    /// `cosmic-text`'s shaper. **Send + Sync**, checked with a failing control before this was
    /// designed around it — see Q12, which has now been wrong about two libraries in a row.
    system: cosmic_text::FontSystem,
    /// Asset id to the shaper's own font id, so a `Text` naming an id can be shaped.
    loaded: BTreeMap<String, cosmic_text::fontdb::ID>,
    /// Ids that would not load, and why.
    failures: BTreeMap<String, FontFailure>,
    /// The rasteriser's own cache of glyph bitmaps.
    swash: cosmic_text::SwashCache,
    /// Every glyph drawn so far, packed into one texture.
    atlas: crate::GlyphAtlas,
}

impl Service for FontCache {}

/// Hand-written because `FontSystem` is not `Debug` and a `Service` must be.
impl std::fmt::Debug for FontCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FontCache")
            .field("loaded", &self.loaded.len())
            .field("failures", &self.failures.len())
            .finish()
    }
}

impl Default for FontCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FontCache {
    /// A cache with **no fonts at all**, not even the system's.
    ///
    /// Deliberately empty: see the module docs. A game ships its fonts, so an empty database is the
    /// honest starting point and a missing font is a reported failure rather than a silent
    /// substitution that looks fine on the developer's machine.
    #[must_use]
    pub fn new() -> FontCache {
        FontCache {
            // An empty `fontdb::Database`. `FontSystem::new` would scan the operating system.
            system: cosmic_text::FontSystem::new_with_locale_and_db(
                "en-US".to_string(),
                cosmic_text::fontdb::Database::new(),
            ),
            loaded: BTreeMap::new(),
            failures: BTreeMap::new(),
            swash: cosmic_text::SwashCache::new(),
            atlas: crate::GlyphAtlas::new(),
        }
    }

    /// The glyph atlas, for handing to `TextureCache::insert_decoded` under
    /// [`GLYPH_ATLAS_ID`](crate::GLYPH_ATLAS_ID).
    ///
    /// It changes whenever a glyph is seen for the first time, so a draw pass re-uploads when
    /// [`FontCache::atlas_revision`] moves rather than every frame.
    #[must_use]
    pub fn atlas(&self) -> &crate::GlyphAtlas {
        &self.atlas
    }

    /// How many distinct glyphs have been rasterised.
    ///
    /// **Doubles as the atlas's revision**, which is what it is for: the texture only ever gains
    /// entries, so a count that has not moved means a texture that has not changed, and a draw pass
    /// can skip re-uploading a megabyte.
    #[must_use]
    pub fn atlas_revision(&self) -> usize {
        self.atlas.len()
    }

    /// Whether a glyph has been refused for want of room in the atlas.
    #[must_use]
    pub fn atlas_is_full(&self) -> bool {
        self.atlas.is_full()
    }

    /// Loads a font from bytes under an id, bypassing the asset system.
    ///
    /// **For tests and for a font a program produced**, which is `TextureCache::insert_decoded`'s
    /// role one asset kind along. The ordinary path is [`FontCache::ensure`], which reads bytes the
    /// asset system loaded — a game names a font by declared id (ADR 0020), and this is not a way
    /// around that.
    pub fn insert_font(&mut self, id: &str, bytes: &[u8]) {
        let ids = self
            .system
            .db_mut()
            .load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
                bytes.to_vec(),
            )));
        assert!(!ids.is_empty(), "the test font should parse");
        self.loaded.insert(id.to_string(), ids[0]);
    }

    /// Loads `id` if it is not loaded, reading its bytes from `assets`.
    ///
    /// Cheap to call repeatedly: an id already loaded, or already failed, returns immediately.
    pub fn ensure(&mut self, id: &str, assets: &Assets) {
        if self.loaded.contains_key(id) || self.failures.contains_key(id) {
            return;
        }

        let Some(asset) = assets.store.get(id) else {
            self.failures.insert(
                id.to_string(),
                FontFailure::NotLoaded { id: id.to_string() },
            );
            return;
        };

        let ids = self
            .system
            .db_mut()
            .load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
                asset.bytes.clone(),
            )));

        // A font *file* can hold several faces (a collection), and an empty result means it held
        // none — which is what a `.png` renamed to `.ttf` looks like from here.
        match ids.first() {
            Some(face) => {
                self.loaded.insert(id.to_string(), *face);
            }
            None => {
                self.failures.insert(
                    id.to_string(),
                    FontFailure::Unreadable {
                        id: id.to_string(),
                        reason: "the file contains no font faces".to_string(),
                    },
                );
            }
        }
    }

    /// Whether this id is loaded and usable.
    #[must_use]
    pub fn is_loaded(&self, id: &str) -> bool {
        self.loaded.contains_key(id)
    }

    /// How many fonts are loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.loaded.len()
    }

    /// Whether nothing is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }

    /// Every id that would not load, and why. In id order.
    pub fn failures(&self) -> impl Iterator<Item = (&str, &FontFailure)> {
        self.failures
            .iter()
            .map(|(id, failure)| (id.as_str(), failure))
    }

    /// Whether anything failed to load.
    #[must_use]
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    /// Shapes `content` in the font named by `font`, wrapped to `width` pixels.
    ///
    /// `width` of `None` means no wrapping: the text is one line however long it is, which is what a
    /// label wants and what a paragraph does not.
    ///
    /// Returns an empty [`ShapedText`] when the font is not loaded — **not an error and not a
    /// substitute font.** ADR 0060's rule applied a third time: a missing asset produces a report
    /// plus the most honest possible output, and for text that is nothing, because a wrong typeface
    /// silently replacing the right one is how a game's look drifts without anybody noticing.
    pub fn shape(
        &mut self,
        content: &str,
        font: &str,
        size: f32,
        line_height: f32,
        width: Option<f32>,
    ) -> ShapedText {
        if !self.loaded.contains_key(font) || content.is_empty() {
            return ShapedText::default();
        }

        let metrics = cosmic_text::Metrics::new(size.max(1.0), line_height.max(1.0));
        let mut buffer = cosmic_text::Buffer::new(&mut self.system, metrics);
        // `None` height means "as tall as it needs to be". Constraining it would *clip* rather than
        // shrink, and a label that silently loses its last line is worse than one that overflows.
        buffer.set_size(width, None);

        // Named by *family*, which is how a shaper refers to a font — the asset id is ours and means
        // nothing to it. Looked up from the face we loaded, so an asset id maps to the family its
        // file actually declares rather than to a name somebody guessed.
        let family = self.family_of(font);
        let attrs = cosmic_text::Attrs::new().family(cosmic_text::Family::Name(&family));
        // Alignment is `None` — start-aligned — because *where a line sits in its box* is this
        // engine's layout problem, not the shaper's. `UiNode::align_children` already answers it,
        // and two systems both aligning text is how text ends up centred twice.
        buffer.set_text(content, &attrs, cosmic_text::Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.system, false);

        self.collect(&buffer, size)
    }

    /// The family name of a loaded asset id, for handing to the shaper.
    fn family_of(&mut self, id: &str) -> String {
        let Some(face_id) = self.loaded.get(id).copied() else {
            return String::new();
        };
        self.system
            .db()
            .face(face_id)
            .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
            .unwrap_or_default()
    }

    /// Turns a shaped buffer into plain data, rasterising anything not yet in the atlas.
    ///
    /// # Why measuring also rasterises
    ///
    /// It would be tidier to separate them, and it would need `PositionedGlyph` to carry the
    /// shaper's own cache key so a second pass could look the glyph up — which is exactly the
    /// foreign type ADR 0036 §4 keeps out. A glyph that is measured is very nearly always a glyph
    /// that is drawn, so the split would buy a boundary leak and almost no work saved.
    fn collect(&mut self, buffer: &cosmic_text::Buffer, size: f32) -> ShapedText {
        let mut shaped = ShapedText::default();
        let mut fonts: Vec<cosmic_text::fontdb::ID> = Vec::new();

        for run in buffer.layout_runs() {
            shaped.lines += 1;
            shaped.width = shaped.width.max(run.line_w);
            shaped.height = shaped.height.max(run.line_top + run.line_height);

            for glyph in run.glyphs {
                // Deduplicated by identity, so `fonts` is short and the index is stable within one
                // shaping. A string in one font produces one entry.
                let font = match fonts.iter().position(|id| *id == glyph.font_id) {
                    Some(index) => index,
                    None => {
                        fonts.push(glyph.font_id);
                        fonts.len() - 1
                    }
                };

                let pixels = if glyph.font_size > 0.0 {
                    glyph.font_size
                } else {
                    size
                };

                // Rasterised at the position the shaper would draw it at, which is what decides the
                // sub-pixel rounding. Asking at `(0, 0)` and moving it afterwards is what makes text
                // look slightly soft.
                let physical = glyph.physical((0.0, 0.0), 1.0);
                // Two disjoint fields borrowed at once, which the borrow checker allows because it
                // tracks them separately — worth knowing, since the same call written through a
                // helper method on `self` would not compile.
                let image = self.atlas.ensure_glyph(
                    &mut self.system,
                    &mut self.swash,
                    &physical,
                    // The shaper's font id is opaque to us, so it is hashed into the atlas key by
                    // its own bits rather than by anything meaningful.
                    font as u64,
                    glyph.glyph_id,
                    pixels,
                );

                shaped.glyphs.push(PositionedGlyph {
                    glyph: glyph.glyph_id,
                    font,
                    left: glyph.x,
                    // `line_y` is the baseline, which is what a glyph is positioned against. Using
                    // `line_top` instead would sit every descender wrong.
                    baseline: run.line_y,
                    size: pixels,
                    // A zero-sized entry — a space — is reported as nothing to draw rather than as a
                    // zero-by-zero sprite, so a draw pass has one thing to check instead of two.
                    image: image.filter(|image| image.width > 0.0 && image.height > 0.0),
                });
            }
        }

        // Mapped after the loop, because this borrows `self` shared and the loop above borrows three
        // of its fields mutably.
        shaped.fonts = fonts
            .into_iter()
            .map(|face| self.asset_id_of(face))
            .collect();
        shaped
    }

    /// The asset id a shaper font id came from, or its family name when it came from fallback.
    fn asset_id_of(&self, face: cosmic_text::fontdb::ID) -> String {
        for (id, loaded) in &self.loaded {
            if *loaded == face {
                return id.clone();
            }
        }
        // Reached only when fallback chose a face we did not load under an id. Reporting the family
        // is more use than reporting nothing, because "which font is my text actually in" is the
        // question this field exists to answer.
        self.system
            .db()
            .face(face)
            .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
            .unwrap_or_else(|| "unknown".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real font, embedded so the tests need no fixture on disk and no system fonts.
    ///
    /// Built here rather than shipped: writing a minimal but *valid* TrueType file is a known,
    /// bounded thing, and it keeps the test suite from depending on a licence or a download.
    fn a_font() -> Vec<u8> {
        crate::test_font::single_glyph_font()
    }

    fn cache_with_a_font() -> FontCache {
        let mut cache = FontCache::new();
        cache.insert_font("test", &a_font());
        cache
    }

    #[test]
    fn a_new_cache_holds_no_fonts_at_all() {
        // **Including the system's.** A game whose text falls back to whatever is installed looks
        // different on every machine, and correct on the one it was developed on.
        let cache = FontCache::new();
        assert!(cache.is_empty());
        assert!(!cache.is_loaded("sans-serif"));
    }

    #[test]
    fn an_unknown_font_shapes_to_nothing_and_is_reported() {
        // ADR 0060's rule a third time: a report plus the most honest output, which for text is
        // nothing. A substitute typeface silently replacing the right one is how a look drifts.
        let mut cache = FontCache::new();
        let shaped = cache.shape("hello", "missing", 16.0, 20.0, None);

        assert!(shaped.glyphs.is_empty());
        assert_eq!(shaped.width, 0.0);
    }

    #[test]
    fn shaping_a_string_produces_a_glyph_for_each_character() {
        let mut cache = cache_with_a_font();
        let shaped = cache.shape("AAA", "test", 16.0, 20.0, None);

        assert_eq!(shaped.glyphs.len(), 3);
        assert_eq!(shaped.lines, 1);
        assert!(shaped.width > 0.0, "three glyphs should have a width");
        assert_eq!(shaped.fonts, vec!["test".to_string()]);
    }

    #[test]
    fn glyphs_advance_along_the_line_rather_than_stacking() {
        // The failure this catches is a shaper wired up so every glyph lands at x = 0, which draws
        // as a single smudged character and is entirely plausible from the code.
        let mut cache = cache_with_a_font();
        let shaped = cache.shape("AAA", "test", 16.0, 20.0, None);

        assert!(shaped.glyphs[1].left > shaped.glyphs[0].left);
        assert!(shaped.glyphs[2].left > shaped.glyphs[1].left);
        // Same line, so the same baseline.
        assert_eq!(shaped.glyphs[0].baseline, shaped.glyphs[2].baseline);
    }

    #[test]
    fn a_newline_makes_a_second_line_further_down() {
        let mut cache = cache_with_a_font();
        let shaped = cache.shape("A\nA", "test", 16.0, 20.0, None);

        assert_eq!(shaped.lines, 2);
        assert_eq!(shaped.glyphs.len(), 2);
        // +Y is **down**, so the second line's baseline is the larger number. Getting this backwards
        // draws paragraphs upside down and is the same flip the layout module warns about.
        assert!(shaped.glyphs[1].baseline > shaped.glyphs[0].baseline);
        assert!(shaped.height >= 40.0, "two lines at 20 each");
    }

    #[test]
    fn a_bigger_size_makes_a_wider_string() {
        // Cheap, and it catches the size being dropped on the way through — which would look right
        // in every test that only counts glyphs.
        let mut cache = cache_with_a_font();
        let small = cache.shape("AAA", "test", 12.0, 16.0, None);
        let large = cache.shape("AAA", "test", 48.0, 56.0, None);

        assert!(
            large.width > small.width * 2.0,
            "48pt should be much wider than 12pt: {} vs {}",
            large.width,
            small.width
        );
        assert!(large.glyphs[0].size > small.glyphs[0].size);
    }

    #[test]
    fn shaping_rasterises_its_glyphs_into_the_atlas() {
        let mut cache = cache_with_a_font();
        assert_eq!(cache.atlas_revision(), 0);

        let shaped = cache.shape("AAA", "test", 32.0, 40.0, None);

        // Three glyphs, but one *distinct* glyph: the atlas is keyed on the glyph, not the run.
        assert_eq!(cache.atlas_revision(), 1);
        assert!(!cache.atlas_is_full());

        let image = shaped.glyphs[0].image.expect("the box glyph has pixels");
        // The test font's box is 400x700 units at 1000 per em, so at 32 px it is about 13 x 22.
        assert!(
            (10.0..18.0).contains(&image.width),
            "unexpected width {}",
            image.width
        );
        assert!(
            (18.0..26.0).contains(&image.height),
            "unexpected height {}",
            image.height
        );
        // Every glyph in the run points at the same atlas entry.
        assert_eq!(shaped.glyphs[2].image, shaped.glyphs[0].image);
    }

    #[test]
    fn the_atlas_really_contains_the_glyph_and_not_just_a_reservation() {
        // **The "look at the output" check.** Everything above would pass against a packer that
        // allocated regions and copied no pixels, and the symptom of that is invisible text — which
        // is indistinguishable from a missing font, a wrong colour, or a layout bug.
        let mut cache = cache_with_a_font();
        let shaped = cache.shape("A", "test", 32.0, 40.0, None);
        let image = shaped.glyphs[0].image.expect("pixels");

        let atlas = cache.atlas().texture();
        let size = atlas.width as f32;
        // The middle of the region, which for a solid box is solid.
        let x = ((image.region[0] + image.region[2] * 0.5) * size) as u32;
        let y = ((image.region[1] + image.region[3] * 0.5) * size) as u32;
        let alpha = atlas.pixels[((y * atlas.width + x) * 4 + 3) as usize];

        assert!(
            alpha > 200,
            "the middle of a solid box should be nearly opaque, got {alpha}"
        );
        // And the colour channels stayed white, so `Sprite::color` can tint it.
        assert_eq!(atlas.pixels[((y * atlas.width + x) * 4) as usize], 255);
    }

    #[test]
    fn shaping_the_same_string_twice_rasterises_nothing_new() {
        // The atlas only ever gains entries, so a revision that has not moved means a texture that
        // has not changed — which is what lets a draw pass skip re-uploading a megabyte every frame.
        let mut cache = cache_with_a_font();
        cache.shape("AAA", "test", 32.0, 40.0, None);
        let after_first = cache.atlas_revision();

        cache.shape("AAAAAA", "test", 32.0, 40.0, None);

        assert_eq!(cache.atlas_revision(), after_first);
    }

    #[test]
    fn the_same_glyph_at_a_different_size_is_rasterised_again() {
        // Not an optimisation missed — a glyph at 12 px is not a scaled copy of one at 48 px, and
        // reusing one for the other is how text goes blurry at one size and crisp at another.
        let mut cache = cache_with_a_font();
        cache.shape("A", "test", 12.0, 16.0, None);
        cache.shape("A", "test", 48.0, 56.0, None);

        assert_eq!(cache.atlas_revision(), 2);
    }

    #[test]
    fn an_empty_string_shapes_to_nothing_without_failing() {
        let mut cache = cache_with_a_font();
        let shaped = cache.shape("", "test", 16.0, 20.0, None);
        assert!(shaped.glyphs.is_empty());
        assert_eq!(shaped.lines, 0);
    }

    #[test]
    fn a_font_that_is_not_a_font_is_reported_by_id() {
        let mut cache = FontCache::new();
        let mut assets = Assets::from_catalogue(amadeo_assets::AssetCatalogue::new());
        let root = std::env::temp_dir().join(format!(
            "amadeo-font-cache-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir");
        std::fs::write(root.join("broken.ttf"), b"this is not a font").expect("write");
        assets
            .catalogue
            .insert(
                amadeo_assets::Sidecar::new("broken"),
                std::path::Path::new("broken.ttf"),
            )
            .expect("a distinct id");
        let Assets {
            catalogue, store, ..
        } = &mut assets;
        store.load_all(catalogue, &root, ["broken"]);

        cache.ensure("broken", &assets);
        let _ = std::fs::remove_dir_all(&root);

        assert!(!cache.is_loaded("broken"));
        assert!(cache.has_failures());
        let (id, failure) = cache.failures().next().expect("one failure");
        assert_eq!(id, "broken");
        assert!(format!("{failure}").contains("broken"));
    }
}
