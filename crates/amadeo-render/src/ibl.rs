//! Image-based lighting: turning a picture of the sky into the light it casts — ADR 0049.
//!
//! # What this is for
//!
//! Until now the only ambient light in the engine was the constant `0.12`, added to every surface
//! regardless of which way it faced or what was around it. That is why shadowed areas read as flat
//! grey holes and why a metal renders black: a metal has no diffuse at all, so with nothing to
//! reflect there is nothing to see.
//!
//! Image-based lighting replaces the constant with **an actual environment**. Every direction of the
//! sky becomes a light source, so a surface facing up picks up sky and one facing a red wall picks up
//! red. It is the single largest step available towards a scene looking real (ADR 0045), and it is
//! what makes the metallic-roughness model of ADR 0048 pay off.
//!
//! # Why the work happens here, on the CPU, at load
//!
//! Two convolutions have to happen before a shader can use an environment, and both are expensive:
//! one for diffuse and one for specular. Doing them on the GPU would be faster and would break
//! **invariant I7** — every subsystem must be headless-capable, and a prefilter that needs a device
//! could not run in a headless test or on a machine with no GPU.
//!
//! So they run here, in plain Rust, exactly as [`mip_chain`](amadeo_image::mip_chain) does. The cost
//! is paid once at load, behind ADR 0021's barrier, where gameplay cannot observe it.
//!
//! **This code uses `sin`, `cos`, `atan2` and `sqrt` freely, and that is allowed here.** ADR 0044
//! bans transcendentals from anything deciding *gameplay state*, because their precision varies by
//! platform. Everything in this file runs at load and its output is pixels. A prefiltered mip level
//! differing in its last bit between two machines changes a reflection by an amount no one can see,
//! and changes no collider, no position and no hash. The same carve-out `mip_chain` and the Scarp's
//! `turf` generator already carry.
//!
//! # The two halves, in plain terms
//!
//! **Diffuse** ([`irradiance`]). A matte surface facing some direction catches light from the whole
//! hemisphere in front of it, weighted by angle. That sum does not depend on where the viewer is, so
//! it can be computed once per direction and stored in a small cube — 16 pixels a side is plenty,
//! because the answer varies very smoothly.
//!
//! **Specular** ([`prefilter_specular`]). A shiny surface reflects a narrow cone, a rough one a wide
//! one. So this stores *several* blurred copies of the environment — sharp in the top mip level,
//! blurrier in each one below — and the shader picks a level from the material's roughness. This is
//! the "split-sum" approach Karis introduced for Unreal Engine 4 and is what essentially every
//! real-time renderer now does.

use amadeo_image::HdrImage;

/// How many pixels a side the diffuse cube gets.
///
/// Small on purpose: irradiance is the smoothest signal in rendering — it is the whole hemisphere
/// averaged — so detail here would be storing noise. Doubling it quadruples the convolution cost and
/// changes the picture by nothing.
pub const IRRADIANCE_SIZE: u32 = 16;

/// How many pixels a side the sharpest specular level gets.
pub const SPECULAR_SIZE: u32 = 128;

/// How many roughness levels the specular chain holds, from mirror to fully rough.
///
/// Six covers 128 down to 4 pixels a side. Below that a face is too small to hold a direction.
pub const SPECULAR_LEVELS: u32 = 6;

/// How many directions each specular texel samples.
///
/// The quality/load-time dial. Too few and a bright sun becomes a scatter of separate dots rather
/// than a smooth blur, which is the characteristic artefact of an under-sampled prefilter.
const SPECULAR_SAMPLES: u32 = 64;

/// The six faces of a cube map, in the order every graphics API expects them.
///
/// `+X, -X, +Y, -Y, +Z, -Z`. The order is not a choice — wgpu uploads a cube texture as six array
/// layers in exactly this sequence, so anything else would compile, upload, and render with the sky
/// rotated in a way that is very hard to attribute.
pub const FACE_COUNT: usize = 6;

/// An environment stored as six square faces of a cube.
///
/// A cube rather than the equirectangular rectangle a `.hdr` file usually arrives as, because a
/// rectangle wraps a sphere very unevenly: its top and bottom rows squash a single point across the
/// whole width, so the poles get hundreds of times more storage than the horizon and sampling near
/// them is a mess. Six flat faces are close to uniform, and a GPU can sample one with a direction
/// vector directly.
#[derive(Debug, Clone, PartialEq)]
pub struct Cubemap {
    /// Pixels along one edge of one face.
    pub size: u32,
    /// Six faces, each `size * size` linear RGBA pixels, in [`FACE_COUNT`]'s order.
    pub faces: Vec<Vec<[f32; 4]>>,
}

impl Cubemap {
    /// A cube map with every pixel the same colour.
    #[must_use]
    pub fn solid(size: u32, colour: [f32; 4]) -> Cubemap {
        Cubemap {
            size,
            faces: vec![vec![colour; (size * size) as usize]; FACE_COUNT],
        }
    }

    /// The pixel a direction points at, without interpolation.
    ///
    /// Nearest rather than bilinear, deliberately: this is used by the convolutions below, which
    /// average thousands of samples anyway, so filtering each one would cost four times as much to
    /// blur something about to be blurred.
    #[must_use]
    pub fn sample(&self, direction: [f32; 3]) -> [f32; 4] {
        let (face, u, v) = direction_to_face(direction);
        let size = self.size as f32;
        // `min` guards the boundary: a u of exactly 1.0 would index one past the last pixel.
        let x = ((u * size) as u32).min(self.size - 1);
        let y = ((v * size) as u32).min(self.size - 1);
        self.faces[face][(y * self.size + x) as usize]
    }

    /// Builds a cube map by projecting an equirectangular image onto it.
    ///
    /// Equirectangular — longitude across, latitude down — is how a `.hdr` environment map is almost
    /// always distributed, because it is one plain rectangle. This is the one-time conversion into
    /// the shape a GPU can actually sample.
    #[must_use]
    pub fn from_equirectangular(source: &HdrImage, size: u32) -> Cubemap {
        let mut faces = Vec::with_capacity(FACE_COUNT);
        for face in 0..FACE_COUNT {
            let mut pixels = Vec::with_capacity((size * size) as usize);
            for y in 0..size {
                for x in 0..size {
                    let direction = face_direction(face, x, y, size);
                    pixels.push(sample_equirectangular(source, direction));
                }
            }
            faces.push(pixels);
        }
        Cubemap { size, faces }
    }

    /// Half the size, each pixel the average of the four it covers.
    ///
    /// Averaged in **linear light**, which needs no transform here because these pixels already are
    /// linear light — unlike [`mip_chain`](amadeo_image::mip_chain), whose whole subtlety is that
    /// sRGB bytes are not. An HDR environment never had that problem.
    #[must_use]
    fn halved(&self) -> Cubemap {
        let size = (self.size / 2).max(1);
        let mut faces = Vec::with_capacity(FACE_COUNT);

        for face in &self.faces {
            let mut pixels = Vec::with_capacity((size * size) as usize);
            for y in 0..size {
                for x in 0..size {
                    let x0 = (x * 2).min(self.size - 1);
                    let y0 = (y * 2).min(self.size - 1);
                    let x1 = (x0 + 1).min(self.size - 1);
                    let y1 = (y0 + 1).min(self.size - 1);

                    let mut total = [0.0f32; 4];
                    for (sx, sy) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
                        let pixel = face[(sy * self.size + sx) as usize];
                        for channel in 0..4 {
                            total[channel] += pixel[channel] * 0.25;
                        }
                    }
                    pixels.push(total);
                }
            }
            faces.push(pixels);
        }
        Cubemap { size, faces }
    }
}

/// Everything a shader needs to light a surface from an environment.
///
/// Both halves are precomputed, which is the whole point: neither convolution could run per pixel
/// per frame, and neither depends on anything that changes between frames.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentMap {
    /// The diffuse half: for each direction, the total light reaching a matte surface facing it.
    pub irradiance: Cubemap,
    /// The specular half: the environment blurred more at each level, sharpest first.
    ///
    /// Level `i` corresponds to roughness `i / (levels - 1)`, so the shader turns a material's
    /// roughness into a level by multiplying. Level 0 is untouched — a perfect mirror reflects the
    /// environment exactly as it is.
    pub specular: Vec<Cubemap>,
}

impl EnvironmentMap {
    /// Prefilters an equirectangular HDR image into both halves.
    ///
    /// **Slow, and meant to be.** This is seconds of work at load, not milliseconds — see the module
    /// header for why it is here rather than on the GPU, and ADR 0049 for what would move it.
    #[must_use]
    pub fn from_equirectangular(source: &HdrImage) -> EnvironmentMap {
        let base = Cubemap::from_equirectangular(source, SPECULAR_SIZE);
        EnvironmentMap {
            irradiance: irradiance(&base, IRRADIANCE_SIZE),
            specular: prefilter_specular(&base),
        }
    }

    /// The whole thing from one colour — a plain, uniform sky.
    ///
    /// What a game gets when it names no environment map. Deliberately **not** black: black would
    /// make this an invisible regression from the `0.12` constant it replaces, turning every
    /// shadowed surface into a hole. See ADR 0049 on why the default is a dim neutral.
    #[must_use]
    pub fn solid(colour: [f32; 4]) -> EnvironmentMap {
        EnvironmentMap {
            irradiance: Cubemap::solid(IRRADIANCE_SIZE, colour),
            specular: (0..SPECULAR_LEVELS)
                .map(|level| Cubemap::solid((SPECULAR_SIZE >> level).max(1), colour))
                .collect(),
        }
    }
}

/// Which face a direction lands on, and where on it, as `(face, u, v)` with u and v in `0..1`.
fn direction_to_face(direction: [f32; 3]) -> (usize, f32, f32) {
    let [x, y, z] = direction;
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());

    // The largest component decides the face; the other two, divided by it, give the position on
    // that face. This is exactly what a GPU does when sampling a cube texture with a direction.
    let (face, major, sc, tc) = if ax >= ay && ax >= az {
        if x > 0.0 {
            (0, ax, -z, -y)
        } else {
            (1, ax, z, -y)
        }
    } else if ay >= az {
        if y > 0.0 {
            (2, ay, x, z)
        } else {
            (3, ay, x, -z)
        }
    } else if z > 0.0 {
        (4, az, x, -y)
    } else {
        (5, az, -x, -y)
    };

    let major = major.max(1e-8);
    let u = (sc / major + 1.0) * 0.5;
    let v = (tc / major + 1.0) * 0.5;
    (face, u.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
}

/// The direction a texel of a face points, the exact inverse of [`direction_to_face`].
fn face_direction(face: usize, x: u32, y: u32, size: u32) -> [f32; 3] {
    // Pixel centres, so a face's samples are spread evenly rather than biased to one corner.
    let u = 2.0 * ((x as f32 + 0.5) / size as f32) - 1.0;
    let v = 2.0 * ((y as f32 + 0.5) / size as f32) - 1.0;

    let direction = match face {
        0 => [1.0, -v, -u],
        1 => [-1.0, -v, u],
        2 => [u, 1.0, v],
        3 => [u, -1.0, -v],
        4 => [u, -v, 1.0],
        _ => [-u, -v, -1.0],
    };
    normalise(direction)
}

/// Reads an equirectangular image in the direction given.
fn sample_equirectangular(source: &HdrImage, direction: [f32; 3]) -> [f32; 4] {
    // Longitude around the vertical axis, latitude from straight up. `atan2` handles every quadrant,
    // which is why it is used rather than `atan` plus sign fixing.
    let longitude = direction[2].atan2(direction[0]);
    let latitude = direction[1].clamp(-1.0, 1.0).acos();

    let u = (longitude / std::f32::consts::TAU + 0.5).rem_euclid(1.0);
    let v = latitude / std::f32::consts::PI;

    let x = ((u * source.width as f32) as u32).min(source.width.saturating_sub(1));
    let y = ((v * source.height as f32) as u32).min(source.height.saturating_sub(1));
    source.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 1.0])
}

/// Convolves an environment into diffuse irradiance.
///
/// For each output direction, sums the light arriving from the whole hemisphere facing it, weighted
/// by how square-on it arrives — which is Lambert's cosine law, and is the same `N·L` a direct light
/// uses, just integrated over every direction at once instead of one.
///
/// Sampled from a **downsampled** copy of the source rather than the full one. The answer is an
/// average over a hemisphere, so the fine detail contributes nothing an average would keep, and the
/// cost falls by the square of the reduction.
#[must_use]
pub fn irradiance(source: &Cubemap, size: u32) -> Cubemap {
    // Small enough to sum exhaustively rather than sampling randomly, which removes noise entirely:
    // every output texel sees every input texel, so there is nothing left to be lucky or unlucky
    // about. That determinism is worth more here than resolution.
    let mut coarse = source.clone();
    while coarse.size > 16 {
        coarse = coarse.halved();
    }

    // Every source texel's direction and the solid angle it covers, computed once and reused for
    // all `6 * size * size` outputs rather than recomputed inside the inner loop.
    let mut incoming: Vec<([f32; 3], [f32; 4], f32)> = Vec::new();
    for (face, pixels) in coarse.faces.iter().enumerate() {
        for y in 0..coarse.size {
            for x in 0..coarse.size {
                let direction = face_direction(face, x, y, coarse.size);
                let colour = pixels[(y * coarse.size + x) as usize];
                incoming.push((direction, colour, texel_solid_angle(x, y, coarse.size)));
            }
        }
    }

    let mut faces = Vec::with_capacity(FACE_COUNT);
    for face in 0..FACE_COUNT {
        let mut pixels = Vec::with_capacity((size * size) as usize);
        for y in 0..size {
            for x in 0..size {
                let normal = face_direction(face, x, y, size);
                let mut total = [0.0f32; 3];
                let mut weight = 0.0f32;

                for (direction, colour, solid_angle) in &incoming {
                    let cosine = dot(normal, *direction);
                    // Behind the surface, so it contributes nothing. Not `abs`: light from behind
                    // does not reach the front.
                    if cosine <= 0.0 {
                        continue;
                    }
                    let contribution = cosine * solid_angle;
                    for channel in 0..3 {
                        total[channel] += colour[channel] * contribution;
                    }
                    weight += contribution;
                }

                // Normalising by the accumulated weight rather than by a constant makes this the
                // *average* incoming radiance, which is what the shader multiplies by albedo. It
                // also makes a uniform sky come out exactly its own colour, which is the property
                // the tests pin.
                let scale = if weight > 0.0 { 1.0 / weight } else { 0.0 };
                pixels.push([total[0] * scale, total[1] * scale, total[2] * scale, 1.0]);
            }
        }
        faces.push(pixels);
    }
    Cubemap { size, faces }
}

/// How much of the sphere one texel of a cube face covers.
///
/// Texels near a face's corner cover noticeably less than those at its centre, because a flat square
/// projected onto a sphere stretches. Ignoring this tilts every convolution towards the corners —
/// which shows up as the eight corners of the cube being subtly brighter than they should be.
fn texel_solid_angle(x: u32, y: u32, size: u32) -> f32 {
    let u = 2.0 * ((x as f32 + 0.5) / size as f32) - 1.0;
    let v = 2.0 * ((y as f32 + 0.5) / size as f32) - 1.0;
    // The projected area falls off with the cube of the distance from the cube's centre to the
    // texel, which for a unit cube is `sqrt(1 + u^2 + v^2)`.
    let distance_squared = 1.0 + u * u + v * v;
    1.0 / (distance_squared * distance_squared.sqrt())
}

/// Blurs an environment once per roughness level, sharpest first.
///
/// Each level answers "what does a surface of *this* roughness reflect, looking in this direction" —
/// which is the environment averaged over the cone that roughness scatters into. Level 0 is a
/// mirror and is copied rather than blurred.
#[must_use]
pub fn prefilter_specular(source: &Cubemap) -> Vec<Cubemap> {
    let mut levels = Vec::with_capacity(SPECULAR_LEVELS as usize);

    for level in 0..SPECULAR_LEVELS {
        let size = (source.size >> level).max(1);
        let roughness = level as f32 / (SPECULAR_LEVELS - 1) as f32;

        if level == 0 {
            // A mirror reflects the environment exactly. Resizing it would blur what is meant to be
            // the one perfectly sharp level.
            levels.push(source.clone());
            continue;
        }

        let mut faces = Vec::with_capacity(FACE_COUNT);
        for face in 0..FACE_COUNT {
            let mut pixels = Vec::with_capacity((size * size) as usize);
            for y in 0..size {
                for x in 0..size {
                    let normal = face_direction(face, x, y, size);
                    pixels.push(prefilter_texel(source, normal, roughness));
                }
            }
            faces.push(pixels);
        }
        levels.push(Cubemap { size, faces });
    }
    levels
}

/// One prefiltered texel: the environment averaged over the cone this roughness scatters into.
///
/// **The approximation this shares with every real-time renderer**: it assumes you are looking at
/// the surface straight on, so the reflected direction and the view direction are both the normal.
/// A surface seen at a glancing angle really reflects a stretched, comet-shaped highlight, and this
/// gives it a round one. Karis named this as the split-sum's main error and shipped it anyway,
/// because storing the alternative would need a whole extra dimension of cube maps.
fn prefilter_texel(source: &Cubemap, normal: [f32; 3], roughness: f32) -> [f32; 4] {
    let mut total = [0.0f32; 3];
    let mut weight = 0.0f32;

    for sample in 0..SPECULAR_SAMPLES {
        // A deterministic low-discrepancy sequence rather than a random one, so two runs on two
        // machines produce the same prefiltered map. Randomness here would make a *texture* depend
        // on an RNG, which is exactly what this project refuses everywhere else.
        let (u1, u2) = hammersley(sample, SPECULAR_SAMPLES);
        let half = importance_sample_ggx(u1, u2, normal, roughness);

        // Reflecting the normal about the sampled half-vector gives the direction light would have
        // come from to bounce towards the viewer.
        let light = sub(scale(half, 2.0 * dot(normal, half)), normal);
        let cosine = dot(normal, light);
        if cosine <= 0.0 {
            continue;
        }

        let colour = source.sample(light);
        for channel in 0..3 {
            // Weighted by the cosine, which biases towards directions that actually contribute and
            // is what Karis found gives a visibly better result than a flat average.
            total[channel] += colour[channel] * cosine;
        }
        weight += cosine;
    }

    let scale_by = if weight > 0.0 { 1.0 / weight } else { 0.0 };
    [
        total[0] * scale_by,
        total[1] * scale_by,
        total[2] * scale_by,
        1.0,
    ]
}

/// The Hammersley sequence: evenly spread points in the unit square, without randomness.
///
/// The second coordinate is the first's bits reversed, which spreads successive points as far from
/// each other as possible. That is what makes 64 samples look like a smooth blur where 64 random
/// ones would look like 64 dots.
fn hammersley(index: u32, count: u32) -> (f32, f32) {
    (
        index as f32 / count as f32,
        index.reverse_bits() as f32 * 2.328_306_4e-10,
    )
}

/// Picks a microfacet direction, weighted the way GGX says they are distributed.
///
/// **Must agree with `distribution_ggx` in `mesh.wgsl`.** They are two views of one model — this one
/// generates directions, that one measures how many point a given way — and if they disagreed, a
/// prefiltered reflection would be subtly the wrong shape for the material shading it. That
/// coupling is the reason this file lives in `amadeo-render` beside the shader rather than in
/// `amadeo-image` with the other pixel work.
fn importance_sample_ggx(u1: f32, u2: f32, normal: [f32; 3], roughness: f32) -> [f32; 3] {
    let a = roughness * roughness;

    let phi = std::f32::consts::TAU * u1;
    let cos_theta = ((1.0 - u2) / (1.0 + (a * a - 1.0) * u2)).max(0.0).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();

    // In a space where the normal is "up".
    let local = [phi.cos() * sin_theta, phi.sin() * sin_theta, cos_theta];

    // Any two axes at right angles to the normal will do — this is a rotation about it, and a
    // rotated sample set covers the same cone.
    let up = if normal[2].abs() < 0.999 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let tangent = normalise(cross(up, normal));
    let bitangent = cross(normal, tangent);

    normalise(add(
        add(scale(tangent, local[0]), scale(bitangent, local[1])),
        scale(normal, local[2]),
    ))
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], by: f32) -> [f32; 3] {
    [a[0] * by, a[1] * by, a[2] * by]
}

fn normalise(a: [f32; 3]) -> [f32; 3] {
    let length = dot(a, a).sqrt();
    if length < 1e-8 {
        return [0.0, 0.0, 1.0];
    }
    scale(a, 1.0 / length)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How far apart two colours are, worst channel.
    fn difference(a: [f32; 4], b: [f32; 4]) -> f32 {
        (0..3).fold(0.0f32, |worst, channel| {
            worst.max((a[channel] - b[channel]).abs())
        })
    }

    #[test]
    fn a_direction_and_a_texel_are_inverses_of_each_other() {
        // **The single most likely thing to be wrong in this file**, and the least likely to be
        // noticed: a face mapping that disagrees with itself puts the sky on the wrong side of the
        // world, or mirrors it, and the result still looks like *a* sky.
        for face in 0..FACE_COUNT {
            for (x, y) in [(0u32, 0u32), (3, 5), (7, 7), (15, 2)] {
                let direction = face_direction(face, x, y, 16);
                let (back, u, v) = direction_to_face(direction);

                assert_eq!(
                    back, face,
                    "face {face} texel ({x},{y}) landed on face {back}"
                );
                let (bx, by) = ((u * 16.0) as u32, (v * 16.0) as u32);
                assert_eq!(
                    (bx.min(15), by.min(15)),
                    (x, y),
                    "face {face} texel ({x},{y}) came back as ({bx},{by})"
                );
            }
        }
    }

    #[test]
    fn a_uniform_sky_irradiates_uniformly_and_keeps_its_colour() {
        // The property that catches a convolution normalised wrongly. Light arriving equally from
        // every direction must leave a surface reading exactly that colour whichever way it faces —
        // any weighting mistake shows up as either the wrong brightness or a variation across the
        // cube that should not be there.
        let sky = Cubemap::solid(32, [0.4, 0.6, 0.9, 1.0]);
        let diffuse = irradiance(&sky, 8);

        for face in &diffuse.faces {
            for pixel in face {
                assert!(
                    difference(*pixel, [0.4, 0.6, 0.9, 1.0]) < 0.02,
                    "a uniform sky must irradiate to its own colour, got {pixel:?}"
                );
            }
        }
    }

    #[test]
    fn irradiance_follows_which_way_a_surface_faces() {
        // The point of the whole diffuse half: a surface under a bright sky and dark ground reads
        // bright looking up and dark looking down. A convolution that ignored direction — or the
        // constant `0.12` this replaces — would give both the same.
        let mut sky = Cubemap::solid(32, [0.05, 0.05, 0.05, 1.0]);
        // Face 2 is +Y, which is up.
        sky.faces[2] = vec![[3.0, 3.0, 3.0, 1.0]; (32 * 32) as usize];

        let diffuse = irradiance(&sky, 8);
        // Sample the middle of the up face and the middle of the down face.
        let up = diffuse.faces[2][(4 * 8 + 4) as usize];
        let down = diffuse.faces[3][(4 * 8 + 4) as usize];

        assert!(
            up[0] > down[0] * 3.0,
            "a surface facing a bright sky must be much brighter than one facing dark ground: \
             up {up:?} against down {down:?}"
        );
    }

    #[test]
    fn a_uniform_sky_stays_its_own_colour_at_every_roughness() {
        // Blurring something already uniform must change nothing. This is what catches a prefilter
        // whose weights do not sum to one — which would show as reflections getting brighter or
        // darker with roughness, an effect nothing physical does.
        let sky = Cubemap::solid(16, [0.3, 0.5, 0.7, 1.0]);
        let levels = prefilter_specular(&sky);

        assert_eq!(levels.len(), SPECULAR_LEVELS as usize);
        for (level, cube) in levels.iter().enumerate() {
            for face in &cube.faces {
                for pixel in face {
                    assert!(
                        difference(*pixel, [0.3, 0.5, 0.7, 1.0]) < 0.02,
                        "level {level} drifted from the sky's colour: {pixel:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_rougher_surface_gathers_from_further_off_its_normal() {
        // The specular chain's reason to exist, tested the way the blur actually manifests.
        //
        // **Not** by comparing a surface facing the light against one facing away — a first attempt
        // did that and it failed for a correct reason worth recording. This prefilter assumes the
        // viewer is looking straight on (see `prefilter_texel`), so it only ever gathers from the
        // hemisphere around the normal. A surface facing *down* sees nothing of the sky at any
        // roughness, which is right rather than a bug.
        //
        // So: a dark sky with a bright ring around the edge of the up face, and a normal pointing
        // straight up at the dark middle of it. A mirror sees only the dark. A rough surface's cone
        // is wide enough to reach the ring.
        let mut sky = Cubemap::solid(32, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..32u32 {
            for x in 0..32u32 {
                let edge = !(6..26).contains(&x) || !(6..26).contains(&y);
                if edge {
                    sky.faces[2][(y * 32 + x) as usize] = [8.0, 8.0, 8.0, 1.0];
                }
            }
        }

        let up = [0.0, 1.0, 0.0];
        let mirror = prefilter_texel(&sky, up, 0.05);
        let rough = prefilter_texel(&sky, up, 1.0);

        assert!(
            mirror[0] < 0.5,
            "a mirror looking at the dark middle must stay dark, got {mirror:?}"
        );
        assert!(
            rough[0] > mirror[0] + 1.0,
            "a rough surface's cone must reach the bright ring the mirror cannot see: \
             rough {rough:?} against mirror {mirror:?}"
        );
    }

    #[test]
    fn an_equirectangular_image_lands_the_right_way_up() {
        // A sky map put on upside down is the classic conversion bug, and it survives every test
        // that only checks colours. Bright top half, dark bottom half: +Y must come out bright.
        let mut source = HdrImage::solid(64, 32, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..16 {
            for x in 0..64 {
                source.pixels[(y * 64 + x) as usize] = [5.0, 5.0, 5.0, 1.0];
            }
        }

        let cube = Cubemap::from_equirectangular(&source, 8);
        let up = cube.faces[2][(4 * 8 + 4) as usize];
        let down = cube.faces[3][(4 * 8 + 4) as usize];

        assert!(
            up[0] > 4.0,
            "the top of an equirectangular image is up, got {up:?}"
        );
        assert!(down[0] < 1.0, "and the bottom is down, got {down:?}");
    }

    #[test]
    fn hammersley_points_spread_across_the_square() {
        // If this collapsed to one point, every prefilter sample would read the same direction and
        // the blur would silently do nothing.
        let points: Vec<(f32, f32)> = (0..16).map(|i| hammersley(i, 16)).collect();
        let spread = points.iter().fold(0.0f32, |worst, (_, v)| worst.max(*v));
        assert!(spread > 0.5, "the sequence should reach across the square");
        assert!(
            points
                .iter()
                .all(|(u, v)| (0.0..1.0).contains(u) && (0.0..1.0).contains(v)),
            "every point must stay inside the unit square"
        );
    }
}
