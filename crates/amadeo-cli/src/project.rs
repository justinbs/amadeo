//! Finding the project, and the one thing the CLI needs to know about it.
//!
//! A project is a directory with an `amadeo.toml` in it:
//!
//! ```text
//! # Which package hosts the agent. `amadeo describe` launches this.
//! game = "quad-demo"
//! ```
//!
//! # Why this parses a subset of TOML rather than depending on a TOML crate
//!
//! It reads `key = "value"` lines and `#` comments, which is a small and genuinely valid subset of
//! TOML — so the file stays a real TOML file that an editor will highlight and a full parser would
//! accept. Two keys do not justify a dependency, and ADR 0016 already committed to hand-writing the
//! protocol parser for the same reason. If this file ever grows tables or arrays, take the
//! dependency rather than growing the parser; a half-TOML parser that accepts *almost* TOML is
//! exactly the kind of thing that produces confusing errors years later.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// The file that marks a directory as an Amadeo project.
pub(crate) const MANIFEST: &str = "amadeo.toml";

/// What the CLI knows about the project it is standing in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Project {
    /// The directory holding `amadeo.toml`.
    pub(crate) root: PathBuf,
    /// The cargo package that hosts the agent — the one `cargo run -p` launches.
    pub(crate) game: String,
}

impl Project {
    /// Finds the project containing `start`, walking up to the filesystem root.
    ///
    /// Walking up is what lets `amadeo describe` work from anywhere inside a project, the way
    /// `cargo` and `git` do.
    ///
    /// # Errors
    ///
    /// If no `amadeo.toml` is found, or the one found is malformed.
    pub(crate) fn discover(start: &Path) -> Result<Project> {
        let mut directory = Some(start);

        while let Some(current) = directory {
            let candidate = current.join(MANIFEST);
            if candidate.is_file() {
                return Project::load(&candidate);
            }
            directory = current.parent();
        }

        bail!(
            "no {MANIFEST} found in {} or any parent directory.\n\
             An Amadeo project is a directory containing {MANIFEST}:\n\
             \n    game = \"my-game\"\n\n\
             Name the cargo package that hosts the agent. \
             Pass --package to override it for one command.",
            start.display()
        )
    }

    /// Reads a manifest at an exact path.
    ///
    /// # Errors
    ///
    /// If the file cannot be read, or does not name a game.
    pub(crate) fn load(path: &Path) -> Result<Project> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("could not read {}", path.display()))?;

        let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();

        let mut game: Option<String> = None;

        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                bail!(
                    "{}:{}: expected `key = \"value\"`, found `{line}`",
                    path.display(),
                    number + 1
                );
            };

            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match key {
                "game" => game = Some(value.to_string()),
                other => bail!(
                    "{}:{}: unknown setting `{other}`. \
                     The only setting is `game`, naming the package that hosts the agent",
                    path.display(),
                    number + 1
                ),
            }
        }

        let game = game.with_context(|| {
            format!(
                "{} does not name a game. Add:\n\n    game = \"my-game\"\n",
                path.display()
            )
        })?;

        if game.is_empty() {
            bail!("{}: `game` is empty", path.display());
        }

        Ok(Project { root, game })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a manifest into a fresh temporary directory and reads it back.
    fn load_text(text: &str) -> Result<Project> {
        let directory = std::env::temp_dir().join(format!(
            "amadeo-cli-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).expect("temp dir");
        let path = directory.join(MANIFEST);
        std::fs::write(&path, text).expect("write manifest");

        let loaded = Project::load(&path);
        let _ = std::fs::remove_file(&path);
        loaded
    }

    #[test]
    fn reads_the_game_name() {
        let project = load_text("game = \"quad-demo\"\n").expect("valid");
        assert_eq!(project.game, "quad-demo");
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let project = load_text("# which package\n\ngame = \"demo\"\n\n").expect("valid");
        assert_eq!(project.game, "demo");
    }

    #[test]
    fn a_manifest_with_no_game_says_what_to_add() {
        let error = load_text("# nothing here\n").expect_err("no game");
        assert!(error.to_string().contains("game = "), "got: {error}");
    }

    #[test]
    fn an_unknown_setting_is_refused_rather_than_ignored() {
        // Ignoring it means a typo'd key silently does nothing, which is the worst of both.
        let error = load_text("gaem = \"demo\"\n").expect_err("typo");
        assert!(
            error.to_string().contains("unknown setting `gaem`"),
            "got: {error}"
        );
    }

    #[test]
    fn discovery_failing_explains_what_a_project_is() {
        // The error a new user is most likely to hit first, so it has to teach.
        let nowhere = std::env::temp_dir().join("amadeo-definitely-not-a-project");
        std::fs::create_dir_all(&nowhere).expect("temp dir");
        let error = Project::discover(&nowhere).expect_err("not a project");
        assert!(error.to_string().contains(MANIFEST), "got: {error}");
        assert!(error.to_string().contains("game = "), "got: {error}");
    }
}
