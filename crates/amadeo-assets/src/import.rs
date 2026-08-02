//! Giving a new asset file a sidecar, so it can be referred to.
//!
//! ADR 0020 made the declared `id` an asset's identity, which raises an obvious papercut: a file
//! dropped into the tree is invisible until somebody writes a sidecar for it. This module is the
//! answer to that, and it is what makes the ADR ergonomic rather than bureaucratic — the id defaults
//! to the filename stem, so `textures/wall_concrete.png` becomes `id = "wall_concrete"` with nobody
//! typing anything.
//!
//! # Prepare, then apply
//!
//! [`ImportPlan::prepare`] works out what *would* be written without touching the disk;
//! [`ImportPlan::apply`] writes it. Split for two reasons: a dry run is then the same code path as a
//! real one rather than a second implementation that can drift, and the whole plan is validated
//! before the first file is created — so an import that would collide fails having written nothing,
//! instead of leaving half a tree imported.
//!
//! # The default is a starting value, not a rule
//!
//! Once a sidecar exists, import never touches it again. Renaming `wall.png` to `wall_old.png`
//! afterwards does not change its id, and re-running import does not "fix" it to match the new
//! filename. That asymmetry is the entire point of ADR 0020: a move is a refactor nobody expects to
//! have consequences, and a rename of an *id* is a decision someone made on purpose.

use crate::AssetCatalogue;
use crate::scan::{Scan, ScanError};
use crate::sidecar::{Sidecar, sidecar_path_for};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A sidecar an import would create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedSidecar {
    /// The asset file, relative to the scan root.
    pub asset: PathBuf,
    /// Where the sidecar goes, relative to the scan root.
    pub sidecar: PathBuf,
    /// The id it will declare — the filename stem.
    pub id: String,
}

/// Why an import could not go ahead.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportProblem {
    /// The filename cannot become an id.
    #[error(
        "`{asset}` cannot be imported automatically: `{stem}` is not a usable id. \
         An id appears bare in a scene file (`from {stem}`), so it needs letters, digits, \
         underscores, dashes and dots only.\n\
         Fix: rename the file, or hand-write `{sidecar}` with an id you choose"
    )]
    UnusableStem {
        /// The asset that could not be named.
        asset: String,
        /// The stem that was rejected.
        stem: String,
        /// The sidecar that would have to be written by hand.
        sidecar: String,
    },

    /// The default id is already taken.
    #[error(
        "`{asset}` would import as `{id}`, but `{taken_by}` already claims that id. \
         Two assets cannot share one, because a scene saying `from {id}` would be ambiguous.\n\
         Fix: rename one of the files, or hand-write `{sidecar}` with a different id"
    )]
    Collision {
        /// The asset being imported.
        asset: String,
        /// The id it wanted.
        id: String,
        /// Whatever already holds that id.
        taken_by: String,
        /// The sidecar that would have to be written by hand.
        sidecar: String,
    },

    /// A sidecar could not be written.
    #[error("could not write {path}: {message}")]
    Unwritable {
        /// The sidecar that failed.
        path: String,
        /// The underlying message.
        message: String,
    },
}

/// An import that could not go ahead.
///
/// Carries **every** problem, like [`ScanError`] and `amadeo_scene::validate`, because whoever is
/// fixing these cannot ask a follow-up question.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    /// The tree could not be scanned in the first place.
    #[error("{0}")]
    Scan(#[from] ScanError),

    /// The scan was fine but the import itself is not possible.
    #[error(
        "the import found {} problem(s):\n{}",
        problems.len(),
        problems.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n")
    )]
    Refused {
        /// Everything wrong, in sorted order so it is reproducible.
        problems: Vec<ImportProblem>,
    },
}

/// Every sidecar an import would create.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportPlan {
    /// The directory the plan is relative to.
    pub root: PathBuf,
    /// The sidecars to write, in asset-path order.
    pub sidecars: Vec<PlannedSidecar>,
    /// How many assets already had one and were left alone.
    pub already_imported: usize,
}

impl ImportPlan {
    /// Works out which sidecars are missing, without writing anything.
    ///
    /// # Errors
    ///
    /// [`ImportError::Scan`] if the tree is already broken — an import into a tree with duplicate
    /// ids would be adding to a mess rather than fixing one. [`ImportError::Refused`] listing every
    /// filename that cannot become an id and every default that would collide.
    pub fn prepare(root: &Path) -> Result<ImportPlan, ImportError> {
        let scan = AssetCatalogue::scan(root)?;
        ImportPlan::from_scan(root, &scan)
    }

    /// The planning half, separated from the disk walk so it can be tested directly.
    ///
    /// # Errors
    ///
    /// [`ImportError::Refused`] as for [`ImportPlan::prepare`].
    pub fn from_scan(root: &Path, scan: &Scan) -> Result<ImportPlan, ImportError> {
        let mut problems = Vec::new();
        let mut sidecars = Vec::new();

        // Ids claimed within this plan, so two new files with one stem collide with *each other*
        // and not just with what was already on disk. `textures/wall.png` and `props/wall.png`
        // arriving together is the common case and it has to be caught here rather than producing
        // two sidecars that then fail the next scan.
        let mut claimed: BTreeMap<String, String> = BTreeMap::new();

        for asset in &scan.unimported {
            let shown = show(asset);
            let sidecar = sidecar_path_for(asset);

            let Some(id) = Sidecar::default_id_for(asset) else {
                let stem = asset
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                problems.push(ImportProblem::UnusableStem {
                    asset: shown,
                    stem,
                    sidecar: show(&sidecar),
                });
                continue;
            };

            // Against the catalogue first, then against this plan.
            let taken_by = scan
                .catalogue
                .get(&id)
                .map(|entry| show(&entry.source))
                .or_else(|| claimed.get(&id).cloned());

            if let Some(taken_by) = taken_by {
                problems.push(ImportProblem::Collision {
                    asset: shown,
                    id,
                    taken_by,
                    sidecar: show(&sidecar),
                });
                continue;
            }

            claimed.insert(id.clone(), shown);
            sidecars.push(PlannedSidecar {
                asset: asset.clone(),
                sidecar,
                id,
            });
        }

        if !problems.is_empty() {
            return Err(ImportError::Refused { problems });
        }

        Ok(ImportPlan {
            root: root.to_path_buf(),
            sidecars,
            already_imported: scan.catalogue.len(),
        })
    }

    /// Whether there is nothing to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sidecars.is_empty()
    }

    /// Writes every planned sidecar.
    ///
    /// Returns the paths written, relative to the root.
    ///
    /// # Errors
    ///
    /// [`ImportError::Refused`] listing every sidecar that could not be written. It keeps going
    /// after a failure rather than stopping, so a read-only file in the middle of a tree does not
    /// hide the other nineteen that would also have failed.
    pub fn apply(&self) -> Result<Vec<PathBuf>, ImportError> {
        let mut written = Vec::new();
        let mut problems = Vec::new();

        for planned in &self.sidecars {
            let absolute = self.root.join(&planned.sidecar);
            let text = Sidecar::new(&planned.id).to_text();

            // `write` truncates, so this would clobber a sidecar that appeared between prepare and
            // apply. Refusing an existing file keeps import from ever destroying a hand-written id,
            // which is the one thing in this module that would be unrecoverable.
            if absolute.exists() {
                problems.push(ImportProblem::Unwritable {
                    path: show(&planned.sidecar),
                    message: "it already exists; import never overwrites a declared id".to_string(),
                });
                continue;
            }

            match std::fs::write(&absolute, text) {
                Ok(()) => written.push(planned.sidecar.clone()),
                Err(error) => problems.push(ImportProblem::Unwritable {
                    path: show(&planned.sidecar),
                    message: error.to_string(),
                }),
            }
        }

        if problems.is_empty() {
            Ok(written)
        } else {
            Err(ImportError::Refused { problems })
        }
    }
}

/// A path as it should appear in a message: forward slashes, everywhere.
///
/// Same normalisation the scan applies, and for the same reason — these strings are compared in
/// tests and read by an agent, so they cannot differ between Windows and Linux.
fn show(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `Scan` by hand, so planning can be tested without a filesystem.
    fn scan_with(catalogued: &[(&str, &str)], unimported: &[&str]) -> Scan {
        let mut catalogue = AssetCatalogue::new();
        for (id, path) in catalogued {
            catalogue
                .insert(Sidecar::new(*id), Path::new(path))
                .expect("distinct");
        }
        Scan {
            catalogue,
            unimported: unimported.iter().map(PathBuf::from).collect(),
            orphaned: Vec::new(),
        }
    }

    fn plan(catalogued: &[(&str, &str)], unimported: &[&str]) -> Result<ImportPlan, ImportError> {
        ImportPlan::from_scan(Path::new("root"), &scan_with(catalogued, unimported))
    }

    #[test]
    fn a_new_asset_gets_an_id_from_its_filename_stem() {
        // The thing that makes ADR 0020 cost nothing on the common path.
        let planned = plan(&[], &["textures/wall_concrete.png"]).expect("plannable");

        assert_eq!(planned.sidecars.len(), 1);
        assert_eq!(planned.sidecars[0].id, "wall_concrete");
        assert_eq!(
            planned.sidecars[0].sidecar,
            PathBuf::from("textures/wall_concrete.png.ama-meta")
        );
    }

    #[test]
    fn an_asset_that_already_has_a_sidecar_is_left_alone() {
        // Re-running import must be a no-op, or it would rewrite ids to match filenames and undo
        // every rename anyone had made.
        let planned = plan(&[("wall_old", "textures/wall.png")], &[]).expect("plannable");

        assert!(planned.is_empty());
        assert_eq!(planned.already_imported, 1);
    }

    #[test]
    fn a_filename_that_cannot_be_an_id_is_refused_with_the_manual_fix() {
        // Guessing something broken would produce a scene line that does not parse, two layers away
        // from the cause.
        let error = plan(&[], &["my wall.png"]).expect_err("space in the name");
        let message = error.to_string();

        assert!(message.contains("not a usable id"), "got: {message}");
        assert!(message.contains("my wall.png.ama-meta"), "got: {message}");
    }

    #[test]
    fn a_default_that_collides_with_an_existing_id_is_refused() {
        let error =
            plan(&[("wall", "textures/wall.png")], &["props/wall.png"]).expect_err("id taken");
        let message = error.to_string();

        assert!(message.contains("already claims"), "got: {message}");
        assert!(message.contains("textures/wall.png"), "got: {message}");
        assert!(message.contains("props/wall.png"), "got: {message}");
    }

    #[test]
    fn two_new_files_with_one_stem_collide_with_each_other() {
        // The case that would otherwise write two sidecars and fail the *next* scan instead of this
        // import, which is a much more confusing place to find out.
        let error = plan(&[], &["textures/wall.png", "props/wall.png"]).expect_err("same stem");

        assert!(error.to_string().contains("already claims"), "got: {error}");
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let error = plan(&[], &["my wall.png", "a/x.png", "b/x.png"]).expect_err("two problems");
        let ImportError::Refused { problems } = &error else {
            panic!("expected Refused, got {error:?}");
        };

        assert_eq!(problems.len(), 2, "got: {error}");
    }

    #[test]
    fn nothing_is_written_when_anything_would_fail() {
        // Prepare validates the whole plan before apply touches the disk, so a tree is never left
        // half-imported.
        assert!(plan(&[], &["ok.png", "my wall.png"]).is_err());
    }

    #[test]
    fn planned_sidecars_are_valid_input_to_the_parser() {
        // The round trip that matters: what import writes, the scan must read back with the same id.
        let planned = plan(&[], &["textures/wall.png"]).expect("plannable");
        let text = Sidecar::new(&planned.sidecars[0].id).to_text();
        let parsed = Sidecar::parse(&text, Path::new("textures/wall.png.ama-meta")).expect("valid");

        assert_eq!(parsed.id, "wall");
        assert_eq!(text, "id = \"wall\"\n");
    }

    /// End-to-end against a real directory, since the point of the module is touching the disk.
    #[test]
    fn prepare_and_apply_turn_a_bare_file_into_a_catalogued_asset() {
        let root = std::env::temp_dir().join(format!(
            "amadeo-import-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("textures")).expect("temp dir");
        std::fs::write(root.join("textures/wall.png"), "asset").expect("write");

        let planned = ImportPlan::prepare(&root).expect("plannable");
        assert_eq!(planned.sidecars.len(), 1);

        // Before applying, the file is invisible to the catalogue.
        assert!(
            AssetCatalogue::scan(&root)
                .expect("scans")
                .catalogue
                .is_empty()
        );

        let written = planned.apply().expect("writes");
        assert_eq!(written, [PathBuf::from("textures/wall.png.ama-meta")]);

        let scan = AssetCatalogue::scan(&root).expect("scans");
        assert!(scan.catalogue.contains("wall"));
        assert!(scan.unimported.is_empty());

        // And it is idempotent, which is what makes it safe to run on every build.
        assert!(ImportPlan::prepare(&root).expect("plannable").is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }
}
