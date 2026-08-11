// Bloom: light bleeding out of the bright parts of an image — ADR 0034's declared-but-undrawn effect.
//
// # Why it is three passes rather than three lines in post.wgsl
//
// Every other effect in the post chain is arithmetic on one pixel: exposure scales it, tonemapping
// curves it, grading corrects it. Bloom is the one that needs to know about its *neighbours*, over a
// radius wide enough to read as a glow — which no amount of work at one pixel can produce.
//
// So it is a **bright pass** that keeps only what is brighter than the threshold, then a **separable
// blur** — horizontal, then vertical — and `post.wgsl` adds the result back. Two 1D blurs cost 2n
// samples where one 2D blur of the same radius costs n², which is why every engine does it this way.
//
// # Why it works at half resolution
//
// The output is a wide, soft glow: it has no detail to lose. Halving each axis quarters the work,
// and the bilinear filtering on the way back up is free smoothing on top of the blur. This is the
// standard arrangement and the reason bloom is cheap enough to leave on.
//
// # How wide the glow is, and how to make it wider
//
// Nine taps on a half-resolution target reach four half-res texels, which is **eight
// full-resolution pixels** — a tight, bright halo rather than a broad atmospheric haze. That is a
// real limitation and it is the honest ceiling of one blur at one resolution.
//
// Widening it is **not** a matter of more taps: doubling the radius doubles the cost linearly and
// starts to band, because a nine-tap Gaussian stretched over thirty pixels is sampling a smooth
// function far too sparsely. The way every engine does it instead is a **downsample chain** — blur
// into a half, then a quarter, then an eighth, and add them back on the way up — which buys a radius
// that grows geometrically for a cost that shrinks geometrically. That is the next step here, and it
// is a change to these passes alone: nothing above `bloom.wgsl` knows how the glow was produced.
//
// # Why the bright pass applies exposure
//
// `post.wgsl` documents the order the engine fixes: exposure scales light *before* anything looks at
// it, and bloom needs values still above the display range. So "bright" has to mean bright after
// exposure — thresholding the raw scene would make the effect depend on a number the author set for
// a different reason, and a scene that dialled exposure down would still bloom.

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

// `Post` and `post` come from `post_uniform.wgsl`, prepended at pipeline creation.

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // The same oversized triangle every full-screen pass in this backend uses. See present.wgsl.
    let uv = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );

    var out: VertexOutput;
    out.clip_position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

// Keeps only what is brighter than the threshold, and keeps its colour.
//
// The subtraction is scaled rather than clamped: a pixel just over the threshold contributes a
// little and one far over contributes nearly all of itself, so there is no hard edge in the glow
// where the threshold falls. A plain `if brightness > threshold` gives a bloom with a visible
// outline, which reads as a bug rather than as light.
@fragment
fn fs_bright(in: VertexOutput) -> @location(0) vec4<f32> {
    let exposure = post.controls.x;
    let threshold = post.bloom.x;

    let colour = textureSample(source_texture, source_sampler, in.uv).rgb * exposure;

    // The brightest channel, rather than a luminance weighting. A saturated red light should bloom
    // as much as a white one of the same intensity; weighting by luminance would make it bloom about
    // a fifth as much, which is wrong for anything stylised and wrong for a warning light.
    let brightness = max(colour.r, max(colour.g, colour.b));
    let over = max(brightness - threshold, 0.0);
    let contribution = over / max(brightness, 0.0001);

    return vec4<f32>(colour * contribution, 1.0);
}

// A nine-tap Gaussian along one axis.
//
// The weights are a normalised Gaussian: they sum to one, so a uniform area keeps its brightness
// rather than getting darker or brighter each pass. Written out rather than computed because they
// are constants and a loop computing `exp` per pixel would be doing the same arithmetic every frame
// forever.
fn blur(uv: vec2<f32>, step: vec2<f32>) -> vec3<f32> {
    let w0 = 0.2270270270;
    let w1 = 0.1945945946;
    let w2 = 0.1216216216;
    let w3 = 0.0540540541;
    let w4 = 0.0162162162;

    var total = textureSample(source_texture, source_sampler, uv).rgb * w0;
    total = total + textureSample(source_texture, source_sampler, uv + step * 1.0).rgb * w1;
    total = total + textureSample(source_texture, source_sampler, uv - step * 1.0).rgb * w1;
    total = total + textureSample(source_texture, source_sampler, uv + step * 2.0).rgb * w2;
    total = total + textureSample(source_texture, source_sampler, uv - step * 2.0).rgb * w2;
    total = total + textureSample(source_texture, source_sampler, uv + step * 3.0).rgb * w3;
    total = total + textureSample(source_texture, source_sampler, uv - step * 3.0).rgb * w3;
    total = total + textureSample(source_texture, source_sampler, uv + step * 4.0).rgb * w4;
    total = total + textureSample(source_texture, source_sampler, uv - step * 4.0).rgb * w4;
    return total;
}

// Two entry points rather than one plus a direction uniform: the direction is a property of the
// *pass*, not of the frame, so making it data would mean a uniform written twice per frame with two
// different values and a second bind group to hold them. Two pipelines over one shader is the
// cheaper arrangement and the clearer one.
@fragment
fn fs_blur_horizontal(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = 1.0 / f32(textureDimensions(source_texture).x);
    return vec4<f32>(blur(in.uv, vec2<f32>(texel, 0.0)), 1.0);
}

@fragment
fn fs_blur_vertical(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = 1.0 / f32(textureDimensions(source_texture).y);
    return vec4<f32>(blur(in.uv, vec2<f32>(0.0, texel)), 1.0);
}
