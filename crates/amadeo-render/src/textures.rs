//! From an asset id to pixels: the decoded-texture cache.
//!
//! # The gap this closes
//!
//! A [`Sprite`](crate::Sprite) names its texture by asset **id** (ADR 0020), and `amadeo-assets`
//! deliberately hands over undecoded **bytes** — a `.png` is compressed and a GPU cannot read one.
//! Something has to sit between them. This is it: id → bytes → [`TextureData`] → held until asked
//! for again.
//!
//! # Every id always answers
//!
//! [`TextureCache::get`] never returns `None`. A texture that is missing, unloaded, or corrupt falls
//! back — first to a game-supplied placeholder asset, then to a magenta-and-black check built into
//! the engine with no file behind it at all.
//!
//! That last step matters more than it looks. ADR 0021 requires a missing asset to produce a
//! *visible stand-in plus a structured report* rather than a crash, and a placeholder that is itself
//! a file on disk cannot cover the case where files on disk are the problem. So the final fallback
//! is code.
//!
//! The report is the other half, and it is not optional either: [`TextureCache::failures`] lists
//! every id that fell back and why. A frame that silently draws magenta is a frame an agent cannot
//! diagnose.
//!
//! # Decoding cannot move a replay
//!
//! [`TextureCache`] is a [`Service`], so `World::state_hash` excludes it by trait bound (ADR 0009),
//! and ADR 0021 already forbids gameplay from observing asset state. A texture that fails to decode
//! changes what is on screen and nothing else.
//!
//! # Why decoding is lazy rather than done at the load barrier
//!
//! The *bytes* are behind ADR 0021's barrier — they are resident before the first tick. Decoding is
//! a pure function of those bytes, so doing it on first draw cannot observe anything the barrier was
//! protecting; it just moves work off the critical path at startup and onto the first frame that
//! needs it.
//!
//! The cost is a possible hitch on the first frame a texture appears. That is a real trade and it is
//! deliberately taken **untuned**: a decode-everything-at-the-barrier pass is about ten lines, and
//! this project adds those ten lines when a hitch is *measured* (ADR 0023, ADR 0024), not when one
//! is imagined.

use crate::backend::FrameData;
use amadeo_assets::Assets;
use amadeo_ecs::Service;
use amadeo_image::{DecodeError, PixelFormat, TextureData, decode};
use std::collections::BTreeMap;

/// The asset id a game gives its own missing-texture stand-in.
///
/// An engine convention rather than a setting, for the same reason `amadeo.toml` is: one well-known
/// name that needs no wiring beats a configuration point every game has to remember. Ship an asset
/// with this id and it replaces the built-in check; ship nothing and the built-in is used.
pub const PLACEHOLDER_TEXTURE_ID: &str = "placeholder";

/// Why an id could not be turned into pixels.
///
/// Kept as structured data rather than a formatted string because `render.describe` will report it
/// over the protocol, and an agent reading "why is this magenta" wants the id and the reason apart.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextureFailure {
    /// The asset system has no bytes for this id.
    ///
    /// Either it was never declared in the scene's `assets` block, or its file could not be read.
    /// `amadeo assets` shows which.
    #[error(
        "no bytes are loaded for texture `{id}`, so a placeholder is being drawn instead.\n\
         Check that a scene declares it in its `assets` block, and run `amadeo assets` to see \
         whether the file behind it was readable"
    )]
    NotLoaded {
        /// The id that was asked for.
        id: String,
    },

    /// The bytes are there but are not an image this engine can read.
    #[error(
        "texture `{id}` could not be decoded, so a placeholder is being drawn instead: {cause}"
    )]
    Undecodable {
        /// The id that was asked for.
        id: String,
        /// What the decoder said, including which format it tried.
        cause: DecodeError,
    },
}

/// Decoded textures, by asset id.
///
/// A [`Service`]: rendering machinery, never simulation state.
#[derive(Debug)]
pub struct TextureCache {
    /// Successfully decoded, by id. Ordered so anything generated from it is reproducible.
    decoded: BTreeMap<String, TextureData>,
    /// Ids that fell back, and why. An id appears in exactly one of these two maps.
    failures: BTreeMap<String, TextureFailure>,
    /// The last-resort image, built in code so it exists even when no file does.
    built_in: TextureData,
}

impl Service for TextureCache {}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TextureCache {
    /// An empty cache holding only the built-in placeholder.
    #[must_use]
    pub fn new() -> TextureCache {
        TextureCache {
            decoded: BTreeMap::new(),
            failures: BTreeMap::new(),
            built_in: built_in_placeholder(),
        }
    }

    /// Decodes `id` if it is not already decoded, reading its bytes from `assets`.
    ///
    /// Cheap to call every frame: an id that is already decoded, or that already failed, returns
    /// immediately. **A failure is remembered**, so a broken file is decoded once rather than once
    /// per frame forever — which would turn one bad asset into a permanent frame-rate problem.
    ///
    /// Call [`TextureCache::forget`] to make it try again, which is what hot-reload will do.
    pub fn ensure(&mut self, id: &str, assets: &Assets) {
        if self.decoded.contains_key(id) || self.failures.contains_key(id) {
            return;
        }

        let Some(asset) = assets.store.get(id) else {
            self.failures.insert(
                id.to_string(),
                TextureFailure::NotLoaded { id: id.to_string() },
            );
            return;
        };

        match decode(&asset.bytes, id) {
            Ok(texture) => {
                self.decoded.insert(id.to_string(), texture);
            }
            Err(cause) => {
                self.failures.insert(
                    id.to_string(),
                    TextureFailure::Undecodable {
                        id: id.to_string(),
                        cause,
                    },
                );
            }
        }
    }

    /// The pixels to draw for an id. **Never fails.**
    ///
    /// Falls back in three steps, most specific first:
    ///
    /// 1. the texture itself, if it decoded;
    /// 2. the asset called [`PLACEHOLDER_TEXTURE_ID`], if the game shipped one and it decoded;
    /// 3. the built-in magenta check, which needs no file and therefore cannot itself be missing.
    #[must_use]
    pub fn get(&self, id: &str) -> &TextureData {
        if let Some(texture) = self.decoded.get(id) {
            return texture;
        }
        if let Some(placeholder) = self.decoded.get(PLACEHOLDER_TEXTURE_ID) {
            return placeholder;
        }
        &self.built_in
    }

    /// Whether this id decoded successfully, as opposed to falling back.
    ///
    /// The question [`TextureCache::get`] deliberately does not answer, kept separate so the draw
    /// path has no branch in it.
    #[must_use]
    pub fn is_decoded(&self, id: &str) -> bool {
        self.decoded.contains_key(id)
    }

    /// Drops what is known about an id, so the next [`TextureCache::ensure`] tries again.
    ///
    /// Clears a remembered *failure* as well as a success — re-importing a corrupt file is exactly
    /// the case where retrying matters. Returns whether anything was actually forgotten.
    pub fn forget(&mut self, id: &str) -> bool {
        let had_texture = self.decoded.remove(id).is_some();
        let had_failure = self.failures.remove(id).is_some();
        had_texture || had_failure
    }

    /// Every id that fell back to a placeholder, and why. In id order.
    ///
    /// ADR 0021's structured report. This is what makes "the screen is magenta" answerable rather
    /// than merely visible.
    pub fn failures(&self) -> impl Iterator<Item = (&str, &TextureFailure)> {
        self.failures
            .iter()
            .map(|(id, failure)| (id.as_str(), failure))
    }

    /// Whether anything fell back.
    #[must_use]
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    /// How many textures decoded successfully.
    #[must_use]
    pub fn len(&self) -> usize {
        self.decoded.len()
    }

    /// Whether nothing has decoded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.decoded.is_empty()
    }

    /// Every successfully decoded id, in order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.decoded.keys().map(String::as_str)
    }
}

/// The image drawn when there is no image: a 2x2 magenta and near-black check.
///
/// Deliberately built from a literal rather than read from `assets/textures/placeholder.ppm`. The
/// file version exists too and takes priority, but the *last* fallback cannot depend on a file
/// being loadable, because "a file was not loadable" is the situation it exists to cover.
///
/// The colours match the committed `placeholder.ppm` so the two are indistinguishable on screen —
/// which is the point, since which one you got is not something you should have to work out by
/// looking.
fn built_in_placeholder() -> TextureData {
    const MAGENTA: [u8; 4] = [230, 0, 230, 255];
    const NEAR_BLACK: [u8; 4] = [26, 26, 31, 255];

    let mut pixels = Vec::with_capacity(16);
    for row in 0..2 {
        for column in 0..2 {
            let colour = if (row + column) % 2 == 0 {
                MAGENTA
            } else {
                NEAR_BLACK
            };
            pixels.extend_from_slice(&colour);
        }
    }

    TextureData {
        width: 2,
        height: 2,
        format: PixelFormat::Rgba8UnormSrgb,
        pixels,
    }
}

/// Decodes every texture this frame's batches name, if it is not decoded already.
///
/// Split out of the render system so the two service borrows are visible in one place: it needs
/// [`TextureCache`] mutably and [`Assets`] shared, which are two entries in the same map, so the
/// cache is taken out of the world for the duration.
///
/// Does nothing if either service is absent — a headless test that never installed an asset system
/// still renders, drawing placeholders.
pub fn decode_frame_textures(world: &mut amadeo_ecs::World, frame: &FrameData) {
    if frame.batch_count() == 0 || !world.has_service::<TextureCache>() {
        return;
    }

    world.with_service_taken::<TextureCache, ()>(|world, cache| {
        let Some(assets) = world.service::<Assets>() else {
            return;
        };
        for batch in frame.batches() {
            cache.ensure(&batch.texture, assets);
        }
        // The game's own placeholder is decoded alongside, so step 2 of `get`'s fallback is
        // available the first time it is needed rather than the frame after.
        cache.ensure(PLACEHOLDER_TEXTURE_ID, assets);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_assets::{AssetCatalogue, Sidecar};
    use std::path::{Path, PathBuf};

    /// A real directory holding real files, plus the `Assets` that names them.
    ///
    /// Real files rather than a hand-built store because `Assets::load` is the path a game takes,
    /// and a test that bypassed it would not notice if that path broke.
    struct Fixture {
        root: PathBuf,
        assets: Assets,
    }

    impl Fixture {
        fn new(name: &str, files: &[(&str, &[u8])]) -> Fixture {
            let root = std::env::temp_dir().join(format!(
                "amadeo-tex-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("temp dir");

            let mut catalogue = AssetCatalogue::new();
            for (id, bytes) in files {
                let file = format!("{id}.ppm");
                std::fs::write(root.join(&file), bytes).expect("write");
                catalogue
                    .insert(Sidecar::new(*id), Path::new(&file))
                    .expect("distinct");
            }

            let mut assets = Assets::from_catalogue(catalogue);
            let ids: Vec<String> = files.iter().map(|(id, _)| (*id).to_string()).collect();
            // `Assets::load` needs a root; this fixture builds its catalogue by hand, so the store
            // is filled directly with the same call `load` would make.
            let Assets {
                catalogue, store, ..
            } = &mut assets;
            store.load_all(catalogue, &root, ids.iter().map(String::as_str));

            Fixture { root, assets }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const RED_PPM: &[u8] = b"P3 1 1 255 255 0 0";
    const GREEN_PPM: &[u8] = b"P3 1 1 255 0 255 0";

    #[test]
    fn a_loaded_texture_decodes_to_its_pixels() {
        let fixture = Fixture::new("basic", &[("wall", RED_PPM)]);
        let mut cache = TextureCache::new();
        cache.ensure("wall", &fixture.assets);

        assert!(cache.is_decoded("wall"));
        assert!(!cache.has_failures());
        assert_eq!(cache.get("wall").pixel(0, 0), Some([255, 0, 0, 255]));
    }

    #[test]
    fn an_unloaded_id_falls_back_to_the_built_in_check() {
        // The whole point: a missing texture draws *something*, and says so.
        let fixture = Fixture::new("missing", &[]);
        let mut cache = TextureCache::new();
        cache.ensure("nothing_here", &fixture.assets);

        assert!(!cache.is_decoded("nothing_here"));
        let drawn = cache.get("nothing_here");
        assert_eq!((drawn.width, drawn.height), (2, 2));
        assert_eq!(drawn.pixel(0, 0), Some([230, 0, 230, 255]));
    }

    #[test]
    fn a_games_own_placeholder_takes_priority_over_the_built_in() {
        let fixture = Fixture::new("custom", &[(PLACEHOLDER_TEXTURE_ID, GREEN_PPM)]);
        let mut cache = TextureCache::new();
        cache.ensure(PLACEHOLDER_TEXTURE_ID, &fixture.assets);

        // An id that does not exist gets the game's placeholder, not the engine's.
        assert_eq!(cache.get("absent").pixel(0, 0), Some([0, 255, 0, 255]));
    }

    #[test]
    fn a_corrupt_file_falls_back_and_says_what_was_wrong() {
        let fixture = Fixture::new("corrupt", &[("broken", b"P3 4 4 255 1 2 3")]);
        let mut cache = TextureCache::new();
        cache.ensure("broken", &fixture.assets);

        let (id, failure) = cache.failures().next().expect("one failure");
        assert_eq!(id, "broken");

        let message = failure.to_string();
        assert!(message.contains("broken"), "{message}");
        assert!(message.contains("placeholder"), "{message}");
        // The decoder's own diagnosis survives into the report.
        assert!(message.contains("colour samples"), "{message}");
    }

    #[test]
    fn an_unloaded_id_says_where_to_look() {
        // Pillar 5: the error carries the fix, because an agent cannot ask a follow-up question.
        let fixture = Fixture::new("advice", &[]);
        let mut cache = TextureCache::new();
        cache.ensure("wall", &fixture.assets);

        let (_, failure) = cache.failures().next().expect("one failure");
        let message = failure.to_string();
        assert!(message.contains("assets` block"), "{message}");
        assert!(message.contains("amadeo assets"), "{message}");
    }

    #[test]
    fn a_failure_is_remembered_rather_than_retried_every_frame() {
        // One broken asset must not become a permanent per-frame cost.
        let fixture = Fixture::new("remember", &[("broken", b"P3 9 9 255 1")]);
        let mut cache = TextureCache::new();

        cache.ensure("broken", &fixture.assets);
        assert_eq!(cache.failures().count(), 1);

        // Second call is a map lookup and nothing else; the failure list does not grow.
        cache.ensure("broken", &fixture.assets);
        assert_eq!(cache.failures().count(), 1);
    }

    #[test]
    fn forgetting_an_id_lets_it_be_decoded_again() {
        // What hot-reload will call. Clearing a remembered *failure* is the half that matters:
        // re-exporting a corrupt file is exactly when a retry should work.
        let fixture = Fixture::new("forget", &[("wall", RED_PPM)]);
        let mut cache = TextureCache::new();

        cache.ensure("ghost", &fixture.assets);
        assert!(cache.has_failures());

        assert!(cache.forget("ghost"));
        assert!(!cache.has_failures());
        assert!(!cache.forget("never_seen"));

        cache.ensure("wall", &fixture.assets);
        assert!(cache.forget("wall"));
        assert!(!cache.is_decoded("wall"));
    }

    #[test]
    fn an_id_is_never_both_decoded_and_failed() {
        let fixture = Fixture::new("exclusive", &[("wall", RED_PPM), ("bad", b"P3 2 2 255 1")]);
        let mut cache = TextureCache::new();
        cache.ensure("wall", &fixture.assets);
        cache.ensure("bad", &fixture.assets);
        cache.ensure("absent", &fixture.assets);

        assert_eq!(cache.ids().collect::<Vec<_>>(), vec!["wall"]);
        let failed: Vec<&str> = cache.failures().map(|(id, _)| id).collect();
        assert_eq!(failed, vec!["absent", "bad"]);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn two_files_in_different_formats_decode_alike() {
        // The cache does not care which format an id was authored in -- that is the whole reason
        // format sniffing lives in the decoder rather than here. Two spellings of one red pixel,
        // ASCII and binary; PNG's own coverage is in `amadeo-image`.
        let binary_red: &[u8] = b"P6 1 1 255\n\xff\x00\x00";
        let fixture = Fixture::new(
            "formats",
            &[("as_text", RED_PPM), ("as_binary", binary_red)],
        );

        let mut cache = TextureCache::new();
        cache.ensure("as_text", &fixture.assets);
        cache.ensure("as_binary", &fixture.assets);

        assert!(cache.is_decoded("as_binary"), "{:?}", cache.failures);
        assert_eq!(cache.get("as_text").pixels, cache.get("as_binary").pixels);
    }

    #[test]
    fn the_cache_is_not_part_of_the_state_hash() {
        // The structural guarantee, stated as a test. `TextureCache` is a Service, and ADR 0009
        // excludes those by trait bound -- so decoding a texture cannot move a replay.
        use amadeo_ecs::World;

        let mut bare = World::new();
        let baseline = bare.state_hash();

        let fixture = Fixture::new("hash", &[("wall", RED_PPM)]);
        let mut cache = TextureCache::new();
        cache.ensure("wall", &fixture.assets);
        assert!(cache.is_decoded("wall"));

        bare.insert_service(cache);
        assert_eq!(bare.state_hash(), baseline);
    }

    #[test]
    fn the_built_in_placeholder_needs_no_files_at_all() {
        // The fallback of last resort, with no asset system in the world whatsoever.
        let cache = TextureCache::new();
        let drawn = cache.get("anything");

        assert_eq!(drawn.format, PixelFormat::Rgba8UnormSrgb);
        assert_eq!(drawn.pixels.len(), 2 * 2 * 4);
        // Alternating, so it reads as a check rather than a flat colour.
        assert_ne!(drawn.pixel(0, 0), drawn.pixel(1, 0));
        assert_eq!(drawn.pixel(0, 0), drawn.pixel(1, 1));
    }
}
