// Ambient occlusion from the depth prepass — ADR 0083.
//
// `view.wgsl` is prepended to this at pipeline creation, exactly as it is to `mesh.wgsl` and
// `sky.wgsl`, so `MeshView` and its binding come from the one declaration. Do not restate them here:
// the last time two files each held a copy of that struct, they drifted and the symptom was a sky
// drawn facing the wrong way with nothing failing.
//
// # What this measures
//
// For each pixel: how much of the hemisphere above it is blocked by something nearer the camera.
// A point in the middle of a wall is blocked by nothing and comes back 1. A point in the corner
// where two walls meet has a whole quadrant of its sky taken up by the other wall and comes back
// low. That is the quantity a baked texture cannot know, because the two walls are different meshes
// with different textures and neither one's UV space contains the other.
//
// # The method, and why the noise is deliberate
//
// A spiral of samples around each pixel, each one asking "is the surface there nearer than the flat
// plane through this pixel would be". The spiral is **rotated by a different angle at every pixel**,
// which is what turns the sampling error from banding into noise — and noise is far easier to remove
// than banding, because a blur removes it and nothing removes a band. That trade is why there is a
// second pass.
//
// This is the Alchemy/SAO family rather than HBAO or GTAO. Both of those are better and both need
// several times the samples; this one reconstructs its normals from depth, needs no normal buffer,
// and is what a forward renderer can afford beside a shadow pass it is already paying for.

@group(1) @binding(0) var depth_map: texture_depth_2d;

// The measured, noisy occlusion — what the blur pass reads. Named apart from `occlusion_map` in
// `view.wgsl`, which is prepended to this file and declares the *finished* map on group 0.
@group(1) @binding(1) var measured_map: texture_2d<f32>;
@group(1) @binding(2) var measured_sampler: sampler;

// A full-screen triangle, not a quad: three vertices rather than six, and no diagonal seam across
// the middle where two triangles meet. The same trick `post.wgsl` uses.
struct Varying {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Varying {
    var out: Varying;
    let x = f32(i32(index) / 2) * 4.0 - 1.0;
    let y = f32(i32(index) & 1) * 4.0 - 1.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    // Screen space runs downwards where clip space runs up, which is the one flip in this file.
    out.uv = vec2<f32>((x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5);
    return out;
}

// How many samples the spiral takes.
//
// **Sixteen, and it is a floor rather than a ceiling.** Below about twelve the noise survives the
// blur as visible clumping; above about twenty-four the picture stops changing. This is the number
// Alchemy's own paper settles on and the number Unity's URP ships as its "high" setting.
const SAMPLES: i32 = 16;

// How many turns the spiral makes over those samples.
//
// Co-prime-ish with the sample count on purpose: a spiral whose turns divide evenly into its samples
// puts every sample on one of a handful of rays, which is a star rather than a disc.
const TURNS: f32 = 7.0;

// Turn a depth-buffer reading into a distance along the view axis, in world units.
//
// The depth buffer stores a **hyperbolic** function of distance — most of its precision sits near
// the camera, which is the whole reason it works — so this is not a lerp between the planes. It is
// the algebraic inverse of the projection `Mat4::perspective` builds:
//
//     z_clip = z_view · far/(near−far) + near·far/(near−far),  w_clip = −z_view
//
// which rearranges to what is below. `z = 0` gives exactly `near` and `z = 1` gives exactly `far`,
// and both are worth checking by hand when this is ever changed.
fn linear_depth(z: f32) -> f32 {
    let near = view.clip_params.x;
    let far = view.clip_params.y;
    return (near * far) / (z * (near - far) + far);
}

// Where a pixel is in the world, from its depth.
//
// `sky_right`, `sky_up` and `sky_forward` already carry the field of view and the aspect ratio and
// are already in the uniform for the sky pass, so this needs no inverse projection matrix — which
// the engine does not have and would have to grow a general matrix inverse to get. `sky_forward` is
// a unit vector along the view axis and the other two are perpendicular to it, so the ray built here
// has a forward component of exactly one and scaling it by the linear depth lands on the surface.
fn world_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let ray = view.sky_right.xyz * ndc.x + view.sky_up.xyz * ndc.y + view.sky_forward.xyz;
    return view.eye.xyz + ray * linear_depth(depth);
}

// A per-pixel rotation with no visible pattern, from the fragment coordinate alone.
//
// Interleaved gradient noise, which Jorge Jimenez introduced for exactly this job at SIGGRAPH 2014
// and which Frostbite, Unreal and CryEngine all use. It is one dot product and one fract: cheaper
// than a texture lookup, needs no texture bound, and gives a value whose neighbours differ strongly
// — which is what a blur needs, because a blur can only average away variation that is actually
// there between adjacent pixels.
fn dither(position: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(position, vec2<f32>(0.06711056, 0.00583715))));
}

@fragment
fn measure(in: Varying) -> @location(0) f32 {
    let intensity = view.occlusion_params.x;
    // **No early return on zero intensity, unlike fog**, and the reason is the derivative below
    // rather than a preference: any branch above a `dpdx` puts it in control flow FXC refuses. It
    // costs nothing to leave out — intensity multiplies straight into the result at the end, so zero
    // gives exactly 1.0 and off is still off exactly. Nothing declares these passes at zero
    // intensity anyway; the graph never creates them.

    let size = vec2<f32>(textureDimensions(depth_map));
    let texel = vec2<i32>(in.uv * size);
    let depth = textureLoad(depth_map, texel, 0);

    let origin = world_position(in.uv, depth);

    // **The normal comes from the depth buffer's own derivatives, not from a normal buffer.**
    //
    // `dpdx`/`dpdy` of the reconstructed position are two vectors lying in the surface, so their
    // cross product is perpendicular to it. This is exact on a flat surface and wrong across a
    // silhouette, where the two neighbours belong to different objects — which shows as a one-pixel
    // rim the blur then softens.
    //
    // The alternative is a second render target in the prepass carrying real normals. It is more
    // correct, and it doubles the prepass's bandwidth for an artefact one pixel wide.
    //
    // **Computed here, above every early return, and that is not tidiness.** A derivative asks the
    // hardware what the neighbouring pixels in the same quad hold, so it is only defined where every
    // pixel of the quad reaches the same instruction. Behind the sky test below it would be in
    // varying control flow, which is undefined in the specification and a hard compile error under
    // FXC — the compiler `docs/07` records as far stricter than the one a real GPU uses, and the one
    // Windows CI runs.
    let normal = normalize(cross(dpdx(origin), dpdy(origin)));

    // Nothing was drawn here — this is sky. Sky is not occluded by anything and, more importantly,
    // its reconstructed position is at the far plane, which would make it occlude every real surface
    // near the edge of the frame.
    if depth >= 1.0 {
        return 1.0;
    }

    let radius = view.occlusion_params.y;
    let bias = view.occlusion_params.z;

    // The spiral's starting angle, different at every pixel.
    let angle = dither(in.clip_position.xy) * 6.28318530718;

    var occlusion = 0.0;
    for (var index = 0; index < SAMPLES; index = index + 1) {
        // Radius grows as the square root of the index, which spreads the samples evenly over the
        // *area* of the disc. Growing it linearly clusters them at the centre, so half the samples
        // measure nearly the same point.
        let fraction = (f32(index) + 0.5) / f32(SAMPLES);
        let spin = angle + fraction * TURNS * 6.28318530718;
        let distance = sqrt(fraction) * radius;

        // The offset is built in world units and projected back to the screen through the same ray
        // basis, which keeps the radius a property of the scene rather than of the window size — so
        // a corner does not get darker when somebody resizes the game.
        let offset = vec2<f32>(cos(spin), sin(spin)) * distance;

        // Screen-space step for that world offset at this depth. Dividing by the view distance is
        // the perspective: the same world radius covers fewer pixels further away.
        let view_distance = linear_depth(depth);
        let scale = vec2<f32>(
            1.0 / max(length(view.sky_right.xyz), 1e-4),
            1.0 / max(length(view.sky_up.xyz), 1e-4)
        ) * 0.5 / max(view_distance, 1e-4);
        let sample_uv = in.uv + vec2<f32>(offset.x * scale.x, -offset.y * scale.y);

        if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
            continue;
        }

        let sample_texel = vec2<i32>(sample_uv * size);
        let sample_depth = textureLoad(depth_map, sample_texel, 0);
        if sample_depth >= 1.0 {
            continue;
        }

        let towards = world_position(sample_uv, sample_depth) - origin;
        let along = dot(towards, normal);
        let span = length(towards);

        // **The Alchemy estimator.** How far the sample sits above this surface, over how far away
        // it is — a sample directly overhead blocks a lot of sky, one out at the same height near
        // the horizon blocks very little. The bias subtracted from the numerator is the
        // self-occlusion guard: without it, the depth noise across one flat surface reads as
        // occlusion and the wall bands.
        let contribution = max(along - bias, 0.0) / (span * span + 1e-4);

        // Anything past the radius is a different surface entirely -- a far wall behind a near one --
        // and counting it would draw a dark halo around every silhouette. Faded rather than cut, so
        // the boundary itself does not become an edge.
        let falloff = clamp(1.0 - (span * span) / (radius * radius), 0.0, 1.0);
        occlusion = occlusion + contribution * falloff;
    }

    // `2σ/N`, which is the Alchemy paper's own normalisation with `intensity` standing in for σ.
    //
    // **It was `occlusion * radius * intensity / SAMPLES` first, and that was wrong by about a
    // factor of four.** The reasoning was that `contribution` divides by a squared distance and so
    // carries units of 1/length, which a radius would cancel. It does — but it also throttled the
    // whole effect to a few per cent, and the first Atrium capture with occlusion on measured
    // **byte-identical** to the one without it at every probed pixel. Debugging that took three
    // renders: the prepass depth was right, the reconstructed normals were right, and the estimator
    // was quietly returning almost exactly one.
    //
    // Dropping the radius also makes the dial behave the way an author expects: a larger reach
    // gathers more occluders and therefore darkens more, rather than darkening less.
    let visibility = 1.0 - clamp(occlusion * 2.0 * intensity / f32(SAMPLES), 0.0, 1.0);
    return visibility;
}

@fragment
fn blur(in: Varying) -> @location(0) f32 {
    // A plain 4 x 4 box, which is what the dither pattern above is built to be averaged by: sixteen
    // neighbours carry sixteen different spiral rotations, so the average is the same estimate at
    // sixteen times the sample count.
    //
    // **Not depth-weighted, deliberately, and this is a real trade.** A bilateral blur would keep
    // occlusion from bleeding across a silhouette, at the cost of reading depth again per tap and of
    // a kernel that stops being separable. The bleed is a few pixels of an already soft quantity
    // multiplied into ambient light alone; the cost is per pixel of the frame. Revisit it if a
    // silhouette against a bright background ever shows a dark fringe.
    let size = vec2<f32>(textureDimensions(measured_map));
    let texel = 1.0 / size;

    var total = 0.0;
    for (var y = -2; y <= 1; y = y + 1) {
        for (var x = -2; x <= 1; x = x + 1) {
            // The half-texel offset makes the four-wide kernel symmetric about the pixel: without
            // it a run from -2 to 1 leans one texel to one side, which slides every contact shadow
            // half a pixel in the same direction.
            let at = in.uv + (vec2<f32>(f32(x), f32(y)) + 0.5) * texel;
            total = total + textureSample(measured_map, measured_sampler, at).r;
        }
    }
    return total / 16.0;
}
