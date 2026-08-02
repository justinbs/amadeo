// Instanced textured sprites.
//
// One draw call per batch, where a batch is every sprite sharing a sort order and a texture
// (ADR 0023). Like quad.wgsl there is no vertex or index buffer: the four corners come from the
// vertex index, so the only per-frame upload is the instance data.
//
// The camera bind group (group 0) is shared with quad.wgsl. Group 1 is this pipeline's own, and is
// rebound once per batch -- which is the state change batching exists to minimise.

struct Camera {
    // World-space point at the centre of the view.
    center: vec2<f32>,
    // Half the visible world size. Dividing by this maps world space to -1..1 clip space.
    half_extents: vec2<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

struct InstanceInput {
    // xy = world centre, zw = the x axis: the sprite's full width in world space, already carrying
    // every parent's rotation and scale.
    @location(0) center_axis_x: vec4<f32>,
    // xy = the y axis. zw is padding, to keep the 16-byte alignment WGSL and the vertex layout
    // both require.
    @location(1) axis_y: vec4<f32>,
    // Linear RGBA, multiplied into the sampled texel.
    @location(2) color: vec4<f32>,
    // Sub-rectangle of the texture as [x, y, width, height], each in 0..1. This is what makes a
    // tilesheet work: every tile is one region of one shared texture, so they all batch together.
    @location(3) region: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    // Four vertices drawn as a triangle strip. The index's low two bits give the corner directly:
    //   0 -> (0,0)   1 -> (1,0)   2 -> (0,1)   3 -> (1,1)
    let corner = vec2<f32>(
        f32(vertex_index & 1u),
        f32((vertex_index >> 1u) & 1u),
    );
    // Recentred to -0.5..0.5, so the axes below are full extents halved by the offsets -- the same
    // convention quad.wgsl uses.
    let local = corner - vec2<f32>(0.5, 0.5);

    let center = instance.center_axis_x.xy;
    let axis_x = instance.center_axis_x.zw;
    let axis_y = instance.axis_y.xy;

    // No trigonometry in either direction: the CPU handed over the axes rather than a size and an
    // angle precisely so this is two multiplies and an add. See `SpriteInstance` for the
    // measurement behind that.
    let world = center + axis_x * local.x + axis_y * local.y;
    let ndc = (world - camera.center) / camera.half_extents;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);

    // World space has +Y upward; texture space has v = 0 on the *top* row. So the vertical
    // coordinate is flipped here. Getting this backwards renders every sprite upside down, which is
    // obvious on a photograph and invisible on a symmetrical test pattern -- hence
    // `a_sprite_is_not_drawn_upside_down` in the backend tests.
    out.uv = instance.region.xy + vec2<f32>(corner.x, 1.0 - corner.y) * instance.region.zw;
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Multiply rather than replace, so `Sprite::color` tints. White leaves the texture untouched,
    // which is why it is the default.
    return textureSample(sprite_texture, sprite_sampler, in.uv) * in.color;
}
