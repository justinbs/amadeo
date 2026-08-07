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

use crate::components::{Camera, Projection, Quad, SortOrder, Sprite};
use crate::{Renderer, local_matrix};
use amadeo_ecs::{Entity, World};
use amadeo_transform::{GlobalTransform, Mat4, Transform};

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
    /// A piece of 3D geometry — **Q26**, and what M2.5's exit gate 3 measures culling through.
    Mesh {
        /// The declared asset id of the geometry.
        mesh: String,
        /// The declared asset id of the material. Empty means the default.
        material: String,
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
    ///
    /// **Which one** matters since ADR 0031 made a camera an entity: this is the active orthographic
    /// camera with the lowest `order` drawing to the window, or a default when the world has none.
    /// `describe_frame_through` asks about a different one.
    pub camera: Camera,
    /// That camera's world position, which lives on its `Transform` rather than on the camera.
    ///
    /// **Three components since Q26**, and the widening was the point rather than a detail: a 2D
    /// camera's z is zero and reporting two numbers was honest for it, but a 3D camera's height above
    /// the ground is most of what decides its view. Dropping it silently would be the same class of
    /// confidently-wrong answer that `render.describe` used to give a 3D world.
    pub eye: [f32; 3],
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
    // "What is on screen" stopped having one answer when a world gained the ability to hold several
    // cameras, so this picks the one that draws first to the window and says which in the reply. A
    // world with no camera gets a default rather than an empty description: the entities and their
    // world positions are still real, and refusing to answer would make `render.describe` useless on
    // exactly the half-built world an agent most wants to look at.
    //
    // **`primary_view` rather than `primary_camera`**, which is the whole of Q26 at this level: the
    // latter filters to orthographic cameras, so asking a 3D world what was on screen used to return
    // a *default* camera nobody authored, and zero entities. That is worse than an error, and it cost
    // a debugging detour in session 13.
    let (camera, eye_matrix) =
        crate::primary_view(world).unwrap_or_else(|| (Camera::default(), Mat4::IDENTITY));
    describe_frame_with(world, camera, &eye_matrix)
}

/// The same description, computed through one specific camera.
///
/// For a world with more than one — a minimap, a security monitor, the editor's viewport. `entity`
/// must carry a [`Camera`]; anything else returns `None`, because silently falling back to a
/// different camera would answer a question nobody asked.
#[must_use]
pub fn describe_frame_through(world: &World, entity: Entity) -> Option<FrameDescription> {
    let camera = world.get::<Camera>(entity)?.clone();
    let eye_matrix = crate::camera_matrix(world, entity);
    Some(describe_frame_with(world, camera, &eye_matrix))
}

fn describe_frame_with(world: &World, camera: Camera, eye_matrix: &Mat4) -> FrameDescription {
    let viewport = world
        .service::<Renderer>()
        .map_or((1280, 720), Renderer::viewport);

    let projection = ScreenProjection::new(&camera, eye_matrix, viewport);
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
            &flat_corners(center, size),
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
            &flat_corners(center, size),
        ));
    }

    describe_meshes(world, &projection, &mut drawn);

    // Sorted so two descriptions of one world are identical, and a diff between ticks shows only
    // what moved. Entity is the tie-break because two things can share an order.
    drawn.sort_by_key(|entry| (entry.order, entry.entity.index(), entry.entity.generation()));

    FrameDescription {
        viewport: [viewport.0, viewport.1],
        camera,
        eye: eye_matrix.translation(),
        drawn,
    }
}

/// The four corners of a flat, axis-aligned rectangle in the z = 0 plane, at a world centre.
///
/// Quads and sprites are flat and unrotated in the plane the 2D camera looks at, so four corners
/// describe them exactly. Kept separate from the mesh path because a mesh's corners come from its
/// geometry and its model matrix, which is a different question with a different answer.
fn flat_corners(center: [f32; 2], size: [f32; 2]) -> [[f32; 3]; 4] {
    let half = [size[0] / 2.0, size[1] / 2.0];
    [
        [center[0] - half[0], center[1] - half[1], 0.0],
        [center[0] + half[0], center[1] - half[1], 0.0],
        [center[0] + half[0], center[1] + half[1], 0.0],
        [center[0] - half[0], center[1] + half[1], 0.0],
    ]
}

/// Adds every mesh entity to the description — **Q26**.
///
/// # Why this needs the mesh cache
///
/// A `Mesh` component is two asset ids and says nothing about how big the geometry is. The size on
/// screen is the whole point of the answer, so the bounds have to come from the loaded
/// [`MeshCache`], via [`MeshData::bounds`](crate::MeshData::bounds) — the same box frustum culling
/// will test, so the two cannot disagree about what is on screen.
///
/// A mesh whose geometry has not loaded is **skipped**, matching the collection pass exactly: the
/// renderer draws nothing for it, so reporting it as on screen would be describing a frame that will
/// not happen.
fn describe_meshes(world: &World, projection: &ScreenProjection, drawn: &mut Vec<DrawnEntity>) {
    let Some(meshes) = world.service::<crate::MeshCache>() else {
        return;
    };

    for (entity, (transform, mesh, order, global)) in world.query::<(
        &Transform,
        &crate::Mesh,
        Option<&SortOrder>,
        Option<&GlobalTransform>,
    )>() {
        let Some(data) = meshes.get(&mesh.mesh) else {
            continue;
        };
        let Some((min, max)) = data.bounds() else {
            continue;
        };

        let model = match global {
            Some(global) => global.to_mat4(),
            None => local_matrix(transform),
        };

        // All eight, because a rotated box's image is not the image of its two extremes — and under
        // perspective the near face is bigger than the far one, so the two that happen to be nearest
        // do not bound the rest either.
        let mut corners = [[0.0_f32; 3]; 8];
        for (index, corner) in corners.iter_mut().enumerate() {
            let pick = |axis: usize| {
                if index & (1 << axis) == 0 {
                    min[axis]
                } else {
                    max[axis]
                }
            };
            let point = [pick(0), pick(1), pick(2)];
            let transformed = model.transform_point4(point);
            *corner = [transformed[0], transformed[1], transformed[2]];
        }

        drawn.push(projection.entry(
            entity,
            DrawnKind::Mesh {
                mesh: mesh.mesh.clone(),
                material: mesh.material.clone(),
            },
            order.copied().unwrap_or_default().order,
            &corners,
        ));
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

/// World space to screen pixels, through whatever projection the camera actually has.
///
/// # A matrix rather than the hand-rolled 2D projection this used to be
///
/// It was two divisions and a flip, which is all an orthographic camera needs and is exactly why
/// `render.describe` could not see 3D (**Q26**). A perspective camera's answer needs the camera's
/// *orientation* and a perspective divide, and neither exists in a pair of half-extents.
///
/// Building the same `view_projection` the backend builds — `projection * inverse(camera matrix)` —
/// means the two cannot disagree about what is on screen, which matters because M2.5's exit gate 3
/// measures frustum culling *through this*. A describe that projected differently from the renderer
/// would report culling that did not happen, or miss culling that did.
struct ScreenProjection {
    view_projection: Mat4,
    viewport: (u32, u32),
}

impl ScreenProjection {
    fn new(camera: &Camera, eye_matrix: &Mat4, viewport: (u32, u32)) -> ScreenProjection {
        let aspect = viewport.0 as f32 / viewport.1.max(1) as f32;

        let projection = match camera.projection {
            Projection::Perspective { fov, near, far } => Mat4::perspective(fov, aspect, near, far),
            // Width follows the aspect ratio, so resizing shows more world rather than stretching
            // it. The depth range is deliberately generous: a 2D world puts everything at z = 0 and
            // only the x and y of the result are ever read, so near and far exist to contain the
            // scene rather than to order it.
            Projection::Orthographic { height } => {
                let half_height = height / 2.0;
                Mat4::orthographic(half_height * aspect, half_height, -1000.0, 1000.0)
            }
        };

        // A camera's matrix places it in the world; looking *through* it is the inverse.
        // `inverse_rigid` is enough because a camera is a rotation and a translation — a scaled
        // camera is not a thing anything authors, and falling back to the identity for one would put
        // the view at the origin, which is at least obvious rather than subtly skewed.
        let camera_view = eye_matrix.inverse_rigid().unwrap_or(Mat4::IDENTITY);

        ScreenProjection {
            view_projection: projection.mul(&camera_view),
            viewport,
        }
    }

    /// Builds one entry from a set of world-space corners.
    ///
    /// **Corners rather than a centre and a size**, which is what a rotated box requires: the two
    /// extremes of a box are not the extremes of its image once it is turned, and under perspective
    /// the near face is larger than the far one. Projecting every corner and taking the screen
    /// rectangle is correct for both and is eight multiplies.
    fn entry(
        &self,
        entity: Entity,
        kind: DrawnKind,
        order: i32,
        corners: &[[f32; 3]],
    ) -> DrawnEntity {
        let projected: Vec<[f32; 2]> = corners
            .iter()
            .filter_map(|corner| self.to_screen(*corner))
            .collect();

        // Every corner behind the camera. Reported as present but not visible rather than omitted:
        // the entity *is* in the world and an agent asking "why can I not see it" deserves the
        // entry, which is the same reason an off-screen entity is reported at all.
        if projected.is_empty() {
            return DrawnEntity {
                entity,
                kind,
                order,
                center: [0.0, 0.0],
                size: [0.0, 0.0],
                visible: false,
            };
        }

        let mut min = projected[0];
        let mut max = projected[0];
        for point in &projected[1..] {
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }

        let size = [max[0] - min[0], max[1] - min[1]];
        let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
        let visible = max[0] > 0.0
            && min[0] < self.viewport.0 as f32
            && max[1] > 0.0
            && min[1] < self.viewport.1 as f32;

        DrawnEntity {
            entity,
            kind,
            order,
            center,
            size,
            visible,
        }
    }

    /// One world point in screen pixels, origin top-left. `None` if it is behind the camera.
    fn to_screen(&self, world: [f32; 3]) -> Option<[f32; 2]> {
        let clip = self.view_projection.transform_point4(world);
        // Behind the eye under perspective, where w is the view-space depth. An orthographic
        // projection always gives w = 1, so this costs a comparison and never fires.
        if clip[3] <= 0.0 {
            return None;
        }
        let ndc = [clip[0] / clip[3], clip[1] / clip[3]];
        Some([
            (ndc[0] + 1.0) / 2.0 * self.viewport.0 as f32,
            // Flipped: world +Y is up, screen +Y is down.
            (1.0 - ndc[1]) / 2.0 * self.viewport.1 as f32,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NullBackend;

    fn world_with_view(width: u32, height: u32) -> World {
        let mut world = World::new();
        world.insert_service(Renderer::new(Box::new(NullBackend::new(width, height))));
        let eye = world.spawn();
        world.insert(eye, Transform::at(0.0, 0.0));
        world.insert(eye, Camera::orthographic(10.0));
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

    /// A world seen through a perspective camera five units back along +Z, looking at the origin.
    fn world_with_a_3d_view() -> World {
        let mut world = World::new();
        world.insert_service(Renderer::new(Box::new(NullBackend::new(800, 600))));

        let eye = world.spawn();
        world.insert(
            eye,
            Transform {
                translation: [0.0, 0.0, 5.0],
                ..Transform::default()
            },
        );
        world.insert(eye, Camera::perspective(60.0));

        let mut meshes = crate::MeshCache::new();
        meshes.insert("cube", crate::BoxMesh::default().tessellate());
        world.insert_service(meshes);
        world
    }

    fn add_mesh(world: &mut World, at: [f32; 3]) -> Entity {
        let entity = world.spawn();
        world.insert(
            entity,
            Transform {
                translation: at,
                ..Transform::default()
            },
        );
        world.insert(entity, crate::Mesh::new("cube", "paint"));
        entity
    }

    #[test]
    fn a_mesh_is_reported_through_a_perspective_camera() {
        // **Q26.** Before this, `describe_frame` filtered to orthographic cameras, so a 3D world got
        // an answer about a *default* camera nobody authored and a count of zero drawn entities.
        // Plausible and wrong, which is worse than an error — it sent session 13 looking for a
        // streaming bug that did not exist.
        let mut world = world_with_a_3d_view();
        let entity = add_mesh(&mut world, [0.0, 0.0, 0.0]);

        let description = describe_frame(&world);
        assert_eq!(
            description.eye,
            [0.0, 0.0, 5.0],
            "the camera's own position"
        );

        let drawn = description.find(entity).expect("a mesh is drawable");
        assert_eq!(
            drawn.kind,
            DrawnKind::Mesh {
                mesh: "cube".to_string(),
                material: "paint".to_string(),
            }
        );
        assert!(drawn.visible);
        // Directly ahead of the camera, so its screen rectangle straddles the middle.
        assert!((drawn.center[0] - 400.0).abs() < 0.01, "{:?}", drawn.center);
        assert!((drawn.center[1] - 300.0).abs() < 0.01, "{:?}", drawn.center);
        assert!(
            drawn.size[0] > 1.0 && drawn.size[1] > 1.0,
            "{:?}",
            drawn.size
        );
    }

    #[test]
    fn something_behind_the_camera_is_not_visible() {
        // The case an orthographic projection cannot express at all, and the one a perspective
        // divide gets catastrophically wrong if the sign of w is ignored: a point behind the eye
        // divides to a mirrored position that looks like a perfectly ordinary place on screen.
        let mut world = world_with_a_3d_view();
        // The camera is at z = 5 looking down -Z, so z = 20 is well behind it.
        let behind = add_mesh(&mut world, [0.0, 0.0, 20.0]);
        let ahead = add_mesh(&mut world, [0.0, 0.0, 0.0]);

        let description = describe_frame(&world);
        assert!(!description.find(behind).expect("still reported").visible);
        assert!(description.find(ahead).expect("drawable").visible);
        assert_eq!(description.visible_count(), 1);
    }

    #[test]
    fn something_outside_the_frustum_is_reported_as_not_visible() {
        // **What M2.5's exit gate 3 measures culling with.** The gate says frustum culling must be
        // shown to reduce draw calls "through `render.describe` rather than believed", which needs
        // this to have an opinion about a mesh that is off to one side.
        let mut world = world_with_a_3d_view();
        let ahead = add_mesh(&mut world, [0.0, 0.0, 0.0]);
        let far_left = add_mesh(&mut world, [-60.0, 0.0, 0.0]);

        let description = describe_frame(&world);
        assert!(description.find(ahead).expect("drawable").visible);
        assert!(!description.find(far_left).expect("drawable").visible);
        assert_eq!(description.off_screen_count(), 1);
    }

    #[test]
    fn a_nearer_mesh_covers_more_of_the_screen_than_a_further_one() {
        // That the perspective divide is actually happening. Under an orthographic projection both
        // would report the same size, which is exactly the wrong answer this used to give — and it
        // would look entirely reasonable in a report.
        let mut world = world_with_a_3d_view();
        let near = add_mesh(&mut world, [0.0, 0.0, 3.0]);
        let far = add_mesh(&mut world, [0.0, 0.0, -20.0]);

        let description = describe_frame(&world);
        let near_size = description.find(near).expect("drawable").size[1];
        let far_size = description.find(far).expect("drawable").size[1];

        assert!(
            near_size > far_size * 4.0,
            "near {near_size} should dwarf far {far_size}"
        );
    }

    #[test]
    fn a_mesh_whose_geometry_never_loaded_is_not_reported() {
        // Matching the collection pass exactly: the renderer draws nothing for a mesh it does not
        // have, so reporting it would describe a frame that will not happen. The same reasoning
        // `a_mesh_whose_geometry_never_loaded_is_skipped_rather_than_substituted` gives for the
        // frame itself.
        let mut world = world_with_a_3d_view();
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        world.insert(entity, crate::Mesh::new("no_such_mesh", ""));

        assert!(describe_frame(&world).find(entity).is_none());
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
