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

use crate::mesh::{ArchMesh, BoxMesh, MeshData, PlaneMesh, Vertex};
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
            // UVs in metres (ADR 0078 §3): `u` is arc length round the barrel and `v` is height. The
            // side of a cylinder is developable -- you can unroll it flat -- so arc length is a real
            // distance rather than an analogy, which is why this producer can join the flat ones.
            // The mean radius is used, so a frustum gets one consistent circumference rather than a
            // texture that slides as the radius changes.
            let circumference = std::f32::consts::TAU * (bottom + top) * 0.5;
            let (u0, u1) = (
                circumference * step as f32 / sides as f32,
                circumference * (step + 1) as f32 / sides as f32,
            );

            // **Where flat shading happens, and why it is one line rather than a second code path.**
            // The vertices are already unshared per facet -- they have to be, for the seam's UVs -- so
            // giving all four the *facet's* normal is the whole of it. The facet's normal is the ring
            // normal half a step along, which is exact rather than averaged.
            //
            // This does not use `MeshData::flat_shade`, and the reason is **not** that a cone's facet
            // is non-planar: its two chords are parallel, so a lateral facet is a planar trapezoid and
            // `flat_shade` would give both its triangles the same normal. (An earlier version of this
            // comment claimed otherwise and was wrong.) The two real reasons are at the **tip**, where
            // the facet collapses to a triangle so `flat_shade`'s cross product is zero-length and it
            // falls back to whatever normal the vertex already had -- and on a *sphere*, where a quad
            // genuinely is not planar, so per-triangle shading creases every facet down its diagonal.
            let (a_normal, b_normal) = if self.flat {
                let (_, facet) = ring(bottom, -half, step as f32 + 0.5);
                (facet, facet)
            } else {
                (a_edge_normal, b_edge_normal)
            };

            let first = data.vertices.len() as u32;
            for (position, normal, uv) in [
                (a_low, a_normal, [u0, self.height.max(0.0001)]),
                (b_low, b_normal, [u1, self.height.max(0.0001)]),
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
        // The cap disc in metres, centred on the axis (ADR 0078 §3).
        uv: [0.0, 0.0],
        ..Vertex::default()
    });

    for step in 0..=sides {
        let (sine, cosine) = sin_cos_degrees(360.0 * step as f32 / sides as f32);
        data.vertices.push(Vertex {
            position: [radius * sine, y, radius * cosine],
            normal,
            // The cap's own disc, so a texture reads as a circle rather than as a stretched strip --
            // now in metres from the axis, so a wide drum's lid and a narrow one's carry the same
            // stone at the same size.
            uv: [radius * sine, radius * cosine],
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

            // UVs in metres, like every other flat producer (ADR 0078 §3). The face's own two
            // in-plane dimensions, taken from its normal — and the **slope** measured along its
            // actual incline rather than its footprint, or a steep ramp's texture would be
            // compressed exactly in the direction the eye is most likely to notice.
            let span: [f32; 2] = if normal[0] != 0.0 {
                [self.depth, self.height_front.max(self.height_back)]
            } else if normal[1] < 0.0 {
                [self.width, self.depth]
            } else if normal[2] != 0.0 {
                [self.width, self.height_front.max(self.height_back)]
            } else {
                let rise = self.height_front - self.height_back;
                [self.width, (self.depth * self.depth + rise * rise).sqrt()]
            };

            let first = data.vertices.len() as u32;
            for (corner, uv) in corners
                .iter()
                .zip([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
            {
                data.vertices.push(Vertex {
                    position: *corner,
                    normal,
                    uv: [uv[0] * span[0], uv[1] * span[1]],
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

/// Below this length of cross product, a triangle is treated as collapsed.
///
/// The cross product of two edges has the length of **twice the triangle's area**, in square world
/// units. `1e-9` therefore rejects a triangle smaller than about half a square micrometre, which no
/// authored geometry has and which only an exactly-coincident pair of corners produces here — a cone's
/// tip, a sphere's pole, a ramp's absent back face.
///
/// **One constant, used by `has_area` and by the winding test**, because they are asking the same
/// question and had drifted three orders of magnitude apart: this rejected below `1e-6` while the test
/// skipped below `1e-9`. At millimetre scale that gap puts a legitimate facet inside the reject band,
/// so the looser of the two was the wrong one to keep.
const DEGENERATE_CROSS: f32 = 1e-9;

/// Whether three points span a real triangle rather than a line or a point.
///
/// Compares the *squared* length against [`DEGENERATE_CROSS`] squared, so no square root is taken.
fn has_area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> bool {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let squared = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
    squared > DEGENERATE_CROSS * DEGENERATE_CROSS
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
            append_transformed(
                &mut data,
                &block,
                [0.0, height / 2.0, (near + far) / 2.0],
                [0.0, 0.0, 0.0],
            );
        }

        data
    }
}

/// Copies one mesh into another, placed.
///
/// # This replaced a translate-only helper, and the reason is worth keeping
///
/// It used to be `append_translated`, whose doc comment said rotation was refused *deliberately*
/// because "a rotation would have to rotate the normals and the tangents too, and getting that wrong
/// is a shape that shades correctly until a light moves". That warning was right, and
/// [`CompoundMesh`] needs the thing it refused, so the answer is to do it once and test it rather
/// than to keep a second path.
///
/// Keeping both was considered and rejected: the general path has to exist and be correct anyway, so
/// a translate-only twin would be a second thing to keep in sync for a saving measured in load-time
/// microseconds (tessellation happens once, in `App::load_meshes`, per ADR 0026). Godot's
/// `SurfaceTool::append_from`, Unity's `Mesh.CombineMeshes` and Blender's join all take a full
/// transform and always apply it.
///
/// **The cheap path still exists — it is the branch below, not a second function.** A part with no
/// rotation takes a plain copy, so the safety `append_translated` had by construction is still here
/// and is now inside the thing that has tests.
///
/// # No scale, and that is not an omission
///
/// A part is a **parametric primitive**: it already carries its own dimensions, so a wider leg is
/// `radius 0.08` rather than a scale factor. That removes the whole hazard class scaling brings —
/// non-uniform scale needs the **inverse transpose** to transform a normal, which is a subtler rule
/// than rotation and would be a second silent way to get shading wrong.
fn append_transformed(
    into: &mut MeshData,
    source: &MeshData,
    position: [f32; 3],
    rotation: [f32; 3],
) {
    let first = into.vertices.len() as u32;

    // Exactly zero, not near-zero: this is an authored value, and a part that says `rotation 0 0 0`
    // means it. Anything else goes through the general path, which is correct for zero as well —
    // this branch is a saving, never a difference.
    if rotation == [0.0, 0.0, 0.0] {
        into.vertices
            .extend(source.vertices.iter().map(|vertex| Vertex {
                position: [
                    vertex.position[0] + position[0],
                    vertex.position[1] + position[1],
                    vertex.position[2] + position[2],
                ],
                ..*vertex
            }));
    } else {
        // ADR 0053's trigonometry, so a compound tessellates identically on every machine.
        let turn = amadeo_transform::Mat4::from_euler_degrees(rotation);
        // A pure rotation has no translation column, so transforming a *direction* through it is the
        // same call as transforming a point — the fourth column contributes nothing. That is why
        // normals and tangents can use `transform_point4` here and could not if this composed the
        // translation into the matrix.
        let turned = |vector: [f32; 3]| -> [f32; 3] {
            let out = turn.transform_point4(vector);
            [out[0], out[1], out[2]]
        };

        into.vertices.extend(source.vertices.iter().map(|vertex| {
            let placed = turned(vertex.position);
            let tangent = turned([vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]]);
            Vertex {
                position: [
                    placed[0] + position[0],
                    placed[1] + position[1],
                    placed[2] + position[2],
                ],
                normal: turned(vertex.normal),
                // **The handedness sign is not a direction and does not turn.** A rotation
                // preserves orientation, so the bitangent the shader recovers as
                // `cross(normal, tangent.xyz) * w` stays on the same side.
                tangent: [tangent[0], tangent[1], tangent[2], vertex.tangent[3]],
                ..*vertex
            }
        }));
    }

    into.indices
        .extend(source.indices.iter().map(|index| index + first));
}

/// Which primitive a [`Part`] is — ADR 0074 §2.
///
/// # Why every variant wraps its shape in a field called `shape`
///
/// `#[derive(Reflect)]` supports named-field variants and refuses tuple ones, because positional
/// fields have no names to put in a scene file. So `Cylinder(CylinderMesh)` is not available and
/// `Cylinder { shape: CylinderMesh }` is. The cost is one indent in the file; the alternative was
/// inlining each primitive's fields into a variant, which would define every shape **twice** and put
/// the two copies a hundred lines apart.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub enum Solid {
    /// A box.
    Box {
        /// The primitive's own parameters.
        shape: BoxMesh,
    },
    /// A flat plane.
    Plane {
        /// The primitive's own parameters.
        shape: PlaneMesh,
    },
    /// A vaulted section.
    Arch {
        /// The primitive's own parameters.
        shape: ArchMesh,
    },
    /// A column, cone or frustum.
    Cylinder {
        /// The primitive's own parameters.
        shape: CylinderMesh,
    },
    /// A ball.
    Sphere {
        /// The primitive's own parameters.
        shape: SphereMesh,
    },
    /// A ramp.
    Wedge {
        /// The primitive's own parameters.
        shape: WedgeMesh,
    },
    /// A flight of steps.
    Stair {
        /// The primitive's own parameters.
        shape: StairMesh,
    },
}

impl Default for Solid {
    /// A unit box, matching what `Solid::Box` with nothing said would be.
    fn default() -> Self {
        Self::Box {
            shape: BoxMesh::default(),
        }
    }
}

impl Solid {
    /// Turns whichever primitive this is into geometry.
    ///
    /// The whole of the dispatch, and the reason `Solid` wraps the primitives rather than restating
    /// them: adding a shape to the engine is one variant and one arm.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        match self {
            Solid::Box { shape } => shape.tessellate(),
            Solid::Plane { shape } => shape.tessellate(),
            Solid::Arch { shape } => shape.tessellate(),
            Solid::Cylinder { shape } => shape.tessellate(),
            Solid::Sphere { shape } => shape.tessellate(),
            Solid::Wedge { shape } => shape.tessellate(),
            Solid::Stair { shape } => shape.tessellate(),
        }
    }
}

/// One axis of repetition for a [`Part`] — ADR 0074 §3's `array`.
///
/// A run of racking is one part and one of these. Two of them on the same part is a **grid**, which
/// is why this is a list rather than a pair of "count and second count" fields: N axes fall out of
/// the list length instead of needing a special case for two.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Repeat {
    /// How many copies in total, including the original. `1` is no repetition.
    #[reflect(min = 1.0, max = 4096.0, default = 1)]
    pub count: u32,
    /// How far each copy moves from the one before it, in the compound's own space.
    #[reflect(unit = "world units", default = [0.0, 0.0, 0.0])]
    pub step: [f32; 3],
}

impl Default for Repeat {
    fn default() -> Self {
        Self {
            count: 1,
            step: [0.0, 0.0, 0.0],
        }
    }
}

/// One primitive placed inside a [`CompoundMesh`].
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Part {
    /// Which primitive, and its parameters.
    ///
    /// **Required**, with no default: a part with no shape is not a part. ADR 0076's line — a field
    /// defaults when it is authored data somebody might not care about, and stays required when its
    /// absence is a mistake.
    pub solid: Solid,
    /// Where it sits, in the compound's own space.
    #[reflect(unit = "world units", default = [0.0, 0.0, 0.0])]
    pub position: [f32; 3],
    /// How it is turned, as Euler degrees in ADR 0018's order.
    ///
    /// **No scale, deliberately**, and `append_transformed` says why. A part is parametric and carries its
    /// own dimensions, so a wider leg is a larger `radius` rather than a scale factor, which keeps
    /// the inverse-transpose normal rule out of this format entirely.
    #[reflect(unit = "degrees", default = [0.0, 0.0, 0.0])]
    pub rotation: [f32; 3],
    /// Repetition, one entry per axis. Empty is a single copy.
    #[reflect(default = Vec::new())]
    pub repeat: Vec<Repeat>,
    /// Mirror this part across the YZ, XZ and XY planes — ADR 0074 §3's `mirror`.
    ///
    /// A symmetrical fitting is one half and a mirror. Each flag **adds** the reflected copy rather
    /// than replacing the original, so one flag makes a pair.
    #[reflect(default = [false, false, false])]
    pub mirror: [bool; 3],
}

impl Default for Part {
    fn default() -> Self {
        Self {
            solid: Solid::default(),
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            repeat: Vec::new(),
            mirror: [false, false, false],
        }
    }
}

/// Several primitives assembled into one mesh — ADR 0074 §2.
///
/// # What this buys that four more primitives did not
///
/// A table is five parts, a lamp fitting is a cylinder and a cage of thin bars, a run of racking is
/// one part and a [`Repeat`]. The games already build those out of prefab children, which costs an
/// entity, a transform and a draw call each; this is **one mesh, one asset, one draw call**, and it
/// is a file a person or an agent can read.
///
/// Union is concatenation, so no boolean geometry is involved. **Subtraction is deliberately not
/// here** (ADR 0074 §2): a robust triangle boolean has a long tail of degenerate cases and a fragile
/// one is worse than none.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct CompoundMesh {
    /// The parts, in file order. Order does not affect the result.
    ///
    /// **Required**, with no default: an empty compound tessellates to nothing, which is exactly the
    /// "draws nothing while reporting no fault" case ADR 0075 refuses to make the default.
    pub parts: Vec<Part>,
}

impl Default for CompoundMesh {
    /// One unit box, so a default compound is something rather than nothing.
    fn default() -> Self {
        Self {
            parts: vec![Part::default()],
        }
    }
}

impl Component for CompoundMesh {}

impl CompoundMesh {
    /// Assembles every part into one mesh.
    ///
    /// # Tangents are generated once, at the end
    ///
    /// Not per part. A per-part call would give the seam between two parts two different tangent
    /// frames, and a normal map would light across it wrong — invisible until a light moves, which is
    /// the same failure mode `append_transformed` exists to prevent for normals.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        let mut data = MeshData::default();

        for part in &self.parts {
            let shape = part.solid.tessellate();

            // Every offset this part is repeated at, including `[0, 0, 0]` for the original. Built
            // first so the mirror pass below sees the finished run rather than having to repeat
            // itself, and so two `Repeat` entries compose into a grid with no special case.
            let mut offsets = vec![[0.0_f32, 0.0, 0.0]];
            for repeat in &part.repeat {
                let count = repeat.count.clamp(1, 4096);
                let mut grown = Vec::with_capacity(offsets.len() * count as usize);
                for base in &offsets {
                    for step in 0..count {
                        let along = step as f32;
                        grown.push([
                            base[0] + repeat.step[0] * along,
                            base[1] + repeat.step[1] * along,
                            base[2] + repeat.step[2] * along,
                        ]);
                    }
                }
                offsets = grown;
            }

            for offset in &offsets {
                let position = [
                    part.position[0] + offset[0],
                    part.position[1] + offset[1],
                    part.position[2] + offset[2],
                ];

                // Placed first, then mirrored — **not the other way round**. A mirror flag means
                // "reflect this part through the compound's own plane", so it has to act on the part
                // where it actually sits, after its rotation. Reflecting the raw shape and then
                // rotating it would turn the copy the wrong way whenever the part is rotated, which
                // is precisely when a symmetrical fitting needs a mirror.
                let mut placed = MeshData::default();
                append_transformed(&mut placed, &shape, position, part.rotation);

                // Each flag **doubles what exists so far**, so `[true, true, false]` gives four
                // copies rather than three — one per quadrant, which is what a symmetrical fitting
                // wants and what Blender's mirror modifier does. Reflecting only the original would
                // leave the fourth quadrant empty.
                let mut copies = vec![placed];
                for (axis, wanted) in part.mirror.iter().enumerate() {
                    if !wanted {
                        continue;
                    }
                    let reflected: Vec<MeshData> = copies
                        .iter()
                        .map(|copy| mirror_across(copy, axis))
                        .collect();
                    copies.extend(reflected);
                }

                for copy in &copies {
                    // Already placed, so this is the cheap identity branch: a straight concatenation
                    // with the index offset applied.
                    append_transformed(&mut data, copy, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
                }
            }
        }

        data.generate_tangents();
        data
    }
}

/// Geometry given directly, as vertices and triangles — ADR 0074 §4 and ADR 0035's promised form.
///
/// # This is the dump target, not the path
///
/// ADR 0074 is emphatic and this doc comment is the place a reader meets it: raw vertex data exists
/// so that **importers and generators have somewhere to land**, and so `amadeo-gltf` stays honest. It
/// is not how anything is authored by hand. A door is [`CompoundMesh`] and two numbers; a door as two
/// hundred vertices is a file nobody can edit and a diff nobody can review, which is invariant I1
/// technically satisfied and practically lost.
///
/// **If you are typing one of these by hand, you want a compound.**
///
/// # Flat lists rather than a list of structs
///
/// Positions are `[x, y, z, x, y, z, …]` and indices are flat triples. A `Vec<Vertex>` would nest a
/// struct per vertex, which for real geometry is thousands of indented blocks — the scene format
/// would technically hold it and no tool or person would survive reading it. Flat lists keep a
/// generated mesh to a handful of long lines.
///
/// Normals are optional: an empty list means "work them out from the triangles", which is what a
/// generator that only knows positions wants. UVs likewise default to nothing.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct VertexMesh {
    /// Vertex positions, three numbers each.
    #[reflect(default = Vec::new())]
    pub positions: Vec<f32>,
    /// Triangle corners, three indices each, into [`VertexMesh::positions`].
    #[reflect(default = Vec::new())]
    pub indices: Vec<u32>,
    /// Vertex normals, three numbers each. **Empty means derive them from the triangles.**
    #[reflect(default = Vec::new())]
    pub normals: Vec<f32>,
    /// Texture coordinates, two numbers each. Empty means every vertex gets `[0, 0]`.
    #[reflect(default = Vec::new())]
    pub uvs: Vec<f32>,
}

impl Default for VertexMesh {
    /// Empty, which is the honest default for a dump target: there is no sensible stand-in geometry.
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            indices: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
        }
    }
}

impl Component for VertexMesh {}

impl VertexMesh {
    /// Turns the flat lists into geometry.
    ///
    /// **Nothing here is fatal.** A trailing partial vertex, an index past the end, or a partial
    /// triangle is dropped rather than refused, for ADR 0021's reason: a game with one damaged asset
    /// should start and be visibly wrong rather than fail to run. The damage is visible — a hole —
    /// and `amadeo check` is where a malformed file gets reported.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        let count = self.positions.len() / 3;
        let mut data = MeshData::default();

        for index in 0..count {
            let at = index * 3;
            let normal = if self.normals.len() >= at + 3 {
                [self.normals[at], self.normals[at + 1], self.normals[at + 2]]
            } else {
                // Filled in below from the triangles. Not zero-length here, because a zero normal
                // normalises to `NaN` and a `NaN` spreads through everything it touches.
                [0.0, 1.0, 0.0]
            };
            let uv_at = index * 2;
            let uv = if self.uvs.len() >= uv_at + 2 {
                [self.uvs[uv_at], self.uvs[uv_at + 1]]
            } else {
                [0.0, 0.0]
            };

            data.vertices.push(Vertex {
                position: [
                    self.positions[at],
                    self.positions[at + 1],
                    self.positions[at + 2],
                ],
                normal,
                uv,
                ..Vertex::default()
            });
        }

        for triangle in self.indices.chunks_exact(3) {
            if triangle.iter().all(|corner| (*corner as usize) < count) {
                data.indices.extend(triangle);
            }
        }

        // No normals supplied, so take them from the triangles. `flat_shade` is exactly that
        // operation and it already handles a degenerate triangle by leaving its vertices alone.
        if self.normals.len() < count * 3 {
            data.flat_shade();
        }

        data.generate_tangents();
        data
    }
}

/// Reflects a mesh through the plane perpendicular to `axis`.
///
/// # Three things change together, and two of them are easy to forget
///
/// A reflection is not a rotation: it **reverses orientation**. So as well as negating one component
/// of every position and normal, the triangles have to be **wound the other way** — otherwise every
/// face ends up pointing into the solid — and the tangent's handedness sign has to flip, because the
/// bitangent the shader recovers as `cross(normal, tangent.xyz) * w` would otherwise come out on the
/// wrong side.
///
/// Unlike the rotation case, **the winding half of this is catchable**:
/// `every_primitive_is_wound_to_match_its_own_normals` compares a triangle's winding against its own
/// normals, and a reflection that flipped one without the other is exactly that disagreement. That is
/// why this is safer to write than `append_transformed`'s rotation was, and why the mirror test
/// below leans on the winding check rather than duplicating it.
fn mirror_across(source: &MeshData, axis: usize) -> MeshData {
    let flip = |mut vector: [f32; 3]| {
        vector[axis] = -vector[axis];
        vector
    };

    let vertices = source
        .vertices
        .iter()
        .map(|vertex| {
            let mut tangent = vertex.tangent;
            tangent[axis] = -tangent[axis];
            // The handedness sign, which a reflection *does* flip — unlike a rotation.
            tangent[3] = -tangent[3];
            Vertex {
                position: flip(vertex.position),
                normal: flip(vertex.normal),
                tangent,
                ..*vertex
            }
        })
        .collect();

    // Reversed corner order, which is what keeps a face pointing out of the reflected solid.
    let indices = source
        .indices
        .chunks_exact(3)
        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect();

    MeshData { vertices, indices }
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
            // The same threshold `has_area` uses, from the same constant — these two are asking the
            // same question and used to disagree by three orders of magnitude.
            if area < DEGENERATE_CROSS {
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
            // An assembly, which is a new way to get this wrong rather than the same shapes again:
            // its parts are rotated, repeated and mirrored, and each of those touches winding or
            // normals. The rotated part is the one this check cannot fully vouch for — see
            // `rotating_a_part_rotates_its_normals_and_tangents_with_it` — but the *mirrored* one it
            // catches completely, because a reflection reverses orientation.
            (
                "compound",
                CompoundMesh {
                    parts: vec![
                        Part {
                            solid: Solid::Cylinder {
                                shape: CylinderMesh::default(),
                            },
                            position: [0.4, 0.0, 0.0],
                            rotation: [0.0, 0.0, 25.0],
                            repeat: vec![Repeat {
                                count: 3,
                                step: [0.0, 0.6, 0.0],
                            }],
                            mirror: [true, false, false],
                        },
                        Part {
                            solid: Solid::Wedge {
                                shape: WedgeMesh::default(),
                            },
                            rotation: [15.0, 40.0, 0.0],
                            ..Part::default()
                        },
                    ],
                }
                .tessellate(),
            ),
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

    /// Builds a type from a value naming no fields, so every field takes its declared default.
    ///
    /// The whole of ADR 0075's drift hazard in one helper: a default is written twice, in a
    /// `#[reflect(default = ...)]` attribute and in `impl Default`, and nothing but an assertion stops
    /// the two disagreeing. It matters more since ADR 0076, because `describe --example` now
    /// *publishes* the declared value — so a drift is authoring advice that does not match what the
    /// engine builds.
    fn from_nothing<T: amadeo_reflect::Reflect>() -> T {
        T::from_value(&amadeo_reflect::Value::Struct(
            std::collections::BTreeMap::new(),
        ))
        .unwrap_or_else(|error| {
            panic!(
                "`{}` does not declare a default for every field: {error}",
                T::STATIC_NAME
            )
        })
    }

    /// A compound of one part, with everything else left at its default.
    fn one_part(solid: Solid, position: [f32; 3]) -> Part {
        Part {
            solid,
            position,
            ..Part::default()
        }
    }

    #[test]
    fn a_compound_is_the_union_of_its_parts() {
        // ADR 0074 §2: union is concatenation, so a compound's triangles are exactly its parts' and
        // no boolean geometry is involved. Two boxes side by side, checked as a count and as a reach.
        let single = BoxMesh {
            size: [1.0, 1.0, 1.0],
        }
        .tessellate();

        let pair = CompoundMesh {
            parts: vec![
                one_part(
                    Solid::Box {
                        shape: BoxMesh {
                            size: [1.0, 1.0, 1.0],
                        },
                    },
                    [-2.0, 0.0, 0.0],
                ),
                one_part(
                    Solid::Box {
                        shape: BoxMesh {
                            size: [1.0, 1.0, 1.0],
                        },
                    },
                    [2.0, 0.0, 0.0],
                ),
            ],
        }
        .tessellate();

        assert_eq!(pair.indices.len(), single.indices.len() * 2);
        assert_eq!(pair.vertices.len(), single.vertices.len() * 2);

        let widest = pair
            .vertices
            .iter()
            .fold(0.0_f32, |wide, v| wide.max(v.position[0]));
        assert!(
            (widest - 2.5).abs() < 0.001,
            "the far box's outer face should reach 2.5, got {widest}"
        );
    }

    #[test]
    fn a_repeat_makes_a_run_and_two_repeats_make_a_grid() {
        // ADR 0074 §3's `array`, and the reason `repeat` is a **list** rather than a pair of count
        // fields: a grid is two entries and needs no special case, so N axes fall out of the length.
        let leg = || Solid::Cylinder {
            shape: CylinderMesh {
                radius: 0.05,
                top_radius: 0.05,
                height: 0.7,
                ..CylinderMesh::default()
            },
        };
        let one = CompoundMesh {
            parts: vec![one_part(leg(), [0.0, 0.0, 0.0])],
        }
        .tessellate();

        let run = CompoundMesh {
            parts: vec![Part {
                solid: leg(),
                repeat: vec![Repeat {
                    count: 4,
                    step: [0.5, 0.0, 0.0],
                }],
                ..Part::default()
            }],
        }
        .tessellate();
        assert_eq!(run.indices.len(), one.indices.len() * 4);

        let grid = CompoundMesh {
            parts: vec![Part {
                solid: leg(),
                repeat: vec![
                    Repeat {
                        count: 4,
                        step: [0.5, 0.0, 0.0],
                    },
                    Repeat {
                        count: 3,
                        step: [0.0, 0.0, 0.5],
                    },
                ],
                ..Part::default()
            }],
        }
        .tessellate();
        assert_eq!(
            grid.indices.len(),
            one.indices.len() * 12,
            "two repeats should multiply, not add"
        );

        // The run really is spread out rather than stacked in one place.
        let reach = run
            .vertices
            .iter()
            .fold(0.0_f32, |wide, v| wide.max(v.position[0]));
        assert!(reach > 1.5, "a four-step run only reached {reach}");
    }

    #[test]
    fn a_mirror_adds_a_reflected_copy_and_flips_its_handedness() {
        // ADR 0074 §3's `mirror`: a symmetrical fitting is one half and a flag. Each flag **adds**
        // rather than replaces, so one flag makes a pair and two make four.
        let bracket = || Part {
            solid: Solid::Box {
                shape: BoxMesh {
                    size: [0.2, 0.2, 0.2],
                },
            },
            position: [1.0, 0.0, 0.0],
            ..Part::default()
        };

        let alone = CompoundMesh {
            parts: vec![bracket()],
        }
        .tessellate();
        let paired = CompoundMesh {
            parts: vec![Part {
                mirror: [true, false, false],
                ..bracket()
            }],
        }
        .tessellate();

        assert_eq!(paired.indices.len(), alone.indices.len() * 2);

        // The copy is on the other side, which is the whole point.
        let leftmost = paired
            .vertices
            .iter()
            .fold(f32::MAX, |low, v| low.min(v.position[0]));
        assert!(
            (leftmost + 1.1).abs() < 0.001,
            "the mirrored copy should reach -1.1, got {leftmost}"
        );

        // Two flags double twice.
        let four = CompoundMesh {
            parts: vec![Part {
                mirror: [true, true, false],
                ..bracket()
            }],
        }
        .tessellate();
        assert_eq!(four.indices.len(), alone.indices.len() * 4);
    }

    #[test]
    fn a_mirrored_part_is_not_inside_out() {
        // **The failure a reflection introduces that a rotation does not.** A reflection reverses
        // orientation, so negating positions without reversing the winding leaves every face of the
        // mirrored copy pointing into the solid. Unlike the rotation case, the winding check *can*
        // see this — which is why `mirror_across` is safer to write than `append_transformed` was,
        // and why this test is one line of leaning on the existing check rather than a new one.
        let assembly = CompoundMesh {
            parts: vec![Part {
                solid: Solid::Wedge {
                    shape: WedgeMesh::default(),
                },
                position: [0.8, 0.0, 0.4],
                rotation: [0.0, 35.0, 0.0],
                mirror: [true, false, true],
                ..Part::default()
            }],
        }
        .tessellate();

        if let Err(problem) = wound_to_match_normals(&assembly) {
            panic!("a mirrored, rotated part: {problem}");
        }
    }

    #[test]
    fn rotating_a_part_rotates_its_normals_and_tangents_with_it() {
        // **Written before `append_transformed` existed, because it is the one defect in this feature
        // that nothing else can see.**
        //
        // `every_primitive_is_wound_to_match_its_own_normals` compares a triangle's winding against
        // its own normals — and a rotation moves *both*, together, consistently. So a part whose
        // normals were left unrotated while its positions turned would pass that test perfectly, and
        // would render as a shape that shades correctly until a light moves. That is the failure
        // `append_translated`'s doc comment refused rotation to avoid; this is what replaces the
        // refusal.
        //
        // The assertion is direct: rotate a part, and every normal must equal the unrotated part's
        // normal put through the same matrix. Nothing about winding, nothing about pixels.
        let part = BoxMesh {
            size: [2.0, 0.5, 1.0],
        }
        .tessellate();

        let rotation = [0.0, 90.0, 0.0];
        let matrix = amadeo_transform::Mat4::from_euler_degrees(rotation);

        let mut rotated = MeshData::default();
        append_transformed(&mut rotated, &part, [0.0, 0.0, 0.0], rotation);

        assert_eq!(
            rotated.vertices.len(),
            part.vertices.len(),
            "a transform must not change how many vertices a part has"
        );

        for (index, (before, after)) in part.vertices.iter().zip(&rotated.vertices).enumerate() {
            let expected = matrix.transform_point4(before.normal);
            for axis in 0..3 {
                assert!(
                    (after.normal[axis] - expected[axis]).abs() < 1e-5,
                    "vertex {index}'s normal came out {:?}, expected {:?} — the positions turned and \
                     the normals did not, which shades correctly until a light moves",
                    after.normal,
                    [expected[0], expected[1], expected[2]]
                );
            }

            // The tangent turns too, and its handedness does *not*: `w` is a sign, not a direction,
            // and a rotation cannot flip it.
            let expected_tangent =
                matrix.transform_point4([before.tangent[0], before.tangent[1], before.tangent[2]]);
            for (axis, wanted) in expected_tangent.iter().take(3).enumerate() {
                assert!(
                    (after.tangent[axis] - wanted).abs() < 1e-5,
                    "vertex {index}'s tangent did not turn with its part"
                );
            }
            assert_eq!(
                after.tangent[3], before.tangent[3],
                "a rotation must not change a tangent's handedness"
            );
        }
    }

    #[test]
    fn a_stair_is_unchanged_by_the_move_to_a_general_transform() {
        // The migration test, written before `append_translated` was deleted. `StairMesh` was the one
        // shape composing parts, through a translate-only helper that could not get a normal wrong;
        // this proves the general path did not regress it.
        //
        // **Positions and normals rather than whole vertices**, deliberately: adding a
        // `generate_tangents` call to `StairMesh` later is an orthogonal change, and comparing whole
        // vertices would make this test fail for that unrelated reason.
        let data = StairMesh::default().tessellate();

        // The property the old helper guaranteed by construction: every box in the flight keeps
        // axis-aligned normals, because a translation cannot turn one.
        for vertex in &data.vertices {
            let axes = vertex
                .normal
                .iter()
                .filter(|component| component.abs() > 0.001)
                .count();
            assert_eq!(
                axes, 1,
                "a stair's normals should still be axis-aligned, got {:?}",
                vertex.normal
            );
        }

        // And the flight is still where it was: the arithmetic in `total_rise`/`total_run` is what a
        // level designer places against.
        let tallest = data
            .vertices
            .iter()
            .fold(0.0_f32, |high, v| high.max(v.position[1]));
        assert!(
            (tallest - StairMesh::default().total_rise()).abs() < 0.001,
            "the flight reaches {tallest}, not its own total rise"
        );
    }

    #[test]
    fn every_authored_type_in_this_crate_declares_its_defaults() {
        // ADR 0076: **if a type is authored in a text file and has a sensible `Default`, its fields
        // declare that default.** Before it, `describe --example` handed an author `BoxMesh size
        // 0.0 0.0 0.0` — the type 23 of 23 `.mesh` assets use, drawing nothing — plus a dead camera,
        // a black environment and three lights at zero intensity. Every one of those types already
        // held the right values in a hand-written `Default`; they were simply not in the schema.
        use crate::{
            ArchMesh, BoxMesh, Camera, DirectionalLight, Environment, Material, PlaneMesh,
            PointLight, SpotLight,
        };
        assert_eq!(from_nothing::<BoxMesh>(), BoxMesh::default());
        assert_eq!(from_nothing::<PlaneMesh>(), PlaneMesh::default());
        assert_eq!(from_nothing::<ArchMesh>(), ArchMesh::default());
        assert_eq!(from_nothing::<Material>(), Material::default());
        assert_eq!(from_nothing::<Camera>(), Camera::default());
        assert_eq!(from_nothing::<Environment>(), Environment::default());
        assert_eq!(
            from_nothing::<DirectionalLight>(),
            DirectionalLight::default()
        );
        assert_eq!(from_nothing::<PointLight>(), PointLight::default());
        assert_eq!(from_nothing::<SpotLight>(), SpotLight::default());
    }

    #[test]
    fn every_shapes_declared_defaults_agree_with_its_default_impl() {
        // ADR 0075's named hazard, once per type in this file: each default is written twice, in a
        // `#[reflect(default = ...)]` attribute and in `impl Default`, and nothing but this stops the
        // two drifting. It matters more here than for `Material`, because `describe --example` now
        // *reports* the declared values — so a drift would publish authoring advice that does not
        // match what the engine builds.
        assert_eq!(from_nothing::<CylinderMesh>(), CylinderMesh::default());
        assert_eq!(from_nothing::<SphereMesh>(), SphereMesh::default());
        assert_eq!(from_nothing::<WedgeMesh>(), WedgeMesh::default());
        assert_eq!(from_nothing::<StairMesh>(), StairMesh::default());
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

        // **The sphere, which this test did not cover until the assertion was tried against a broken
        // build.** Replacing the sphere's facet normal with its corners' normals — real flat shading
        // switched off — left this test green, because everything above only looks at a cylinder. The
        // capture test caught it and this did not; a unit test named for the whole feature should.
        let faceted_ball = SphereMesh {
            flat: true,
            ..SphereMesh::default()
        }
        .tessellate();

        let mut facet_normals = Vec::new();
        for (index, facet) in faceted_ball.vertices.chunks_exact(4).enumerate() {
            for vertex in facet {
                assert_eq!(
                    vertex.normal, facet[0].normal,
                    "facet {index} of a faceted sphere has more than one normal"
                );
            }
            facet_normals.push(facet[0].normal);
        }
        // And the facets do not all share *one* normal, which "one normal per facet" would also be
        // satisfied by if the whole ball were flat.
        assert!(
            facet_normals.windows(2).any(|pair| pair[0] != pair[1]),
            "every facet of the sphere has the same normal, so it is a disc rather than a ball"
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
