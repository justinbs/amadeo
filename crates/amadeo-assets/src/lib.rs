//! Assets: what exists on disk, what it is called, and how to find it again.
//!
//! ```
//! use amadeo_assets::{AssetCatalogue, Sidecar};
//! use std::path::Path;
//!
//! let mut catalogue = AssetCatalogue::new();
//! catalogue
//!     .insert(Sidecar::new("wall_concrete"), Path::new("textures/wall_concrete.png"))
//!     .expect("first one in wins the name");
//!
//! // Scenes and code refer to an asset by its declared id, never by its path (ADR 0020).
//! assert!(catalogue.contains("wall_concrete"));
//! assert_eq!(catalogue.len(), 1);
//! ```
//!
//! # An asset is named, not located
//!
//! ADR 0020. The identity is a declared `id` in the asset's `.ama-meta` sidecar, defaulting to the
//! filename stem on import — so it reads exactly like a path, and yet moving or renaming the file
//! changes nothing. The same reasoning ADR 0017 applied to components one layer down: coupling
//! identity to *where something lives* turns a refactor into a silent breakage.
//!
//! # The rule loading has to obey — ADR 0021
//!
//! **Gameplay may hold an asset id. It may never observe an asset's *state*.** No simulation system
//! asks whether an asset is loaded, how big it is, or what is in it. Anything gameplay needs — a
//! hitbox, a collision shape, a footstep's timing — is **authored** in the scene file, never derived
//! from the loaded file.
//!
//! That is what makes determinism structural rather than conventional: "is it loaded yet" depends on
//! disk speed and OS scheduling, so a simulation that can ask it does not reproduce. One that cannot
//! ask has nothing to branch on, and an asset arriving at tick 900 instead of tick 300 changes what
//! is on screen and nothing else.
//!
//! Rendering and audio sit outside the deterministic zone and *are* free to look. A missing texture
//! draws a placeholder and reports itself; it does not crash and it does not stall the tick.
//!
//! The cost is real and is the accepted trade: you cannot size a hitbox from a sprite, so you type
//! it out.
//!
//! # What is not here yet
//!
//! **Loading itself.** This layer answers "what assets exist and where are their files". Reading
//! bytes, decoding them, handing out handles, and hot-reloading come next, to the rule above plus
//! the load barrier — a scene declares what it needs, and no tick runs until all of it is resident.
//!
//! **The import pipeline.** Compiling `.png` into an internal format so the runtime never parses
//! source formats. Needs the loading layer underneath it.

mod sidecar;

pub use sidecar::{
    SIDECAR_EXTENSION, Sidecar, SidecarError, SidecarErrorKind, asset_path_for, is_usable_id,
    sidecar_path_for,
};

use amadeo_ecs::Service;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Why an asset could not join the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogueError {
    /// Two assets claim the same id.
    #[error(
        "two assets both claim the id `{id}`:\n  {first}\n  {second}\n\
         An id is an asset's identity, so a duplicate is genuinely ambiguous -- a scene saying \
         `from {id}` could mean either. Rename one in its .ama-meta sidecar"
    )]
    DuplicateId {
        /// The contested id.
        id: String,
        /// The file that claimed it first.
        first: String,
        /// The file that collided with it.
        second: String,
    },
}

/// One asset the catalogue knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEntry {
    /// The declared id — this asset's identity.
    pub id: String,
    /// Where the asset file is, relative to the project root.
    ///
    /// Bookkeeping, not identity. It is what a loader opens and what a diagnostic prints, and it may
    /// change without anything that refers to the asset noticing (ADR 0020).
    pub source: PathBuf,
    /// Everything the sidecar declared besides the id.
    pub settings: BTreeMap<String, String>,
}

/// Every asset in a project, by id.
///
/// A [`Service`] rather than a `Resource`: it is engine machinery describing what is on disk, not
/// simulation state, so it is excluded from the state hash (ADR 0009). Two runs of the same game
/// with the same scene must agree on their state hash whether or not an unrelated texture exists.
#[derive(Debug, Default)]
pub struct AssetCatalogue {
    /// Ordered by id, so listing is reproducible and diffable — the same reason every other registry
    /// in this engine uses a `BTreeMap` (invariant I3).
    assets: BTreeMap<String, AssetEntry>,
}

impl Service for AssetCatalogue {}

impl AssetCatalogue {
    /// An empty catalogue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an asset.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::DuplicateId`] if another asset already claims the id, naming **both**
    /// files. A collision resolved silently is a bug that surfaces somewhere else entirely — the
    /// same reason `ComponentRegistry::register` refuses a duplicate component name (ADR 0017).
    pub fn insert(&mut self, sidecar: Sidecar, source: &Path) -> Result<(), CatalogueError> {
        if let Some(existing) = self.assets.get(&sidecar.id) {
            return Err(CatalogueError::DuplicateId {
                id: sidecar.id,
                first: existing.source.display().to_string(),
                second: source.display().to_string(),
            });
        }

        self.assets.insert(
            sidecar.id.clone(),
            AssetEntry {
                id: sidecar.id,
                source: source.to_path_buf(),
                settings: sidecar.settings,
            },
        );
        Ok(())
    }

    /// Looks an asset up by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&AssetEntry> {
        self.assets.get(id)
    }

    /// Whether an id is known.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.assets.contains_key(id)
    }

    /// How many assets are catalogued.
    #[must_use]
    pub fn len(&self) -> usize {
        self.assets.len()
    }

    /// Whether the catalogue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    /// Every asset, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &AssetEntry> {
        self.assets.values()
    }

    /// Every id, in order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.assets.keys().map(String::as_str)
    }

    /// Ids close to `wanted`, for a "did you mean" on a failed lookup.
    ///
    /// Deliberately simple: a shared prefix or a substring match, not an edit distance. An agent
    /// that guessed `wall` when the id is `wall_concrete` is the common case, and a cheap rule
    /// catches it. Pillar 5 wants the error to carry the fix; it does not need to be clever.
    #[must_use]
    pub fn similar_to(&self, wanted: &str) -> Vec<&str> {
        let lowered = wanted.to_ascii_lowercase();
        self.ids()
            .filter(|id| {
                let other = id.to_ascii_lowercase();
                other.contains(&lowered) || lowered.contains(&other)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue_of(entries: &[(&str, &str)]) -> AssetCatalogue {
        let mut catalogue = AssetCatalogue::new();
        for (id, path) in entries {
            catalogue
                .insert(Sidecar::new(*id), Path::new(path))
                .expect("distinct ids");
        }
        catalogue
    }

    #[test]
    fn an_asset_is_found_by_its_id_not_its_path() {
        let catalogue = catalogue_of(&[("wall", "textures/wall.png")]);

        assert!(catalogue.contains("wall"));
        assert_eq!(
            catalogue.get("wall").map(|a| a.source.to_str().unwrap()),
            Some("textures/wall.png")
        );
        // The path is bookkeeping. Nothing looks an asset up by it.
        assert!(!catalogue.contains("textures/wall.png"));
    }

    #[test]
    fn moving_a_file_does_not_change_the_id() {
        // The whole point of ADR 0020, stated as a test so it cannot quietly stop being true.
        let before = catalogue_of(&[("wall", "textures/wall.png")]);
        let after = catalogue_of(&[("wall", "textures/interior/stone/wall.png")]);

        assert_eq!(
            before.ids().collect::<Vec<_>>(),
            after.ids().collect::<Vec<_>>()
        );
        assert!(after.contains("wall"));
    }

    #[test]
    fn a_duplicate_id_names_both_files() {
        // A collision is genuinely ambiguous, and a message naming one file leaves you hunting for
        // the other.
        let mut catalogue = catalogue_of(&[("wall", "textures/wall.png")]);
        let error = catalogue
            .insert(Sidecar::new("wall"), Path::new("props/wall.png"))
            .expect_err("duplicate");

        let message = error.to_string();
        assert!(message.contains("textures/wall.png"), "got: {message}");
        assert!(message.contains("props/wall.png"), "got: {message}");
        assert!(message.contains(".ama-meta"), "got: {message}");
    }

    #[test]
    fn listing_is_ordered_regardless_of_insertion_order() {
        // Anything generated from this gets committed and diffed, so the order cannot depend on
        // which file the scan happened to reach first (invariant I3).
        let forwards = catalogue_of(&[("a", "a.png"), ("m", "m.png"), ("z", "z.png")]);
        let backwards = catalogue_of(&[("z", "z.png"), ("m", "m.png"), ("a", "a.png")]);

        assert_eq!(
            forwards.ids().collect::<Vec<_>>(),
            backwards.ids().collect::<Vec<_>>()
        );
        assert_eq!(forwards.ids().collect::<Vec<_>>(), vec!["a", "m", "z"]);
    }

    #[test]
    fn a_near_miss_suggests_what_was_probably_meant() {
        let catalogue = catalogue_of(&[
            ("wall_concrete", "a.png"),
            ("wall_brick", "b.png"),
            ("floor_tile", "c.png"),
        ]);

        // The common agent mistake: guessing the stem of a longer id.
        let suggestions = catalogue.similar_to("wall");
        assert!(
            suggestions.contains(&"wall_concrete"),
            "got: {suggestions:?}"
        );
        assert!(suggestions.contains(&"wall_brick"), "got: {suggestions:?}");
        assert!(!suggestions.contains(&"floor_tile"), "got: {suggestions:?}");

        assert!(catalogue.similar_to("nothing_like_it").is_empty());
    }

    #[test]
    fn settings_travel_with_the_asset() {
        let mut sidecar = Sidecar::new("wall");
        sidecar.settings.insert("filter".into(), "nearest".into());

        let mut catalogue = AssetCatalogue::new();
        catalogue
            .insert(sidecar, Path::new("wall.png"))
            .expect("inserts");

        assert_eq!(
            catalogue
                .get("wall")
                .unwrap()
                .settings
                .get("filter")
                .map(String::as_str),
            Some("nearest")
        );
    }
}
