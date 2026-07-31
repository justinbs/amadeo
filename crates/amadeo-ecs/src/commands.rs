//! Deferred structural changes.
//!
//! # The problem this solves
//!
//! A query holds a borrow of the world for as long as it runs, so a system cannot spawn, despawn, or
//! add a component from inside one:
//!
//! ```compile_fail
//! # use amadeo_ecs::World;
//! # let mut world = World::new();
//! # #[derive(Debug)] struct Health(f32);
//! world.for_each_mut::<Health>(|entity, health| {
//!     if health.0 <= 0.0 {
//!         world.despawn(entity);   // rejected: `world` is already borrowed
//!     }
//! });
//! ```
//!
//! That restriction is not an inconvenience to work around — it is the borrow checker preventing a
//! genuine bug. Removing an entity mid-iteration would reorder the very rows being walked.
//!
//! # The answer
//!
//! Queue the change, apply it at a defined point:
//!
//! ```
//! use amadeo_core::StableHash;
//! use amadeo_ecs::{Command, Commands, Component, World};
//! use amadeo_reflect::Reflect;
//!
//! #[derive(Debug, StableHash, Reflect)]
//! struct Health(f32);
//! impl Component for Health {}
//!
//! let mut world = World::new();
//! world.insert_service(Commands::new());
//!
//! let doomed = world.spawn();
//! world.insert(doomed, Health(-1.0));
//!
//! world.with_service_taken::<Commands, ()>(|world, commands| {
//!     world.for_each_mut::<Health>(|entity, health| {
//!         if health.0 <= 0.0 {
//!             commands.despawn(entity);
//!         }
//!     });
//! });
//!
//! assert!(world.contains(doomed), "not applied yet");
//! world.flush_commands();
//! assert!(!world.contains(doomed));
//! ```
//!
//! # Ordering
//!
//! Commands apply in the order they were queued. With a single-threaded schedule that is entirely
//! determined by system order, which is itself deterministic — so the merge satisfies ADR 0005
//! without any extra sorting. When parallel execution arrives, each worker will need its own buffer
//! and they will be concatenated in a fixed worker order; the per-buffer ordering here does not
//! change.

use crate::component::Component;
use crate::entity::Entity;
use crate::service::Service;
use crate::world::World;
use std::fmt;

/// What a queued command does. Carried for diagnostics and introspection only — the actual work is
/// in the closure beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Create an entity and initialise it.
    Spawn,
    /// Remove an entity and all its components.
    Despawn,
    /// Add or overwrite a component.
    Insert,
    /// Remove a component.
    Remove,
    /// An arbitrary caller-supplied change.
    Custom,
}

/// One queued change.
struct QueuedCommand {
    kind: Command,
    /// The entity this command concerns, where there is one. Diagnostics only.
    subject: Option<Entity>,
    /// The change itself.
    ///
    /// `Send + Sync` because [`Service`] requires both, and keeping that bound uniform with
    /// [`Resource`](crate::Resource) is simpler to reason about than special-casing this one type.
    /// In practice it costs nothing: a closure capturing entities and components is already both,
    /// since `Component` itself requires `Send + Sync`.
    apply: Box<dyn FnOnce(&mut World) + Send + Sync>,
}

impl fmt::Debug for QueuedCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Hand-written: a boxed closure has no Debug of its own.
        f.debug_struct("QueuedCommand")
            .field("kind", &self.kind)
            .field("subject", &self.subject)
            .finish_non_exhaustive()
    }
}

/// A queue of structural changes to apply later.
///
/// A [`Service`], not a resource, and that is safe **only because the buffer is always empty at a
/// tick boundary** — the app loop flushes after every stage. If commands were ever allowed to
/// survive into the next tick they would be simulation state, and leaving them out of the state hash
/// would hide a real divergence.
#[derive(Debug, Default)]
pub struct Commands {
    queued: Vec<QueuedCommand>,
}

impl Service for Commands {}

impl Commands {
    /// Creates an empty command buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many commands are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queued.len()
    }

    /// Whether anything is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }

    /// The kinds of command currently queued, in order. For diagnostics and introspection.
    pub fn pending(&self) -> impl Iterator<Item = (Command, Option<Entity>)> {
        self.queued
            .iter()
            .map(|command| (command.kind, command.subject))
    }

    /// Queues an entity for removal.
    pub fn despawn(&mut self, entity: Entity) -> &mut Self {
        self.queued.push(QueuedCommand {
            kind: Command::Despawn,
            subject: Some(entity),
            apply: Box::new(move |world| {
                world.despawn(entity);
            }),
        });
        self
    }

    /// Queues a component to be added or overwritten.
    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) -> &mut Self {
        self.queued.push(QueuedCommand {
            kind: Command::Insert,
            subject: Some(entity),
            apply: Box::new(move |world| {
                world.insert(entity, value);
            }),
        });
        self
    }

    /// Queues a component to be removed.
    pub fn remove<T: Component>(&mut self, entity: Entity) -> &mut Self {
        self.queued.push(QueuedCommand {
            kind: Command::Remove,
            subject: Some(entity),
            apply: Box::new(move |world| {
                world.remove::<T>(entity);
            }),
        });
        self
    }

    /// Queues an entity to be created, then initialised by `build`.
    ///
    /// # Why the entity arrives in a closure
    ///
    /// The obvious API would be `let entity = commands.spawn();` returning a handle immediately. That
    /// requires reserving entity ids ahead of time, which is a real design with real edge cases
    /// (pool exhaustion, interaction with the free list, collisions with direct spawns).
    ///
    /// Handing the entity to a closure at flush time avoids all of it: the entity is created by the
    /// world, at a point where the world is not borrowed, and `build` initialises it there.
    ///
    /// **The limitation:** the new entity's handle is not available to *other* commands queued in the
    /// same batch, so one spawned entity cannot be made to reference another until the next flush.
    /// That case is rare, and the workaround — queue the link in a `Custom` command after the flush —
    /// is straightforward. Revisit if it stops being rare.
    ///
    /// ```
    /// # use amadeo_core::StableHash;
    /// # use amadeo_ecs::{Commands, Component, World};
    /// # use amadeo_reflect::Reflect;
    /// # #[derive(Debug, PartialEq, StableHash, Reflect)] struct Marker(u32);
    /// # impl Component for Marker {}
    /// let mut world = World::new();
    /// world.insert_service(Commands::new());
    ///
    /// world.with_service_taken::<Commands, ()>(|_world, commands| {
    ///     commands.spawn_with(|world, entity| {
    ///         world.insert(entity, Marker(7));
    ///     });
    /// });
    /// world.flush_commands();
    ///
    /// assert_eq!(world.iter::<Marker>().count(), 1);
    /// ```
    pub fn spawn_with(
        &mut self,
        build: impl FnOnce(&mut World, Entity) + Send + Sync + 'static,
    ) -> &mut Self {
        self.queued.push(QueuedCommand {
            kind: Command::Spawn,
            subject: None,
            apply: Box::new(move |world| {
                let entity = world.spawn();
                build(world, entity);
            }),
        });
        self
    }

    /// Queues an arbitrary change.
    ///
    /// An escape hatch for anything the typed methods do not cover. Prefer the typed methods where
    /// they fit — they carry a meaningful [`Command`] kind, which is what makes a pending queue
    /// readable in diagnostics.
    pub fn queue(&mut self, change: impl FnOnce(&mut World) + Send + Sync + 'static) -> &mut Self {
        self.queued.push(QueuedCommand {
            kind: Command::Custom,
            subject: None,
            apply: Box::new(change),
        });
        self
    }

    /// Removes and returns everything queued, leaving the buffer empty.
    ///
    /// Private: `QueuedCommand` is an implementation detail, and `World::flush_commands` lives in
    /// this module so it does not need wider visibility.
    fn take(&mut self) -> Vec<QueuedCommand> {
        std::mem::take(&mut self.queued)
    }
}

impl World {
    /// Applies every queued command, in the order it was queued.
    ///
    /// Called by the app loop after each stage. Does nothing if no [`Commands`] service is installed.
    ///
    /// Commands queued *by* a command are not applied in the same pass — they land in the buffer and
    /// wait for the next flush. That bound is deliberate: a command that queues another could
    /// otherwise loop forever inside a single flush, and a hang is much harder to diagnose than a
    /// one-frame delay.
    pub fn flush_commands(&mut self) {
        let Some(mut commands) = self.remove_service::<Commands>() else {
            return;
        };
        let batch = commands.take();
        // Put the (now empty) buffer back first, so commands running below can queue into it.
        self.insert_service(commands);

        for command in batch {
            (command.apply)(self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::StableHash;
    use amadeo_reflect::Reflect;

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Health(f32);
    impl Component for Health {}

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Shield(u32);
    impl Component for Shield {}

    fn world_with_commands() -> World {
        let mut world = World::new();
        world.insert_service(Commands::new());
        world
    }

    #[test]
    fn commands_do_nothing_until_flushed() {
        let mut world = world_with_commands();
        let entity = world.spawn();
        world.insert(entity, Health(10.0));

        world.with_service_taken::<Commands, ()>(|_world, commands| {
            commands.despawn(entity);
        });

        assert!(world.contains(entity), "must not apply early");
        world.flush_commands();
        assert!(!world.contains(entity));
    }

    #[test]
    fn despawn_from_inside_a_query() {
        // The case the whole module exists for.
        let mut world = world_with_commands();
        let alive = world.spawn();
        let doomed = world.spawn();
        world.insert(alive, Health(5.0));
        world.insert(doomed, Health(-1.0));

        world.with_service_taken::<Commands, ()>(|world, commands| {
            world.for_each_mut::<Health>(|entity, health| {
                if health.0 <= 0.0 {
                    commands.despawn(entity);
                }
            });
        });
        world.flush_commands();

        assert!(world.contains(alive));
        assert!(!world.contains(doomed));
    }

    #[test]
    fn insert_and_remove_are_deferred() {
        let mut world = world_with_commands();
        let entity = world.spawn();
        world.insert(entity, Health(1.0));

        world.with_service_taken::<Commands, ()>(|_world, commands| {
            commands.insert(entity, Shield(3));
            commands.remove::<Health>(entity);
        });

        assert!(world.has::<Health>(entity), "not applied yet");
        world.flush_commands();

        assert!(!world.has::<Health>(entity));
        assert_eq!(world.get::<Shield>(entity), Some(&Shield(3)));
    }

    #[test]
    fn spawn_with_initialises_the_new_entity() {
        let mut world = world_with_commands();

        world.with_service_taken::<Commands, ()>(|_world, commands| {
            for value in 0..3u32 {
                commands.spawn_with(move |world, entity| {
                    world.insert(entity, Shield(value));
                });
            }
        });
        world.flush_commands();

        let mut shields: Vec<u32> = world.iter::<Shield>().map(|(_, s)| s.0).collect();
        shields.sort_unstable();
        assert_eq!(shields, vec![0, 1, 2]);
    }

    #[test]
    fn spawning_from_inside_a_query() {
        // Every entity with Health spawns one Shield entity. The classic "fire a projectile" shape.
        let mut world = world_with_commands();
        for value in 0..3u32 {
            let entity = world.spawn();
            world.insert(entity, Health(value as f32));
        }

        world.with_service_taken::<Commands, ()>(|world, commands| {
            world.for_each_mut::<Health>(|_entity, health| {
                let level = health.0 as u32;
                commands.spawn_with(move |world, entity| {
                    world.insert(entity, Shield(level));
                });
            });
        });
        world.flush_commands();

        assert_eq!(world.iter::<Shield>().count(), 3);
        assert_eq!(world.entity_count(), 6);
    }

    #[test]
    fn commands_apply_in_queue_order() {
        // Determinism depends on this: the last write must win in a predictable way.
        let mut world = world_with_commands();
        let entity = world.spawn();

        world.with_service_taken::<Commands, ()>(|_world, commands| {
            commands.insert(entity, Shield(1));
            commands.insert(entity, Shield(2));
            commands.insert(entity, Shield(3));
        });
        world.flush_commands();

        assert_eq!(world.get::<Shield>(entity), Some(&Shield(3)));
    }

    #[test]
    fn queue_runs_arbitrary_changes() {
        let mut world = world_with_commands();

        world.with_service_taken::<Commands, ()>(|_world, commands| {
            commands.queue(|world| {
                let entity = world.spawn();
                world.insert(entity, Shield(42));
            });
        });
        world.flush_commands();

        assert_eq!(world.iter::<Shield>().count(), 1);
    }

    #[test]
    fn commands_queued_during_a_flush_wait_for_the_next_one() {
        // Deliberate bound: a command queueing another could otherwise spin forever inside one
        // flush, and a hang is far worse to diagnose than a one-frame delay.
        let mut world = world_with_commands();

        world.with_service_taken::<Commands, ()>(|_world, commands| {
            commands.queue(|world| {
                world.with_service_taken::<Commands, ()>(|_world, commands| {
                    commands.queue(|world| {
                        let entity = world.spawn();
                        world.insert(entity, Shield(9));
                    });
                });
            });
        });

        world.flush_commands();
        assert_eq!(world.iter::<Shield>().count(), 0, "inner command deferred");

        world.flush_commands();
        assert_eq!(world.iter::<Shield>().count(), 1);
    }

    #[test]
    fn flushing_without_the_service_is_harmless() {
        let mut world = World::new();
        world.flush_commands();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn flushing_an_empty_buffer_is_harmless() {
        let mut world = world_with_commands();
        world.flush_commands();
        world.flush_commands();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn commands_targeting_stale_entities_are_ignored() {
        // A system can legitimately queue a despawn for something another system already removed.
        let mut world = world_with_commands();
        let entity = world.spawn();
        world.insert(entity, Health(1.0));

        world.with_service_taken::<Commands, ()>(|_world, commands| {
            commands.despawn(entity);
            commands.insert(entity, Shield(1));
            commands.remove::<Health>(entity);
        });

        world.flush_commands();
        assert!(!world.contains(entity));
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn pending_queue_is_inspectable() {
        let mut world = world_with_commands();
        let entity = world.spawn();

        world.with_service_taken::<Commands, ()>(|_world, commands| {
            commands.despawn(entity);
            commands.insert(entity, Shield(1));
            commands.spawn_with(|_world, _entity| {});

            assert_eq!(commands.len(), 3);
            assert!(!commands.is_empty());

            let pending: Vec<Command> = commands.pending().map(|(kind, _)| kind).collect();
            assert_eq!(
                pending,
                vec![Command::Despawn, Command::Insert, Command::Spawn]
            );
            // The subject is carried so diagnostics can name the entity involved.
            let subjects: Vec<Option<Entity>> =
                commands.pending().map(|(_, subject)| subject).collect();
            assert_eq!(subjects, vec![Some(entity), Some(entity), None]);
        });
    }

    #[test]
    fn buffer_is_empty_after_a_flush() {
        // Load-bearing: Commands is a Service and therefore unhashed, which is only sound because
        // nothing survives a tick boundary.
        let mut world = world_with_commands();
        let entity = world.spawn();
        world.with_service_taken::<Commands, ()>(|_world, commands| {
            commands.despawn(entity);
        });

        world.flush_commands();
        let commands = world.service::<Commands>().expect("installed");
        assert!(commands.is_empty());
    }

    #[test]
    fn identical_command_batches_produce_identical_state() {
        let build = || {
            let mut world = world_with_commands();
            for value in 0..5u32 {
                let entity = world.spawn();
                world.insert(entity, Health(value as f32));
            }
            world.with_service_taken::<Commands, ()>(|world, commands| {
                world.for_each_mut::<Health>(|entity, health| {
                    if health.0 < 2.0 {
                        commands.despawn(entity);
                    } else {
                        commands.insert(entity, Shield(health.0 as u32));
                    }
                });
            });
            world.flush_commands();
            world
        };

        assert_eq!(build().state_hash(), build().state_hash());
    }
}
