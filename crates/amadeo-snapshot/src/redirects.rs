//! Renames, as a text file rather than as code (ADR 0069 §4).
//!
//! # Why this exists at all
//!
//! Defaulting a missing field lets a save survive a component *gaining* one. It does nothing for a
//! **rename**, which is the most common breaking change there is — and discovering after release
//! that the mechanism does not exist means a save file that cannot be recovered by anybody, which
//! is the harm Q37 exists to prevent.
//!
//! # Why a file and not a registration call
//!
//! This is Unreal's `CoreRedirects`, which is an ini file of `OldName -> NewName` rather than
//! migration code. Data instead of registered functions is the same grain as ADR 0068's facts and
//! ADR 0066's tracks, and it has the same payoff: a rename is fixed by editing a file a person can
//! read, in a session with no agent in it, without recompiling anything.
//!
//! ```text
//! amadeo-redirects 1
//! component Sprinting Running
//! field CharacterController top_speed max_speed
//! ```
//!
//! # The ordering rule, which is the part that bites
//!
//! **Component redirects apply first, and a field redirect names the component by its NEW name.**
//! When a type and one of its fields are renamed in the same patch, that order is what decides
//! whether the second redirect fires at all — and getting it the other way round produces a file
//! that looks correct and silently does nothing.

use std::collections::BTreeMap;

/// Why a redirect file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct RedirectError {
    /// Which line, counting from one.
    pub line: usize,
    /// What was wrong, and what would have been right.
    pub message: String,
}

/// The format version this build writes and reads.
pub const REDIRECT_VERSION: u32 = 1;

/// Old names mapped to current ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Redirects {
    /// Old component or resource name to new. One namespace, because a rename is a rename and
    /// keeping two tables would mean an author having to know which kind a name was.
    components: BTreeMap<String, String>,
    /// `(current component name, old field name)` to new field name.
    fields: BTreeMap<(String, String), String>,
}

/// How many times a rename may be followed before it is treated as a cycle.
///
/// A name renamed twice across two patches (`A` to `B`, then `B` to `C`) has to reach `C` from an
/// old save, so following the chain is the behaviour an author expects. A bound rather than a cycle
/// set because the bound is one line and no real project has sixteen renames of one type.
const MAX_HOPS: usize = 16;

impl Redirects {
    /// An empty set, which changes nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads a `.redirects` file.
    ///
    /// Blank lines and lines starting with `#` are ignored, so a file can carry a note about which
    /// release each rename belongs to — which is the first thing anybody maintaining one wants.
    ///
    /// # Errors
    ///
    /// A [`RedirectError`] naming the line and what a valid entry looks like.
    pub fn parse(text: &str) -> Result<Self, RedirectError> {
        let mut lines = text
            .lines()
            .enumerate()
            .map(|(index, line)| (index + 1, line));

        let (number, header) = lines
            .by_ref()
            .find(|(_, line)| !is_ignorable(line))
            .ok_or_else(|| RedirectError {
                line: 1,
                message: format!(
                    "a redirect file must start with `amadeo-redirects {REDIRECT_VERSION}`, and \
                     this one is empty"
                ),
            })?;

        let version = header
            .trim()
            .strip_prefix("amadeo-redirects ")
            .and_then(|rest| rest.trim().parse::<u32>().ok())
            .ok_or_else(|| RedirectError {
                line: number,
                message: format!(
                    "a redirect file must start with `amadeo-redirects {REDIRECT_VERSION}`, and \
                     this starts with `{}`",
                    header.trim()
                ),
            })?;

        if version != REDIRECT_VERSION {
            return Err(RedirectError {
                line: number,
                message: format!(
                    "this file is redirect format version {version}, and this build reads \
                     {REDIRECT_VERSION}"
                ),
            });
        }

        let mut redirects = Self::new();
        for (number, line) in lines {
            if is_ignorable(line) {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts.as_slice() {
                ["component", from, to] => {
                    redirects
                        .components
                        .insert((*from).to_string(), (*to).to_string());
                }
                ["field", owner, from, to] => {
                    redirects.fields.insert(
                        ((*owner).to_string(), (*from).to_string()),
                        (*to).to_string(),
                    );
                }
                _ => {
                    return Err(RedirectError {
                        line: number,
                        message: format!(
                            "`{}` is not a redirect. They are written `component OldName NewName` \
                             or `field ComponentName old_field new_field`",
                            line.trim()
                        ),
                    });
                }
            }
        }

        Ok(redirects)
    }

    /// Whether anything is declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.components.is_empty() && self.fields.is_empty()
    }

    /// The current name for a component or resource, following a chain of renames.
    ///
    /// Returns the name unchanged when nothing redirects it, so a caller never has to check first.
    #[must_use]
    pub fn component(&self, name: &str) -> String {
        let mut current = name.to_string();
        for _ in 0..MAX_HOPS {
            match self.components.get(&current) {
                Some(next) => current = next.clone(),
                None => return current,
            }
        }
        // Sixteen hops means the file redirects a name back to itself. Returning the name it
        // started as is the least surprising answer: the component then either resolves or is
        // reported as unknown, rather than the load failing with a message about redirects.
        name.to_string()
    }

    /// The current name for a field of `owner`, where `owner` is already the **new** component name.
    ///
    /// See the module docs for why that is not the old one.
    #[must_use]
    pub fn field(&self, owner: &str, name: &str) -> String {
        let mut current = name.to_string();
        for _ in 0..MAX_HOPS {
            match self.fields.get(&(owner.to_string(), current.clone())) {
                Some(next) => current = next.clone(),
                None => return current,
            }
        }
        name.to_string()
    }
}

/// Blank, or a comment.
fn is_ignorable(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with('#')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_renames_reads_back() {
        let redirects = Redirects::parse(
            "amadeo-redirects 1\n\
             component Sprinting Running\n\
             field CharacterController top_speed max_speed\n",
        )
        .expect("valid");

        assert_eq!(redirects.component("Sprinting"), "Running");
        assert_eq!(
            redirects.field("CharacterController", "top_speed"),
            "max_speed"
        );
    }

    #[test]
    fn a_name_nothing_redirects_comes_back_unchanged() {
        // So a caller never has to ask whether a redirect exists before using the answer.
        let redirects = Redirects::new();
        assert_eq!(redirects.component("Transform"), "Transform");
        assert_eq!(redirects.field("Transform", "scale"), "scale");
    }

    #[test]
    fn a_rename_renamed_again_reaches_the_current_name() {
        // Two patches, two renames, and a save from before either of them. Following the chain is
        // what an author writing the second line expects, and stopping at the first hop would
        // silently drop the component.
        let redirects =
            Redirects::parse("amadeo-redirects 1\ncomponent A B\ncomponent B C\n").expect("valid");
        assert_eq!(redirects.component("A"), "C");
    }

    #[test]
    fn a_cycle_resolves_to_the_original_rather_than_hanging() {
        let redirects =
            Redirects::parse("amadeo-redirects 1\ncomponent A B\ncomponent B A\n").expect("valid");
        // The name then either resolves or is reported as unknown, which is a far better failure
        // than a load that never returns.
        assert_eq!(redirects.component("A"), "A");
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        // The first thing anybody maintaining one of these wants is to note which release a rename
        // belongs to.
        let redirects = Redirects::parse(
            "amadeo-redirects 1\n\n# 1.1: the lantern became a torch\ncomponent Lantern Torch\n",
        )
        .expect("valid");
        assert_eq!(redirects.component("Lantern"), "Torch");
    }

    #[test]
    fn a_line_that_is_not_a_redirect_says_what_one_looks_like() {
        let error = Redirects::parse("amadeo-redirects 1\nLantern Torch\n").expect_err("bad line");
        assert_eq!(error.line, 2);
        assert!(
            error.message.contains("component OldName NewName"),
            "{error}"
        );
    }

    #[test]
    fn a_file_that_is_not_a_redirect_file_says_so() {
        let error = Redirects::parse("amadeo-snapshot 2\n").expect_err("wrong format");
        assert!(error.message.contains("amadeo-redirects"), "{error}");
    }
}
