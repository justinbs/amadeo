//! Writing a [`Snapshot`] out in canonical form.
//!
//! # The format
//!
//! ```text
//! amadeo-snapshot 1
//! tick 240
//! state-hash 54d624e36fa50dd4
//!
//! resources
//!   Camera2d
//!     center 0.0 0.0
//!     height 10.0
//!   SimRng
//!     increment 1
//!     state 6364136223846793006
//!
//! entities
//!   0:0
//!     Transform
//!       rotation 0.0 0.0 0.0
//!       scale 1.0 1.0 1.0
//!       translation 0.0 0.0 0.0
//!   1:0
//!
//! free
//!   4:2
//!   3:1
//! ```
//!
//! Two spaces per level, like every other text format here. Blocks appear in a fixed order and are
//! **omitted entirely when empty**, so a world with no free slots has no `free` block rather than an
//! empty one — one representation per state is what byte-stability needs.
//!
//! An entity with no components is a line with nothing under it (`1:0` above). That is not the same
//! as the entity being absent, and the parser keeps them apart.
//!
//! # Byte-stability, and where it comes from
//!
//! Invariant I2: writing an unchanged snapshot produces an identical file. Almost all of it falls
//! out of the data structures — resources and components live in `BTreeMap`s, so they emerge sorted
//! without anyone remembering to sort them, and entities are captured already ordered.
//!
//! What is left is number formatting, which is shared with `amadeo-scene` rather than reimplemented.
//! `format_float` there has three requirements that are each easy to get subtly wrong, and having
//! two copies of it would mean two chances to get one of them wrong differently.
//!
//! # Why the free list is last
//!
//! It is the least interesting part to a reader and the most important part to correctness, and
//! those pull in opposite directions. Putting it at the end keeps a diff of two snapshots readable
//! — the entities are what changed — while the block header keeps it findable.

use crate::{FORMAT_VERSION, Snapshot};
use amadeo_ecs::Entity;
use amadeo_reflect::Value;
use amadeo_scene::inline_value;
use std::fmt::Write as _;

/// Two spaces, matching the scene and replay formats.
pub(crate) const INDENT: usize = 2;

/// Renders a snapshot in canonical form.
///
/// Always ends with a newline, and always uses LF — a `.gitattributes` entry pins that, because
/// `core.autocrlf` on Windows would otherwise rewrite committed files on checkout and break every
/// byte comparison (the failure that kept CI red for four commits in session 6).
#[must_use]
pub fn to_text(snapshot: &Snapshot) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "amadeo-snapshot {FORMAT_VERSION}");
    let _ = writeln!(out, "tick {}", snapshot.tick.0);
    // Hex, and fixed-width. A state hash is a `u64`, and the protocol already learned in session 6
    // that these have to travel as text rather than as JSON numbers.
    let _ = writeln!(out, "state-hash {:016x}", snapshot.state_hash);

    if !snapshot.resources.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "resources");
        for (name, value) in &snapshot.resources {
            write_named_value(&mut out, 1, name, value);
        }
    }

    if !snapshot.entities.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "entities");
        for row in &snapshot.entities {
            let _ = writeln!(out, "{}{}", pad(1), entity_text(row.entity));
            for (name, value) in &row.components {
                write_named_value(&mut out, 2, name, value);
            }
        }
    }

    if !snapshot.free_slots.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "free");
        for slot in &snapshot.free_slots {
            let _ = writeln!(out, "{}{}", pad(1), entity_text(*slot));
        }
    }

    out
}

/// `n` levels of indentation.
fn pad(level: usize) -> String {
    " ".repeat(level * INDENT)
}

/// Writes a named block — a resource or a component — and everything under it.
///
/// The same shape as any other field, which is why it just delegates: a resource that reflects as a
/// struct gets an indented block, and one that reflects as a scalar gets its value on the same line.
fn write_named_value(out: &mut String, level: usize, name: &str, value: &Value) {
    write_field(out, level, name, value);
}

/// Writes one field, nesting when the value has parts.
///
/// Four shapes, and the parser tells them apart by exactly the same signals:
///
/// | Value | Written as |
/// |---|---|
/// | a scalar, or a flat list | `name 1.0 2.0` — on one line |
/// | `Unit` | `name ()` — explicit, so it cannot be confused with an empty struct |
/// | a struct or a map | `name`, then its fields indented one level |
/// | a list that will not inline | `name`, then one `- ` item per line |
///
/// # Why maps need this, and it is not hypothetical
///
/// `InputState` is two maps and is a resource in every game. Without nesting, its value would fall
/// out in `Display` form — `{8831028638596390904 => {value: 0}}` — which no parser here reads back.
/// A snapshot of any real game would capture and then fail to restore, which is the worst kind of
/// broken: it looks like it worked until you need it.
///
/// A **struct and a map are written identically**, deliberately. `Reflect for BTreeMap` accepts
/// either (ADR 0027), for the same reason a text parser has no schema and cannot know which it is
/// looking at — so the round trip is `Map -> text -> Struct -> Map`, and it is byte-stable.
fn write_field(out: &mut String, level: usize, name: &str, value: &Value) {
    if let Some(inline) = inline_value(value) {
        let _ = writeln!(out, "{}{name} {inline}", pad(level));
        return;
    }

    match value {
        // Explicit rather than a bare name, so an empty struct and a unit value stay distinct.
        Value::Unit => {
            let _ = writeln!(out, "{}{name} ()", pad(level));
        }
        Value::Struct(fields) | Value::Map(fields) => {
            let _ = writeln!(out, "{}{name}", pad(level));
            for (field, inner) in fields {
                write_field(out, level + 1, field, inner);
            }
        }
        Value::List(items) => {
            let _ = writeln!(out, "{}{name}", pad(level));
            for item in items {
                write_field(out, level + 1, "-", item);
            }
        }
        // An enum variant carrying data: the variant on the name's line, its fields beneath. The
        // *fieldless* case never reaches here — `inline_value` returns the bare variant name.
        //
        // Added in session 8 alongside ADR 0032. Before it, a payload enum fell through to the
        // `Display` arm below and came out as `Orthographic({height: 8})`, which nothing reads back
        // — the same shape of defect the map handling above exists to prevent, found the same way:
        // by snapshotting a real game and looking at the file.
        Value::Enum(variant) => {
            let _ = writeln!(out, "{}{name} {}", pad(level), variant.variant);
            if let Value::Struct(fields) = variant.payload.as_ref() {
                for (field, inner) in fields {
                    write_field(out, level + 1, field, inner);
                }
            }
        }
        // Nothing reaches here: `inline_value` handles every scalar, and the arms above cover the
        // rest. Written in `Display` form rather than skipped so that if a new `Value` variant ever
        // appears, the data survives to be seen in the file instead of vanishing.
        other => {
            let _ = writeln!(out, "{}{name} {other}", pad(level));
        }
    }
}

/// Renders an entity handle as `index:generation`, matching `Entity`'s own `Display`.
///
/// Spelled out here rather than assumed, because the file format depends on it: changing `Display`
/// would silently change what every snapshot looks like.
#[must_use]
pub(crate) fn entity_text(entity: Entity) -> String {
    format!("{}:{}", entity.index(), entity.generation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Snapshot, SnapshotEntity};
    use amadeo_core::Tick;
    use std::collections::BTreeMap;

    fn empty() -> Snapshot {
        Snapshot {
            tick: Tick(0),
            state_hash: 0,
            resources: BTreeMap::new(),
            entities: Vec::new(),
            free_slots: Vec::new(),
        }
    }

    #[test]
    fn an_empty_world_is_a_header_and_nothing_else() {
        // One representation per state: no empty blocks, so byte-stability does not depend on
        // anyone deciding whether to write them.
        let text = to_text(&empty());
        assert_eq!(
            text,
            "amadeo-snapshot 1\ntick 0\nstate-hash 0000000000000000\n"
        );
    }

    #[test]
    fn the_header_carries_the_version_the_tick_and_the_hash() {
        let snapshot = Snapshot {
            tick: Tick(240),
            state_hash: 0x54d6_24e3_6fa5_0dd4,
            ..empty()
        };
        let text = to_text(&snapshot);

        assert!(text.starts_with("amadeo-snapshot 1\n"), "{text}");
        assert!(text.contains("tick 240\n"), "{text}");
        // Fixed-width hex, so two snapshots line up in a diff.
        assert!(text.contains("state-hash 54d624e36fa50dd4\n"), "{text}");
    }

    #[test]
    fn a_short_hash_is_padded_so_diffs_line_up() {
        let text = to_text(&Snapshot {
            state_hash: 0xff,
            ..empty()
        });
        assert!(text.contains("state-hash 00000000000000ff"), "{text}");
    }

    #[test]
    fn an_entity_with_no_components_is_a_bare_line() {
        // Not the same as being absent, and the format has to keep them apart -- a component-less
        // entity is still in the state hash.
        let snapshot = Snapshot {
            entities: vec![SnapshotEntity {
                entity: test_entity(1, 0),
                components: BTreeMap::new(),
            }],
            ..empty()
        };

        let text = to_text(&snapshot);
        assert!(text.contains("entities\n  1:0\n"), "{text}");
    }

    #[test]
    fn components_are_indented_under_their_entity() {
        let mut components = BTreeMap::new();
        components.insert(
            "Velocity".to_string(),
            Value::structure([("x", Value::F32(1.5)), ("y", Value::F32(0.0))]),
        );

        let text = to_text(&Snapshot {
            entities: vec![SnapshotEntity {
                entity: test_entity(0, 0),
                components,
            }],
            ..empty()
        });

        assert!(
            text.contains("  0:0\n    Velocity\n      x 1.5\n      y 0.0\n"),
            "{text}"
        );
    }

    #[test]
    fn a_float_that_looks_whole_still_writes_as_a_float() {
        // The subtle half of byte-stability, and the reason `format_float` is shared with
        // `amadeo-scene` rather than rewritten: `0.0` printed as `0` would parse back as an integer,
        // and the value would survive while its type did not.
        let mut components = BTreeMap::new();
        components.insert(
            "Position".to_string(),
            Value::structure([("x", Value::F32(0.0))]),
        );

        let text = to_text(&Snapshot {
            entities: vec![SnapshotEntity {
                entity: test_entity(0, 0),
                components,
            }],
            ..empty()
        });

        assert!(text.contains("x 0.0"), "{text}");
    }

    #[test]
    fn the_free_list_keeps_its_order() {
        // The order IS the data: the stack is drained last-in-first-out, so the last line is the
        // next slot to be reused. Sorting it would change which entity the next spawn produces.
        let text = to_text(&Snapshot {
            free_slots: vec![test_entity(4, 2), test_entity(3, 1)],
            ..empty()
        });

        let free = text.split("free\n").nth(1).expect("free block");
        assert_eq!(free, "  4:2\n  3:1\n");
    }

    #[test]
    fn a_resource_that_is_not_a_struct_is_written_inline() {
        let mut resources = BTreeMap::new();
        resources.insert("Countdown".to_string(), Value::U64(9));

        let text = to_text(&Snapshot {
            resources,
            ..empty()
        });
        assert!(text.contains("  Countdown 9\n"), "{text}");
    }

    #[test]
    fn a_map_nests_rather_than_falling_out_as_debug_text() {
        // Found by running this against the real game rather than by reasoning about it.
        // `InputState` is two maps and is a resource in every game, so without nesting a snapshot
        // of anything real would capture and then refuse to restore.
        let mut axes = std::collections::BTreeMap::new();
        axes.insert(
            "8831028638596390904".to_string(),
            Value::structure([("previous", Value::F32(0.0)), ("value", Value::F32(0.5))]),
        );

        let mut resources = BTreeMap::new();
        resources.insert(
            "InputState".to_string(),
            Value::structure([("axes", Value::Map(axes))]),
        );

        let text = to_text(&Snapshot {
            resources,
            ..empty()
        });

        assert!(
            text.contains(
                "  InputState\n    axes\n      8831028638596390904\n        previous 0.0\n        value 0.5\n"
            ),
            "{text}"
        );
        // And nothing fell out in `Display` form.
        assert!(!text.contains("=>"), "{text}");
    }

    #[test]
    fn an_empty_map_and_a_unit_are_written_differently() {
        // **Both are spelled out, and this test's shape changed once already.** It used to assert
        // that an empty map wrote as a bare name — which turned out not to round-trip at all: a
        // field with no value is not something this format has, and it read back as `Unit`. Both are
        // now explicit markers, and they are still distinct, which is what this has always been for.
        let mut resources = BTreeMap::new();
        resources.insert(
            "Both".to_string(),
            Value::structure([
                ("empty", Value::Map(std::collections::BTreeMap::new())),
                ("nothing", Value::Unit),
            ]),
        );

        let text = to_text(&Snapshot {
            resources,
            ..empty()
        });
        assert!(text.contains("    empty {}\n"), "{text}");
        assert!(text.contains("    nothing ()\n"), "{text}");
    }

    #[test]
    fn an_empty_map_reads_back_as_an_empty_map() {
        // The half the test above did not have, and the reason it was wrong. `Facts` in
        // `modules/amadeo-behaviour` is the first component in the engine to hold a map that starts
        // empty, and a monster that had never perceived anything could not be saved.
        let mut resources = BTreeMap::new();
        resources.insert(
            "Facts".to_string(),
            Value::structure([("known", Value::Map(std::collections::BTreeMap::new()))]),
        );

        let snapshot = Snapshot {
            resources,
            ..empty()
        };
        let read = crate::parse(&to_text(&snapshot)).expect("an empty map must read back");
        assert_eq!(read.resources, snapshot.resources);
    }

    #[test]
    fn writing_is_reproducible() {
        // Invariant I2 at its bluntest.
        let snapshot = Snapshot {
            tick: Tick(7),
            state_hash: 42,
            entities: vec![SnapshotEntity {
                entity: test_entity(0, 0),
                components: BTreeMap::new(),
            }],
            ..empty()
        };
        assert_eq!(to_text(&snapshot), to_text(&snapshot));
    }

    /// Builds an entity handle, which is otherwise only mintable inside `amadeo-ecs`.
    fn test_entity(index: u32, generation: u32) -> Entity {
        // A world is the only public source of handles, so this spawns and despawns to reach the
        // wanted index and generation. Slow and obvious beats a test-only constructor leaking into
        // the ECS's public surface.
        let mut world = amadeo_ecs::World::new();
        let mut entity = world.spawn();
        while entity.index() < index {
            entity = world.spawn();
        }
        for _ in 0..generation {
            world.despawn(entity);
            entity = world.spawn();
        }
        entity
    }

    #[test]
    fn the_test_helper_produces_the_handles_it_claims() {
        // The helper above is doing something non-obvious, so it gets its own check.
        assert_eq!(entity_text(test_entity(3, 0)), "3:0");
        assert_eq!(entity_text(test_entity(0, 2)), "0:2");
    }
}
