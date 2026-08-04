// Instanced 3D meshes with one directional light.
//
// Unlike quad.wgsl and sprite.wgsl, this pipeline has a real vertex buffer: geometry comes from a
// mesh asset (ADR 0035) rather than from four corners derivable from the vertex index. It is the
// first thing in this backend that needs one.
//
// # Shading is diffuse, not PBR, and that is on purpose
//
// The material already carries the metallic-roughness fields glTF defines (ADR 0033), and nothing
// here reads metallic or roughness yet. Getting geometry, depth, projection and lighting onto the
// screen is one problem; getting the reflectance maths right is another, and mixing them means a
// wrong picture with two candidate causes. RenderBackend isolates this file completely, so upgrading
// it to PBR later is the cheap change four ADRs have found it to be.

struct MeshView {
    // World to clip space: the camera's inverse transform, then its projection.
    view_projection: mat4x4<f32>,
    // xyz = the direction light travels, normalised. w unused.
    light_direction: vec4<f32>,
    // rgb = light colour with intensity already folded in. a unused.
    light_colour: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: MeshView;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// One per instance: where this copy of the mesh sits, and what it is made of. The model matrix
// arrives as four vec4s because a vertex attribute cannot be a matrix.
struct InstanceInput {
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(7) base_colour: vec4<f32>,
    // rgb = emissive. a unused.
    @location(8) emissive: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) base_colour: vec4<f32>,
    @location(2) emissive: vec3<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );

    let world = model * vec4<f32>(vertex.position, 1.0);

    var out: VertexOutput;
    out.clip_position = view.view_projection * world;
    // Normals are rotated by the model's basis, NOT by the full matrix -- the fourth column is
    // translation, and moving a direction is meaningless.
    //
    // This is correct for uniform scale and wrong for non-uniform scale, which needs the inverse
    // transpose. Left as is deliberately: the fix belongs with the instance data (a normal matrix
    // per instance) rather than here, and nothing authors a non-uniformly scaled mesh yet.
    // Re-normalised in the fragment stage, so a uniform scale of any size still lights correctly.
    let basis = mat3x3<f32>(
        instance.model_0.xyz,
        instance.model_1.xyz,
        instance.model_2.xyz,
    );
    out.normal = basis * vertex.normal;
    out.base_colour = instance.base_colour;
    out.emissive = instance.emissive.rgb;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Interpolating a normal across a triangle shortens it, so it is re-normalised here rather than
    // in the vertex stage. This also absorbs whatever uniform scale the model matrix applied.
    let normal = normalize(in.normal);

    // Light travels along `light_direction`, so the vector *towards* the light is its negative.
    // max() rather than abs(): a surface facing away is in shadow, not lit from behind.
    let towards_light = -normalize(view.light_direction.xyz);
    let lambert = max(dot(normal, towards_light), 0.0);

    // A small ambient term so an unlit face is dark rather than pure black. Not a lighting model --
    // it is a stand-in until there is something better, and it is deliberately small enough to read
    // as "in shadow" rather than as "flat".
    let ambient = 0.03;

    let lit = in.base_colour.rgb * (view.light_colour.rgb * lambert + vec3<f32>(ambient));
    // Emissive is added rather than multiplied, and is not affected by the light -- that is what
    // makes it emissive. Above 1.0 it pushes into the HDR range the post pass tonemaps (ADR 0034).
    return vec4<f32>(lit + in.emissive, in.base_colour.a);
}
