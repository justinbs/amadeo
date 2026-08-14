//! **Items, stacks and containers** — `docs/05`'s `mod-inventory`, and ADR 0070.
//!
//! # An item is an entity, always
//!
//! The fork `docs/05` recorded is whether an item is an entity or a value: a stack of fifty arrows
//! is one row in a list, and a dropped arrow is a thing in the world with a collider, and both have
//! to be the same item. This module answers *entity*, and a stack is **one entity with a count**, so
//! the property that made a value attractive turns out not to be a property of values at all.
//!
//! # Storing something removes its `Transform`
//!
//! That is the whole mechanism, and it works because of something that was measured rather than
//! assumed. The three passes that put an entity in the world all **require** a `Transform` and skip
//! an entity without one:
//!
//! - `collect_meshes` queries `(&Mesh, &Transform, …)`
//! - `step_physics` queries `(&RigidBody, &Transform, …)`
//! - `propagate_transforms` reads `Transform` and `continue`s when it is absent
//!
//! So an item in a bag needs no flag, no second representation and no conversion. Its mesh, its
//! collider, its `Interactable` and its own per-item state stay exactly where they were, and putting
//! a `Transform` back drops it on the floor with all of that intact.
//!
//! # What this module does not know
//!
//! What an item *means*. There is no "use item" hook, no registered behaviour and no notion of
//! equipment, because a key and a flashlight differ in ways that are genre knowledge — invariant I4
//! one level up, the same split ADR 0068 draws for behaviour, where the game writes the facts and
//! reads the state.
//!
//! ```no_run
//! # use amadeo_ecs::World;
//! # use amadeo_inventory::{Inventory, Item, contents, store};
//! # let mut world = World::new();
//! # let bag = world.spawn();
//! # let key = world.spawn();
//! world.insert(bag, Inventory { slots: 8 });
//! world.insert(key, Item::single("brass-key"));
//!
//! store(&mut world, key, bag).expect("there is room");
//! assert_eq!(contents(&world, bag), vec![(0, key)]);
//! ```

use amadeo_app::App;
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity, World};
use amadeo_events::{Event, WorldEvents};
use amadeo_reflect::{Reflect, RegistryError};
use amadeo_transform::{GlobalTransform, Transform};

/// What something is, and how many of it this entity represents.
///
/// # Why the count lives here rather than in its own component
///
/// Every item has a count, if only `1`, so a separate `Stack` would be a component that is always
/// present — which is a field wearing a costume. Keeping `max_stack` beside it means "does this
/// stack" is answered by the item itself rather than by a table somewhere, and `1` is how something
/// says it does not.
#[derive(Debug, Clone, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Item {
    /// What kind of thing this is — normally the asset id of the prefab it was spawned from.
    ///
    /// **Two stacks merge only when these match exactly.** A `String` rather than an id type
    /// because every asset id in this engine is one (`Mesh::mesh`, `Material`'s textures), and a
    /// bespoke type here would be the only one.
    pub kind: String,
    /// How many this entity represents. One entity, `count` things.
    pub count: u32,
    /// The most that may sit in one stack. `1` means it does not stack at all.
    ///
    /// Per item rather than a global rule, because a game with stacking arrows and unstackable
    /// swords is the normal case rather than an awkward one.
    pub max_stack: u32,
}

impl Item {
    /// One of something, unstackable.
    #[must_use]
    pub fn single(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            count: 1,
            max_stack: 1,
        }
    }

    /// `count` of something that stacks up to `max_stack`.
    #[must_use]
    pub fn stack(kind: &str, count: u32, max_stack: u32) -> Self {
        Self {
            kind: kind.to_string(),
            count,
            max_stack,
        }
    }

    /// How many more would fit in this stack.
    #[must_use]
    pub fn room(&self) -> u32 {
        self.max_stack.saturating_sub(self.count)
    }
}

impl Component for Item {}

/// Something that holds items.
///
/// A container is an ordinary entity — a chest in the room, a player, a corpse — and nothing here
/// cares which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Inventory {
    /// How many slots it has. Slot indices run from `0` to `slots - 1`.
    pub slots: u32,
}

impl Component for Inventory {}

/// Where an item is, when it is not in the world.
///
/// # The slot is authored, and that is the same call ADR 0063 made
///
/// [`contents`] sorts by this rather than returning whatever a query yields. Query order is
/// *reproducible* — archetype order, then row order — but it is not **stable**: an item's position
/// in the list would move when an unrelated component was added to it, so "my sword is in the third
/// slot" would depend on archetype churn. An authored index does not, and a grid inventory needs one
/// anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableHash, Reflect)]
pub struct StoredIn {
    /// Which container holds it.
    pub container: Entity,
    /// Which slot, counting from zero.
    pub slot: u32,
}

impl Component for StoredIn {}

/// Something was picked up.
///
/// Past tense, because it is a fact rather than a request (`CLAUDE.md` §6). Carries both ends, for
/// the reason `Interacted` does: a game with two players in one room needs to know who took it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableHash, Reflect)]
pub struct ItemStored {
    /// What was stored.
    pub item: Entity,
    /// What it went into.
    pub container: Entity,
}

impl Event for ItemStored {}

/// Something was dropped into the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableHash, Reflect)]
pub struct ItemDropped {
    /// What was dropped.
    pub item: Entity,
    /// What it came out of.
    pub container: Entity,
}

impl Event for ItemDropped {}

/// Why something could not be stored.
///
/// Every variant names what was wrong and what would have worked, because a game showing "you can't
/// carry that" has to know *which* reason it was.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// The entity has no [`Item`] on it.
    #[error("entity {entity} is not an item: it has no `Item` component")]
    NotAnItem {
        /// The entity that was offered.
        entity: Entity,
    },

    /// The container has no [`Inventory`] on it.
    #[error("entity {entity} is not a container: it has no `Inventory` component")]
    NotAContainer {
        /// The entity that was offered.
        entity: Entity,
    },

    /// Every slot is taken and no existing stack had room.
    #[error(
        "`{kind}` will not fit: all {slots} slots are full and no stack of it has room left. \
         Take something out, or raise the container's `slots`"
    )]
    Full {
        /// What was being stored.
        kind: String,
        /// How many slots the container has.
        slots: u32,
    },

    /// A container was about to be put inside itself.
    #[error(
        "entity {entity} cannot be stored in itself. A container inside another container is fine; \
         this one is a cycle"
    )]
    IntoItself {
        /// The entity on both ends.
        entity: Entity,
    },
}

/// Puts an item into a container, merging into an existing stack when it can.
///
/// Returns the slot it ended up in. **Removes the item's `Transform`**, which is what takes it out
/// of the world — see the module docs for why that is sufficient.
///
/// When the item merges completely into an existing stack, that stack's `count` grows and the
/// **offered entity is despawned**, because two entities describing one pile of arrows is exactly
/// the state that goes wrong later. The returned slot is the stack it joined.
///
/// # Errors
///
/// [`StoreError`], naming which of the four things was wrong.
pub fn store(world: &mut World, item: Entity, container: Entity) -> Result<u32, StoreError> {
    if item == container {
        return Err(StoreError::IntoItself { entity: item });
    }

    let Some(offered) = world.get::<Item>(item).cloned() else {
        return Err(StoreError::NotAnItem { entity: item });
    };
    let Some(inventory) = world.get::<Inventory>(container).copied() else {
        return Err(StoreError::NotAContainer { entity: container });
    };

    let held = contents(world, container);

    // Merging first, because a slot spent on a stack that had room is a slot lost for no reason.
    if offered.max_stack > 1 {
        for (slot, existing) in &held {
            let Some(other) = world.get::<Item>(*existing) else {
                continue;
            };
            if other.kind != offered.kind || other.room() == 0 {
                continue;
            }

            let moved = offered.count.min(other.room());
            if let Some(target) = world.get_mut::<Item>(*existing) {
                target.count += moved;
            }

            if moved == offered.count {
                world.despawn(item);
                return Ok(*slot);
            }
            // A partial merge: the rest still needs a slot of its own, so carry on with what is
            // left. Written back before the search below reads it again.
            if let Some(remaining) = world.get_mut::<Item>(item) {
                remaining.count -= moved;
            }
            break;
        }
    }

    let taken: Vec<u32> = held.iter().map(|(slot, _)| *slot).collect();
    let Some(slot) = (0..inventory.slots).find(|slot| !taken.contains(slot)) else {
        return Err(StoreError::Full {
            kind: offered.kind,
            slots: inventory.slots,
        });
    };

    world.insert(item, StoredIn { container, slot });
    // The two components that put a thing in the world. `GlobalTransform` is derived and every
    // reader requires a `Transform` beside it, so leaving it would be harmless -- but a stale one in
    // a snapshot dump is alarming for no reason, and removing it costs nothing.
    world.remove::<Transform>(item);
    world.remove::<GlobalTransform>(item);

    world.send_event(ItemStored { item, container });
    Ok(slot)
}

/// Takes an item out of its container and puts it into the world at `place`.
///
/// The exact inverse of [`store`]: `StoredIn` comes off and a `Transform` goes on, and everything
/// else about the entity was never touched. Returns the container it came out of, or `None` if it
/// was not stored anywhere.
pub fn drop_at(world: &mut World, item: Entity, place: [f32; 3]) -> Option<Entity> {
    let stored = world.get::<StoredIn>(item).copied()?;

    world.remove::<StoredIn>(item);
    world.insert(
        item,
        Transform {
            translation: place,
            ..Transform::default()
        },
    );

    world.send_event(ItemDropped {
        item,
        container: stored.container,
    });
    Some(stored.container)
}

/// What a container holds, as `(slot, item)` pairs **sorted by slot**.
///
/// Sorted rather than in query order, so the answer does not depend on archetype layout — see
/// [`StoredIn`] for why that distinction matters.
///
/// # A despawned container still answers
///
/// This is a lookup by *handle*, so items still record themselves as being in a container that has
/// since been despawned, and they come back here. That is deliberate rather than an oversight, and
/// it was found by a test written expecting the opposite.
///
/// The alternative — filtering by whether the container is alive — would make an orphaned item
/// invisible to every function in this module while still existing, which is the worse failure: a
/// game emptying a dead chest onto the floor wants to *see* what was in it. Nothing is ambiguous,
/// because an entity handle carries a generation: a new entity reusing the slot is a different
/// handle and does not inherit the contents.
///
/// [`orphaned`] is how you find out the container is gone.
#[must_use]
pub fn contents(world: &World, container: Entity) -> Vec<(u32, Entity)> {
    let mut held: Vec<(u32, Entity)> = world
        .query::<(&StoredIn,)>()
        .filter(|(_, (stored,))| stored.container == container)
        .map(|(entity, (stored,))| (stored.slot, entity))
        .collect();
    held.sort_unstable();
    held
}

/// How many of one kind a container holds, counting across stacks.
///
/// The question a game actually asks — "do I have a key", "how much ammunition is left" — which is
/// not the same as how many slots are used.
#[must_use]
pub fn count_of(world: &World, container: Entity, kind: &str) -> u32 {
    contents(world, container)
        .into_iter()
        .filter_map(|(_, entity)| world.get::<Item>(entity))
        .filter(|item| item.kind == kind)
        .map(|item| item.count)
        .sum()
}

/// Items whose container no longer exists.
///
/// ADR 0015's call for a `Parent` pointing at a dead entity, one module along: nothing panics and
/// nothing is destroyed, because whether an orphaned item is a leak or a spilled bag is the game's
/// decision. This reports them; acting on them is the game's job.
#[must_use]
pub fn orphaned(world: &World) -> Vec<Entity> {
    world
        .query::<(&StoredIn,)>()
        .filter(|(_, (stored,))| !world.contains(stored.container))
        .map(|(entity, _)| entity)
        .collect()
}

/// Registers the components and the two events.
///
/// **No systems.** Storing and dropping are things a game *does* at a moment it chooses, not work
/// that happens every tick — so unlike `modules/amadeo-character` and `modules/amadeo-interaction`
/// there is no ordering to get right and nothing to declare.
///
/// # Errors
///
/// [`RegistryError`] if any of the components is already registered under a different type.
pub fn install(app: &mut App) -> Result<(), RegistryError> {
    app.register_component::<Item>()?;
    app.register_component::<Inventory>()?;
    app.register_component::<StoredIn>()?;
    app.register_event::<ItemStored>();
    app.register_event::<ItemDropped>();
    Ok(())
}
