//! Archetype-based entity component system.
//!
//! # The model
//!
//! An **entity** is an id. A **component** is plain data attached to an entity. A **system** is a
//! function that queries components and mutates them. There are no classes with `update()` methods
//! and no behaviour attached to entities — that is ADR 0004, and it is the main adjustment for anyone
//! arriving from Unity or Godot.
//!
//! ```
//! use amadeo_core::StableHash;
//! use amadeo_ecs::{Component, World};
//! use amadeo_reflect::Reflect;
//!
//! #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
//! struct Position { x: f32 }
//! #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
//! struct Velocity { x: f32 }
//!
//! impl Component for Position {}
//! impl Component for Velocity {}
//!
//! let mut world = World::new();
//! let entity = world.spawn();
//! world.insert(entity, Position { x: 0.0 });
//! world.insert(entity, Velocity { x: 2.0 });
//!
//! // A system: move everything by its velocity.
//! world.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
//!     position.x += velocity.x;
//! });
//!
//! assert_eq!(world.get::<Position>(entity), Some(&Position { x: 2.0 }));
//! ```
//!
//! # Storage
//!
//! Entities with identical component sets share an **archetype**: a table whose columns are
//! contiguous `Vec<T>`, one per component type. Systems iterate those slices directly.
//!
//! Per ADR 0008 the columns are type-erased behind a safe trait object and downcast **once per
//! archetype per query**, never per entity. So the whole crate — the most-read code in the engine —
//! contains no `unsafe`, while the inner loop still walks contiguous typed memory.
//!
//! # Determinism
//!
//! Several choices here exist only to satisfy invariant I3, and reversing them would silently break
//! every golden replay test:
//!
//! - [`ComponentId`] hashes the type *name*, not `TypeId`, because `TypeId` is not stable across
//!   builds.
//! - Archetype lookup uses `BTreeMap` and sorted id vectors, never a hash map.
//! - [`World::state_hash`] sorts by entity before hashing, so storage churn cannot change the result.

mod archetype;
mod commands;
mod component;
mod entity;
mod query;
mod registry;
mod resource;
mod service;
mod type_hash;
mod world;

// Exported only so `QueryTerm` can name it. All of its methods are crate-private.
#[doc(hidden)]
pub use archetype::Archetype;
pub use commands::{Command, Commands};
pub use component::{Component, ComponentId};
pub use entity::Entity;
pub use query::{QueryIter, QueryTerm};
pub use registry::{ComponentRegistry, RegistryError};
pub use resource::{Resource, ResourceId};
pub use service::{Service, ServiceId};
pub use world::World;
