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

// The material's base colour texture, multiplied into `base_colour` (ADR 0033).
//
// Always bound, like the shadow map above and for the same reason: a material naming no texture gets
// a 1×1 opaque **white** placeholder, and white is the identity of the multiply — so binding it is
// arithmetically the same as not sampling, and there is one pipeline rather than a textured one and
// an untextured one that can drift apart.
//
// White rather than the magenta `TextureCache` placeholder, which means "an asset is missing and you
// should notice". An untextured material is not missing anything.
@group(2) @binding(0) var base_colour_map: texture_2d<f32>;
@group(2) @binding(1) var base_colour_sampler: sampler;

// The normal map (ADR 0047), sharing the sampler above because it is sampled at the same coordinate
// on the same surface and wants the same repeat, filter and mip behaviour.
//
// Always bound, on the same argument as the two above: a material naming no normal map gets a 1×1
// (128, 128, 255) placeholder, which decodes to (0, 0, 1) — "leaning nowhere" — and leaves the
// geometric normal exactly as it was.
//
// **Uploaded as a linear format, not sRGB.** These bytes are a direction rather than a colour, and
// sampling them through the sRGB curve would bend every one of them.
@group(2) @binding(2) var normal_map: texture_2d<f32>;

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
    // xyz = the direction the texture's u axis runs across the surface, w = ±1 handedness.
    // Location 9 rather than 3 because 3..8 belong to the instance buffer; the two share one
    // namespace. See the pipeline's vertex layout.
    @location(9) tangent: vec4<f32>,
};

// One per instance: where this copy of the mesh sits, and what it is made of. The model matrix
// arrives as four vec4s because a vertex attribute cannot be a matrix.
struct InstanceInput {
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(7) base_colour: vec4<f32>,
    // rgb = emissive, a = normal_strength. The alpha channel was spare and a normal map's strength
    // is one float, so it rides along rather than growing the instance by another sixteen bytes.
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
    @location(4) uv: vec2<f32>,
    // The tangent in world space, with its handedness carried through untouched in w. Rotated by the
    // model's basis exactly as the normal is, and re-normalised per pixel for the same reason.
    @location(5) tangent: vec4<f32>,
    @location(6) normal_strength: f32,
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
    // The same basis, because a tangent is a direction lying in the surface and moves with it. The
    // handedness in w is a sign rather than a direction, so it passes through untransformed -- a
    // negatively scaled model would flip it, which nothing authors and which the normal above has
    // the same gap for.
    out.tangent = vec4<f32>(basis * vertex.tangent.xyz, vertex.tangent.w);
    out.base_colour = instance.base_colour;
    out.emissive = instance.emissive.rgb;
    out.normal_strength = instance.emissive.a;
    out.world_position = world.xyz;
    out.uv = vertex.uv;
    return out;
}

// The surface normal at this pixel, after the normal map has had its say.
//
// # What a normal map is doing
//
// The image stores, per pixel, which way the surface leans -- as if the flat triangle were finely
// bumpy. Lighting that per-pixel direction instead of the triangle's own is what puts mortar grooves
// between bricks and grain in wood without a single extra vertex.
//
// The directions are stored **in tangent space**: relative to the surface, not to the world. That is
// what makes one image tile across a curved wall -- "lean left" means the same thing everywhere on
// the surface, where a world direction would not. Converting one to the other is what the tangent
// frame is for.
fn shade_normal(in: VertexOutput) -> vec3<f32> {
    // Interpolation across a triangle shortens both vectors, so both are re-normalised here.
    let normal = normalize(in.normal);
    let tangent = normalize(in.tangent.xyz);

    // Sampled and decoded from 0..1 to -1..1. A flat pixel is (0.5, 0.5, 1.0) stored, which comes
    // out as (0, 0, 1): straight along the normal, changing nothing.
    let sampled = textureSample(normal_map, base_colour_sampler, in.uv).xyz * 2.0 - 1.0;

    // Strength scales the sideways lean and leaves z alone, so 0.0 is exactly the flat surface and
    // there is no value at which this degenerates. Scaling all three would only rescale a vector
    // that is about to be normalised, which would do nothing at all.
    let leaning = vec3<f32>(sampled.xy * in.normal_strength, sampled.z);

    // Gram-Schmidt again, per pixel: interpolating a tangent across a triangle can leave it slightly
    // out of the surface, and the frame has to be square or it shears the direction it maps.
    let square_tangent = normalize(tangent - normal * dot(normal, tangent));
    let bitangent = cross(normal, square_tangent) * in.tangent.w;

    // Tangent space to world space. The columns are where each tangent-space axis points in the
    // world, which is exactly what the three vectors are.
    let to_world = mat3x3<f32>(square_tangent, bitangent, normal);
    return normalize(to_world * leaning);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // The geometric normal bent by the normal map — see `shade_normal`. Re-normalising the
    // interpolated inputs happens in there, which is also what absorbs whatever uniform scale the
    // model matrix applied.
    let normal = shade_normal(in);

    // Light travels along `light_direction`, so the vector *towards* the light is its negative.
    // max() rather than abs(): a surface facing away is in shadow, not lit from behind.
    let towards_light = -normalize(view.light_direction.xyz);
    let lambert = max(dot(normal, towards_light), 0.0);

    // A small ambient term so an unlit surface is dark rather than pure black. Not a lighting model
    // -- it is a stand-in until there is something better.
    //
    // **Raised from 0.03 to 0.12 when shadows arrived**, and the reason is worth keeping. Before
    // shadow maps, the only ambient-only pixels were faces turned away from the light, which are
    // small and read fine as near-black. With shadows, whole areas of *floor* are ambient-only, and
    // at 0.03 they came out as holes in the world rather than as shade. Real outdoor shadow is
    // roughly a tenth to a fifth of direct sun, because the sky is also a light -- which is what
    // this is standing in for, and what should eventually replace it as an authored sky colour on
    // `Environment`.
    let ambient = 0.12;

    // Shadow multiplies the *direct* light only. Ambient is deliberately left out of it: a surface
    // in shadow is still lit by the sky, and multiplying ambient too would make every shadow a
    // silhouette of pure black rather than something you can still see into.
    //
    // **Fed the geometric normal, not the mapped one.** The slope-scaled bias exists to match how
    // much depth one shadow-map texel spans, and the shadow map was drawn from the *geometry* --
    // a normal map does not move a single triangle. Using the bumpy normal would vary the bias
    // pixel to pixel across a flat wall and speckle it with acne.
    let geometric_lambert = max(dot(normalize(in.normal), towards_light), 0.0);
    let shadow = shadow_factor(in.world_position, geometric_lambert);

    // The material's base colour times its texture. Multiplied rather than replaced, which is what
    // glTF's metallic-roughness model specifies and what ADR 0033 followed: the texture carries the
    // pattern and `base_colour` tints it, so one stone texture serves grey stone and red stone
    // without a second image. An untextured material samples white and is unchanged.
    let sampled = textureSample(base_colour_map, base_colour_sampler, in.uv);
    let albedo = in.base_colour * sampled;

    let lit = albedo.rgb * (view.light_colour.rgb * lambert * shadow + vec3<f32>(ambient));
    // Emissive is added rather than multiplied, and is not affected by the light -- that is what
    // makes it emissive. Above 1.0 it pushes into the HDR range the post pass tonemaps (ADR 0034).
    return vec4<f32>(lit + in.emissive, albedo.a);
}
