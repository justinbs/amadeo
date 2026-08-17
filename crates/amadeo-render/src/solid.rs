//! Parametric geometry — ADR 0074.
//!
//! # Why this module exists
//!
//! Session 20's engine review measured what five sessions of renderer work had been sitting on:
//! **23 of 23 `.mesh` assets in the repository were `BoxMesh`.** The renderer had cascaded shadows,
//! spot shadows, IBL, PBR, MSAA, bloom and fog; the content language had one noun. That, and not the
//! renderer, is why the games looked the way they did.
//!
//! ADR 0035 promised a mesh asset would carry *"either a procedural shape or vertex data"*, and the
//! vertex-data half was never built. ADR 0074 answers it differently and more usefully: **geometry is
//! described by parameters, in text, and the description is the source of truth.**
//!
//! # What that buys, and it is not mainly about shapes
//!
//! A model is now a scene document like everything else, so `amadeo fmt` formats it, `amadeo check`
//! validates it against the real schema, a snapshot captures it, prefab overrides reach it, and the
//! editor — when it exists — opens it. That is invariant I1 holding under geometry, which is the same
//! argument ADR 0071 makes for levels.
//!
//! And it is parametric, which is the difference between a format that can *express* a model and one
//! an author is productive in. A door is `width` and `height`, so a wider door is one number rather
//! than a new file.
//!
//! # What it will never do
//!
//! Organic shape. No faces, no creatures, no cloth, no folds. That is a real and permanent limit,
//! chosen with open eyes (ADR 0074's consequences), and it is why `docs/12-the-bar.md` states low
//! poly as a first-class art direction rather than as a fallback. Organic work arrives through the
//! glTF importer.
//!
//! # The one rule every producer in here follows
//!
//! **Triangles are wound to match their own normals.** `CLAUDE.md` requires the test for any new mesh
//! producer, and `amadeo-voxel` is why: it shipped every quad wound against its own normal for two
//! sessions, because its tests checked normals and normals are computed independently of winding.
//! ADR 0052 turned backface culling off, so a mistake here does not make a shape invisible — it makes
//! it lit from behind, which reads as a missing light rather than as a wrong sign.

use crate::mesh::{MeshData, Vertex};
use amadeo_core::{StableHash, sin_cos_degrees};
use amadeo_ecs::Component;
use amadeo_reflect::Reflect;

/// How many sides a curved primitive gets when nothing says otherwise.
///
/// Twelve reads as round at prop scale and costs nothing. It is a named constant because every
/// primitive here defaults to it, and a project changing its poly budget should change one number.
pub const DEFAULT_SIDES: u32 = 12;

/// The widest and narrowest a side count may be.
///
/// Three is the smallest thing that is still a solid; 128 is past the point where more sides change
/// the picture and well before they change the frame time.
const SIDE_LIMITS: (u32, u32) = (3, 128);

/// A round column, and the workhorse of the set.
///
/// Pipes, posts, drums, bollards, lamp housings, tunnel liners, tree trunks. With `top_radius` at
/// zero it is a cone; with the two radii different it is a frustum, which is what most "round" props
/// actually are.
///
/// Stands along **Y**, centred on its own origin, because that is what a column does and it means a
/// prop placed at floor level needs `height / 2` rather than a matrix.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct CylinderMesh {
    /// Radius at the bottom.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 0.5)]
    pub radius: f32,
    /// Radius at the top. **Zero makes a cone**, and anything else makes a frustum.
    ///
    /// **Defaults to 0.5 rather than to whatever `radius` is**, because the scene format has no way
    /// to say "the same as that other field". So a file that sets `radius` and omits this one gets a
    /// frustum — set both, or neither. The result is visibly a frustum rather than quietly wrong,
    /// which is why this is a documented interaction and not a required field.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 0.5)]
    pub top_radius: f32,
    /// Height along Y.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 1.0)]
    pub height: f32,
    /// How many flat sides the curve is built from.
    #[reflect(min = 3.0, max = 128.0, default = DEFAULT_SIDES)]
    pub sides: u32,
    /// Whether to close the ends.
    ///
    /// Open is not an oversight: a pipe seen from outside never shows its caps, and leaving them off
    /// halves the triangles. A closed cylinder is the default because an open one that should have
    /// been closed reads as a hole.
    #[reflect(default = true)]
    pub capped: bool,
    /// Shade each facet as a flat plane instead of blending across the curve.
    ///
    /// **This is the difference between a low-poly prop and a smooth one at the same triangle count.**
    /// Smooth normals blend across a facet edge, which is right for a drum meant to read as round and
    /// wrong for a six-sided post meant to read as six-sided: the silhouette stays hexagonal while the
    /// shading pretends it is a tube, and the result looks unfinished rather than stylised.
    /// `docs/12-the-bar.md` §3 makes low poly a first-class art direction, so this is not an
    /// afterthought.
    ///
    /// Defaults to **smooth**, which is what every existing asset already gets.
    #[reflect(default = false)]
    pub flat: bool,
}

impl Default for CylinderMesh {
    fn default() -> Self {
        Self {
            radius: 0.5,
            top_radius: 0.5,
            height: 1.0,
            sides: DEFAULT_SIDES,
            capped: true,
            flat: false,
        }
    }
}

impl Component for CylinderMesh {}

impl CylinderMesh {
    /// Turns the parameters into geometry.
    ///
    /// # Why the side normals are not the vertex positions
    ///
    /// On a straight cylinder the outward normal is the radial direction and the two are the same
    /// after normalising. On a **cone or frustum** they are not: the surface leans, so a normal that
    /// pointed straight out would light a cone as though it were a tube. The slope is folded in
    /// below, which is the one piece of arithmetic in this type worth reading twice.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        let sides = self.sides.clamp(SIDE_LIMITS.0, SIDE_LIMITS.1);
        let bottom = self.radius.max(0.0);
        let top = self.top_radius.max(0.0);
        let half = self.height.max(0.0001) / 2.0;

        // How far the surface leans, as a rise over the run. A tube is zero; a cone is steep. The
        // normal's vertical component is this, which is what makes a cone shade like a cone.
        let lean = (bottom - top) / self.height.max(0.0001);
        let scale = (1.0 + lean * lean).sqrt();

        let mut data = MeshData::default();
        // `step` is a float so a facet's **midpoint** can be sampled as well as its edges, which is
        // what a flat normal needs -- see below.
        let ring = |radius: f32, y: f32, step: f32| -> ([f32; 3], [f32; 3]) {
            let (sine, cosine) = sin_cos_degrees(360.0 * step / sides as f32);
            (
                [radius * sine, y, radius * cosine],
                [sine / scale, lean / scale, cosine / scale],
            )
        };

        // The side, as a quad strip. `sides + 1` positions so the seam has its own pair of vertices
        // with u = 0 and u = 1 -- sharing them would wrap the texture backwards across one facet.
        for step in 0..sides {
            let (a_low, a_edge_normal) = ring(bottom, -half, step as f32);
            let (a_high, _) = ring(top, half, step as f32);
            let (b_low, b_edge_normal) = ring(bottom, -half, (step + 1) as f32);
            let (b_high, _) = ring(top, half, (step + 1) as f32);
            let (u0, u1) = (step as f32 / sides as f32, (step + 1) as f32 / sides as f32);

            // **Where flat shading happens, and why it is one line rather than a second code path.**
            // The vertices are already unshared per facet -- they have to be, for the seam's UVs -- so
            // giving all four the *facet's* normal is the whole of it. The facet's normal is the ring
            // normal half a step along, which is exact rather than approximate, and is why this does
            // not use `MeshData::flat_shade`: that works per *triangle*, and on a cone the two
            // triangles of one facet are not coplanar, so it would put a crease down the middle of
            // every side.
            let (a_normal, b_normal) = if self.flat {
                let (_, facet) = ring(bottom, -half, step as f32 + 0.5);
                (facet, facet)
            } else {
                (a_edge_normal, b_edge_normal)
            };

            let first = data.vertices.len() as u32;
            for (position, normal, uv) in [
                (a_low, a_normal, [u0, 1.0]),
                (b_low, b_normal, [u1, 1.0]),
                (b_high, b_normal, [u1, 0.0]),
                (a_high, a_normal, [u0, 0.0]),
            ] {
                data.vertices.push(Vertex {
                    position,
                    normal,
                    uv,
                    ..Vertex::default()
                });
            }
            // A degenerate ring -- a cone's tip -- still emits its quad, which collapses to a
            // triangle with a zero-length edge. Harmless, and cheaper than a special case that would
            // have to be right at both ends.
            data.indices
                .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }

        if self.capped {
            cap(&mut data, bottom, -half, sides, false);
            cap(&mut data, top, half, sides, true);
        }

        data.generate_tangents();
        data
    }
}

/// One end of a round primitive, as a fan around a centre vertex.
///
/// `upward` decides both the normal and the winding, which is the whole of the difference between
/// the two ends and the easiest thing here to get backwards.
fn cap(data: &mut MeshData, radius: f32, y: f32, sides: u32, upward: bool) {
    if radius <= 0.0 {
        return;
    }
    let normal = [0.0, if upward { 1.0 } else { -1.0 }, 0.0];
    let centre = data.vertices.len() as u32;
    data.vertices.push(Vertex {
        position: [0.0, y, 0.0],
        normal,
        uv: [0.5, 0.5],
        ..Vertex::default()
    });

    for step in 0..=sides {
        let (sine, cosine) = sin_cos_degrees(360.0 * step as f32 / sides as f32);
        data.vertices.push(Vertex {
            position: [radius * sine, y, radius * cosine],
            normal,
            // The cap's own disc, so a texture reads as a circle rather than as a stretched strip.
            uv: [0.5 + 0.5 * sine, 0.5 + 0.5 * cosine],
            ..Vertex::default()
        });
    }

    for step in 0..sides {
        let a = centre + 1 + step;
        let b = centre + 2 + step;
        // Which way round is the whole of the difference between the two ends, and it is the
        // easiest thing in this module to get backwards -- it was, first time. The ring is walked
        // with `[r·sin, y, r·cos]`, so it runs *clockwise* seen from +Y; a face pointing up
        // therefore needs its corners in that order and a face pointing down needs them reversed.
        if upward {
            data.indices.extend([centre, a, b]);
        } else {
            data.indices.extend([centre, b, a]);
        }
    }
}

/// A ball, built by subdividing latitude and longitude.
///
/// Lamps, bulbs, boulders, fruit, planets at prop scale. **Segments** are the horizontal divisions
/// and **rings** the vertical ones; the poles are degenerate quads for the same reason a cone's tip
/// is, which keeps the code one loop instead of three cases.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct SphereMesh {
    /// Radius.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 0.5)]
    pub radius: f32,
    /// Divisions around the equator.
    #[reflect(min = 3.0, max = 128.0, default = DEFAULT_SIDES)]
    pub segments: u32,
    /// Divisions from pole to pole.
    #[reflect(min = 2.0, max = 128.0, default = DEFAULT_SIDES / 2)]
    pub rings: u32,
    /// Shade each facet as a flat plane instead of blending across the curve.
    ///
    /// See [`CylinderMesh::flat`]. It matters more on a sphere than anywhere else: a smooth 12×6 ball
    /// has a visibly polygonal *outline* and perfectly round *shading*, so it reads as a low-poly
    /// model somebody forgot to finish. Faceted, the same 144 triangles read as deliberate.
    ///
    /// Defaults to **smooth**, which is also the cheaper of the two here — see
    /// [`SphereMesh::tessellate`].
    #[reflect(default = false)]
    pub flat: bool,
}

impl Default for SphereMesh {
    fn default() -> Self {
        Self {
            radius: 0.5,
            segments: DEFAULT_SIDES,
            rings: DEFAULT_SIDES / 2,
            flat: false,
        }
    }
}

impl Component for SphereMesh {}

impl SphereMesh {
    /// Turns the parameters into geometry.
    ///
    /// # Smooth shares its vertices; flat cannot
    ///
    /// A smooth sphere's normal at a grid point is the same for all four quads meeting there, so one
    /// vertex serves all four and a 12×6 ball is 91 vertices. This used to emit four **unshared**
    /// vertices per quad regardless — 288 for the same 144 triangles, paying flat shading's cost
    /// without getting any faceting for it.
    ///
    /// Flat needs the unsharing, because a facet's corners carry that facet's normal and a shared
    /// corner cannot carry four different ones. So the two paths differ in exactly that, and the
    /// branch below is the whole difference.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        let segments = self.segments.clamp(SIDE_LIMITS.0, SIDE_LIMITS.1);
        let rings = self.rings.clamp(2, SIDE_LIMITS.1);
        let radius = self.radius.max(0.0001);

        // A point on the unit sphere from fractional ring and segment counts. Fractional so a facet's
        // **midpoint** can be sampled for its flat normal, exactly as the cylinder's `ring` does.
        let unit_at = |ring: f32, segment: f32| -> [f32; 3] {
            // 0 at the north pole, 180 at the south.
            let (sin_lat, cos_lat) = sin_cos_degrees(180.0 * ring / rings as f32);
            let (sin_lon, cos_lon) = sin_cos_degrees(360.0 * segment / segments as f32);
            [sin_lat * sin_lon, cos_lat, sin_lat * cos_lon]
        };

        // The whole grid first, then the quads over it. Sampling each vertex from its own angles
        // rather than accumulating means a rounding error cannot walk the sphere open at the seam.
        let mut grid: Vec<([f32; 3], [f32; 2])> = Vec::new();
        for ring in 0..=rings {
            for segment in 0..=segments {
                grid.push((
                    unit_at(ring as f32, segment as f32),
                    [segment as f32 / segments as f32, ring as f32 / rings as f32],
                ));
            }
        }

        let mut data = MeshData::default();
        let scaled = |unit: [f32; 3]| [unit[0] * radius, unit[1] * radius, unit[2] * radius];

        if !self.flat {
            // One vertex per grid point, shared by every quad that meets there.
            for (unit, uv) in &grid {
                data.vertices.push(Vertex {
                    // On a sphere centred at the origin the unit position *is* the normal, which is
                    // the one place in this module where that shortcut is honest.
                    position: scaled(*unit),
                    normal: *unit,
                    uv: *uv,
                    ..Vertex::default()
                });
            }
        }

        let stride = segments + 1;
        for ring in 0..rings {
            for segment in 0..segments {
                let corners = [
                    (ring * stride + segment) as usize,
                    (ring * stride + segment + 1) as usize,
                    ((ring + 1) * stride + segment + 1) as usize,
                    ((ring + 1) * stride + segment) as usize,
                ];

                let base = if self.flat {
                    // This facet's own normal, sampled at its centre. Exact rather than averaged, and
                    // the reason this does not use `MeshData::flat_shade`: that is per *triangle*, and
                    // a sphere's quad is not planar, so it would crease every facet down its diagonal.
                    let facet = unit_at(ring as f32 + 0.5, segment as f32 + 0.5);
                    let first = data.vertices.len() as u32;
                    for index in corners {
                        let (unit, uv) = grid[index];
                        data.vertices.push(Vertex {
                            position: scaled(unit),
                            normal: facet,
                            uv,
                            ..Vertex::default()
                        });
                    }
                    first
                } else {
                    0
                };

                // **Reversed from the usual order**, for the same reason the caps are: longitude is
                // walked as `[sin, ·, cos]`, which runs clockwise seen from +Y, while latitude runs
                // downward — so the corners as listed are clockwise from outside and the quad has to
                // be wound the other way to face out.
                //
                // Indices are into the shared grid when smooth, and into this facet's own four
                // vertices when flat — which is what `base` and `local` reconcile.
                for triangle in [[0_usize, 2, 1], [0, 3, 2]] {
                    let positions = triangle.map(|corner| scaled(grid[corners[corner]].0));
                    // Every quad touching a pole has one degenerate triangle, because both of its
                    // top (or bottom) corners *are* the pole. Emitting them costs indices and gives
                    // `every_primitive_is_wound_to_match_its_own_normals` something it has to skip.
                    if !has_area(positions[0], positions[1], positions[2]) {
                        continue;
                    }
                    data.indices.extend(triangle.map(|corner| {
                        if self.flat {
                            base + u32::try_from(corner).unwrap_or(0)
                        } else {
                            u32::try_from(corners[corner]).unwrap_or(0)
                        }
                    }));
                }
            }
        }

        data.generate_tangents();
        data
    }
}

/// A box with a sloped top: a ramp, a buttress, a wedge of debris, a desk return.
///
/// The slope runs along **Z**, rising from `height_back` at -Z to `height_front` at +Z, and the base
/// sits on **y = 0** rather than centred — because a wedge is a thing you put on a floor, and a
/// centred one needs a correction at every use.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct WedgeMesh {
    /// Width along X.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 1.0)]
    pub width: f32,
    /// Depth along Z.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 1.0)]
    pub depth: f32,
    /// Height at the +Z end.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 1.0)]
    pub height_front: f32,
    /// Height at the -Z end. Equal to the front makes an ordinary box.
    ///
    /// **Zero is a true ramp and is the default**, which means the -Z face and half of each side face
    /// collapse. `tessellate` drops the collapsed triangles rather than emitting zero-area ones, so
    /// the default wedge is a clean five-face solid rather than a six-face one with two dead faces.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 0.0)]
    pub height_back: f32,
}

impl Default for WedgeMesh {
    fn default() -> Self {
        Self {
            width: 1.0,
            depth: 1.0,
            height_front: 1.0,
            height_back: 0.0,
        }
    }
}

impl Component for WedgeMesh {}

impl WedgeMesh {
    /// Turns the parameters into geometry.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        let x = self.width.max(0.0001) / 2.0;
        let z = self.depth.max(0.0001) / 2.0;
        let (front, back) = (self.height_front.max(0.0), self.height_back.max(0.0));

        // The sloped face's normal, from the rise over the run. A flat top gives (0, 1, 0), which is
        // what makes `height_front == height_back` an ordinary box rather than a special case.
        let rise = back - front;
        let run = self.depth.max(0.0001);
        let length = (rise * rise + run * run).sqrt();
        let slope = [0.0, run / length, rise / length];

        let mut data = MeshData::default();
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            // The slope, from the low edge up to the high one.
            (
                slope,
                [[-x, front, z], [x, front, z], [x, back, -z], [-x, back, -z]],
            ),
            // Bottom.
            (
                [0.0, -1.0, 0.0],
                [[-x, 0.0, -z], [x, 0.0, -z], [x, 0.0, z], [-x, 0.0, z]],
            ),
            // Front, +Z.
            (
                [0.0, 0.0, 1.0],
                [[-x, 0.0, z], [x, 0.0, z], [x, front, z], [-x, front, z]],
            ),
            // Back, -Z.
            (
                [0.0, 0.0, -1.0],
                [[x, 0.0, -z], [-x, 0.0, -z], [-x, back, -z], [x, back, -z]],
            ),
            // Right, +X.
            (
                [1.0, 0.0, 0.0],
                [[x, 0.0, z], [x, 0.0, -z], [x, back, -z], [x, front, z]],
            ),
            // Left, -X.
            (
                [-1.0, 0.0, 0.0],
                [[-x, 0.0, -z], [-x, 0.0, z], [-x, front, z], [-x, back, -z]],
            ),
        ];

        for (normal, corners) in faces {
            // **A ramp has no back face, and this is where that is handled.** With `height_back` at
            // zero — the commonest wedge there is — the -Z quad collapses onto a line and the two
            // side quads collapse to triangles. Emitting them regardless cost four vertices and two
            // zero-area triangles per collapsed face.
            //
            // Dropping them matters for more than the vertex count: a zero-area triangle has no
            // meaningful winding and no meaningful normal, so it is the one thing
            // `every_primitive_is_wound_to_match_its_own_normals` has to skip. The fewer of them
            // exist, the more of the mesh that test actually checks.
            let live: Vec<[usize; 3]> = [[0_usize, 1, 2], [0, 2, 3]]
                .into_iter()
                .filter(|triangle| {
                    has_area(
                        corners[triangle[0]],
                        corners[triangle[1]],
                        corners[triangle[2]],
                    )
                })
                .collect();
            if live.is_empty() {
                continue;
            }

            let first = data.vertices.len() as u32;
            for (corner, uv) in corners
                .iter()
                .zip([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
            {
                data.vertices.push(Vertex {
                    position: *corner,
                    normal,
                    uv,
                    ..Vertex::default()
                });
            }
            for triangle in live {
                data.indices.extend(
                    triangle
                        .iter()
                        .map(|corner| first + u32::try_from(*corner).unwrap_or(0)),
                );
            }
        }

        data.generate_tangents();
        data
    }
}

/// Whether three points span a real triangle rather than a line or a point.
///
/// Twice the area is the length of the cross product of two edges, so this compares that against a
/// small epsilon instead of taking a square root. The threshold is generous on purpose: a face that
/// is *nearly* collapsed contributes nothing visible and has an unstable normal, which is worse than
/// a face that is missing.
fn has_area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> bool {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2] > 1e-12
}

/// A flight of steps, as one mesh rather than as a stack of boxes.
///
/// Steps are the commonest architectural thing an engine cannot say, and a stair built from N prefab
/// children is N entities, N transforms and N draw calls for something that is one object. This is
/// one mesh, one asset, and three numbers.
///
/// Climbs along **+Z** and rises along **+Y**, with the bottom step's front face at `z = 0` — so a
/// stair placed at the foot of a slope needs no correction.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct StairMesh {
    /// Width across, along X.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units", default = 1.2)]
    pub width: f32,
    /// How many steps.
    #[reflect(min = 1.0, max = 512.0, default = 8)]
    pub steps: u32,
    /// The height of one step.
    ///
    /// The default is a shallow 0.18 m against a 0.28 m run, which is roughly a real building's
    /// stair rather than the 1:1 blocks an engine demo usually has.
    #[reflect(min = 0.0, max = 100.0, unit = "world units", default = 0.18)]
    pub rise: f32,
    /// The depth of one step.
    #[reflect(min = 0.0, max = 100.0, unit = "world units", default = 0.28)]
    pub run: f32,
}

impl Default for StairMesh {
    fn default() -> Self {
        Self {
            width: 1.2,
            steps: 8,
            rise: 0.18,
            run: 0.28,
        }
    }
}

impl Component for StairMesh {}

impl StairMesh {
    /// The total height of the flight, which is what a level designer actually places against.
    #[must_use]
    pub fn total_rise(&self) -> f32 {
        self.rise * self.steps as f32
    }

    /// The total depth of the flight.
    #[must_use]
    pub fn total_run(&self) -> f32 {
        self.run * self.steps as f32
    }

    /// Turns the parameters into geometry.
    ///
    /// Each step is a solid box from the ground up, rather than a tread and a riser. That is more
    /// triangles than the minimum and it is what makes the sides read as a solid flight instead of a
    /// floating staircase — and a stair you can see under is the classic tell of one built the cheap
    /// way.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        let steps = self.steps.clamp(1, 512);
        let mut data = MeshData::default();

        for step in 0..steps {
            let height = self.rise * (step + 1) as f32;
            let near = self.run * step as f32;
            let far = self.run * (step + 1) as f32;
            let block = crate::mesh::BoxMesh {
                size: [self.width.max(0.0001), height, self.run.max(0.0001)],
            }
            .tessellate();
            // Placed so the box's base sits on y = 0 and its front face on this step's near edge.
            append_translated(&mut data, &block, [0.0, height / 2.0, (near + far) / 2.0]);
        }

        data
    }
}

/// Copies one mesh into another, moved.
///
/// The whole of what a compound needs, and it is deliberately not a general transform: a rotation
/// would have to rotate the normals and the tangents too, and getting *that* wrong is a shape that
/// shades correctly until a light moves. Rotation belongs on the part, where the scene format
/// already has a `Transform` that is tested.
fn append_translated(into: &mut MeshData, source: &MeshData, offset: [f32; 3]) {
    let first = into.vertices.len() as u32;
    into.vertices
        .extend(source.vertices.iter().map(|vertex| Vertex {
            position: [
                vertex.position[0] + offset[0],
                vertex.position[1] + offset[1],
                vertex.position[2] + offset[2],
            ],
            ..*vertex
        }));
    into.indices
        .extend(source.indices.iter().map(|index| index + first));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle, as (corners, averaged vertex normal).
    fn triangles(data: &MeshData) -> Vec<([[f32; 3]; 3], [f32; 3])> {
        data.indices
            .chunks_exact(3)
            .map(|face| {
                let v: Vec<&Vertex> = face.iter().map(|i| &data.vertices[*i as usize]).collect();
                let average = [
                    (v[0].normal[0] + v[1].normal[0] + v[2].normal[0]) / 3.0,
                    (v[0].normal[1] + v[1].normal[1] + v[2].normal[1]) / 3.0,
                    (v[0].normal[2] + v[1].normal[2] + v[2].normal[2]) / 3.0,
                ];
                ([v[0].position, v[1].position, v[2].position], average)
            })
            .collect()
    }

    /// Whether every triangle faces the same way its own normals do.
    ///
    /// **The test `CLAUDE.md` requires of every new mesh producer**, applied to all of them at once.
    /// Degenerate triangles — a cone's tip, a sphere's poles — are skipped rather than failed: they
    /// have no facing to be wrong about, and demanding one would fail a correct mesh.
    fn wound_to_match_normals(data: &MeshData) -> Result<(), String> {
        for (corners, normal) in triangles(data) {
            let a = [
                corners[1][0] - corners[0][0],
                corners[1][1] - corners[0][1],
                corners[1][2] - corners[0][2],
            ];
            let b = [
                corners[2][0] - corners[0][0],
                corners[2][1] - corners[0][1],
                corners[2][2] - corners[0][2],
            ];
            let cross = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
            if area < 1e-9 {
                continue;
            }
            let agreement = cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2];
            if agreement <= 0.0 {
                return Err(format!(
                    "a triangle at {corners:?} is wound against its own normal {normal:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn every_primitive_is_wound_to_match_its_own_normals() {
        // ADR 0052 turned backface culling off, so a mistake here does not make a shape invisible —
        // it lights every surface from behind, which reads as a missing light rather than a wrong
        // sign. `amadeo-voxel` shipped exactly that for two sessions.
        let cases: Vec<(&str, MeshData)> = vec![
            ("cylinder", CylinderMesh::default().tessellate()),
            (
                "cone",
                CylinderMesh {
                    top_radius: 0.0,
                    ..CylinderMesh::default()
                }
                .tessellate(),
            ),
            (
                "frustum",
                CylinderMesh {
                    top_radius: 0.2,
                    ..CylinderMesh::default()
                }
                .tessellate(),
            ),
            (
                "open cylinder",
                CylinderMesh {
                    capped: false,
                    ..CylinderMesh::default()
                }
                .tessellate(),
            ),
            ("sphere", SphereMesh::default().tessellate()),
            // The faceted variants, because `flat` changes the *normals* and this test compares
            // winding against them — so a flat shape is a genuinely new case here rather than the
            // same geometry twice. The cone is the one that matters: its facet normal is sampled at a
            // different angle from its corners, which is exactly where a sign could go wrong.
            (
                "faceted cylinder",
                CylinderMesh {
                    flat: true,
                    ..CylinderMesh::default()
                }
                .tessellate(),
            ),
            (
                "faceted cone",
                CylinderMesh {
                    top_radius: 0.0,
                    flat: true,
                    ..CylinderMesh::default()
                }
                .tessellate(),
            ),
            (
                "faceted sphere",
                SphereMesh {
                    flat: true,
                    ..SphereMesh::default()
                }
                .tessellate(),
            ),
            ("wedge", WedgeMesh::default().tessellate()),
            (
                "flat wedge",
                WedgeMesh {
                    height_back: 1.0,
                    ..WedgeMesh::default()
                }
                .tessellate(),
            ),
            ("stair", StairMesh::default().tessellate()),
        ];

        for (name, data) in cases {
            if let Err(problem) = wound_to_match_normals(&data) {
                panic!("{name}: {problem}");
            }
        }
    }

    #[test]
    fn a_cylinder_reaches_its_authored_size() {
        let solid = CylinderMesh {
            radius: 2.0,
            top_radius: 2.0,
            height: 5.0,
            sides: 32,
            capped: true,
            flat: false,
        };
        let data = solid.tessellate();
        let widest = data.vertices.iter().fold(0.0f32, |wide, v| {
            wide.max(v.position[0].hypot(v.position[2]))
        });
        let tallest = data
            .vertices
            .iter()
            .fold(0.0f32, |high, v| high.max(v.position[1]));
        assert!((widest - 2.0).abs() < 0.001, "radius came out {widest}");
        assert!(
            (tallest - 2.5).abs() < 0.001,
            "half height came out {tallest}"
        );
    }

    #[test]
    fn a_cone_shades_like_a_cone_rather_than_a_tube() {
        // **The one piece of arithmetic in this module worth a test of its own.** On a straight
        // cylinder the outward normal is the radial direction; on a cone the surface leans, so a
        // normal that pointed straight out would light it as a tube. The side normals must therefore
        // have a positive Y component — they tilt upward towards the tip.
        let data = CylinderMesh {
            radius: 1.0,
            top_radius: 0.0,
            height: 2.0,
            sides: 16,
            capped: false,
            flat: false,
        }
        .tessellate();

        for vertex in &data.vertices {
            assert!(
                vertex.normal[1] > 0.1,
                "a cone's side normal {:?} is flat, so it will shade like a tube",
                vertex.normal
            );
            let length = (vertex.normal[0] * vertex.normal[0]
                + vertex.normal[1] * vertex.normal[1]
                + vertex.normal[2] * vertex.normal[2])
                .sqrt();
            assert!((length - 1.0).abs() < 0.001, "normal is not unit: {length}");
        }
    }

    #[test]
    fn a_spheres_normals_are_its_directions() {
        let data = SphereMesh {
            radius: 3.0,
            segments: 24,
            rings: 12,
            flat: false,
        }
        .tessellate();
        for vertex in &data.vertices {
            let distance = (vertex.position[0] * vertex.position[0]
                + vertex.position[1] * vertex.position[1]
                + vertex.position[2] * vertex.position[2])
                .sqrt();
            assert!((distance - 3.0).abs() < 0.001, "off the sphere: {distance}");
            for axis in 0..3 {
                assert!(
                    (vertex.normal[axis] - vertex.position[axis] / 3.0).abs() < 0.001,
                    "normal {:?} does not point away from the middle",
                    vertex.normal
                );
            }
        }
    }

    #[test]
    fn a_flat_wedge_is_an_ordinary_box() {
        // The degenerate case, and the one that says `height_front == height_back` needs no special
        // handling: the slope's normal falls out as straight up on its own.
        let data = WedgeMesh {
            width: 2.0,
            depth: 2.0,
            height_front: 1.0,
            height_back: 1.0,
        }
        .tessellate();
        let top = data
            .vertices
            .iter()
            .filter(|v| v.position[1] > 0.9)
            .filter(|v| v.normal[1] > 0.9)
            .count();
        assert!(top >= 4, "a flat wedge's top should face straight up");
    }

    #[test]
    fn a_stair_climbs_to_its_own_arithmetic() {
        let stair = StairMesh {
            width: 1.5,
            steps: 10,
            rise: 0.2,
            run: 0.3,
        };
        assert!((stair.total_rise() - 2.0).abs() < 0.0001);
        assert!((stair.total_run() - 3.0).abs() < 0.0001);

        let data = stair.tessellate();
        let tallest = data
            .vertices
            .iter()
            .fold(0.0f32, |high, v| high.max(v.position[1]));
        let deepest = data
            .vertices
            .iter()
            .fold(0.0f32, |deep, v| deep.max(v.position[2]));
        let lowest = data
            .vertices
            .iter()
            .fold(f32::MAX, |low, v| low.min(v.position[1]));

        assert!((tallest - 2.0).abs() < 0.001, "top step at {tallest}");
        assert!((deepest - 3.0).abs() < 0.001, "last step ends at {deepest}");
        // **On the floor, not centred.** A stair is placed at the foot of a slope, and one that
        // needed `total_rise / 2` subtracting at every use would be wrong half the time.
        assert!(lowest.abs() < 0.001, "the flight starts at {lowest}");
    }

    #[test]
    fn a_stair_is_solid_underneath() {
        // Each step is a box from the ground up rather than a tread and a riser. A stair you can see
        // under is the classic tell of one built the cheap way, and it costs a few triangles to
        // avoid.
        let data = StairMesh::default().tessellate();
        let on_the_floor = data
            .vertices
            .iter()
            .filter(|v| v.position[1] < 0.001)
            .count();
        assert!(
            on_the_floor >= 8,
            "only {on_the_floor} vertices reach the floor, so the flight is hollow"
        );
    }

    #[test]
    fn every_shapes_declared_defaults_agree_with_its_default_impl() {
        // ADR 0075's named hazard, once per type in this file: each default is written twice, in a
        // `#[reflect(default = ...)]` attribute and in `impl Default`, and nothing but this stops the
        // two drifting. It matters more here than for `Material`, because `describe --example` now
        // *reports* the declared values — so a drift would publish authoring advice that does not
        // match what the engine builds.
        use amadeo_reflect::{Reflect, Value};
        let nothing = Value::Struct(std::collections::BTreeMap::new());

        assert_eq!(
            CylinderMesh::from_value(&nothing).expect("all fields declare defaults"),
            CylinderMesh::default()
        );
        assert_eq!(
            SphereMesh::from_value(&nothing).expect("all fields declare defaults"),
            SphereMesh::default()
        );
        assert_eq!(
            WedgeMesh::from_value(&nothing).expect("all fields declare defaults"),
            WedgeMesh::default()
        );
        assert_eq!(
            StairMesh::from_value(&nothing).expect("all fields declare defaults"),
            StairMesh::default()
        );
    }

    #[test]
    fn a_default_shape_is_worth_drawing() {
        // What the defaults are actually *for*. `describe <Shape> --example` reports them, so an agent
        // authoring its first cylinder gets these numbers — and before they existed it got the range
        // minimums instead: `radius 0.0`, `height 0.0`, `sides 3`. That is a legal instance of a
        // cylinder that draws nothing, offered as advice on how to write one.
        for (name, data) in [
            ("CylinderMesh", CylinderMesh::default().tessellate()),
            ("SphereMesh", SphereMesh::default().tessellate()),
            ("WedgeMesh", WedgeMesh::default().tessellate()),
            ("StairMesh", StairMesh::default().tessellate()),
        ] {
            assert!(
                !data.indices.is_empty(),
                "a default `{name}` tessellates to nothing"
            );
            let widest = data
                .vertices
                .iter()
                .flat_map(|v| v.position)
                .fold(0.0_f32, |a, b| a.max(b.abs()));
            assert!(
                widest > 0.1,
                "a default `{name}` is {widest} across, which is not a shape anybody can see"
            );
        }
    }

    #[test]
    fn faceting_gives_each_facet_one_normal_and_smoothing_does_not() {
        // The whole of what `flat` is for, stated as the thing you could see on a screen: on a faceted
        // shape every vertex of one facet shares a normal and the next facet's differs, so the edge
        // between them catches light. On a smooth one the normals vary continuously, so it does not.
        //
        // Counting *distinct* normals is what separates the two without depending on any particular
        // facet's direction. A 12-sided cylinder has 12 side facets, so faceted it has 12 side
        // normals; smooth it has one per ring position, which is 13 — the seam is doubled so its UVs
        // can run 0 to 1. Those numbers being close is fine: the test is that the flat one is
        // *constant within a facet*, which is checked directly below.
        let faceted = CylinderMesh {
            capped: false,
            flat: true,
            ..CylinderMesh::default()
        }
        .tessellate();

        // Four vertices per facet, in facet order, so a chunk is exactly one facet.
        for (index, facet) in faceted.vertices.chunks_exact(4).enumerate() {
            let first = facet[0].normal;
            for vertex in facet {
                assert_eq!(
                    vertex.normal, first,
                    "facet {index} of a faceted cylinder has more than one normal, so it will \
                     still shade as a curve"
                );
            }
        }

        let smooth = CylinderMesh {
            capped: false,
            ..CylinderMesh::default()
        }
        .tessellate();
        let varies_within_a_facet = smooth
            .vertices
            .chunks_exact(4)
            .any(|facet| facet.iter().any(|v| v.normal != facet[0].normal));
        assert!(
            varies_within_a_facet,
            "a smooth cylinder's facets each have one normal, which is flat shading by accident"
        );
    }

    #[test]
    fn a_smooth_sphere_shares_its_vertices_and_a_faceted_one_cannot() {
        // A smooth sphere's normal at a grid point is the same for all four quads meeting there, so
        // one vertex serves all four. This used to emit four unshared vertices per quad regardless —
        // paying flat shading's vertex cost without getting any faceting for it, which is three times
        // the vertices for the same picture.
        let smooth = SphereMesh::default().tessellate();
        let faceted = SphereMesh {
            flat: true,
            ..SphereMesh::default()
        }
        .tessellate();

        // 12 segments and 6 rings: a shared grid is 13 × 7 = 91.
        assert_eq!(smooth.vertices.len(), 91);
        // Faceted has to unshare, because a corner cannot carry four facets' normals at once.
        assert!(
            faceted.vertices.len() > smooth.vertices.len() * 2,
            "faceted came out at {} vertices, which is too few to be unshared",
            faceted.vertices.len()
        );

        // Both describe the same surface, and the pole triangles are dropped from both.
        assert_eq!(smooth.indices.len(), faceted.indices.len());
        assert!(
            smooth.indices.iter().all(|i| (*i as usize) < 91),
            "an index points outside the shared grid"
        );
    }

    #[test]
    fn a_ramp_has_no_collapsed_faces() {
        // `WedgeMesh::default` is a true ramp: `height_back` is zero, so the -Z quad collapses onto a
        // line and half of each side quad collapses too. Those used to be emitted as zero-area
        // triangles.
        //
        // They are worth dropping for a reason beyond the vertex count: a zero-area triangle has no
        // meaningful winding, so it is the one thing `every_primitive_is_wound_to_match_its_own_
        // normals` has to skip — and the fewer that exist, the more of the mesh that test checks.
        let data = WedgeMesh::default().tessellate();

        let collapsed = triangles(&data)
            .iter()
            .filter(|(corners, _)| !has_area(corners[0], corners[1], corners[2]))
            .count();
        assert_eq!(
            collapsed, 0,
            "a default ramp still emits {collapsed} zero-area triangles"
        );

        // And it is still a closed-looking solid: five faces rather than six, which is what a ramp is.
        assert!(
            data.indices.len() / 3 >= 8,
            "a ramp should still have five faces' worth of triangles, got {}",
            data.indices.len() / 3
        );
    }
}
