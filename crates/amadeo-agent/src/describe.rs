//! "What can I do?" — the schema, as JSON.
//!
//! Pillar 2 of `docs/03-ai-native-design.md`. This is what `amadeo describe` prints, and the point
//! of it is that the answer is **generated from the code**: never a stale doc, never a guess. If the
//! schema says `Transform` has no `rotation_degrees` field, that is known before the code is
//! written rather than after a confusing debug session.

use crate::json::Json;
use amadeo_ecs::ComponentRegistry;
use amadeo_reflect::{FieldInfo, Interpolation, ScalarKind, SyncPolicy, TypeInfo, TypeKind};

/// The version of the description document's own shape.
///
/// Bumped when the *layout below* changes incompatibly, so a tool reading an old dump can tell
/// "written by an older engine" from "corrupt". Separate from any component's own version.
pub const DESCRIBE_FORMAT_VERSION: u32 = 1;

/// Describes every registered component.
///
/// ```text
/// {
///   "components": { "Transform": { ... } },
///   "format_version": 1
/// }
/// ```
#[must_use]
pub fn describe(registry: &ComponentRegistry) -> Json {
    let components = registry
        .types()
        .iter()
        .map(|info| (info.name.clone(), describe_type(info)));

    Json::object([
        ("components", Json::Object(components.collect())),
        (
            "format_version",
            Json::Int(i64::from(DESCRIBE_FORMAT_VERSION)),
        ),
    ])
}

/// Describes one type.
#[must_use]
pub fn describe_type(info: &TypeInfo) -> Json {
    let mut members = vec![
        ("name", Json::string(&info.name)),
        ("docs", Json::string(&info.docs)),
        ("version", Json::Int(i64::from(info.version))),
    ];

    match &info.kind {
        TypeKind::Struct { fields } => {
            members.push(("kind", Json::string("struct")));
            // An array, not an object: `TypeInfo` keeps fields in declaration order because that is
            // how the author wrote them and how the type reads best. Turning it into an object would
            // sort them and lose that.
            members.push((
                "fields",
                Json::Array(fields.iter().map(describe_field).collect()),
            ));
        }
        TypeKind::Enum { variants } => {
            members.push(("kind", Json::string("enum")));
            members.push((
                "variants",
                Json::Array(
                    variants
                        .iter()
                        .map(|variant| {
                            Json::object([
                                ("name", Json::string(&variant.name)),
                                ("docs", Json::string(&variant.docs)),
                                (
                                    "fields",
                                    Json::Array(
                                        variant.fields.iter().map(describe_field).collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ));
        }
        TypeKind::Scalar(scalar) => {
            members.push(("kind", Json::string("scalar")));
            members.push(("scalar", Json::string(scalar_name(*scalar))));
        }
        TypeKind::List { element } => {
            members.push(("kind", Json::string("list")));
            members.push(("element", Json::string(element)));
        }
        TypeKind::Optional { inner } => {
            members.push(("kind", Json::string("optional")));
            members.push(("inner", Json::string(inner)));
        }
        // Reported as its own kind rather than as a struct, because the difference is exactly what a
        // client needs in order to render it: a struct gets a fixed inspector with one row per known
        // field, a map gets an add-and-remove list. The data is indistinguishable in JSON — both are
        // objects — so the schema is the only place this can be said.
        TypeKind::Map { key, value } => {
            members.push(("kind", Json::string("map")));
            members.push(("key", Json::string(key)));
            members.push(("value", Json::string(value)));
        }
    }

    Json::object(members)
}

/// Describes one field, including the metadata that keeps an agent from guessing.
fn describe_field(field: &FieldInfo) -> Json {
    let mut members = vec![
        ("name", Json::string(&field.name)),
        ("type", Json::string(&field.type_name)),
        ("docs", Json::string(&field.docs)),
    ];

    // Omitted rather than emitted as null when absent: a reader checking `"unit" in field` should
    // get a straight answer, and a document full of nulls is harder to read.
    if let Some(unit) = &field.unit {
        members.push(("unit", Json::string(unit)));
    }
    if let Some(range) = &field.range {
        members.push((
            "range",
            Json::object([
                ("min", Json::Float(range.min)),
                ("max", Json::Float(range.max)),
            ]),
        ));
    }

    // Only when it says something. Every field defaults to not-replicated (ADR 0012), and printing
    // that on all of them would bury the ones that do replicate.
    if field.replication.is_replicated() {
        members.push((
            "replication",
            Json::object([
                ("sync", Json::string(sync_name(field.replication.sync))),
                (
                    "interpolate",
                    Json::string(interpolation_name(field.replication.interpolate)),
                ),
            ]),
        ));
    }

    Json::object(members)
}

fn scalar_name(scalar: ScalarKind) -> &'static str {
    match scalar {
        ScalarKind::Bool => "bool",
        ScalarKind::SignedInt => "int",
        ScalarKind::UnsignedInt => "uint",
        ScalarKind::Float32 => "f32",
        ScalarKind::Float64 => "f64",
        ScalarKind::String => "string",
    }
}

/// The same spellings the `#[reflect(sync = "...")]` attribute accepts, so what an agent reads is
/// what it can write back.
fn sync_name(sync: SyncPolicy) -> &'static str {
    match sync {
        SyncPolicy::Never => "never",
        SyncPolicy::OnChange => "on_change",
        SyncPolicy::Always => "always",
    }
}

/// As with [`sync_name`], these match the attribute's spellings exactly.
fn interpolation_name(interpolation: Interpolation) -> &'static str {
    match interpolation {
        Interpolation::None => "none",
        Interpolation::Linear => "linear",
        Interpolation::Angular => "angular",
    }
}
