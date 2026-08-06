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
    // World to the *light's* clip space, for looking up the shadow map (ADR 0038).
    light_view_projection: mat4x4<f32>,
    // xyz = the direction light travels, normalised. w unused.
    light_direction: vec4<f32>,
    // rgb = light colour with intensity already folded in. a unused.
    light_colour: vec4<f32>,
    // x = depth bias, y = one shadow-map texel in UV, z = 1 when a real shadow map is bound.
    shadow_params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: MeshView;

// The shadow map, and a sampler that *compares* rather than returning a value.
//
// `texture_depth_2d` with a `sampler_comparison` is what makes hardware PCF available: rather than
// reading a depth and comparing it here, `textureSampleCompare` does four comparisons across the
// neighbouring texels and returns how many passed. That is a soft edge for the price of one sample,
// and it is why the sampler is declared this way rather than as an ordinary one.
//
// Always bound, even with shadows off — a 1×1 placeholder stands in, so there is one pipeline rather
// than two. `shadow_params.z` is what tells the difference.
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

// How much light reaches this point: 1.0 in full light, 0.0 fully shadowed.
fn shadow_factor(world: vec3<f32>, lambert: f32) -> f32 {
    if view.shadow_params.z < 0.5 {
        return 1.0;
    }

    let light_clip = view.light_view_projection * vec4<f32>(world, 1.0);
    // The light's projection is orthographic, so w is always 1 and there is no perspective divide to
    // do. Dividing anyway would be harmless and would also imply this works for a spot light, which
    // it does not yet.
    let projected = light_clip.xyz;

    // Clip space is -1..1 across and 0..1 deep; the texture is 0..1 across with v running downward.
    var uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);

    // Outside the shadow map's box there is no information, so nothing is shadowed. Treating
    // "outside" as shadowed would put a hard dark edge at the shadow distance, which reads as a wall
    // of darkness following the player around.
    if projected.z > 1.0 || uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }

    // A slope-scaled bias: a surface seen edge-on by the light spans much more depth across one
    // shadow-map texel than one facing it square, so it needs proportionally more offset. Using one
    // flat bias for both is what forces the choice between acne on slopes and peter-panning on flat
    // ground -- this needs neither.
    let slope = clamp(1.0 - lambert, 0.0, 1.0);
    let bias = view.shadow_params.x * (1.0 + slope * 3.0);
    let reference = projected.z - bias;

    // Three by three comparisons rather than one, each already hardware-filtered across four texels.
    // One sample gives a stair-stepped edge that follows the shadow map's pixels rather than the
    // geometry; this softens it to something that reads as a shadow.
    let texel = view.shadow_params.y;
    var total = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            total = total + textureSampleCompare(shadow_map, shadow_sampler, uv + offset, reference);
        }
    }
    return total / 9.0;
}

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
    // Carried through so the fragment stage can look this point up in the shadow map. World space
    // rather than light-clip space: interpolating world position and transforming per pixel is one
    // matrix multiply, and it keeps the light matrix in exactly one place.
    @location(3) world_position: vec3<f32>,
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
    out.world_position = world.xyz;
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

    // Shadow multiplies the *direct* light only. Ambient is deliberately left out of it: a surface
    // in shadow is still lit by the sky, and multiplying ambient too would make every shadow a
    // silhouette of pure black rather than something you can still see into.
    let shadow = shadow_factor(in.world_position, lambert);

    let lit = in.base_colour.rgb * (view.light_colour.rgb * lambert * shadow + vec3<f32>(ambient));
    // Emissive is added rather than multiplied, and is not affected by the light -- that is what
    // makes it emissive. Above 1.0 it pushes into the HDR range the post pass tonemaps (ADR 0034).
    return vec4<f32>(lit + in.emissive, in.base_colour.a);
}
