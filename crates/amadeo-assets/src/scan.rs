//! Walking a directory and turning it into a catalogue.
//!
//! One asset file plus one `.ama-meta` sidecar makes one catalogue entry (ADR 0020). This module is
//! the part that goes to the filesystem and finds them.
//!
//! # Why the walk is sorted
//!
//! Filesystem enumeration order is not reproducible — it differs between Windows and Linux, between
//! two machines, and sometimes between two runs on one machine. Anything derived from it would not
//! be reproducible either, which is invariant I3. So every directory's entries are sorted before
//! being visited, and the catalogue itself is a `BTreeMap`.
//!
//! This costs a sort per directory and buys a scan whose output is the same everywhere, which is
//! what lets `assets.list` be diffed and put in a test.
//!
//! # Why stored paths use forward slashes
//!
//! A path in the catalogue is *reported*, over the agent protocol and in `amadeo assets`. If it came
//! out as `textures\wall.png` on Windows and `textures/wall.png` in CI, the two would not compare
//! equal and every cross-platform assertion about a scan would need a special case. So separators
//! are normalised to `/` on the way in, once, rather than at each of the places that print one.
//!
//! Windows opens a path with forward slashes perfectly well, so nothing downstream has to undo this.

use crate::sidecar::{SIDECAR_EXTENSION, Sidecar, asset_path_for, sidecar_path_for};
use crate::{AssetCatalogue, CatalogueError};
use std::path::{Path, PathBuf};

/// What a scan found on disk.
///
/// The catalogue is the answer; the other two fields are things that are *not* errors but that
/// somebody probably wants to know about. ADR 0020 asks for the first one by name: an asset with no
/// sidecar "is invisible. That is a real papercut and the error must say so plainly rather than
/// `asset not found`". It can only say so if the scan noticed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// Every asset that has a sidecar, by id.
    pub catalogue: AssetCatalogue,

    /// Asset files with no sidecar, so not referenceable yet.
    ///
    /// Sorted, relative to the scan root. `amadeo assets` lists these separately and `amadeo import`
    /// is what turns them into real entries.
    pub unimported: Vec<PathBuf>,

    /// Sidecars whose asset file is gone.
    ///
    /// Usually a file deleted without its sidecar. Reported rather than ignored because the sidecar
    /// still claims an id, and an id claimed by nothing is a confusing thing to trip over later.
    pub orphaned: Vec<PathBuf>,
}

/// One reason a scan was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScanProblem {
    /// Two assets claim the same id.
    #[error("{0}")]
    Duplicate(#[from] CatalogueError),

    /// A sidecar did not parse.
    #[error("{0}")]
    BadSidecar(#[from] crate::SidecarError),

    /// A directory or file could not be read.
    #[error("could not read {path}: {message}")]
    Unreadable {
        /// What could not be read.
        path: String,
        /// The underlying message.
        message: String,
    },
}

/// A scan that could not produce a trustworthy catalogue.
///
/// Carries **every** problem rather than the first, for the same reason `amadeo_scene::validate`
/// does: whoever is fixing these cannot ask a follow-up question, and one problem per round trip is
/// a functional defect rather than a rough edge. Loading stops at the first error; checking does
/// not.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the asset scan found {} problem(s):\n{}",
    problems.len(),
    problems.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n")
)]
pub struct ScanError {
    /// Everything wrong, in the order found — which is sorted order, so it is reproducible.
    pub problems: Vec<ScanProblem>,
}

impl AssetCatalogue {
    /// Scans a directory tree and builds a catalogue from it.
    ///
    /// Every file that is not a sidecar is an asset. An asset with a sidecar becomes an entry; one
    /// without is reported as [`Scan::unimported`] rather than guessed at, because ADR 0020 makes
    /// the declared id the identity and inventing one at scan time would mean the id changed the
    /// moment somebody ran an import.
    ///
    /// # Errors
    ///
    /// [`ScanError`] listing every duplicate id, malformed sidecar, and unreadable path. A missing
    /// `root` is one of those errors rather than an empty result: a mistyped asset directory that
    /// quietly catalogues nothing is exactly the plausible-but-wrong answer this project keeps
    /// refusing elsewhere.
    pub fn scan(root: &Path) -> Result<Scan, ScanError> {
        let mut scan = Scan::default();
        let mut problems = Vec::new();

        // Collected first, then processed, so ordering is decided in one place.
        let mut files = Vec::new();
        collect_files(root, root, &mut files, &mut problems);
        files.sort();

        // Which files are sidecars, so an asset can be matched to one without hitting the disk again
        // per asset. `files` is already sorted, so this stays reproducible.
        let sidecars: std::collections::BTreeSet<PathBuf> = files
            .iter()
            .filter(|path| is_sidecar(path))
            .cloned()
            .collect();

        for relative in &files {
            if is_sidecar(relative) {
                // A sidecar whose asset is missing. `asset_path_for` cannot fail here, since
                // `is_sidecar` just confirmed the extension.
                if let Some(asset) = asset_path_for(relative)
                    && !files.contains(&asset)
                {
                    scan.orphaned.push(relative.clone());
                }
                continue;
            }

            let expected = sidecar_path_for(relative);
            if !sidecars.contains(&expected) {
                scan.unimported.push(relative.clone());
                continue;
            }

            let absolute = root.join(&expected);
            let text = match std::fs::read_to_string(&absolute) {
                Ok(text) => text,
                Err(error) => {
                    problems.push(ScanProblem::Unreadable {
                        path: display(&expected),
                        message: error.to_string(),
                    });
                    continue;
                }
            };

            // The sidecar is parsed against its *relative* path so that a diagnostic reads
            // `textures/wall.png.ama-meta:3`, which is the same string the user would type, rather
            // than an absolute path that differs on every machine.
            match Sidecar::parse(strip_bom(&text), &expected) {
                Ok(sidecar) => {
                    if let Err(error) = scan.catalogue.insert(sidecar, relative) {
                        problems.push(ScanProblem::Duplicate(error));
                    }
                }
                Err(error) => problems.push(ScanProblem::BadSidecar(error)),
            }
        }

        if problems.is_empty() {
            Ok(scan)
        } else {
            Err(ScanError { problems })
        }
    }
}

/// Walks `directory`, pushing every file it finds as a path relative to `root`.
///
/// Recursive rather than iterative because an asset tree is a handful of levels deep and a manual
/// work stack would be more code for no benefit. Entries are sorted at each level so the traversal
/// order is the same everywhere (invariant I3).
fn collect_files(
    root: &Path,
    directory: &Path,
    found: &mut Vec<PathBuf>,
    problems: &mut Vec<ScanProblem>,
) {
    let listing = match std::fs::read_dir(directory) {
        Ok(listing) => listing,
        Err(error) => {
            problems.push(ScanProblem::Unreadable {
                path: display(&relative_to(root, directory)),
                message: if directory == root {
                    format!(
                        "{error}. This is the asset directory the project is configured to use; \
                         create it, or point at a different one"
                    )
                } else {
                    error.to_string()
                },
            });
            return;
        }
    };

    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in listing {
        match entry {
            Ok(entry) => entries.push(entry.path()),
            Err(error) => problems.push(ScanProblem::Unreadable {
                path: display(&relative_to(root, directory)),
                message: error.to_string(),
            }),
        }
    }
    // The reason the whole module is reproducible. `read_dir` order is not defined.
    entries.sort();

    for path in entries {
        if is_hidden(&path) {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, found, problems);
        } else {
            found.push(relative_to(root, &path));
        }
    }
}

/// Whether a directory entry should be ignored entirely.
///
/// Anything whose name starts with a dot. That covers `.gitkeep`, `.gitignore`, `.DS_Store`, and
/// `.git` itself, none of which are assets and all of which would otherwise be reported as
/// unimported and then handed a sidecar by the importer.
///
/// **This is the only rule about what counts as an asset**, and it is deliberately not an extension
/// list. Invariant I4 says the engine holds no genre knowledge, and "which file types are assets" is
/// exactly that kind of opinion — a list that knows about `.png` and `.ogg` is a list that has to be
/// edited before anyone can ship a game using a format nobody thought of.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

/// A path relative to `root`, with separators normalised to `/`.
///
/// Falls back to the path as given when it is not under `root`, which cannot happen for a walk that
/// started at `root` but is cheaper than an unwrap that has to be reasoned about.
fn relative_to(root: &Path, path: &Path) -> PathBuf {
    let trimmed = path.strip_prefix(root).unwrap_or(path);
    PathBuf::from(
        trimmed
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// A path as it should appear in a message: forward slashes, everywhere.
fn display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Whether a path is a sidecar rather than an asset.
fn is_sidecar(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some(SIDECAR_EXTENSION)
}

/// Drops a leading byte-order mark.
///
/// PowerShell writes one when redirecting into a file, so a sidecar edited from a shell on Windows
/// can begin with U+FEFF. Rejecting it produced an error pointing at an invisible character in
/// session 6, which is the least actionable message a parser can produce. Only a *leading* one is
/// skipped — a BOM in the middle of a file is genuine corruption and should still be reported.
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway directory tree, removed when the test ends.
    ///
    /// Built by hand rather than with `tempfile` for the same reason the JSON parser is hand-written
    /// (ADR 0016): a dependency for twenty lines is a poor trade in a workspace that has so far kept
    /// its dependency list to `thiserror`.
    struct Tree {
        root: PathBuf,
    }

    impl Tree {
        fn new(name: &str) -> Tree {
            let root = std::env::temp_dir().join(format!(
                "amadeo-scan-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("temp dir");
            Tree { root }
        }

        /// Writes a file, creating parent directories.
        fn write(&self, relative: &str, contents: &str) -> &Tree {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(&path, contents).expect("write");
            self
        }

        /// An asset plus a sidecar declaring `id`.
        fn asset(&self, relative: &str, id: &str) -> &Tree {
            self.write(relative, "not really an image");
            self.write(
                &format!("{relative}.{SIDECAR_EXTENSION}"),
                &format!("id = \"{id}\"\n"),
            );
            self
        }

        fn scan(&self) -> Result<Scan, ScanError> {
            AssetCatalogue::scan(&self.root)
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn finds_assets_with_sidecars_and_names_them_by_declared_id() {
        let tree = Tree::new("basic");
        tree.asset("textures/wall.png", "wall_concrete");
        tree.asset("audio/step.ogg", "footstep");

        let scan = tree.scan().expect("clean tree");

        assert_eq!(
            scan.catalogue.ids().collect::<Vec<_>>(),
            ["footstep", "wall_concrete"]
        );
        // The declared id wins over the filename stem, which is the whole of ADR 0020.
        assert!(scan.catalogue.contains("wall_concrete"));
        assert!(!scan.catalogue.contains("wall"));
    }

    #[test]
    fn nested_directories_are_walked_and_paths_come_back_relative() {
        let tree = Tree::new("nested");
        tree.asset("textures/interior/stone/wall.png", "wall");

        let scan = tree.scan().expect("clean tree");
        let entry = scan.catalogue.get("wall").expect("found");

        // Relative to the scan root, so a catalogue built on two machines is the same catalogue.
        assert_eq!(
            entry.source.to_string_lossy(),
            "textures/interior/stone/wall.png"
        );
    }

    #[test]
    fn stored_paths_use_forward_slashes_on_every_platform() {
        // This is what makes an `assets.list` reply comparable between a Windows machine and Linux
        // CI. Without it every cross-platform assertion needs a special case.
        let tree = Tree::new("slashes");
        tree.asset("a/b/c.png", "c");

        let scan = tree.scan().expect("clean tree");
        let path = scan
            .catalogue
            .get("c")
            .expect("found")
            .source
            .to_string_lossy()
            .into_owned();

        assert!(!path.contains('\\'), "got: {path}");
        assert_eq!(path, "a/b/c.png");
    }

    #[test]
    fn the_scan_is_the_same_whatever_order_the_filesystem_reports() {
        // Cannot force `read_dir` to reorder, so this asserts the observable consequence: the
        // listing is sorted, not insertion-ordered. Invariant I3.
        let tree = Tree::new("ordered");
        tree.asset("z.png", "z");
        tree.asset("a.png", "a");
        tree.asset("m.png", "m");

        let scan = tree.scan().expect("clean tree");
        assert_eq!(scan.catalogue.ids().collect::<Vec<_>>(), ["a", "m", "z"]);
    }

    #[test]
    fn an_asset_without_a_sidecar_is_reported_rather_than_guessed_at() {
        // ADR 0020: inventing an id at scan time would mean the id changed the moment somebody ran
        // an import, which defeats the point of declaring it.
        let tree = Tree::new("unimported");
        tree.asset("wall.png", "wall");
        tree.write("floor.png", "no sidecar here");

        let scan = tree.scan().expect("not an error, just unimported");

        assert_eq!(scan.catalogue.len(), 1);
        assert_eq!(scan.unimported, [PathBuf::from("floor.png")]);
        assert!(!scan.catalogue.contains("floor"));
    }

    #[test]
    fn a_sidecar_whose_asset_is_gone_is_reported() {
        let tree = Tree::new("orphan");
        tree.write("gone.png.ama-meta", "id = \"gone\"\n");

        let scan = tree.scan().expect("not an error");

        assert_eq!(scan.orphaned, [PathBuf::from("gone.png.ama-meta")]);
        // It still claims its id -- the sidecar parsed fine, there is just nothing behind it.
        assert!(scan.catalogue.is_empty());
    }

    #[test]
    fn two_assets_claiming_one_id_are_refused_naming_both() {
        let tree = Tree::new("duplicate");
        tree.asset("textures/wall.png", "wall");
        tree.asset("props/wall.png", "wall");

        let error = tree.scan().expect_err("duplicate id");
        let message = error.to_string();

        assert!(message.contains("textures/wall.png"), "got: {message}");
        assert!(message.contains("props/wall.png"), "got: {message}");
    }

    #[test]
    fn every_problem_is_reported_at_once_not_just_the_first() {
        // One problem per round trip is a functional defect for an agent, which cannot ask a
        // follow-up question. Same reasoning as `amadeo_scene::validate`.
        let tree = Tree::new("many");
        tree.asset("a.png", "same");
        tree.asset("b.png", "same");
        tree.write("c.png", "asset");
        tree.write("c.png.ama-meta", "this is not a setting\n");

        let error = tree.scan().expect_err("two problems");

        assert_eq!(error.problems.len(), 2, "got: {error}");
        assert!(error.to_string().contains("2 problem"), "got: {error}");
    }

    #[test]
    fn a_malformed_sidecar_names_the_file_and_the_line() {
        let tree = Tree::new("malformed");
        tree.write("wall.png", "asset");
        tree.write("wall.png.ama-meta", "id = \"wall\"\nbroken line\n");

        let error = tree.scan().expect_err("malformed");
        let message = error.to_string();

        // The relative path plus the line, because that is the string the user would type and the
        // place they have to go. An absolute path would differ on every machine.
        assert!(message.contains("wall.png.ama-meta:2"), "got: {message}");
        assert!(message.contains("broken line"), "got: {message}");
    }

    #[test]
    fn a_sidecar_with_no_id_says_which_one_to_add() {
        let tree = Tree::new("noid");
        tree.write("wall.png", "asset");
        tree.write("wall.png.ama-meta", "filter = \"nearest\"\n");

        let error = tree.scan().expect_err("no id");
        assert!(error.to_string().contains("id = \"wall\""), "got: {error}");
    }

    #[test]
    fn a_missing_root_is_an_error_rather_than_an_empty_catalogue() {
        // A mistyped asset directory that quietly catalogues nothing looks exactly like "this
        // project has no assets", which is the plausible-but-wrong answer Pillar 2 exists to kill.
        let error = AssetCatalogue::scan(Path::new("definitely/not/here")).expect_err("missing");

        assert!(
            error.to_string().contains("asset directory"),
            "got: {error}"
        );
    }

    #[test]
    fn a_byte_order_mark_on_a_sidecar_is_tolerated() {
        // PowerShell's redirect writes one. Session 6 hit exactly this on the replay parser and the
        // error pointed at an invisible character.
        let tree = Tree::new("bom");
        tree.write("wall.png", "asset");
        tree.write("wall.png.ama-meta", "\u{feff}id = \"wall\"\n");

        let scan = tree.scan().expect("BOM should not be fatal");
        assert!(scan.catalogue.contains("wall"));
    }

    #[test]
    fn dotfiles_are_not_assets() {
        // Otherwise `.gitkeep` shows up as unimported and the importer hands it a sidecar. A dot
        // prefix is the whole rule -- an extension allowlist would be genre knowledge (I4).
        let tree = Tree::new("dotfiles");
        tree.asset("wall.png", "wall");
        tree.write(".gitkeep", "");
        tree.write(".hidden/secret.png", "not an asset either");

        let scan = tree.scan().expect("clean");

        assert_eq!(scan.catalogue.len(), 1);
        assert!(scan.unimported.is_empty(), "got: {:?}", scan.unimported);
    }

    #[test]
    fn settings_survive_the_scan() {
        let tree = Tree::new("settings");
        tree.write("wall.png", "asset");
        tree.write("wall.png.ama-meta", "id = \"wall\"\nfilter = \"nearest\"\n");

        let scan = tree.scan().expect("clean");
        assert_eq!(
            scan.catalogue
                .get("wall")
                .unwrap()
                .settings
                .get("filter")
                .map(String::as_str),
            Some("nearest")
        );
    }
}
