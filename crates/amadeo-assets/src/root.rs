//! Finding the asset directory, from wherever the process happens to be standing.
//!
//! # The problem this solves
//!
//! A game names its asset directory with a *relative* path — `assets`. Relative to what? The
//! working directory, normally. But the working directory is different in every way a game gets
//! started:
//!
//! | how it was started | working directory |
//! |---|---|
//! | `amadeo describe ...` | the project root — the CLI sets it |
//! | `cargo run -p quad-demo` from the repo root | the repo root |
//! | `cargo run` from inside `games/quad-demo` | `games/quad-demo` |
//! | double-clicking `target/debug/quad-demo.exe` | anywhere at all |
//!
//! Two of those find the assets and two do not, which is the kind of failure that costs an
//! afternoon because the game is not *wrong*, it is just looking somewhere else.
//!
//! # How other engines answer it, and what this one does
//!
//! **Bevy** reads `BEVY_ASSET_ROOT`, then `CARGO_MANIFEST_DIR`, then falls back to the executable's
//! directory. It works, but it makes the answer depend on the environment a process was launched
//! with, which is invisible in a bug report. `CARGO_MANIFEST_DIR` is also per-*crate* rather than
//! per-workspace, so in a workspace like this one it points at `games/quad-demo` rather than the
//! repository root — a fine answer, but a different one, and not the one anybody would guess.
//! Bevy's own issue tracker calls setting that variable by hand "unsanitary".
//!
//! **Godot** anchors on a marker file: `res://` is defined as the directory containing
//! `project.godot`, full stop. No environment, no working directory, one rule.
//!
//! **Amadeo takes Godot's approach**, because this project already has the marker file and already
//! walks up for it — `amadeo-cli` finds `amadeo.toml` exactly this way so that `amadeo describe`
//! works from any subdirectory. Having the game resolve its assets by a *different* rule than the
//! CLI resolves the project would mean the two could disagree about which project they are in, which
//! is a genuinely nasty class of bug and buys nothing.
//!
//! So: **walk up for `amadeo.toml`; the asset directory is relative to whatever contains it.**
//!
//! # The fallback, and why it is last
//!
//! A shipped game has no `amadeo.toml` next to it — the manifest is a development-time thing. So
//! when no marker is found, the executable's own directory is used, which is where a packaged
//! game's assets sit. That is Bevy's fallback and it is the right one; it is just not the *first*
//! thing to try during development.
//!
//! [`AssetRoot::anchor`] records which rule fired, so a game that cannot find its textures can be
//! asked rather than guessed about.

use std::path::{Path, PathBuf};

/// The file that marks a directory as an Amadeo project.
///
/// Deliberately the same file `amadeo-cli` looks for. One marker, one project.
pub const PROJECT_MARKER: &str = "amadeo.toml";

/// Which rule found the asset root.
///
/// Reported by `assets.list`, because "I looked in the wrong place" and "the files are missing" are
/// different problems with the same symptom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// An `amadeo.toml` was found by walking up. The normal development case.
    ProjectMarker,
    /// No marker; the executable's own directory was used. The shipped-game case.
    Executable,
    /// Neither was available, so the working directory was used.
    ///
    /// Only reachable when the executable's path cannot be read, which is rare enough that it is
    /// worth naming rather than folding into the case above.
    WorkingDirectory,
}

impl Anchor {
    /// A short name, for a protocol reply.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Anchor::ProjectMarker => "project-marker",
            Anchor::Executable => "executable",
            Anchor::WorkingDirectory => "working-directory",
        }
    }
}

/// Where a game's assets are, and how that was worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRoot {
    /// The directory to scan.
    pub path: PathBuf,
    /// The directory the relative path was resolved against.
    pub base: PathBuf,
    /// Which rule produced `base`.
    pub anchor: Anchor,
}

/// The directory containing the nearest `amadeo.toml`, walking up from `start`.
///
/// Returns `None` if there is no marker anywhere above `start`.
#[must_use]
pub fn project_root_from(start: &Path) -> Option<PathBuf> {
    let mut directory = Some(start);

    while let Some(current) = directory {
        if current.join(PROJECT_MARKER).is_file() {
            return Some(current.to_path_buf());
        }
        directory = current.parent();
    }

    None
}

/// Resolves an asset directory named relative to the project.
///
/// An absolute `relative` is used unchanged — a caller that already knows exactly where the assets
/// are should not have its answer second-guessed.
///
/// Never fails: the fallbacks bottom out at the working directory. A root that does not exist is
/// reported by [`crate::AssetCatalogue::scan`] instead, which is the layer that can say what was
/// actually missing.
#[must_use]
pub fn resolve(relative: &Path) -> AssetRoot {
    // Absolute paths are already an answer.
    if relative.is_absolute() {
        return AssetRoot {
            path: relative.to_path_buf(),
            base: relative.parent().unwrap_or(relative).to_path_buf(),
            anchor: Anchor::WorkingDirectory,
        };
    }

    let (base, anchor) = base_directory();
    AssetRoot {
        path: base.join(relative),
        base,
        anchor,
    }
}

/// The directory a relative asset path is resolved against.
///
/// Marker first, then the executable, then the working directory. Split out from [`resolve`] so the
/// order is stated once and is readable on its own.
fn base_directory() -> (PathBuf, Anchor) {
    let working = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // The development case, and the one that matches how the CLI finds the project.
    if let Some(root) = project_root_from(&working) {
        return (root, Anchor::ProjectMarker);
    }

    // A game launched from outside its project -- most likely a shipped build, where the marker was
    // never packaged. The executable's own directory is where a packaged game's assets sit.
    //
    // The marker is looked for from the executable too, because `cargo run` from a subdirectory
    // leaves the working directory below the project while the binary still lives inside it.
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        if let Some(root) = project_root_from(directory) {
            return (root.clone(), Anchor::ProjectMarker);
        }
        return (directory.to_path_buf(), Anchor::Executable);
    }

    (working, Anchor::WorkingDirectory)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway directory tree.
    struct Tree {
        root: PathBuf,
    }

    impl Tree {
        fn new(name: &str) -> Tree {
            let root = std::env::temp_dir().join(format!(
                "amadeo-root-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("temp dir");
            Tree { root }
        }

        fn dir(&self, relative: &str) -> PathBuf {
            let path = self.root.join(relative);
            std::fs::create_dir_all(&path).expect("dir");
            path
        }

        fn marker(&self) -> &Tree {
            std::fs::write(self.root.join(PROJECT_MARKER), "game = \"demo\"\n").expect("write");
            self
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn the_marker_is_found_from_the_directory_containing_it() {
        let tree = Tree::new("here");
        tree.marker();

        assert_eq!(project_root_from(&tree.root).as_deref(), Some(&*tree.root));
    }

    #[test]
    fn the_marker_is_found_by_walking_up_from_a_subdirectory() {
        // The case that makes `cargo run` from inside `games/quad-demo` work, and the same rule
        // `amadeo-cli` uses so the two cannot disagree about which project they are in.
        let tree = Tree::new("above");
        tree.marker();
        let deep = tree.dir("games/quad-demo/src");

        assert_eq!(project_root_from(&deep).as_deref(), Some(&*tree.root));
    }

    #[test]
    fn a_tree_with_no_marker_finds_nothing_rather_than_guessing() {
        let tree = Tree::new("bare");
        let deep = tree.dir("a/b");

        // No marker was written, so nothing above `deep` inside the tree qualifies. A parent of the
        // temp directory could in principle hold one, which would be a genuinely surprising machine
        // -- assert only that it did not pick the tree itself.
        assert_ne!(project_root_from(&deep).as_deref(), Some(&*tree.root));
    }

    #[test]
    fn the_nearest_marker_wins() {
        // A game inside a project inside another project resolves to the closest one, because that
        // is the one it belongs to.
        let outer = Tree::new("nested");
        outer.marker();
        let inner = outer.dir("vendor/other-project");
        std::fs::write(inner.join(PROJECT_MARKER), "game = \"other\"\n").expect("write");
        let deep = outer.dir("vendor/other-project/src");

        assert_eq!(project_root_from(&deep).as_deref(), Some(&*inner));
    }

    #[test]
    fn an_absolute_asset_path_is_taken_at_face_value() {
        let tree = Tree::new("absolute");
        let assets = tree.dir("somewhere/assets");

        let resolved = resolve(&assets);
        assert_eq!(resolved.path, assets);
    }

    #[test]
    fn a_relative_path_is_joined_onto_the_base() {
        // Whatever the base turns out to be on this machine, the relative part is appended to it
        // rather than to the working directory.
        let resolved = resolve(Path::new("assets"));

        assert!(resolved.path.ends_with("assets"), "got: {resolved:?}");
        assert_eq!(resolved.path, resolved.base.join("assets"));
    }

    #[test]
    fn the_anchor_is_reportable() {
        // `assets.list` shows this, because "I looked in the wrong place" and "the files are
        // missing" are different problems with identical symptoms.
        assert_eq!(Anchor::ProjectMarker.name(), "project-marker");
        assert_eq!(Anchor::Executable.name(), "executable");
        assert_eq!(Anchor::WorkingDirectory.name(), "working-directory");
    }

    #[test]
    fn resolution_inside_this_repository_finds_the_repository() {
        // The test process runs with its working directory inside the workspace, which does have an
        // `amadeo.toml`. So this asserts the real development path, not a synthetic one.
        let resolved = resolve(Path::new("assets"));

        assert_eq!(resolved.anchor, Anchor::ProjectMarker, "got: {resolved:?}");
        assert!(
            resolved.base.join(PROJECT_MARKER).is_file(),
            "the base should contain the marker, got: {resolved:?}"
        );
    }
}
