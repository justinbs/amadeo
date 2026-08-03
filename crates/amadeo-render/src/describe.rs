//! What is on screen, as data rather than as an image — the agent's cheap eyes.
//!
//! # Why this exists
//!
//! `docs/03-ai-native-design.md` puts it plainly: a structured description of the screen is "far
//! cheaper than an image and often sufficient to verify layout, overlap, or off-screen bugs without
//! vision". M1's exit gate goes further and *requires* it — verification of the milestone's game is
//! to be done "purely through `inspect`, headless runs, and `render.describe`, with screenshots used
//! only for final confirmation".
//!
//! That is the actual bar. An agent with no eyes has to be able to answer "is the player where I
//! think it is", "did the enemy leave the screen", and "are these two things overlapping" — and a
//! PNG answers none of those without vision.
//!
//! # It reads the world, not the frame
//!
//! [`FrameData`](crate::FrameData) would have been the obvious source and is the wrong one. A
//! `SpriteInstance` deliberately carries **no entity id** (ADR 0023) — that is twenty thousand
//! entity handles per frame the GPU has no use for, on the path the whole batcher exists to keep
//! cheap.
//!
//! So this walks the world instead, exactly as the collection pass does, and costs **nothing at all
//! when nobody is asking**. That is the right trade for an introspection API: `world.query` makes
//! the same one.
//!
//! # Screen space, and the one flip in it
//!
//! World space has +Y upward; screen space has +Y downward, with the origin at the top-left. The
//! projection here mirrors `quad.wgsl` and `sprite.wgsl` exactly — same camera, same aspect
//! handling — and then flips Y once at the end. Getting that flip wrong would make every reported
//! position subtly wrong in a way that looks plausible, which is why
//! `a_sprite_below_the_camera_reports_a_larger_screen_y` pins it.

use crate::components::{Camera2d, Quad, SortOrder, Sprite};
use crate::{Renderer, local_matrix};
use amadeo_ecs::{Entity, World};
use amadeo_transform::{GlobalTransform, Transform};

/// What kind of thing was drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawnKind {
    /// An untextured rectangle.
    Quad,
    /// A textured sprite, and the asset id of its texture.
    Sprite {
        /// The declared asset id (ADR 0020), as `amadeo assets` lists it.
        texture: String,
    },
}

/// One entity as it appears on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnEntity {
    /// Which entity, so this can be joined against `world.entity`.
    pub entity: Entity,
    /// Quad or sprite.
    pub kind: DrawnKind,
    /// Draw order. Higher draws later, and therefore on top.
    pub order: i32,
    /// Centre in screen pixels, origin top-left.
    pub center: [f32; 2],
    /// Width and height in screen pixels.
    pub size: [f32; 2],
    /// Whether any part of it falls inside the viewport.
    ///
    /// The question "why can I not see it" most often answers itself here, and answering it needs
    /// no image.
    pub visible: bool,
}

impl DrawnEntity {
    /// The screen rectangle, as `[left, top, right, bottom]`.
    #[must_use]
    pub fn bounds(&self) -> [f32; 4] {
        let half = [self.size[0] / 2.0, self.size[1] / 2.0];
        [
            self.center[0] - half[0],
            self.center[1] - half[1],
            self.center[0] + half[0],
            self.center[1] + half[1],
        ]
    }

    /// Whether this entity's screen rectangle overlaps another's.
    ///
    /// Provided rather than left to the caller because "are these two overlapping" is one of the
    /// three questions this whole module exists to answer, and getting a rectangle intersection
    /// subtly wrong is a classic.
    #[must_use]
    pub fn overlaps(&self, other: &DrawnEntity) -> bool {
        let [left, top, right, bottom] = self.bounds();
        let [other_left, other_top, other_right, other_bottom] = other.bounds();
        left < other_right && other_left < right && top < other_bottom && other_top < bottom
    }
}

/// Everything on screen this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameDescription {
    /// Drawable size in physical pixels.
    pub viewport: [u32; 2],
    /// The camera the description was computed through.
    pub camera: Camera2d,
    /// Every drawable entity, **sorted by draw order then entity** — so two descriptions of the
    /// same world are identical, and a diff of two ticks shows what actually moved (invariant I3).
    pub drawn: Vec<DrawnEntity>,
}

impl FrameDescription {
    /// How many entities fall at least partly inside the viewport.
    #[must_use]
    pub fn visible_count(&self) -> usize {
        self.drawn.iter().filter(|drawn| drawn.visible).count()
    }

    /// How many are entirely outside it.
    #[must_use]
    pub fn off_screen_count(&self) -> usize {
        self.drawn.len() - self.visible_count()
    }

    /// The drawn entry for an entity, if it is drawable at all.
    #[must_use]
    pub fn find(&self, entity: Entity) -> Option<&DrawnEntity> {
        self.drawn.iter().find(|drawn| drawn.entity == entity)
    }
}

/// Describes what a frame drawn from this world would contain.
///
/// Uses the [`Renderer`] service's viewport when one is installed, and 1280x720 otherwise — so a
/// headless test with no renderer still gets sensible screen coordinates rather than nothing.
///
/// Read-only. Describing a world cannot perturb it, for the same reason capturing a snapshot cannot:
/// an agent asking what is on screen must not change what is on screen.
#[must_use]
pub fn describe_frame(world: &World) -> FrameDescription {
    let camera = world.resource::<Camera2d>().copied().unwrap_or_default();
    let viewport = world
        .service::<Renderer>()
        .map_or((1280, 720), Renderer::viewport);

    let projection = Projection::new(camera, viewport);
    let mut drawn = Vec::new();

    for (entity, (transform, quad, order, global)) in world.query::<(
        &Transform,
        &Quad,
        Option<&SortOrder>,
        Option<&GlobalTransform>,
    )>() {
        let (center, size) = placement(transform, global, quad.size);
        drawn.push(projection.entry(
            entity,
            DrawnKind::Quad,
            order.copied().unwrap_or_default().order,
            center,
            size,
        ));
    }

    for (entity, (transform, sprite, order, global)) in world.query::<(
        &Transform,
        &Sprite,
        Option<&SortOrder>,
        Option<&GlobalTransform>,
    )>() {
        let (center, size) = placement(transform, global, sprite.size);
        drawn.push(projection.entry(
            entity,
            DrawnKind::Sprite {
                texture: sprite.texture.clone(),
            },
            order.copied().unwrap_or_default().order,
            center,
            size,
        ));
    }

    // Sorted so two descriptions of one world are identical, and a diff between ticks shows only
    // what moved. Entity is the tie-break because two things can share an order.
    drawn.sort_by_key(|entry| (entry.order, entry.entity.index(), entry.entity.generation()));

    FrameDescription {
        viewport: [viewport.0, viewport.1],
        camera,
        drawn,
    }
}

/// An entity's world-space centre and size, after its parents have had their say.
///
/// The same fallback the collection passes use: without a [`GlobalTransform`] an entity is drawn at
/// its local transform, because a game that forgot to register propagation should see its entities
/// in slightly the wrong place rather than not at all.
fn placement(
    transform: &Transform,
    global: Option<&GlobalTransform>,
    size: [f32; 2],
) -> ([f32; 2], [f32; 2]) {
    let placement = match global {
        Some(global) => *global,
        None => GlobalTransform::from(local_matrix(transform)),
    };
    let matrix = placement.to_mat4();
    let translation = matrix.translation();

    // A transform matrix's columns are its scaled axes, so a column's length is that axis's total
    // world-space scale — parents included.
    let scale_x = matrix.columns[0][0].hypot(matrix.columns[0][1]);
    let scale_y = matrix.columns[1][0].hypot(matrix.columns[1][1]);

    (
        [translation[0], translation[1]],
        [size[0] * scale_x, size[1] * scale_y],
    )
}

/// World space to screen pixels, matching the shaders exactly.
struct Projection {
    camera: Camera2d,
    half_extents: [f32; 2],
    viewport: (u32, u32),
}

impl Projection {
    fn new(camera: Camera2d, viewport: (u32, u32)) -> Projection {
        // Width follows the aspect ratio, so resizing shows more world rather than stretching it.
        // Identical to `WgpuBackend::render`; if that changes, this has to change with it.
        let aspect = viewport.0 as f32 / viewport.1.max(1) as f32;
        let half_height = camera.height / 2.0;
        Projection {
            camera,
            half_extents: [half_height * aspect, half_height],
            viewport,
        }
    }

    /// Builds one entry, projecting its centre and size into screen pixels.
    fn entry(
        &self,
        entity: Entity,
        kind: DrawnKind,
        order: i32,
        world_center: [f32; 2],
        world_size: [f32; 2],
    ) -> DrawnEntity {
        let center = self.to_screen(world_center);
        // A size is a length rather than a position, so it scales but does not translate — and it
        // stays positive under the Y flip.
        let size = [
            world_size[0] / (self.half_extents[0] * 2.0) * self.viewport.0 as f32,
            world_size[1] / (self.half_extents[1] * 2.0) * self.viewport.1 as f32,
        ];

        let half = [size[0] / 2.0, size[1] / 2.0];
        let visible = center[0] + half[0] > 0.0
            && center[0] - half[0] < self.viewport.0 as f32
            && center[1] + half[1] > 0.0
            && center[1] - half[1] < self.viewport.1 as f32;

        DrawnEntity {
            entity,
            kind,
            order,
            center,
            size,
            visible,
        }
    }

    /// One world point in screen pixels, origin top-left.
    fn to_screen(&self, world: [f32; 2]) -> [f32; 2] {
        let ndc = [
            (world[0] - self.camera.center[0]) / self.half_extents[0],
            (world[1] - self.camera.center[1]) / self.half_extents[1],
        ];
        [
            (ndc[0] + 1.0) / 2.0 * self.viewport.0 as f32,
            // Flipped: world +Y is up, screen +Y is down.
            (1.0 - ndc[1]) / 2.0 * self.viewport.1 as f32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NullBackend;

    fn world_with_view(width: u32, height: u32) -> World {
        let mut world = World::new();
        world.insert_resource(Camera2d {
            center: [0.0, 0.0],
            height: 10.0,
        });
        world.insert_service(Renderer::new(Box::new(NullBackend::new(width, height))));
        world
    }

    fn add_quad(world: &mut World, x: f32, y: f32, order: i32) -> Entity {
        let entity = world.spawn();
        world.insert(entity, Transform::at(x, y));
        world.insert(entity, Quad::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]));
        world.insert(entity, SortOrder::new(order));
        entity
    }

    #[test]
    fn an_entity_at_the_camera_centre_lands_in_the_middle_of_the_screen() {
        let mut world = world_with_view(800, 600);
        let entity = add_quad(&mut world, 0.0, 0.0, 0);

        let description = describe_frame(&world);
        let drawn = description.find(entity).expect("drawable");

        assert_eq!(drawn.center, [400.0, 300.0]);
        assert!(drawn.visible);
    }

    #[test]
    fn a_sprite_below_the_camera_reports_a_larger_screen_y() {
        // The one flip in the projection. World +Y is up and screen +Y is down, so something *below*
        // the camera has a *bigger* screen y. Getting this backwards would make every reported
        // position plausible and wrong.
        let mut world = world_with_view(800, 600);
        let above = add_quad(&mut world, 0.0, 2.0, 0);
        let below = add_quad(&mut world, 0.0, -2.0, 0);

        let description = describe_frame(&world);
        let above_y = description.find(above).expect("drawable").center[1];
        let below_y = description.find(below).expect("drawable").center[1];

        assert!(above_y < below_y, "above {above_y}, below {below_y}");
        assert_eq!(above_y, 300.0 - 2.0 / 10.0 * 600.0);
    }

    #[test]
    fn the_view_widens_with_the_aspect_ratio_rather_than_stretching() {
        // `Camera2d::height` is authoritative and width follows, so a wider window shows more world
        // at the same scale. A one-unit quad must therefore be the same pixel size in both.
        let mut narrow = world_with_view(600, 600);
        let a = add_quad(&mut narrow, 0.0, 0.0, 0);

        let mut wide = world_with_view(1200, 600);
        let b = add_quad(&mut wide, 0.0, 0.0, 0);

        let in_narrow = describe_frame(&narrow).find(a).expect("drawable").size;
        let in_wide = describe_frame(&wide).find(b).expect("drawable").size;

        assert_eq!(in_narrow, in_wide);
    }

    #[test]
    fn something_outside_the_view_is_reported_as_not_visible() {
        // "Why can I not see it" answered without an image, which is the whole point.
        let mut world = world_with_view(800, 600);
        let on_screen = add_quad(&mut world, 0.0, 0.0, 0);
        let far_away = add_quad(&mut world, 500.0, 0.0, 0);

        let description = describe_frame(&world);

        assert!(description.find(on_screen).expect("drawable").visible);
        assert!(!description.find(far_away).expect("drawable").visible);
        assert_eq!(description.visible_count(), 1);
        assert_eq!(description.off_screen_count(), 1);
    }

    #[test]
    fn something_half_off_the_edge_still_counts_as_visible() {
        // The boundary case. Judging by the centre alone would call this invisible, and it is not.
        let mut world = world_with_view(800, 600);
        // The view is 10 units tall; at 800x600 it is 13.33 wide, so the right edge is x = 6.67.
        let straddling = add_quad(&mut world, 6.6, 0.0, 0);

        assert!(
            describe_frame(&world)
                .find(straddling)
                .expect("drawable")
                .visible
        );
    }

    #[test]
    fn overlap_is_answerable_without_an_image() {
        let mut world = world_with_view(800, 600);
        let a = add_quad(&mut world, 0.0, 0.0, 0);
        let touching = add_quad(&mut world, 0.5, 0.0, 0);
        let apart = add_quad(&mut world, 4.0, 0.0, 0);

        let description = describe_frame(&world);
        let a = description.find(a).expect("drawable");

        assert!(a.overlaps(description.find(touching).expect("drawable")));
        assert!(!a.overlaps(description.find(apart).expect("drawable")));
    }

    #[test]
    fn entries_come_back_in_draw_order() {
        // So a description reads top-to-bottom the way the screen composites, and so two
        // descriptions of one world are byte-identical (invariant I3).
        let mut world = world_with_view(800, 600);
        add_quad(&mut world, 0.0, 0.0, 5);
        add_quad(&mut world, 1.0, 0.0, -3);
        add_quad(&mut world, 2.0, 0.0, 0);

        let orders: Vec<i32> = describe_frame(&world)
            .drawn
            .iter()
            .map(|drawn| drawn.order)
            .collect();
        assert_eq!(orders, vec![-3, 0, 5]);
    }

    #[test]
    fn a_sprite_reports_the_texture_it_would_use() {
        let mut world = world_with_view(800, 600);
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        world.insert(entity, Sprite::new("wall_concrete", 1.0, 1.0));

        let description = describe_frame(&world);
        assert_eq!(
            description.find(entity).expect("drawable").kind,
            DrawnKind::Sprite {
                texture: "wall_concrete".to_string()
            }
        );
    }

    #[test]
    fn a_parents_transform_reaches_the_reported_position() {
        use amadeo_transform::{Parent, propagate_transforms};

        let mut world = world_with_view(800, 600);
        let mut turned = Transform::default();
        turned.rotation[2] = 90.0;
        let parent = world.spawn();
        world.insert(parent, turned);

        let child = world.spawn();
        world.insert(child, Transform::at(2.0, 0.0));
        world.insert(child, Parent(parent));
        world.insert(child, Quad::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]));

        propagate_transforms(&mut world);
        let description = describe_frame(&world);
        let drawn = description.find(child).expect("drawable");

        // The parent's quarter turn moves the child to world (0, 2), which is above centre.
        assert!((drawn.center[0] - 400.0).abs() < 0.01, "{:?}", drawn.center);
        assert!(drawn.center[1] < 300.0, "{:?}", drawn.center);
    }

    #[test]
    fn a_parents_scale_reaches_the_reported_size() {
        use amadeo_transform::{Parent, propagate_transforms};

        let mut world = world_with_view(800, 600);
        let parent = world.spawn();
        world.insert(
            parent,
            Transform {
                scale: [3.0, 3.0, 1.0],
                ..Transform::default()
            },
        );

        let child = world.spawn();
        world.insert(child, Transform::at(0.0, 0.0));
        world.insert(child, Parent(parent));
        world.insert(child, Quad::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]));

        propagate_transforms(&mut world);
        let unscaled = 1.0 / 10.0 * 600.0;
        let drawn = describe_frame(&world).find(child).expect("drawable").size;

        assert!((drawn[1] - unscaled * 3.0).abs() < 0.01, "got {drawn:?}");
    }

    #[test]
    fn describing_a_world_does_not_change_it() {
        // An agent asking what is on screen must not change what is on screen.
        let mut world = world_with_view(800, 600);
        add_quad(&mut world, 1.0, 2.0, 0);

        let before = world.state_hash();
        for _ in 0..5 {
            let _ = describe_frame(&world);
        }
        assert_eq!(world.state_hash(), before);
    }

    #[test]
    fn a_world_with_no_renderer_still_describes() {
        // Headless, with nothing installed. The gate says verification happens in headless runs, so
        // this path is the normal one rather than a fallback.
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        world.insert(entity, Quad::default());

        let description = describe_frame(&world);
        assert_eq!(description.viewport, [1280, 720]);
        assert_eq!(
            description.find(entity).expect("drawable").center,
            [640.0, 360.0]
        );
    }

    #[test]
    fn describing_is_reproducible() {
        let mut world = world_with_view(800, 600);
        for index in 0..10 {
            add_quad(&mut world, index as f32, 0.0, index % 3);
        }
        assert_eq!(describe_frame(&world), describe_frame(&world));
    }
}
