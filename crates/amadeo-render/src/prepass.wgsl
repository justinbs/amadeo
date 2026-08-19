// The depth prepass: where the camera's geometry is, and nothing else — ADR 0083.
//
// `view.wgsl` is prepended to this at pipeline creation, so `MeshView` and `view` come from the one
// declaration every pipeline that reads that buffer shares.
//
// # Why this exists at all
//
// Ambient occlusion multiplies the *ambient* term while a surface is being shaded (ADR 0083), so it
// has to be known before shading starts. The view pass writes depth and shades in one pass, which
// means by the time the scene depth buffer exists the shading is already done. Depth therefore has
// to be laid down once on its own first — that is the whole content of this file.
//
// # Why it is not `shadow.wgsl` with a different uniform
//
// The two are structurally identical and differ in the one thing a vertex shader here does: which
// matrix takes a vertex to clip space. Sharing them would mean a uniform that is sometimes a light's
// and sometimes a camera's, and `view.wgsl` exists precisely because that kind of sharing drifted
// once and drew a sky facing the wrong way with nothing failing.
//
// It also has a real side benefit nothing depends on yet: a depth buffer already laid down lets the
// shading pass reject occluded fragments before running the whole PBR chain on them. Worth knowing
// before anyone concludes this pass is pure cost.

struct VertexInput {
    @location(0) position: vec3<f32>,
    // Declared and unused. The vertex buffer layout is shared with the mesh and shadow pipelines, so
    // the attributes have to line up; naming them is what keeps that correspondence readable.
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

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
    return view.view_projection * model * vec4<f32>(vertex.position, 1.0);
}
