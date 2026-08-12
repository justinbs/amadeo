//! From an asset id to samples: the decoded-sound cache.
//!
//! # The gap this closes
//!
//! An [`AudioSource`](crate::AudioSource) names its sound by asset **id** (ADR 0020), and
//! `amadeo-assets` deliberately hands over undecoded **bytes** — a `.wav` is a container with a
//! header, not samples. Something has to sit between them. This is it: id → bytes →
//! [`SoundData`] → held until asked for again.
//!
//! It is `amadeo-render`'s `TextureCache` again — named in prose rather than linked, because this
//! crate does not depend on the renderer and must not start. One difference between them is worth
//! stating, because it is a decision rather than an omission:
//!
//! # There is no placeholder sound, and there must not be one
//!
//! A missing texture draws magenta, because a hole in the screen is worse than a wrong colour and a
//! frame that silently drew nothing would be undiagnosable. **Audio has no equivalent.** There is no
//! sound that reads as "this asset is missing" — a beep or a tone is indistinguishable from content,
//! and a game that plays one is a game whose broken asset sounds like a design choice.
//!
//! So a missing sound is **silence**, and the report is the whole of the diagnosis:
//! [`SoundCache::failures`] lists every id that did not load and why. That is ADR 0021's structured
//! report without the visible stand-in, because the visible stand-in has no audible form.
//!
//! # Decoding cannot move a replay
//!
//! [`SoundCache`] is a [`Service`], so `World::state_hash` excludes it by trait bound (ADR 0009).
//! A sound that fails to decode changes what is heard and nothing else.

use crate::backend::SoundData;
use crate::wav::{WavError, decode_wav};
use amadeo_assets::Assets;
use amadeo_ecs::Service;
use std::collections::BTreeMap;

/// Why an id could not be turned into samples.
///
/// Structured rather than a formatted string, for the reason `TextureFailure` gives: an agent asking
/// "why is this silent" wants the id and the reason apart.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SoundFailure {
    /// The asset system has no bytes for this id.
    ///
    /// Either it was never declared in the scene's `assets` block, or its file could not be read.
    /// `amadeo assets` shows which.
    #[error(
        "no bytes are loaded for sound `{id}`, so nothing will be heard from it.\n\
         Check that a scene declares it in its `assets` block, and run `amadeo assets` to see \
         whether the file behind it was readable"
    )]
    NotLoaded {
        /// The id that was asked for.
        id: String,
    },

    /// The bytes are there but are not audio this engine can read.
    #[error("sound `{id}` could not be decoded, so nothing will be heard from it: {cause}")]
    Undecodable {
        /// The id that was asked for.
        id: String,
        /// What the decoder said.
        cause: WavError,
    },
}

/// Decoded sounds, by asset id.
///
/// A [`Service`]: audio machinery, never simulation state.
#[derive(Debug, Default)]
pub struct SoundCache {
    /// Successfully decoded, by id. Ordered so anything generated from it is reproducible.
    decoded: BTreeMap<String, SoundData>,
    /// Ids that failed, and why. An id appears in exactly one of these two maps.
    failures: BTreeMap<String, SoundFailure>,
}

impl Service for SoundCache {}

impl SoundCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> SoundCache {
        SoundCache::default()
    }

    /// Decodes `id` if it is not already decoded, reading its bytes from `assets`.
    ///
    /// Cheap to call every frame: an id that is already decoded, or that already failed, returns
    /// immediately. **A failure is remembered**, so a broken file is decoded once rather than once
    /// per frame forever.
    ///
    /// Call [`SoundCache::forget`] to make it try again, which is what hot-reload will do.
    pub fn ensure(&mut self, id: &str, assets: &Assets) {
        if self.decoded.contains_key(id) || self.failures.contains_key(id) {
            return;
        }

        let Some(asset) = assets.store.get(id) else {
            self.failures.insert(
                id.to_string(),
                SoundFailure::NotLoaded { id: id.to_string() },
            );
            return;
        };

        match decode_wav(&asset.bytes) {
            Ok(sound) => {
                self.decoded.insert(id.to_string(), sound);
            }
            Err(cause) => {
                self.failures.insert(
                    id.to_string(),
                    SoundFailure::Undecodable {
                        id: id.to_string(),
                        cause,
                    },
                );
            }
        }
    }

    /// The samples held under an id, or `None`.
    ///
    /// **Deliberately unlike `TextureCache::get`**, which always returns something. See the module
    /// docs: there is no sound that means "missing".
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SoundData> {
        self.decoded.get(id)
    }

    /// Records already-decoded samples under an id, without going near a file.
    ///
    /// For samples a *program* produced — a test that wants a known waveform, or a game generating
    /// a tone at runtime — where there is no file to point at.
    pub fn insert_decoded(&mut self, id: impl Into<String>, sound: SoundData) {
        self.decoded.insert(id.into(), sound);
    }

    /// Whether this id decoded successfully.
    #[must_use]
    pub fn is_decoded(&self, id: &str) -> bool {
        self.decoded.contains_key(id)
    }

    /// How many sounds are decoded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.decoded.len()
    }

    /// Whether nothing has decoded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.decoded.is_empty()
    }

    /// Drops what is known about an id, so the next [`SoundCache::ensure`] tries again.
    ///
    /// Clears a remembered *failure* as well as a success. Returns whether anything was forgotten.
    pub fn forget(&mut self, id: &str) -> bool {
        let had_sound = self.decoded.remove(id).is_some();
        let had_failure = self.failures.remove(id).is_some();
        had_sound || had_failure
    }

    /// Every id that did not load, and why. In id order.
    ///
    /// ADR 0021's structured report, and here it is the *only* signal — see the module docs.
    pub fn failures(&self) -> impl Iterator<Item = (&str, &SoundFailure)> {
        self.failures
            .iter()
            .map(|(id, failure)| (id.as_str(), failure))
    }

    /// Whether anything failed to load.
    #[must_use]
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_assets::{AssetCatalogue, Sidecar};
    use std::path::{Path, PathBuf};

    /// A temporary asset directory with real files in it, and the `Assets` that catalogues them.
    ///
    /// The same shape `amadeo-render`'s texture tests use, and for the same reason: going through
    /// the real catalogue and the real loader means this tests the path a game takes, rather than a
    /// hand-built store that could diverge from it.
    struct Project {
        root: PathBuf,
        assets: Assets,
    }

    impl Project {
        fn new(name: &str, files: &[(&str, &[u8])]) -> Project {
            let root = std::env::temp_dir().join(format!(
                "amadeo-sound-cache-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("temp dir");

            let mut project = Project {
                root,
                assets: Assets::from_catalogue(AssetCatalogue::new()),
            };
            for (id, bytes) in files {
                let file = format!("{id}.wav");
                std::fs::write(project.root.join(&file), bytes).expect("write");
                project
                    .assets
                    .catalogue
                    .insert(Sidecar::new(*id), Path::new(&file))
                    .expect("distinct id");
            }

            let ids: Vec<String> = project.assets.catalogue.ids().map(str::to_string).collect();
            let Assets {
                catalogue, store, ..
            } = &mut project.assets;
            store.load_all(catalogue, &project.root, ids.iter().map(String::as_str));
            project
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// A one-sample mono WAV, built by hand so the test needs no fixture file.
    fn a_tiny_wav() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&36u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&96_000u32.to_le_bytes()); // byte rate
        bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&16u16.to_le_bytes()); // bits
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0i16.to_le_bytes());
        bytes
    }

    #[test]
    fn a_declared_wav_decodes_once_and_is_held() {
        let project = Project::new("decodes", &[("hum", &a_tiny_wav())]);
        let mut cache = SoundCache::new();

        cache.ensure("hum", &project.assets);
        assert!(cache.is_decoded("hum"));
        assert_eq!(cache.get("hum").expect("decoded").sample_rate, 48_000);

        // Idempotent: asking again neither re-decodes nor duplicates.
        cache.ensure("hum", &project.assets);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn an_undeclared_sound_is_silent_and_says_why() {
        // The whole reason there is no placeholder: silence plus a report, rather than a beep that
        // would be indistinguishable from content.
        let project = Project::new("undeclared", &[("hum", &a_tiny_wav())]);
        let mut cache = SoundCache::new();

        cache.ensure("missing", &project.assets);

        assert!(cache.get("missing").is_none());
        assert!(cache.has_failures());
        let (id, failure) = cache.failures().next().expect("one failure");
        assert_eq!(id, "missing");
        // The message names the id and says what to run, because "no sound" is not a report.
        let text = format!("{failure}");
        assert!(text.contains("missing"), "{text}");
        assert!(text.contains("assets"), "{text}");
    }

    #[test]
    fn a_file_that_is_not_audio_fails_once_rather_than_every_frame() {
        // A broken asset re-decoded every frame is one bad file turned into a permanent frame-rate
        // problem. The remembered failure is what prevents it, and `forget` is what undoes it.
        let project = Project::new("broken", &[("broken", b"this is not a wav file at all")]);
        let mut cache = SoundCache::new();

        cache.ensure("broken", &project.assets);
        assert!(cache.has_failures());
        assert!(!cache.is_decoded("broken"));

        assert!(
            cache.forget("broken"),
            "a remembered failure is forgettable"
        );
        assert!(!cache.has_failures());
    }

    #[test]
    fn generated_samples_need_no_file() {
        let mut cache = SoundCache::new();
        cache.insert_decoded(
            "tone",
            SoundData {
                samples: vec![0.0; 16],
                channels: 1,
                sample_rate: 48_000,
            },
        );
        assert!(cache.is_decoded("tone"));
        assert_eq!(cache.get("tone").expect("held").samples.len(), 16);
    }
}
