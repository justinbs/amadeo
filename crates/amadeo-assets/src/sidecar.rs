//! The `.ama-meta` sidecar: an asset's declared id and its import settings.
//!
//! One sits next to each asset — `textures/wall.png` is described by
//! `textures/wall.png.ama-meta`. It is text, hand-editable, and committed alongside the asset
//! (invariant I1: metadata is not allowed to hide in a binary the way Unity's `.meta` GUIDs do).
//!
//! ```text
//! # textures/wall_concrete.png.ama-meta
//! id = "wall_concrete"
//! filter = "nearest"
//! ```
//!
//! # Why this syntax
//!
//! The same `key = "value"` shape as `amadeo.toml`, so anyone editing either sees one syntax rather
//! than two. It is a genuine TOML subset, which means an editor highlights it correctly and a full
//! TOML parser would accept it.
//!
//! It deliberately does **not** reuse the `.scene` format (ADR 0014). That format's whole design is
//! indentation-as-nesting, and a sidecar is flat — borrowing a nesting syntax for something with no
//! nesting would advertise a capability that does not exist.
//!
//! The parser here is separate from the one `amadeo-cli` uses for `amadeo.toml`, and that
//! duplication is deliberate: about thirty lines each, both independently readable, versus a shared
//! crate that exists only to hold thirty lines and forces a dependency edge from the CLI to the
//! asset layer just to read a project manifest.

use std::collections::BTreeMap;
use std::path::Path;

/// The extension appended to an asset's own filename to find its sidecar.
pub const SIDECAR_EXTENSION: &str = "ama-meta";

/// A sidecar that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{path}:{line}: {kind}")]
pub struct SidecarError {
    /// The file the problem is in.
    pub path: String,
    /// The 1-based line, or 0 when the problem is with the file as a whole.
    pub line: usize,
    /// What went wrong.
    pub kind: SidecarErrorKind,
}

/// What specifically went wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SidecarErrorKind {
    /// A line was neither blank, a comment, nor `key = value`.
    #[error("expected `key = \"value\"`, a `#` comment, or a blank line, found `{found}`")]
    NotASetting {
        /// The offending line, trimmed.
        found: String,
    },

    /// The same key was set twice.
    #[error(
        "`{key}` is set twice; the second would silently win, so it is refused. \
         Delete one of them"
    )]
    DuplicateKey {
        /// The repeated key.
        key: String,
    },

    /// No `id` line.
    #[error(
        "no `id` set. Every asset needs one -- it is how scenes refer to this asset, and it is what \
         lets the file be moved or renamed without breaking them (ADR 0020).\n\
         Add:    id = \"{suggestion}\""
    )]
    MissingId {
        /// The filename stem, which is what an import would have defaulted to.
        suggestion: String,
    },

    /// The `id` was set to nothing.
    #[error("`id` is empty. It is what scenes refer to this asset by, so it has to be something")]
    EmptyId,

    /// The `id` contains something that would be painful in a scene file.
    #[error(
        "`{id}` is not a usable id. Use letters, digits, underscores, dashes and dots -- an id \
         appears bare in a scene file (`from {id}`), so whitespace or quotes in one would make the \
         line ambiguous"
    )]
    UnusableId {
        /// The id that was rejected.
        id: String,
    },
}

/// An asset's declared metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    /// The declared id — the asset's identity (ADR 0020).
    pub id: String,
    /// Everything else, sorted so a rewrite is byte-stable (invariant I2).
    ///
    /// Import settings are deliberately not modelled as typed fields here. What a texture needs
    /// (`filter`, `wrap`) and what an audio clip needs (`loop`, `gain`) are different, and the
    /// importer for each kind is the thing that should know. Holding them as text keeps this layer
    /// out of that argument.
    pub settings: BTreeMap<String, String>,
}

impl Sidecar {
    /// A sidecar with just an id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Sidecar {
            id: id.into(),
            settings: BTreeMap::new(),
        }
    }

    /// The id an asset at this path would get by default on import.
    ///
    /// The filename stem, so `textures/wall_concrete.png` defaults to `wall_concrete`. ADR 0020
    /// chose this so an id reads exactly like a path on day one — an agent listing a directory
    /// guesses right nearly every time — while still being *recorded*, so moving the file later
    /// changes nothing.
    ///
    /// Returns `None` for a path with no usable stem.
    #[must_use]
    pub fn default_id_for(asset: &Path) -> Option<String> {
        let stem = asset.file_stem()?.to_str()?;
        if stem.is_empty() || !is_usable_id(stem) {
            return None;
        }
        Some(stem.to_string())
    }

    /// Reads a sidecar.
    ///
    /// # Errors
    ///
    /// [`SidecarError`] with a line number, for anything malformed or a missing `id`.
    pub fn parse(text: &str, path: &Path) -> Result<Sidecar, SidecarError> {
        let display = path.display().to_string();
        let fail = |line: usize, kind: SidecarErrorKind| SidecarError {
            path: display.clone(),
            line,
            kind,
        };

        let mut settings: BTreeMap<String, String> = BTreeMap::new();
        let mut id: Option<String> = None;

        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim();
            let number = index + 1;

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                return Err(fail(
                    number,
                    SidecarErrorKind::NotASetting {
                        found: line.to_string(),
                    },
                ));
            };

            let key = key.trim().to_string();
            let value = value.trim().trim_matches('"').to_string();

            if key == "id" {
                if id.is_some() {
                    return Err(fail(number, SidecarErrorKind::DuplicateKey { key }));
                }
                if value.is_empty() {
                    return Err(fail(number, SidecarErrorKind::EmptyId));
                }
                if !is_usable_id(&value) {
                    return Err(fail(number, SidecarErrorKind::UnusableId { id: value }));
                }
                id = Some(value);
                continue;
            }

            if settings.insert(key.clone(), value).is_some() {
                return Err(fail(number, SidecarErrorKind::DuplicateKey { key }));
            }
        }

        let id = id.ok_or_else(|| {
            // The suggestion is the id an import would have chosen, so the fix is copy-pasteable.
            // `path` is the *sidecar*, so the asset it describes has to be recovered first --
            // `wall.png.ama-meta` describes `wall.png`, whose default id is `wall`.
            let suggestion = asset_path_for(path)
                .as_deref()
                .and_then(Sidecar::default_id_for)
                .unwrap_or_else(|| "my_asset".to_string());
            fail(0, SidecarErrorKind::MissingId { suggestion })
        })?;

        Ok(Sidecar { id, settings })
    }

    /// Writes a sidecar in canonical form.
    ///
    /// `id` first because it is the identity and the thing a reader looks for; everything else
    /// sorted, so rewriting an unchanged sidecar reproduces it byte for byte (invariant I2).
    /// LF endings, like every other text format here — see `.gitattributes`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("id = \"{}\"\n", self.id));
        for (key, value) in &self.settings {
            out.push_str(&format!("{key} = \"{value}\"\n"));
        }
        out
    }
}

/// The asset a sidecar describes: the same path with `.ama-meta` removed.
///
/// `textures/wall.png.ama-meta` describes `textures/wall.png`. Appending the extension rather than
/// replacing it is deliberate — `wall.png.ama-meta` and `wall.ogg.ama-meta` can coexist in one
/// directory, where a `wall.ama-meta` scheme would make them collide.
///
/// Returns `None` if the path is not a sidecar.
#[must_use]
pub fn asset_path_for(sidecar: &Path) -> Option<std::path::PathBuf> {
    if sidecar.extension()?.to_str()? != SIDECAR_EXTENSION {
        return None;
    }
    Some(sidecar.with_extension(""))
}

/// The sidecar describing an asset: the same path with `.ama-meta` appended.
#[must_use]
pub fn sidecar_path_for(asset: &Path) -> std::path::PathBuf {
    let mut name = asset.as_os_str().to_os_string();
    name.push(".");
    name.push(SIDECAR_EXTENSION);
    std::path::PathBuf::from(name)
}

/// Whether a string can be used as an id.
///
/// An id appears **bare** in a scene file (`from wall_concrete`), so anything that would make that
/// line ambiguous to the scene parser is refused here rather than producing a confusing error two
/// layers away.
#[must_use]
pub fn is_usable_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Sidecar, SidecarError> {
        Sidecar::parse(text, Path::new("textures/wall.png.ama-meta"))
    }

    #[test]
    fn reads_an_id_and_its_settings() {
        let sidecar = parse("id = \"wall\"\nfilter = \"nearest\"\n").expect("valid");
        assert_eq!(sidecar.id, "wall");
        assert_eq!(
            sidecar.settings.get("filter").map(String::as_str),
            Some("nearest")
        );
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let sidecar = parse("# a wall\n\nid = \"wall\"\n\n").expect("valid");
        assert_eq!(sidecar.id, "wall");
    }

    #[test]
    fn round_trips_byte_for_byte() {
        // Invariant I2. A sidecar rewritten unchanged must reproduce exactly, or every import
        // churns the diff.
        let original = "id = \"wall\"\nfilter = \"nearest\"\nwrap = \"clamp\"\n";
        let parsed = parse(original).expect("valid");
        assert_eq!(parsed.to_text(), original);
        assert_eq!(
            Sidecar::parse(&parsed.to_text(), Path::new("x")),
            Ok(parsed)
        );
    }

    #[test]
    fn settings_come_out_sorted_whatever_order_they_went_in() {
        let forwards = parse("id = \"w\"\na = \"1\"\nz = \"2\"\n").expect("valid");
        let backwards = parse("id = \"w\"\nz = \"2\"\na = \"1\"\n").expect("valid");
        assert_eq!(forwards.to_text(), backwards.to_text());
    }

    #[test]
    fn a_missing_id_suggests_the_one_an_import_would_have_used() {
        // The most likely mistake, so the message has to carry the fix rather than the diagnosis.
        let error = parse("filter = \"nearest\"\n").expect_err("no id");
        let message = error.to_string();
        assert!(message.contains("id = \"wall\""), "got: {message}");
        assert!(message.contains("ADR 0020"), "got: {message}");
    }

    #[test]
    fn a_sidecar_path_and_its_asset_convert_both_ways() {
        let asset = Path::new("textures/wall.png");
        let sidecar = sidecar_path_for(asset);

        assert_eq!(sidecar, Path::new("textures/wall.png.ama-meta"));
        assert_eq!(asset_path_for(&sidecar).as_deref(), Some(asset));

        // Appending rather than replacing is what lets two assets with one stem coexist.
        assert_ne!(
            sidecar_path_for(Path::new("wall.png")),
            sidecar_path_for(Path::new("wall.ogg"))
        );

        // Not a sidecar.
        assert_eq!(asset_path_for(Path::new("textures/wall.png")), None);
    }

    #[test]
    fn the_default_id_is_the_filename_stem() {
        assert_eq!(
            Sidecar::default_id_for(Path::new("textures/wall_concrete.png")).as_deref(),
            Some("wall_concrete")
        );
        assert_eq!(
            Sidecar::default_id_for(Path::new("a/b/c/door-01.png")).as_deref(),
            Some("door-01")
        );
    }

    #[test]
    fn a_filename_that_would_make_a_bad_id_gets_none_rather_than_a_bad_default() {
        // Spaces would make `from my wall` ambiguous in a scene file, so the import has to ask
        // rather than guess something broken.
        assert_eq!(Sidecar::default_id_for(Path::new("my wall.png")), None);
    }

    #[test]
    fn ids_that_would_break_a_scene_line_are_refused() {
        // `from <id>` is parsed by splitting on whitespace, so an id with a space in it would be
        // read as an id plus a trailing token -- a confusing error two layers away from the cause.
        for bad in ["my wall", "wall\"quote", "wall#hash", "wall/slash"] {
            let text = format!("id = \"{bad}\"\n");
            assert!(parse(&text).is_err(), "`{bad}` should be refused");
        }
        for good in ["wall", "wall_01", "wall-01", "wall.png", "Wall01"] {
            let text = format!("id = \"{good}\"\n");
            assert!(parse(&text).is_ok(), "`{good}` should be accepted");
        }
    }

    #[test]
    fn a_duplicate_key_is_refused_rather_than_last_one_winning() {
        assert!(parse("id = \"a\"\nid = \"b\"\n").is_err());
        assert!(parse("id = \"a\"\nfilter = \"x\"\nfilter = \"y\"\n").is_err());
    }

    #[test]
    fn a_malformed_line_reports_its_number() {
        let error = parse("id = \"wall\"\nthis is not a setting\n").expect_err("malformed");
        assert_eq!(error.line, 2);
        assert!(error.to_string().contains("this is not a setting"));
    }
}
