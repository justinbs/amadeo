// Drawing the scene from a light's point of view, keeping only how far away things are — ADR 0038.
//
// # There is no fragment stage, and that is the whole point
//
// A shadow map stores depth and nothing else. The depth value is written by the fixed-function part
// of the pipeline from the clip position this vertex stage produces, so a fragment stage would have
// nothing to return. Leaving it out is not an optimisation; it is what the pass is.
//
// # Why it shares MeshView with mesh.wgsl
//
// The same uniform buffer at the same offset feeds both passes, so a view's light matrix is written
// once per frame rather than twice into two buffers that could disagree. This stage reads
// `light_view_projection` where the mesh stage reads `view_projection` — the same geometry, seen from
// two places.

struct MeshView {
    view_projection: mat4x4<f32>,
    light_view_projection: mat4x4<f32>,
    light_direction: vec4<f32>,
    light_colour: vec4<f32>,
    shadow_params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: MeshView;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Identical to mesh.wgsl's, because the shadow pass draws the same instance buffer. The colours are
// unused here and still declared: a vertex buffer layout has to match the shader that consumes it,
// and re-describing the buffer without them would mean maintaining two layouts for one buffer.
struct InstanceInput {
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(7) base_colour: vec4<f32>,
    @location(8) emissive: vec4<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );
    return view.light_view_projection * (model * vec4<f32>(vertex.position, 1.0));
}
