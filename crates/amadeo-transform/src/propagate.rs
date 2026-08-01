//! Composing a child's transform with its parents', every tick.
//!
//! [`Parent`] has recorded hierarchy since ADR 0015 without anything acting on it. This is the thing
//! that acts on it.

use crate::{Mat4, Parent, Transform};
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity, World};
use amadeo_reflect::Reflect;

/// The label the app layer registers [`propagate_transforms`] under.
pub const PROPAGATE_TRANSFORMS: &str = "propagate_transforms";

/// How deep a parent chain may go before it is treated as malformed.
///
/// A `Parent` cycle would otherwise walk forever. Real hierarchies are a handful of levels; a
/// skeleton is tens. 64 is far past anything legitimate and cheap to check.
pub const MAX_DEPTH: usize = 64;

/// Where an entity actually is, after its parents have had their say.
///
/// Computed every tick from [`Transform`] and [`Parent`] — never authored, and never written to a
/// scene file. Because it is derived it is **excluded from the state hash** (ADR 0019), which is
/// what keeps matrix arithmetic from being able to move a replay.
///
/// An entity with no [`Parent`] gets its own transform, unchanged.
// Not a doc comment: `///` on a reflected type is what `amadeo describe` prints.
//
// It is still `Reflect` and still a `Component`, so an agent can inspect it and the renderer can
// query it alongside `Quad`. Only the *hashing* is skipped -- I8 is untouched.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct GlobalTransform {
    /// The composed transform, column-major, ready for the GPU.
    pub matrix: [f32; 16],
}

impl Default for GlobalTransform {
    fn default() -> Self {
        GlobalTransform::from(Mat4::IDENTITY)
    }
}

impl From<Mat4> for GlobalTransform {
    fn from(value: Mat4) -> Self {
        // Column-major, so each source column becomes four consecutive entries.
        let mut matrix = [0.0_f32; 16];
        for (column, source) in value.columns.iter().enumerate() {
            matrix[column * 4..column * 4 + 4].copy_from_slice(source);
        }
        GlobalTransform { matrix }
    }
}

impl GlobalTransform {
    /// The matrix, in the shape the maths uses.
    #[must_use]
    pub fn to_mat4(self) -> Mat4 {
        let mut columns = [[0.0_f32; 4]; 4];
        for (column, target) in columns.iter_mut().enumerate() {
            target.copy_from_slice(&self.matrix[column * 4..column * 4 + 4]);
        }
        Mat4 { columns }
    }

    /// Where this entity ends up in world space.
    #[must_use]
    pub fn translation(self) -> [f32; 3] {
        [self.matrix[12], self.matrix[13], self.matrix[14]]
    }
}

impl Component for GlobalTransform {
    // ADR 0019. Recomputed from scratch every tick by `propagate_transforms`, so hashing it would
    // assert on a value carrying no information its inputs do not already carry -- while making
    // every replay sensitive to matrix arithmetic.
    const DERIVED: bool = true;
}

/// Recomputes every [`GlobalTransform`] from the [`Transform`]/[`Parent`] hierarchy.
///
/// Belongs in `PostSimulation`: gameplay moves things during `Simulation`, and everything downstream
/// — rendering, culling, spatial queries — wants the settled answer.
///
/// # How it walks the hierarchy
///
/// For each entity, walk *up* to its root collecting local matrices, then multiply back down. That
/// is O(depth) per entity rather than the O(1) a sorted single pass would give, and it is chosen
/// deliberately: a topological pass needs a depth-sorted work list rebuilt whenever the hierarchy
/// changes, which is a cache with an invalidation story. This version has no state at all, and
/// hierarchies are shallow. Revisit when a profile says to, not before.
///
/// # Cycles
///
/// A `Parent` cycle is a malformed world — nothing legitimate produces one — but it is reachable by
/// hand-editing a scene file, so it cannot be allowed to hang. Chains longer than [`MAX_DEPTH`] stop
/// and the entity falls back to its **local** transform. That is visibly wrong on screen rather than
/// silently wrong, which is the better failure: the entity appears at its unparented position
/// instead of the process locking up.
pub fn propagate_transforms(world: &mut World) {
    // Collected first because computing needs to read the whole world while writing needs it
    // mutably. `entities()` is sorted, so the work order is deterministic (invariant I3) -- though
    // nothing here depends on order, since each entity's answer comes only from its own ancestors.
    let mut computed: Vec<(Entity, GlobalTransform)> = Vec::new();

    for entity in world.entities() {
        let Some(local) = world.get::<Transform>(entity) else {
            continue;
        };

        let mut matrix = local_matrix(local);
        let mut current = entity;
        let mut depth = 0;

        while let Some(parent) = world.get::<Parent>(current).map(|p| p.0) {
            depth += 1;
            if depth > MAX_DEPTH {
                // A cycle, or a hierarchy deep enough to be indistinguishable from one. Fall back
                // to the local transform rather than a half-composed matrix, which would be
                // meaningless rather than merely wrong.
                matrix = local_matrix(local);
                break;
            }

            // A `Parent` pointing at a despawned entity simply stops the walk (ADR 0015 says the
            // handle stops resolving rather than misbehaving), so the child ends up at its local
            // transform. A parent with no `Transform` of its own contributes nothing.
            let Some(parent_local) = world.get::<Transform>(parent) else {
                break;
            };
            matrix = local_matrix(parent_local).mul(&matrix);
            current = parent;
        }

        computed.push((entity, GlobalTransform::from(matrix)));
    }

    for (entity, global) in computed {
        world.insert(entity, global);
    }
}

/// One entity's own transform as a matrix, with no parent applied.
fn local_matrix(transform: &Transform) -> Mat4 {
    Mat4::from_transform(transform.translation, transform.rotation, transform.scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spawns an entity with a transform, optionally parented.
    fn spawn(world: &mut World, transform: Transform, parent: Option<Entity>) -> Entity {
        let entity = world.spawn();
        world.insert(entity, transform);
        if let Some(parent) = parent {
            world.insert(entity, Parent(parent));
        }
        entity
    }

    fn world_position(world: &World, entity: Entity) -> [f32; 3] {
        world
            .get::<GlobalTransform>(entity)
            .expect("propagated")
            .translation()
    }

    #[test]
    fn a_root_gets_its_own_transform() {
        let mut world = World::new();
        let root = spawn(&mut world, Transform::at(3.0, 4.0), None);

        propagate_transforms(&mut world);

        assert_eq!(world_position(&world, root), [3.0, 4.0, 0.0]);
    }

    #[test]
    fn a_child_is_offset_by_its_parent() {
        let mut world = World::new();
        let parent = spawn(&mut world, Transform::at(10.0, 0.0), None);
        let child = spawn(&mut world, Transform::at(0.0, 5.0), Some(parent));

        propagate_transforms(&mut world);

        assert_eq!(world_position(&world, child), [10.0, 5.0, 0.0]);
    }

    #[test]
    fn a_grandchild_accumulates_the_whole_chain() {
        let mut world = World::new();
        let a = spawn(&mut world, Transform::at(1.0, 0.0), None);
        let b = spawn(&mut world, Transform::at(2.0, 0.0), Some(a));
        let c = spawn(&mut world, Transform::at(4.0, 0.0), Some(b));

        propagate_transforms(&mut world);

        assert_eq!(world_position(&world, c), [7.0, 0.0, 0.0]);
    }

    #[test]
    fn a_parents_rotation_swings_its_child() {
        // The thing propagation is actually for.
        let mut world = World::new();
        let mut turned = Transform::default();
        turned.rotation[2] = 90.0;
        let parent = spawn(&mut world, turned, None);
        let child = spawn(&mut world, Transform::at(2.0, 0.0), Some(parent));

        propagate_transforms(&mut world);

        let at = world_position(&world, child);
        assert!((at[0] - 0.0).abs() < 1e-5, "got {at:?}");
        assert!((at[1] - 2.0).abs() < 1e-5, "got {at:?}");
    }

    #[test]
    fn moving_a_parent_moves_its_child_next_tick() {
        let mut world = World::new();
        let parent = spawn(&mut world, Transform::at(0.0, 0.0), None);
        let child = spawn(&mut world, Transform::at(1.0, 0.0), Some(parent));

        propagate_transforms(&mut world);
        assert_eq!(world_position(&world, child), [1.0, 0.0, 0.0]);

        world
            .get_mut::<Transform>(parent)
            .expect("exists")
            .translation[0] = 10.0;
        propagate_transforms(&mut world);

        assert_eq!(world_position(&world, child), [11.0, 0.0, 0.0]);
    }

    #[test]
    fn a_cycle_falls_back_to_local_rather_than_hanging() {
        // Unreachable through normal authoring, but a hand-edited scene can express it, and a hang
        // is the one failure mode that gives the reader nothing to work with.
        let mut world = World::new();
        let a = spawn(&mut world, Transform::at(1.0, 0.0), None);
        let b = spawn(&mut world, Transform::at(2.0, 0.0), Some(a));
        world.insert(a, Parent(b));

        propagate_transforms(&mut world);

        assert_eq!(world_position(&world, a), [1.0, 0.0, 0.0]);
        assert_eq!(world_position(&world, b), [2.0, 0.0, 0.0]);
    }

    #[test]
    fn a_parent_that_no_longer_exists_leaves_the_child_where_it_is() {
        let mut world = World::new();
        let parent = spawn(&mut world, Transform::at(10.0, 0.0), None);
        let child = spawn(&mut world, Transform::at(1.0, 0.0), Some(parent));
        world.despawn(parent);

        propagate_transforms(&mut world);

        assert_eq!(world_position(&world, child), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn propagation_does_not_move_the_state_hash() {
        // ADR 0019, and the reason it exists. Adding GlobalTransform to every entity must leave
        // replay assertions exactly where they were.
        let mut world = World::new();
        let parent = spawn(&mut world, Transform::at(10.0, 0.0), None);
        spawn(&mut world, Transform::at(1.0, 2.0), Some(parent));

        let before = world.state_hash();
        propagate_transforms(&mut world);
        let after = world.state_hash();

        assert_eq!(
            before, after,
            "a derived component must not contribute to the state hash"
        );
    }

    #[test]
    fn a_real_change_still_moves_the_state_hash() {
        // Guards the test above from passing for the wrong reason -- if state_hash had simply
        // stopped working, both tests would agree and both would be worthless.
        let mut world = World::new();
        let entity = spawn(&mut world, Transform::at(1.0, 0.0), None);

        propagate_transforms(&mut world);
        let before = world.state_hash();

        world
            .get_mut::<Transform>(entity)
            .expect("exists")
            .translation[0] = 2.0;
        let after = world.state_hash();

        assert_ne!(before, after, "moving an entity is a real state change");
    }

    #[test]
    fn propagating_twice_gives_the_same_answer() {
        // It must be idempotent: it runs every tick, and a second run with no input change that
        // produced a different matrix would mean the output depends on the previous output.
        let mut world = World::new();
        let parent = spawn(&mut world, Transform::at(3.0, 1.0), None);
        let child = spawn(&mut world, Transform::at(2.0, 2.0), Some(parent));

        propagate_transforms(&mut world);
        let once = *world.get::<GlobalTransform>(child).expect("propagated");
        propagate_transforms(&mut world);
        let twice = *world.get::<GlobalTransform>(child).expect("propagated");

        assert_eq!(once, twice);
    }

    #[test]
    fn an_entity_without_a_transform_is_left_alone() {
        let mut world = World::new();
        let bare = world.spawn();

        propagate_transforms(&mut world);

        assert!(world.get::<GlobalTransform>(bare).is_none());
    }
}
