// The post pass: applies a camera's Environment (ADR 0034) and brings the picture into a range a
// screen can show.
//
// Reads the high-dynamic-range scene image and writes a displayable one. Everything before this
// pass can be brighter than white; nothing after it can.
//
// # The order here is the engine's, and it is not arbitrary
//
// Exposure scales light before anything looks at it. Bloom needs values still above the display
// range. Tonemapping is what collapses that range. Grading and vignetting are corrections applied to
// the result. ADR 0034 §4 records why content is not allowed to reorder these: a scene file that put
// tonemapping first would mostly produce wrong pictures and would have no way to say so.
//
// Bloom's *blur* is not in this shader, on purpose -- it is a multi-pass job rather than arithmetic
// on one pixel, so it lives in `bloom.wgsl` and its own graph passes. What is here is the one line
// that belongs here: adding the finished blur back, between exposure and tonemapping, so that the
// glow is part of what the tonemap curve compresses rather than something painted over the top of it.
//
// `Post` and `post` come from `post_uniform.wgsl`, prepended at pipeline creation.

// Must match `Tonemap`'s declaration order in environment.rs. A mismatch would silently apply the
// wrong curve, so `tonemap_indices_match_the_shader` in the backend tests pins it.
const TONEMAP_NONE: f32 = 0.0;
const TONEMAP_REINHARD: f32 = 1.0;
const TONEMAP_ACES: f32 = 2.0;

@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;

// The blurred bright parts, at half resolution — sampled back up, which the bilinear filter smooths
// for free.
//
// **Always bound**, like the shadow map and the base-colour texture: when bloom is off, a 1×1 black
// placeholder stands in. Black is the identity of an addition, so binding it is arithmetically the
// same as not sampling, and there is one post pipeline rather than a bloomed one and a plain one
// that can drift apart.
@group(2) @binding(0) var bloom_texture: texture_2d<f32>;
@group(2) @binding(1) var bloom_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // The same oversized triangle present.wgsl uses, and the same y flip. See that file for why one
    // triangle rather than two.
    let uv = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );

    var out: VertexOutput;
    out.clip_position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

// c / (1 + c). Never clips, and desaturates bright areas noticeably.
fn reinhard(color: vec3<f32>) -> vec3<f32> {
    return color / (1.0 + color);
}

// Narkowicz's approximation of the ACES filmic curve -- the standard cheap stand-in for the full
// transform, which is several matrix multiplies and not worth it here. Holds highlight detail and
// keeps colour better than Reinhard.
fn aces_filmic(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((color * (a * color + b)) / (color * (c * color + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(scene_texture, scene_sampler, in.uv);
    var color = source.rgb;

    // 1. Exposure, on linear light, before anything else looks at it.
    color = color * post.controls.x;

    // 2. Bloom, added out of its own passes (`bloom.wgsl`).
    //
    //    **Added before the tonemap, not after**, and that is the whole reason the scene target is
    //    high dynamic range. A glow added afterwards sits on top of an already-compressed picture and
    //    reads as a grey wash; added here it is light, so the curve below compresses it along with
    //    everything else and a bright glow blows out the way a bright thing does.
    //
    //    The bright pass already applied exposure, so this is not scaled again.
    color = color + textureSample(bloom_texture, bloom_sampler, in.uv).rgb * post.bloom.y;

    // 3. Tonemap. `None` clamps, which is what an 8-bit target used to do implicitly -- so the
    //    default environment produces exactly the picture this renderer drew before ADR 0034.
    // Named `curve` rather than `operator`, which is a reserved word in WGSL.
    let curve = post.controls.y;
    if (curve == TONEMAP_REINHARD) {
        color = reinhard(color);
    } else if (curve == TONEMAP_ACES) {
        color = aces_filmic(color);
    } else {
        color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    }

    // 4. Grade: contrast about mid-grey, then saturation about luminance, then tint.
    //
    //    Contrast pivots on 0.5 rather than 0.0 because pivoting on black would only ever darken.
    color = (color - vec3<f32>(0.5)) * post.grade.x + vec3<f32>(0.5);

    //    Rec. 709 luminance weights -- the standard perceptual greens-matter-most mix, not an even
    //    third each, which would turn a saturated red and a saturated green into the same grey.
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luma), color, post.grade.y);

    color = color * post.tint.rgb;

    // 5. Vignette, last, because it is about *where* a pixel is rather than what colour it is.
    //    Distance from the centre in units of the half-diagonal, so the corners reach 1.0 whatever
    //    the aspect ratio -- otherwise a wide window would darken its sides and not its corners.
    let centred = (in.uv - vec2<f32>(0.5)) * 2.0;
    let distance = length(centred) / 1.41421356;
    let falloff = smoothstep(post.controls.w, 1.0, distance);
    color = color * (1.0 - falloff * post.controls.z);

    // Clamped again: grading and tinting can push a value back above 1.0 after the tonemap brought
    // it down, and writing that to an 8-bit target would wrap rather than saturate on some backends.
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), source.a);
}
