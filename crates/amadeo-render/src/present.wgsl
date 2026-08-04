// The present pass: puts the finished off-screen image onto the frame's destination.
//
// Every camera draws into a transient image rather than straight at the window, for the two reasons
// `graph::frame_graph` sets out -- it is where post-processing will be inserted, and it is what lets
// a *windowed* run capture, since a window's own image cannot be read back.
//
// This is also the pass where the destination's format is met. The transient is always RGBA
// (graph::TargetFormat), while a window surface is frequently BGRA; the hardware swizzles while
// writing, so nothing here has to know which it is.
//
// When tonemapping arrives this is the shader it goes in, since it is already the one step that runs
// once over every pixel of the finished picture.

// No vertex buffer and no instance buffer. Three vertices are generated from the vertex index alone.

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // One oversized triangle rather than two forming a quad. Both cover the screen; a single
    // triangle has no diagonal seam down the middle, where the two halves can disagree by a rounding
    // error, and it is one fewer vertex. The three UVs are (0,0), (2,0), (0,2), so the part of the
    // triangle that lands on screen covers exactly 0..1 in both directions and the rest is clipped.
    let uv = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );

    var out: VertexOutput;
    // UV has v = 0 at the top while clip space has y = +1 at the top, so y is flipped here. This is
    // the same trap as sprite.wgsl's `1.0 - corner.y`, and getting it wrong flips the whole screen
    // -- which `capture` would show immediately.
    out.clip_position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // A straight copy today. Sampling is 1:1 -- the transient is the destination's size -- so the
    // nearest-neighbour sampler this shares with sprites reads exactly one texel per pixel.
    return textureSample(source_texture, source_sampler, in.uv);
}
