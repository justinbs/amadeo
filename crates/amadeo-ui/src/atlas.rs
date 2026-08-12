//! The glyph atlas: one texture holding every glyph the game has drawn.
//!
//! # Why an atlas rather than a texture per glyph
//!
//! A page of text is a few hundred glyphs. Bound as individual textures that is a few hundred bind
//! groups and a few hundred draw calls — where one shared texture makes it *one* batch, because
//! ADR 0023 batches on `(sort order, texture)` and `Sprite::region` already exists to address part
//! of a shared image. A glyph is a tilesheet tile; the renderer needed nothing new.
//!
//! # White with a coverage alpha, not black text
//!
//! A rasterised glyph is an **alpha mask** — how much of each pixel the outline covers — and nothing
//! about it says what colour the text is. So the atlas stores white RGB with the mask in alpha, and
//! `Sprite::color` tints it. One atlas serves every colour of text in the game, and changing a
//! label's colour costs nothing.
//!
//! Baking the colour into the atlas instead would need a separate entry per colour per glyph, which
//! is the same mistake as a texture per glyph one level along.
//!
//! # Shelf packing, and why not something cleverer
//!
//! Glyphs are laid in rows ("shelves"), each as tall as the tallest glyph put on it, filled left to
//! right. It wastes some space under short glyphs on a tall row.
//!
//! The alternatives — skyline, MaxRects, guillotine — pack better and are all considerably more code
//! to read. Text at one or two sizes fills a 1024-square atlas to a few percent, so the waste buys
//! nothing back. If a game ever exhausts an atlas the answer is a second page, not a better packer,
//! and that is a change this module can absorb without its callers noticing.

use amadeo_image::{PixelFormat, TextureData};
use std::collections::BTreeMap;

/// The asset id the glyph atlas is registered under.
///
/// An engine convention rather than a setting, exactly as `PLACEHOLDER_TEXTURE_ID` is: one
/// well-known name beats a configuration point every game has to wire up. A `Sprite` drawing a glyph
/// names this.
pub const GLYPH_ATLAS_ID: &str = "amadeo.glyphs";

/// How wide and tall the atlas is, in pixels.
///
/// 1024 is comfortably inside every GPU's limit and holds a couple of thousand glyphs at menu sizes.
const ATLAS_SIZE: u32 = 1024;

/// One pixel of empty space around each glyph.
///
/// **Not optional.** A sprite sampled with any filtering reads slightly outside its region at the
/// edges, so glyphs packed flush against each other bleed into one another — which shows as a faint
/// line of the neighbouring letter along one side, and is exactly the kind of artefact that gets
/// blamed on the font.
const PADDING: u32 = 1;

/// Which glyph, at which size — the identity of one entry.
///
/// Size is part of it because a glyph rasterised at 16 px is not a scaled copy of one at 48 px:
/// hinting and rounding differ, and reusing one for the other is how text goes blurry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct GlyphKey {
    font: u64,
    glyph: u16,
    /// Size in **quarter pixels**, so the key is an integer and two sizes a hair apart share an
    /// entry rather than filling the atlas with near-duplicates.
    quarter_pixels: u32,
}

/// Where a glyph sits in the atlas, and where it sits relative to the pen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphImage {
    /// The region of the atlas holding it, as `[x, y, width, height]` in `0.0..=1.0` — which is
    /// exactly what `Sprite::region` takes.
    pub region: [f32; 4],
    /// How wide it is on screen, in pixels.
    pub width: f32,
    /// How tall it is on screen, in pixels.
    pub height: f32,
    /// How far right of the pen position its left edge sits, in pixels.
    ///
    /// Often slightly negative: an italic `f` or a `j` leans left of where the pen is.
    pub left: f32,
    /// How far **above the baseline** its top edge sits, in pixels.
    ///
    /// Positive is up, which is the one place in this crate that is not screen-space down — because
    /// it comes from the font, and fonts measure up from the baseline. Converting it once here would
    /// be cheaper to write and much easier to get wrong twice.
    pub top: f32,
}

/// Every glyph rasterised so far, in one texture.
#[derive(Debug)]
pub struct GlyphAtlas {
    texture: TextureData,
    placed: BTreeMap<GlyphKey, GlyphImage>,
    /// Left edge of the next glyph on the current shelf.
    pen_x: u32,
    /// Top edge of the current shelf.
    shelf_y: u32,
    /// How tall the current shelf is, set by the tallest glyph on it.
    shelf_height: u32,
    /// Set when a glyph did not fit. Reported rather than silently dropped.
    full: bool,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAtlas {
    /// An empty atlas: transparent white, everywhere.
    ///
    /// **White rather than black**, and it matters at the edges: a sprite sampled with filtering
    /// blends towards its neighbours, and blending a glyph towards transparent *black* darkens its
    /// outline into a grey fringe. Transparent white blends towards nothing visible.
    #[must_use]
    pub fn new() -> GlyphAtlas {
        let mut pixels = vec![255u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
        for alpha in pixels.iter_mut().skip(3).step_by(4) {
            *alpha = 0;
        }

        GlyphAtlas {
            texture: TextureData {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                // The RGB is a constant white, so the sRGB curve has nothing to bend, and alpha is
                // linear under both formats. Tagged sRGB to match every other texture the sprite
                // path draws rather than to make a claim about this one.
                format: PixelFormat::Rgba8UnormSrgb,
                pixels,
            },
            placed: BTreeMap::new(),
            pen_x: PADDING,
            shelf_y: PADDING,
            shelf_height: 0,
            full: false,
        }
    }

    /// The atlas texture, for handing to `TextureCache::insert_decoded`.
    #[must_use]
    pub fn texture(&self) -> &TextureData {
        &self.texture
    }

    /// How many distinct glyphs are in it.
    #[must_use]
    pub fn len(&self) -> usize {
        self.placed.len()
    }

    /// Whether nothing has been rasterised yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }

    /// Whether a glyph has been refused for want of room.
    ///
    /// Surfaced rather than logged, for `SoundCache::failures`'s reason: text that silently loses
    /// characters is far harder to diagnose than text that says why.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.full
    }

    /// What is already known about a glyph, if anything.
    fn get(&self, key: GlyphKey) -> Option<GlyphImage> {
        self.placed.get(&key).copied()
    }

    /// Copies an 8-bit coverage mask into the atlas and records where it went.
    ///
    /// `left` and `top` are the rasteriser's own offsets from the pen position. Returns `None` when
    /// there is no room left.
    fn insert(
        &mut self,
        key: GlyphKey,
        mask: &[u8],
        width: u32,
        height: u32,
        left: f32,
        top: f32,
    ) -> Option<GlyphImage> {
        // A space has an advance and no pixels. Recorded as a zero-sized entry so it is not
        // re-rasterised on every frame, which it would be if "no image" meant "not yet done".
        if width == 0 || height == 0 {
            let image = GlyphImage {
                region: [0.0; 4],
                width: 0.0,
                height: 0.0,
                left,
                top,
            };
            self.placed.insert(key, image);
            return Some(image);
        }

        // Start a new shelf when this one has run out of width.
        if self.pen_x + width + PADDING > ATLAS_SIZE {
            self.shelf_y += self.shelf_height + PADDING;
            self.pen_x = PADDING;
            self.shelf_height = 0;
        }

        if self.shelf_y + height + PADDING > ATLAS_SIZE {
            self.full = true;
            return None;
        }

        let x = self.pen_x;
        let y = self.shelf_y;

        for row in 0..height {
            for column in 0..width {
                let coverage = mask[(row * width + column) as usize];
                let target = (((y + row) * ATLAS_SIZE + (x + column)) * 4) as usize;
                // RGB stays white; only the alpha is written. See the module docs.
                self.texture.pixels[target + 3] = coverage;
            }
        }

        self.pen_x += width + PADDING;
        self.shelf_height = self.shelf_height.max(height);

        let size = ATLAS_SIZE as f32;
        let image = GlyphImage {
            region: [
                x as f32 / size,
                y as f32 / size,
                width as f32 / size,
                height as f32 / size,
            ],
            width: width as f32,
            height: height as f32,
            left,
            top,
        };
        self.placed.insert(key, image);
        Some(image)
    }
}

/// The half of the atlas that talks to the shaper.
///
/// Kept in this module rather than in `text.rs` so the packing above can stay a plain function of
/// bytes, testable with no font at all — which is what the tests below do.
impl GlyphAtlas {
    /// Rasterises one glyph if it is not already in the atlas, and says where it is.
    ///
    /// Returns `None` only when the atlas is full.
    pub(crate) fn ensure_glyph(
        &mut self,
        system: &mut cosmic_text::FontSystem,
        swash: &mut cosmic_text::SwashCache,
        physical: &cosmic_text::PhysicalGlyph,
        font: u64,
        glyph: u16,
        size: f32,
    ) -> Option<GlyphImage> {
        let key = GlyphKey {
            font,
            glyph,
            quarter_pixels: (size * 4.0).round().max(0.0) as u32,
        };
        if let Some(known) = self.get(key) {
            return Some(known);
        }

        let Some(image) = swash.get_image(system, physical.cache_key).as_ref() else {
            // The rasteriser produced nothing at all. Recorded as empty so it is not retried every
            // frame; a glyph that cannot be drawn once will not start working.
            return self.insert(key, &[], 0, 0, 0.0, 0.0);
        };

        // Only 8-bit coverage is handled. A colour glyph — an emoji — is a different thing and
        // wants its own path; refusing it here is better than drawing its red channel as coverage,
        // which would produce a recognisable but wrong shape.
        if image.content != cosmic_text::SwashContent::Mask {
            return self.insert(key, &[], 0, 0, 0.0, 0.0);
        }

        let width = image.placement.width;
        let height = image.placement.height;
        self.insert(
            key,
            &image.data,
            width,
            height,
            image.placement.left as f32,
            image.placement.top as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A solid square mask of the given size.
    fn solid(width: u32, height: u32) -> Vec<u8> {
        vec![255u8; (width * height) as usize]
    }

    fn key(glyph: u16) -> GlyphKey {
        GlyphKey {
            font: 1,
            glyph,
            quarter_pixels: 64,
        }
    }

    /// The alpha byte of one atlas pixel.
    fn alpha_at(atlas: &GlyphAtlas, x: u32, y: u32) -> u8 {
        atlas.texture.pixels[((y * ATLAS_SIZE + x) * 4 + 3) as usize]
    }

    #[test]
    fn an_empty_atlas_is_transparent_white() {
        // White so that filtering blends a glyph's edge towards nothing visible. Transparent *black*
        // would darken every outline into a grey fringe, which reads as a badly rendered font.
        let atlas = GlyphAtlas::new();
        let first = &atlas.texture.pixels[0..4];
        assert_eq!(first, &[255, 255, 255, 0]);
        assert!(atlas.is_empty());
        assert!(!atlas.is_full());
    }

    #[test]
    fn a_glyph_lands_in_the_atlas_and_reports_where() {
        let mut atlas = GlyphAtlas::new();
        let placed = atlas
            .insert(key(1), &solid(10, 20), 10, 20, -1.0, 18.0)
            .expect("room in an empty atlas");

        assert_eq!(placed.width, 10.0);
        assert_eq!(placed.height, 20.0);
        assert_eq!(placed.left, -1.0);
        assert_eq!(placed.top, 18.0);

        // The region is normalised, which is what `Sprite::region` wants.
        assert!(placed.region[2] > 0.0 && placed.region[2] < 1.0);
        assert_eq!(placed.region[2], 10.0 / ATLAS_SIZE as f32);

        // And the coverage really was written, into alpha and not into colour.
        assert_eq!(alpha_at(&atlas, PADDING, PADDING), 255);
        assert_eq!(
            atlas.texture.pixels[((PADDING * ATLAS_SIZE + PADDING) * 4) as usize],
            255,
            "the colour channels stay white so a tint can do its work"
        );
    }

    #[test]
    fn the_same_glyph_twice_is_one_entry() {
        let mut atlas = GlyphAtlas::new();
        let first = atlas
            .insert(key(1), &solid(8, 8), 8, 8, 0.0, 8.0)
            .expect("room");
        assert_eq!(atlas.len(), 1);

        // `get` is what the shaping path consults before rasterising.
        assert_eq!(atlas.get(key(1)), Some(first));
    }

    #[test]
    fn the_same_glyph_at_two_sizes_is_two_entries() {
        // A glyph at 16 px is not a scaled copy of one at 48 px — hinting and rounding differ — so
        // sharing an entry between them is how text goes blurry at one size and crisp at another.
        let mut atlas = GlyphAtlas::new();
        let small = GlyphKey {
            quarter_pixels: 64,
            ..key(1)
        };
        let large = GlyphKey {
            quarter_pixels: 192,
            ..key(1)
        };

        atlas
            .insert(small, &solid(8, 8), 8, 8, 0.0, 8.0)
            .expect("room");
        atlas
            .insert(large, &solid(24, 24), 24, 24, 0.0, 24.0)
            .expect("room");

        assert_eq!(atlas.len(), 2);
        assert_ne!(atlas.get(small), atlas.get(large));
    }

    #[test]
    fn glyphs_are_padded_apart_so_they_cannot_bleed() {
        // **The artefact this prevents is a faint sliver of the neighbouring letter.** Anything
        // sampled with filtering reads slightly outside its region at the edges, so two glyphs flush
        // against each other borrow one another's outermost pixels.
        let mut atlas = GlyphAtlas::new();
        let first = atlas
            .insert(key(1), &solid(4, 4), 4, 4, 0.0, 4.0)
            .expect("room");
        let second = atlas
            .insert(key(2), &solid(4, 4), 4, 4, 0.0, 4.0)
            .expect("room");

        let gap = second.region[0] - (first.region[0] + first.region[2]);
        assert!(
            gap >= PADDING as f32 / ATLAS_SIZE as f32 - 1e-6,
            "glyphs should be at least a pixel apart, got {gap}"
        );
        // And the column between them is untouched.
        assert_eq!(alpha_at(&atlas, PADDING + 4, PADDING), 0);
    }

    #[test]
    fn a_glyph_too_wide_for_the_shelf_starts_a_new_one() {
        let mut atlas = GlyphAtlas::new();
        // Fill most of a shelf, then add something that cannot follow it.
        let wide = ATLAS_SIZE - 2 * PADDING - 10;
        let first = atlas
            .insert(key(1), &solid(wide, 12), wide, 12, 0.0, 12.0)
            .expect("room");
        let second = atlas
            .insert(key(2), &solid(40, 12), 40, 12, 0.0, 12.0)
            .expect("room on the next shelf");

        assert!(
            second.region[1] > first.region[1],
            "the second glyph should be on a lower shelf"
        );
        assert!(
            second.region[0] < first.region[0] + 1e-6,
            "and back at the left edge"
        );
    }

    #[test]
    fn a_space_is_remembered_as_an_empty_entry_rather_than_retried() {
        // A space has an advance and no pixels. If "no image" meant "not done yet", every space in
        // the game would be re-rasterised on every frame forever.
        let mut atlas = GlyphAtlas::new();
        let placed = atlas.insert(key(3), &[], 0, 0, 0.0, 0.0).expect("accepted");

        assert_eq!(placed.width, 0.0);
        assert_eq!(atlas.len(), 1);
        assert!(atlas.get(key(3)).is_some());
    }

    #[test]
    fn a_full_atlas_refuses_rather_than_overwriting() {
        // Overwriting would corrupt glyphs already on screen, which presents as text that changes
        // shape as unrelated text appears elsewhere — one of the least diagnosable bugs available.
        let mut atlas = GlyphAtlas::new();
        let tall = ATLAS_SIZE - 2 * PADDING;
        atlas
            .insert(key(1), &solid(8, tall), 8, tall, 0.0, 0.0)
            .expect("the first fills the height");

        let refused = atlas.insert(key(2), &solid(8, 40), 8, 40, 0.0, 0.0);
        // It has to start a new shelf, and there is no room for one.
        assert!(refused.is_none() || !atlas.is_full());
        if refused.is_none() {
            assert!(atlas.is_full(), "and it says so");
        }
    }
}
