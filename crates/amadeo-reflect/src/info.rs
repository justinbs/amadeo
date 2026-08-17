//! The schema: what a reflected type *is*, in machine-readable form.
//!
//! This is the data behind `amadeo describe` (`docs/03-ai-native-design.md` Pillar 2). It answers
//! "what can I do?" — which components exist, what fields they have, what values are valid, and
//! what each one means.

use std::fmt;

/// A primitive that needs no further description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKind {
    /// `bool`.
    Bool,
    /// Any signed integer width.
    SignedInt,
    /// Any unsigned integer width.
    UnsignedInt,
    /// `f32`.
    Float32,
    /// `f64`.
    Float64,
    /// `String` or `&str`.
    String,
}

impl fmt::Display for ScalarKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            ScalarKind::Bool => "bool",
            ScalarKind::SignedInt => "int",
            ScalarKind::UnsignedInt => "uint",
            ScalarKind::Float32 => "f32",
            ScalarKind::Float64 => "f64",
            ScalarKind::String => "string",
        };
        f.write_str(name)
    }
}

/// An inclusive numeric range a field's value should stay within.
///
/// **Advisory, not enforced.** It drives editor slider bounds and tells an agent what a sensible
/// value looks like — which is most of the point, since "plausible but wrong" is the dominant way an
/// agent breaks game code (Pillar 2). Deserialisation does not reject out-of-range values, because a
/// designer deliberately pushing a value past its usual bounds is a legitimate thing to do and
/// should not be a load failure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    /// Lowest expected value, inclusive.
    pub min: f64,
    /// Highest expected value, inclusive.
    pub max: f64,
}

/// How often a field's value should be sent to other machines.
///
/// Reserved by ADR 0006 and **unused until M6**. It is recorded now because component authors have
/// the most context at the moment they write the component, and the alternative is a sweep across
/// every component in the engine later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncPolicy {
    /// Not replicated. **The default, deliberately.**
    ///
    /// Opting in is a decision someone makes on purpose; opting out is something they forget. A
    /// field that should have replicated and does not is a visible gameplay bug. A cache that
    /// replicates because nobody annotated it is invisible bandwidth, found much later.
    #[default]
    Never,
    /// Sent when the value changes. The common choice for gameplay state.
    OnChange,
    /// Sent every network tick, regardless of change. For values that change constantly anyway.
    Always,
}

/// How a replicated value should be smoothed between received updates.
///
/// Reserved by ADR 0006, unused until M6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    /// Snap to each received value. Correct for discrete state — health, an inventory count, a
    /// behaviour state. Interpolating those produces meaningless in-between values.
    #[default]
    None,
    /// Blend linearly. For positions and scales.
    Linear,
    /// Blend along the shorter arc. For rotations, where linear blending takes the long way round
    /// half the time.
    Angular,
}

/// The multiplayer annotations reserved by ADR 0006.
///
/// Authority is deliberately absent: it belongs to an *entity*, not a field, and already exists as
/// `amadeo_core::Authority`. Duplicating it per-field would invite the two to disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Replication {
    /// How often to send this field.
    pub sync: SyncPolicy,
    /// How to smooth it on the receiving side.
    pub interpolate: Interpolation,
}

impl Replication {
    /// Whether this field participates in replication at all.
    #[must_use]
    pub fn is_replicated(self) -> bool {
        !matches!(self.sync, SyncPolicy::Never)
    }
}

/// One field of a reflected struct.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInfo {
    /// The field's name, exactly as it appears in source and in text files.
    pub name: String,
    /// The name of the field's type, as [`TypeInfo::name`] would report it.
    pub type_name: String,
    /// The field's doc comment, with the leading `///` and one space stripped.
    ///
    /// Load-bearing rather than decorative: this is what an agent reads to understand what a field
    /// means without access to the source (`CLAUDE.md` section 6).
    pub docs: String,
    /// Advisory bounds, if declared.
    pub range: Option<Range>,
    /// A unit of measurement, if declared — `"m/s"`, `"degrees"`, `"hp"`.
    ///
    /// Prevents a whole class of "plausible but wrong": an agent that can see a field is in radians
    /// does not pass it degrees.
    pub unit: Option<String>,
    /// What this field becomes when a file does not mention it — ADR 0075. `None` means required.
    ///
    /// Declared per field with `#[reflect(default = <expr>)]`, and **opt-in**: a field without one
    /// still fails with [`ReflectError::MissingField`](crate::ReflectError::MissingField), which is
    /// right for anything whose absence is a mistake rather than a shrug (`BoxMesh::size` has no
    /// sensible default, and a zero-size box draws nothing while reporting no fault).
    ///
    /// Reported in the schema rather than kept in the attribute so that an agent authoring an asset
    /// can *see* which fields it may leave out and what leaving them out means — `docs/12-the-bar.md`
    /// §3, which is a stronger requirement than I5. It is deliberately outside ADR 0069's layout
    /// fingerprint, alongside docs and ranges: a default cannot move a state hash, so fingerprinting
    /// it would reject good saves over a change that provably does not matter.
    pub default: Option<crate::Value>,
    /// Multiplayer annotations (ADR 0006).
    pub replication: Replication,
}

/// One variant of a reflected enum.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantInfo {
    /// The variant's name.
    pub name: String,
    /// The variant's doc comment.
    pub docs: String,
    /// The variant's fields. Empty for a fieldless variant.
    pub fields: Vec<FieldInfo>,
}

/// What shape a reflected type has.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    /// A primitive.
    Scalar(ScalarKind),
    /// Named fields.
    Struct {
        /// The fields, in declaration order.
        ///
        /// Declaration order, not sorted — this is documentation, and a struct reads best in the
        /// order its author wrote it. Sorting belongs to [`crate::Value`], which is what has to be
        /// byte-stable.
        fields: Vec<FieldInfo>,
    },
    /// A closed set of variants.
    Enum {
        /// The variants, in declaration order.
        variants: Vec<VariantInfo>,
    },
    /// A homogeneous sequence.
    List {
        /// The element type's name.
        element: String,
        /// How many elements, when the count is fixed by the type.
        ///
        /// `Some(2)` for `[f32; 2]`, `None` for `Vec<f32>`.
        ///
        /// # Why this is here rather than read off the name
        ///
        /// It used to be absent, and the arity survived only inside the *name* — `"array<f32, 2>"`.
        /// So anything that needed the count had to parse it back out of a string: the editor
        /// deciding whether to draw two boxes or an add-and-remove list, and `describe --example`
        /// deciding how many numbers to emit. Both would have been re-deriving something the type
        /// already knew and the schema had thrown away.
        ///
        /// Found in session 8 while building `--example`, which is the first thing that had to
        /// *produce* a valid value rather than merely report one.
        length: Option<usize>,
    },
    /// Author-chosen keys pointing at values of one type.
    ///
    /// Distinct from [`TypeKind::Struct`] even though [`crate::Value::Map`] and
    /// [`crate::Value::Struct`] hold the same shape: a struct's field set is fixed and an unknown
    /// name is an error, while a map's keys are data and an unknown one is ordinary. An editor reads
    /// this to decide between a fixed inspector and an add-and-remove list.
    Map {
        /// The key type's name. Always something that renders to a string — see
        /// [`ReflectKey`](crate::ReflectKey).
        key: String,
        /// The value type's name.
        value: String,
    },
    /// A value that may be absent.
    Optional {
        /// The contained type's name.
        inner: String,
    },
}

/// Everything the registry knows about one type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeInfo {
    /// The canonical name. This is the registry key and what appears in text files.
    ///
    /// Short by default (`Transform`, not `amadeo_render::components::Transform`) because a
    /// human types it into a scene file. Collisions are rejected at registration rather than
    /// silently resolved — see [`crate::TypeRegistry::register`].
    pub name: String,
    /// The type's doc comment.
    pub docs: String,
    /// The schema version, bumped when fields change incompatibly.
    ///
    /// Starts at 1. A scene file records the version it was written with, so a loader can tell
    /// "this file predates the rename" from "this file is corrupt". The migration machinery that
    /// consumes it lands with `amadeo-scene`; recording the number now is what makes it possible
    /// later, and costs nothing.
    pub version: u32,
    /// The type's shape.
    pub kind: TypeKind,
}

impl TypeInfo {
    /// The fields, if this is a struct. Empty otherwise.
    #[must_use]
    pub fn fields(&self) -> &[FieldInfo] {
        match &self.kind {
            TypeKind::Struct { fields } => fields,
            _ => &[],
        }
    }

    /// Looks up one field by name.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields().iter().find(|field| field.name == name)
    }

    /// Every field that will be replicated once M6 lands.
    ///
    /// Exists so the annotation can be *tested* today rather than discovered to be wrong in M6.
    pub fn replicated_fields(&self) -> impl Iterator<Item = &FieldInfo> {
        self.fields()
            .iter()
            .filter(|field| field.replication.is_replicated())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, replication: Replication) -> FieldInfo {
        FieldInfo {
            name: name.to_string(),
            type_name: "f32".to_string(),
            docs: String::new(),
            range: None,
            unit: None,
            default: None,
            replication,
        }
    }

    #[test]
    fn replication_defaults_to_not_replicated() {
        let default = Replication::default();
        assert_eq!(default.sync, SyncPolicy::Never);
        assert_eq!(default.interpolate, Interpolation::None);
        assert!(!default.is_replicated());
    }

    #[test]
    fn replicated_fields_lists_only_annotated_ones() {
        let info = TypeInfo {
            name: "Motion".to_string(),
            docs: String::new(),
            version: 1,
            kind: TypeKind::Struct {
                fields: vec![
                    field(
                        "position",
                        Replication {
                            sync: SyncPolicy::OnChange,
                            interpolate: Interpolation::Linear,
                        },
                    ),
                    field("cached_speed", Replication::default()),
                    field(
                        "rotation",
                        Replication {
                            sync: SyncPolicy::Always,
                            interpolate: Interpolation::Angular,
                        },
                    ),
                ],
            },
        };

        let replicated: Vec<&str> = info
            .replicated_fields()
            .map(|field| field.name.as_str())
            .collect();
        assert_eq!(replicated, vec!["position", "rotation"]);
    }

    #[test]
    fn field_lookup_finds_by_name() {
        let info = TypeInfo {
            name: "Motion".to_string(),
            docs: String::new(),
            version: 1,
            kind: TypeKind::Struct {
                fields: vec![field("position", Replication::default())],
            },
        };
        assert!(info.field("position").is_some());
        assert!(info.field("missing").is_none());
    }

    #[test]
    fn non_structs_report_no_fields() {
        let info = TypeInfo {
            name: "f32".to_string(),
            docs: String::new(),
            version: 1,
            kind: TypeKind::Scalar(ScalarKind::Float32),
        };
        assert!(info.fields().is_empty());
        assert_eq!(info.field("anything"), None);
    }
}
