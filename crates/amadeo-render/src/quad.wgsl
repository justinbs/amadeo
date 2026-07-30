// Instanced coloured quads.
//
// One draw call renders every quad in the frame. Each instance carries its own centre, size,
// rotation and colour; the vertex shader builds the four corners from the vertex index, so there is
// no vertex or index buffer to maintain at all.

struct Camera {
    // World-space point at the centre of the view.
    center: vec2<f32>,
    // Half the visible world size. Dividing by this maps world space to -1..1 clip space.
    half_extents: vec2<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct InstanceInput {
    // xy = world centre, zw = full world size.
    @location(0) center_size: vec4<f32>,
    // x = rotation in radians. The remaining three are padding to keep a 16-byte alignment, which
    // both WGSL and the vertex buffer layout require.
    @location(1) rotation: vec4<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    // Four vertices drawn as a triangle strip. The index's low two bits give the corner directly:
    //   0 -> (0,0)   1 -> (1,0)   2 -> (0,1)   3 -> (1,1)
    // Subtracting 0.5 centres the quad on its origin.
    let corner = vec2<f32>(
        f32(vertex_index & 1u) - 0.5,
        f32((vertex_index >> 1u) & 1u) - 0.5,
    );

    let local = corner * instance.center_size.zw;

    let angle = instance.rotation.x;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let rotated = vec2<f32>(
        local.x * cos_a - local.y * sin_a,
        local.x * sin_a + local.y * cos_a,
    );

    let world = instance.center_size.xy + rotated;
    let ndc = (world - camera.center) / camera.half_extents;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
