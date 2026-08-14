//! The `.snapshot` format: a whole world written to a file, and put back exactly.
//!
//! ```
//! use amadeo_ecs::{ComponentRegistry, World};
//! use amadeo_snapshot::{capture, restore};
//!
//! let mut world = World::new();
//! let registry = ComponentRegistry::new();
//! world.spawn();
//!
//! let text = amadeo_snapshot::to_text(&capture(&world, &registry));
//! assert!(text.starts_with("amadeo-snapshot 2\n"));
//!
//! // And back into a fresh world.
//! let document = amadeo_snapshot::parse(&text).expect("valid");
//! let mut restored = World::new();
//! restore(&mut restored, &registry, &document).expect("restores");
//! assert_eq!(restored.state_hash(), world.state_hash());
//! ```
//!
//! # Why this exists
//!
//! ADR 0011's spike measured what actually degrades the agent's iteration loop, and it was **not**
//! compile time — it was **re-simulation**. Getting back to the moment of interest costs about
//! 21 µs per tick and grows linearly with the session: 382 ms to reach five simulated minutes, and
//! worse every minute after that. A snapshot replaces that with a file read.
//!
//! # Why it is a file rather than a handle
//!
//! Not a choice. ADR 0016 makes every CLI invocation a **fresh process that exits**, so an
//! in-memory snapshot would die with the process that took it and could never help the loop it
//! exists to help.
//!
//! # The thing that makes this correct, and it is not obvious
//!
//! `World::state_hash` deliberately excludes the entity allocator's free list — that is bookkeeping,
//! not simulation state. Which means **two worlds can hash identically and then hand out different
//! entity handles on the very next `spawn`**: one has a slot to reuse, the other does not.
//!
//! So a snapshot that captured only the live entities would restore a world that looked correct by
//! every available measure, and diverged a few ticks later once something spawned. This format
//! records the free stack, in order, for exactly that reason — and the tests prove it by **running
//! on** after a restore rather than by comparing hashes, because comparing hashes is precisely the
//! check that cannot see the bug.
//!
//! # What is deliberately not captured
//!
//! **Services.** Asset caches, the GPU device, the renderer, the audio mixer. ADR 0009 keeps them
//! out of the state hash because they are machinery rather than simulation state, and the same
//! reasoning applies here: restoring a world should not reach into the process's devices. A restore
//! puts the *simulation* back; the engine around it carries on as it was.

mod layout;
mod parse;
mod redirects;
mod save;
mod write;

pub use layout::{SchemaEntry, SchemaKind, fingerprint, schema_of};
pub use parse::{ParseError, ParseErrorKind, parse};
pub use redirects::{REDIRECT_VERSION, RedirectError, Redirects};
pub use save::{Defaulted, Dropped, Redirected, SaveReport, restore_save};
pub use write::to_text;

use amadeo_core::Tick;
use amadeo_ecs::{ComponentRegistry, Entity, World};
use amadeo_reflect::Value;
use std::collections::BTreeMap;

/// The format version this build writes, and the only one it reads.
///
/// Bumped when the layout changes in a way an older reader would misunderstand. A snapshot is a
/// short-lived artefact — it captures one moment of one run — so there is no migration path and a
/// mismatch is refused rather than guessed at, the same way `.replay` refuses a tick-rate mismatch
/// (ADR 0007).
///
/// **2 added the `schema` block and the `schema-hash` header** (ADR 0069), which is what lets the
/// same file also serve as a save. Files written by version 1 are refused, which is what the
/// paragraph above permits.
pub const FORMAT_VERSION: u32 = 2;

/// One entity and everything on it.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotEntity {
    /// The exact handle, index and generation both. Restoring a *different* handle would change the
    /// state hash, since both halves are hashed.
    pub entity: Entity,
    /// Components by canonical name, sorted — so a snapshot is diffable and byte-stable (I2).
    pub components: BTreeMap<String, Value>,
}

/// A whole world, captured.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Which tick this world was at. Part of the state hash, so it has to come back exactly.
    pub tick: Tick,
    /// The state hash at capture time.
    ///
    /// Recorded so a restore can **check its own work**: if the world it rebuilt does not hash to
    /// this, the snapshot and the build disagree and continuing would produce a run that looks
    /// valid and is not. This is the format's integrity check, not decoration.
    ///
    /// It is only meaningful against the layout it was computed under — see [`Snapshot::layout`].
    pub state_hash: u64,
    /// A fingerprint of the *shape* of everything in this file, at capture time.
    ///
    /// ADR 0069. `state_hash` describes a particular arrangement of fields, so a build whose
    /// components have changed shape cannot reproduce it even from a perfectly restored world. This
    /// is how a reader tells those two cases apart: matching fingerprints mean the recorded hash
    /// still means what it meant, and [`restore_save`] enforces it exactly like [`restore`] does.
    ///
    /// See [`fingerprint`] for what goes into it and what deliberately does not.
    pub layout: u64,
    /// What this file contains, by name and schema version.
    ///
    /// Written for every component and resource actually present, sorted. Two jobs: it is the input
    /// to [`fingerprint`], and it records each type's [`version`](amadeo_reflect::TypeInfo::version)
    /// so that migrations remain an addition rather than a rewrite. **Nothing reads the version
    /// yet** — ADR 0069 §6 explains why it is written anyway.
    pub schema: Vec<SchemaEntry>,
    /// Every resource, by canonical name.
    pub resources: BTreeMap<String, Value>,
    /// Every live entity, sorted by index then generation.
    pub entities: Vec<SnapshotEntity>,
    /// The entity allocator's free stack, bottom first — the last entry is the next slot reused.
    ///
    /// See the module docs for why omitting this would produce a snapshot that passes every check
    /// and is still wrong.
    pub free_slots: Vec<Entity>,
}

/// Reads a whole world into a [`Snapshot`].
///
/// Read-only: capturing a world cannot perturb it, which matters because an agent taking a snapshot
/// to look at something must not change what it is looking at.
///
/// Components not in `registry` are **skipped silently** — the registry is what defines the set of
/// component types a build knows, and a component that is not in it could not be restored anyway.
/// In practice that set is complete, because invariant I8 and ADR 0016 put registration on `App`.
#[must_use]
pub fn capture(world: &World, registry: &ComponentRegistry) -> Snapshot {
    let entities = world
        .entities()
        .into_iter()
        .map(|entity| SnapshotEntity {
            entity,
            components: registry.components_of(world, entity),
        })
        .collect();

    let schema = schema_of(world, registry);

    Snapshot {
        tick: world.tick(),
        state_hash: world.state_hash(),
        layout: fingerprint(&schema, world, registry),
        schema,
        resources: world.resources().into_iter().collect(),
        entities,
        free_slots: world.free_entity_slots(),
    }
}

/// Why a snapshot could not be put back.
///
/// Every variant names what was wrong and what it means, because a restore that half-worked leaves a
/// world nobody can reason about — and an agent reading this cannot ask a follow-up question.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RestoreError {
    /// A component in the snapshot is not one this build has.
    #[error(
        "this snapshot has a component called `{component}` on entity {entity}, and this build \
         does not know that name.\n\
         The snapshot was probably taken by a different build. Run `amadeo describe` to see what \
         this one has"
    )]
    UnknownComponent {
        /// The name that was not recognised.
        component: String,
        /// Where it was found, as `index:generation`.
        entity: String,
    },

    /// A component's recorded value does not fit the type this build has.
    #[error(
        "entity {entity}'s `{component}` could not be rebuilt: {reason}.\n\
         The component's fields have probably changed since the snapshot was taken"
    )]
    BadComponent {
        /// Which component.
        component: String,
        /// Which entity, as `index:generation`.
        entity: String,
        /// What the type's own `from_value` said.
        reason: String,
    },

    /// A resource's recorded value does not fit the type this build has.
    #[error("resource `{resource}` could not be rebuilt: {reason}")]
    BadResource {
        /// Which resource.
        resource: String,
        /// What the type's own `from_value` said.
        reason: String,
    },

    /// The allocator state in the file is not internally consistent.
    #[error(
        "this snapshot's entity slots do not add up: {live} live plus {free} free should cover \
         every index up to {highest}, but that needs {expected}.\n\
         A slot that is neither live nor free could never be allocated again, so this is refused \
         rather than restored. The file has probably been edited by hand"
    )]
    SlotsDoNotAddUp {
        /// How many live entities the file lists.
        live: usize,
        /// How many free slots it lists.
        free: usize,
        /// The largest index either list mentions.
        highest: u32,
        /// How many slots that implies.
        expected: usize,
    },

    /// The same slot index appears twice.
    #[error(
        "this snapshot mentions slot {index} more than once. Each slot is either live or free, \
         exactly once"
    )]
    DuplicateSlot {
        /// The index that repeated.
        index: u32,
    },

    /// The rebuilt world does not hash to what the file recorded.
    #[error(
        "this snapshot restored, but the result does not match the state it recorded \
         (expected {expected:016x}, got {actual:016x}).\n\
         The world is now in an unreliable state: something about this build differs from the one \
         that took the snapshot, in a way that survived every other check. Do not trust a run \
         continued from here"
    )]
    HashMismatch {
        /// What the file said the state hash was.
        expected: u64,
        /// What the rebuilt world actually hashes to.
        actual: u64,
    },
}

/// Puts a captured world back, exactly.
///
/// The world's **resources must already exist** at their defaults before this is called — a game's
/// own setup runs first, then the snapshot overwrites the recorded ones. `World::insert_resource`
/// records how to rebuild each type as it goes, so a resource the game creates is always one a
/// snapshot can put back, and a resource the game does not create is one a snapshot has no business
/// inventing. A resource in the file that this build does not have is **skipped**, deliberately:
/// dropping a subsystem should not make old snapshots unloadable.
///
/// # It checks its own work
///
/// The last thing this does is compare the rebuilt world's state hash against the one recorded at
/// capture. That turns "the restore silently produced a slightly different world" — the failure that
/// would poison every subsequent assertion — into an error at the moment it happens.
///
/// # Errors
///
/// [`RestoreError`], naming the entity, component, or resource that failed.
pub fn restore(
    world: &mut World,
    registry: &ComponentRegistry,
    snapshot: &Snapshot,
) -> Result<(), RestoreError> {
    validate_slots(snapshot)?;

    let live: Vec<Entity> = snapshot.entities.iter().map(|row| row.entity).collect();
    world.restore_entities(&live, &snapshot.free_slots);
    world.set_tick(snapshot.tick);

    for row in &snapshot.entities {
        for (name, value) in &row.components {
            registry
                .insert(world, row.entity, name, value)
                .map_err(|error| classify_component_error(registry, name, row.entity, &error))?;
        }
    }

    for (name, value) in &snapshot.resources {
        world
            .restore_resource(name, value)
            .map_err(|error| RestoreError::BadResource {
                resource: name.clone(),
                reason: error.to_string(),
            })?;
    }

    let actual = world.state_hash();
    if actual != snapshot.state_hash {
        return Err(RestoreError::HashMismatch {
            expected: snapshot.state_hash,
            actual,
        });
    }

    Ok(())
}

/// Turns a registry failure into the right error, distinguishing "no such name" from "bad value".
///
/// Two different problems with two different fixes: an unknown component means the build is wrong,
/// a bad value means the component's shape changed. Reporting both as one message would leave the
/// reader guessing which.
fn classify_component_error(
    registry: &ComponentRegistry,
    name: &str,
    entity: Entity,
    error: &impl std::fmt::Display,
) -> RestoreError {
    if registry.contains(name) {
        RestoreError::BadComponent {
            component: name.to_string(),
            entity: entity.to_string(),
            reason: error.to_string(),
        }
    } else {
        RestoreError::UnknownComponent {
            component: name.to_string(),
            entity: entity.to_string(),
        }
    }
}

/// Checks that the live and free slots together cover every index exactly once.
///
/// A gap would leave a slot that is neither live nor free, which could never be allocated again — a
/// leak nothing would report. A duplicate would mean two entities in one slot. Neither can come from
/// a captured world; both can come from a hand-edited file, which is a supported thing to do.
pub(crate) fn validate_slots(snapshot: &Snapshot) -> Result<(), RestoreError> {
    let mut seen: BTreeMap<u32, ()> = BTreeMap::new();
    let mut highest = 0u32;

    for entity in snapshot
        .entities
        .iter()
        .map(|row| row.entity)
        .chain(snapshot.free_slots.iter().copied())
    {
        if seen.insert(entity.index(), ()).is_some() {
            return Err(RestoreError::DuplicateSlot {
                index: entity.index(),
            });
        }
        highest = highest.max(entity.index());
    }

    let expected = if seen.is_empty() {
        0
    } else {
        highest as usize + 1
    };
    if seen.len() != expected {
        return Err(RestoreError::SlotsDoNotAddUp {
            live: snapshot.entities.len(),
            free: snapshot.free_slots.len(),
            highest,
            expected,
        });
    }

    Ok(())
}
