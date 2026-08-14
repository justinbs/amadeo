//! The layout fingerprint: whether a recorded state hash still means what it meant.
//!
//! # What this answers
//!
//! ADR 0069. A save file has to survive the game being patched, and a snapshot checks its own work
//! by comparing the rebuilt world's state hash against the one it recorded. Those two requirements
//! collide: a component that gains a field hashes differently, so the recorded number describes a
//! layout that no longer exists and the check refuses a world that was rebuilt perfectly.
//!
//! Dropping the check would be worse than the problem — it is what turns "the restore silently
//! produced a slightly different world" into an error at the moment it happens. So instead the file
//! records a fingerprint of the **shape** of everything it contains, and the check becomes
//! conditional on that fingerprint matching. A player who has not updated gets the full exact check;
//! leniency costs something only in the case that actually needs it.
//!
//! # What goes into it, and what deliberately does not
//!
//! Names and types, in declaration order, recursing through every type a field names. That is
//! exactly the set of things that can change what `StableHash` produces or what shape `from_value`
//! demands — which is the question being asked.
//!
//! **Docs, ranges, units, replication annotations and `version` are all excluded.** They cannot move
//! a state hash. Folding a doc comment in would force a lenient load on every documentation edit:
//! harmless in itself, but it would make the exact path fire so rarely that nobody would notice when
//! it stopped firing at all.
//!
//! # Why it recurses
//!
//! A component's hash is its fields' hashes, all the way down. If `Transform` is unchanged but a
//! struct one of its fields names has gained a field, the state hash moves and a fingerprint over
//! only the top level would say nothing had changed — which is the one failure mode this must not
//! have, because it would enforce a stale hash and reject a good save.

use amadeo_core::StableHasher;
use amadeo_ecs::{ComponentRegistry, World};
use amadeo_reflect::{TypeInfo, TypeKind, TypeRegistry};

/// Whether a name in a snapshot is a component or a resource.
///
/// The two live in separate id spaces (ADR 0017 hashes the canonical name for each independently),
/// so one name can legitimately be both, and a fingerprint that conflated them would miss a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SchemaKind {
    /// A component, built through [`ComponentRegistry`].
    Component,
    /// A resource, built through [`World::restore_resource`].
    Resource,
}

impl SchemaKind {
    /// The word used in a snapshot's `schema` block.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            SchemaKind::Component => "component",
            SchemaKind::Resource => "resource",
        }
    }
}

/// One line of a snapshot's `schema` block: what was in the file, and what version it was.
///
/// The version is [`TypeInfo::version`], which every type already carries and nothing has ever read.
/// **Nothing reads it here either** — it is recorded so that migrations stay an addition rather than
/// a rewrite, because a save written without it could never be migrated: nothing would know what it
/// was written against. See ADR 0069 §6.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchemaEntry {
    /// Component or resource.
    pub kind: SchemaKind,
    /// The canonical name.
    pub name: String,
    /// The schema version this build declared for it.
    pub version: u32,
}

/// The schema of everything a world contains, as the `schema` block records it.
///
/// Only what is actually present, not the whole registry: a component type nobody has an instance of
/// cannot affect this file's state hash, and including it would make the fingerprint differ for
/// changes that provably do not matter to this save.
#[must_use]
pub fn schema_of(world: &World, registry: &ComponentRegistry) -> Vec<SchemaEntry> {
    let mut entries = Vec::new();

    for entity in world.entities() {
        for name in registry.components_of(world, entity).keys() {
            if let Some(info) = registry.info(name) {
                entries.push(SchemaEntry {
                    kind: SchemaKind::Component,
                    name: name.clone(),
                    version: info.version,
                });
            }
        }
    }

    for (name, _) in world.resources() {
        if let Some(info) = world.resource_schema(&name) {
            entries.push(SchemaEntry {
                kind: SchemaKind::Resource,
                name: name.clone(),
                version: info.version,
            });
        }
    }

    // Sorted and deduplicated: every entity carrying a `Transform` contributes the same line, and
    // the file wants one. Sorting is what makes the fingerprint reproducible (invariant I3) and the
    // block byte-stable (I2).
    entries.sort();
    entries.dedup();
    entries
}

/// A fingerprint of the shape of every name in `schema`, as this build defines them.
///
/// Computed identically at capture and at restore. Equal fingerprints mean the layouts are
/// identical and the recorded state hash still means what it meant.
///
/// A name the build no longer has contributes a distinct "absent" marker rather than being skipped,
/// because a component that has been *deleted* is exactly a case where the recorded hash cannot
/// still be right.
#[must_use]
pub fn fingerprint(schema: &[SchemaEntry], world: &World, registry: &ComponentRegistry) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.write_u64(schema.len() as u64);

    for entry in schema {
        hasher.write_u8(match entry.kind {
            SchemaKind::Component => 0,
            SchemaKind::Resource => 1,
        });
        hasher.write_str(&entry.name);

        let info = match entry.kind {
            SchemaKind::Component => registry.info(&entry.name),
            SchemaKind::Resource => world.resource_schema(&entry.name),
        };

        match info {
            Some(info) => {
                // A path rather than a set, so that two sibling fields of the same type are both
                // described in full and only a genuine cycle is cut short.
                let mut path: Vec<String> = Vec::new();
                write_layout(&mut hasher, info, registry.types(), &mut path);
            }
            // Deleted since the file was written. Distinct from any real layout.
            None => hasher.write_u8(0xff),
        }
    }

    hasher.finish()
}

/// Hashes one type's shape, recursing through every type its fields name.
fn write_layout(
    hasher: &mut StableHasher,
    info: &TypeInfo,
    types: &TypeRegistry,
    path: &mut Vec<String>,
) {
    if path.contains(&info.name) {
        // ADR 0030 registers field types transitively and a type may reach itself, so this is a
        // real shape rather than a broken registry. The marker is enough: the type's full layout is
        // already being written further up the path.
        hasher.write_u8(0xfe);
        return;
    }
    path.push(info.name.clone());

    hasher.write_str(&info.name);
    match &info.kind {
        TypeKind::Scalar(scalar) => {
            hasher.write_u8(1);
            hasher.write_str(&scalar.to_string());
        }
        TypeKind::Struct { fields } => {
            hasher.write_u8(2);
            hasher.write_u64(fields.len() as u64);
            // Declaration order, because that is the order `#[derive(StableHash)]` hashes in.
            // Sorting here would make two structs with the same fields in a different order
            // fingerprint the same, and they do not hash the same.
            for field in fields {
                hasher.write_str(&field.name);
                write_named_layout(hasher, &field.type_name, types, path);
            }
        }
        TypeKind::Enum { variants } => {
            hasher.write_u8(3);
            hasher.write_u64(variants.len() as u64);
            for variant in variants {
                hasher.write_str(&variant.name);
                hasher.write_u64(variant.fields.len() as u64);
                for field in &variant.fields {
                    hasher.write_str(&field.name);
                    write_named_layout(hasher, &field.type_name, types, path);
                }
            }
        }
        TypeKind::List { element, length } => {
            hasher.write_u8(4);
            // A fixed length is part of the shape: `[f32; 2]` and `[f32; 3]` hash differently.
            hasher.write_u64(length.unwrap_or(usize::MAX) as u64);
            write_named_layout(hasher, element, types, path);
        }
        TypeKind::Map { key, value } => {
            hasher.write_u8(5);
            hasher.write_str(key);
            write_named_layout(hasher, value, types, path);
        }
        TypeKind::Optional { inner } => {
            hasher.write_u8(6);
            write_named_layout(hasher, inner, types, path);
        }
    }

    path.pop();
}

/// Resolves a named type and hashes its shape, or records that the registry does not have it.
fn write_named_layout(
    hasher: &mut StableHasher,
    name: &str,
    types: &TypeRegistry,
    path: &mut Vec<String>,
) {
    match types.get(name) {
        Some(info) => write_layout(hasher, info, types, path),
        // A hole in the registry rather than a layout change (ADR 0030 says every type a field
        // names should be registered). Recorded rather than ignored, so the fingerprint notices if
        // one appears or disappears.
        None => {
            hasher.write_u8(0xfd);
            hasher.write_str(name);
        }
    }
}
