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
    // xyz = the camera's world position. Needed only since PBR: diffuse light looks the same from
    // everywhere, but a specular highlight is a reflection and moves with the viewer.
    eye: vec4<f32>,
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

// The metallic-roughness map (ADR 0048). **Green is roughness, blue is metallic**, which is glTF
// 2.0's packing rather than a choice made here — so an imported material maps straight across.
//
// Sampled values multiply the material's scalars, so the placeholder is white and a material without
// one is unchanged. Data rather than colour, so it wants `color_space = "linear"` in its sidecar.
@group(2) @binding(3) var metallic_roughness_map: texture_2d<f32>;

// The environment this view is lit by (ADR 0049) -- what replaced the hardcoded ambient constant.
//
// Two cube maps rather than one, because diffuse and specular need the environment blurred in
// completely different ways. `irradiance_map` holds, for each direction, the total light reaching a
// matte surface facing it. `specular_map` holds the environment blurred once per roughness level,
// as a mip chain, so a rough surface reads a blurrier level than a shiny one.
//
// Always bound: a camera naming no sky gets a uniform dim neutral, which is the same 0.12 that used
// to be written into this shader. One pipeline, no branch, and a game that asks for nothing shades
// as it always did.
@group(3) @binding(0) var irradiance_map: texture_cube<f32>;
@group(3) @binding(1) var specular_map: texture_cube<f32>;
@group(3) @binding(2) var environment_sampler: sampler;

// How many roughness levels `specular_map` holds. Must match `SPECULAR_LEVELS` in `ibl.rs`.
const SPECULAR_LEVELS: f32 = 6.0;

// How much of a reflection survives, and how white it goes, at this angle and roughness.
//
// The second half of Karis's split-sum: the first half is the prefiltered environment, this is the
// BRDF integrated over it. Unreal stores this in a lookup texture; this is Lazarov's analytic fit of
// the same function, which is four instructions and saves both a texture and a binding.
//
// Returns a scale and a bias to apply to the surface's reflectance -- `f0 * scale + bias` -- which is
// what makes a surface go mirror-bright at a glancing angle regardless of what colour it is.
fn environment_brdf(n_dot_v: f32, roughness: f32) -> vec2<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = vec4<f32>(roughness, roughness, roughness, roughness) * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * n_dot_v)) * r.x + r.y;
    return vec2<f32>(-1.04, 1.04) * a004 + vec2<f32>(r.z, r.w);
}

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
    // rgb = emissive. a unused.
    @location(8) emissive: vec4<f32>,
    // x = metallic, y = roughness, z = normal_strength. w unused.
    @location(10) surface: vec4<f32>,
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
    @location(7) metallic: f32,
    @location(8) roughness: f32,
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
    out.normal_strength = instance.surface.z;
    out.metallic = instance.surface.x;
    out.roughness = instance.surface.y;
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
fn shade_normal(in: VertexOutput, facing: f32) -> vec3<f32> {
    // Interpolation across a triangle shortens both vectors, so both are re-normalised here.
    //
    // `facing` is -1 on a back face, which is what makes two-sided geometry light correctly. Terrain
    // is an open surface with no underside, so standing beneath it you are looking at the *back* of
    // triangles whose normals point at the sky -- and without the flip the underside of the ground
    // would be lit as brightly as the top of it.
    let normal = normalize(in.normal) * facing;
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

const PI: f32 = 3.14159265359;

// How much light a rough surface scatters back towards the viewer -- the "microfacet distribution".
//
// The model (GGX / Trowbridge-Reitz) treats a surface as a field of microscopic mirrors. This says
// what fraction of them happen to be angled to bounce light from the source straight at the eye. A
// smooth surface has nearly all of them aligned, so the answer spikes hugely in one direction and
// that spike is a sharp highlight; a rough one spreads them out into a broad sheen.
//
// GGX rather than the older Blinn-Phong because of its *tail*: it falls off slowly away from the
// peak, which is what real measured materials do and what stops a highlight ending in a hard ring.
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    // Squared because artists author "perceptual" roughness -- a linear-feeling dial -- and the
    // maths wants the square. glTF specifies this squaring, so an imported value means the same
    // thing here as in Blender.
    let a = roughness * roughness;
    let a2 = a * a;
    let denominator = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * denominator * denominator, 1e-7);
}

// How much of that light is lost to the surface shadowing itself.
//
// At a glancing angle, microscopic bumps hide each other -- some catch light that never reaches the
// eye, some are in the shadow of their neighbours. Without this term a rough surface gets brighter
// than the light falling on it, which is unphysical and reads as a white rim on every edge.
//
// The height-correlated Smith form, which accounts for the two effects being related rather than
// independent. Returned already divided by the `4 * NoL * NoV` the specular term would otherwise
// need, which is why it is called visibility rather than geometry.
fn visibility_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let view = n_dot_l * sqrt(n_dot_v * n_dot_v * (1.0 - a2) + a2);
    let light = n_dot_v * sqrt(n_dot_l * n_dot_l * (1.0 - a2) + a2);
    return 0.5 / max(view + light, 1e-7);
}

// How reflective the surface is at this angle -- the Fresnel term.
//
// Everything becomes a mirror at a shallow enough angle. It is why a road looks wet in the distance
// and why you can see through water at your feet but not across a lake. `f0` is the reflectance
// looking straight down at the surface, and this raises it towards white as the angle flattens.
//
// Schlick's approximation: one power of five, accurate to within a percent or so of the real thing.
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    let factor = pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
    return f0 + (vec3<f32>(1.0) - f0) * factor;
}

@fragment
fn fs_main(
    in: VertexOutput,
    // Which side of the triangle is being shaded. The mesh pipeline culls nothing (ADR 0052), so a
    // surface with no underside -- terrain -- can be seen from beneath, and its normal has to be
    // turned round when it is.
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4<f32> {
    let facing = select(-1.0, 1.0, front_facing);

    // The geometric normal bent by the normal map — see `shade_normal`. Re-normalising the
    // interpolated inputs happens in there, which is also what absorbs whatever uniform scale the
    // model matrix applied.
    let normal = shade_normal(in, facing);

    // Light travels along `light_direction`, so the vector *towards* the light is its negative.
    // max() rather than abs(): a surface facing away is in shadow, not lit from behind.
    let towards_light = -normalize(view.light_direction.xyz);
    let lambert = max(dot(normal, towards_light), 0.0);

    // Towards the viewer, and the half vector between that and the light. The half vector is the
    // direction a microfacet would have to face to bounce this light straight into the eye, which
    // is what `distribution_ggx` is asking about.
    let towards_eye = normalize(view.eye.xyz - in.world_position);
    let half_vector = normalize(towards_light + towards_eye);
    let n_dot_v = max(dot(normal, towards_eye), 1e-4);
    let n_dot_h = max(dot(normal, half_vector), 0.0);
    let v_dot_h = max(dot(towards_eye, half_vector), 0.0);

    // **This used to be `let ambient = 0.12;`** -- one number, added to every surface regardless of
    // which way it faced or what was around it. That constant is why shadowed areas read as flat
    // grey holes and why a metal rendered black, and replacing it is what ADR 0045 called the single
    // biggest step towards looking like a real engine.
    //
    // Its history is worth keeping, because the number was not arbitrary: it started at 0.03 and was
    // raised to 0.12 when shadow maps landed. Before shadows, the only ambient-only pixels were
    // faces turned away from the light -- small, and fine as near-black. With shadows, whole areas of
    // *floor* became ambient-only, and at 0.03 they read as holes in the world rather than as shade.
    // The fix was to guess a brighter constant. The real answer was always that the sky is a light,
    // which is what this now reads instead. A camera naming no sky still gets exactly 0.12, from a
    // uniform neutral cube map, so nothing that relied on the old behaviour changed.

    // Shadow multiplies the *direct* light only. Ambient is deliberately left out of it: a surface
    // in shadow is still lit by the sky, and multiplying ambient too would make every shadow a
    // silhouette of pure black rather than something you can still see into.
    //
    // **Fed the geometric normal, not the mapped one.** The slope-scaled bias exists to match how
    // much depth one shadow-map texel spans, and the shadow map was drawn from the *geometry* --
    // a normal map does not move a single triangle. Using the bumpy normal would vary the bias
    // pixel to pixel across a flat wall and speckle it with acne.
    let geometric_lambert = max(dot(normalize(in.normal) * facing, towards_light), 0.0);
    let shadow = shadow_factor(in.world_position, geometric_lambert);

    // The material's base colour times its texture. Multiplied rather than replaced, which is what
    // glTF's metallic-roughness model specifies and what ADR 0033 followed: the texture carries the
    // pattern and `base_colour` tints it, so one stone texture serves grey stone and red stone
    // without a second image. An untextured material samples white and is unchanged.
    let sampled = textureSample(base_colour_map, base_colour_sampler, in.uv);
    let albedo = in.base_colour * sampled;

    // The metallic-roughness map multiplies the scalars, glTF's packing: green is roughness, blue is
    // metallic. The placeholder is white, so a material without one is exactly its scalars.
    let packed = textureSample(metallic_roughness_map, base_colour_sampler, in.uv);
    let metallic = clamp(in.metallic * packed.b, 0.0, 1.0);
    // Floored well above zero. A perfectly smooth surface concentrates its whole highlight into a
    // single point, which is infinitely bright and one blazing pixel wide -- it aliases horribly and
    // reads as a firefly rather than as polish.
    let roughness = clamp(in.roughness * packed.g, 0.04, 1.0);

    // **What metallic actually means**, and it is two changes at once rather than a dial:
    //
    // A metal has no diffuse colour at all. Light either reflects off it or is absorbed; nothing
    // scatters back out, which is why a gold bar has no "gold-coloured matte" to it. So the diffuse
    // term goes to zero as metallic rises.
    //
    // And a metal's *reflection* is tinted by its own colour, where a dielectric -- wood, stone,
    // plastic, skin -- reflects white regardless of what colour it is. That is the difference
    // between gold and yellow paint, and it is this one line.
    //
    // 0.04 is the reflectance of a typical dielectric looking straight on: about 4% of light bounces
    // off the surface of almost everything that is not a metal.
    let diffuse_colour = albedo.rgb * (1.0 - metallic);
    let f0 = mix(vec3<f32>(0.04), albedo.rgb, metallic);

    // The three terms, combined the way Cook-Torrance specifies.
    let distribution = distribution_ggx(n_dot_h, roughness);
    let visibility = visibility_smith(n_dot_v, lambert, roughness);
    let fresnel = fresnel_schlick(v_dot_h, f0);
    let specular = distribution * visibility * fresnel;

    // Energy conservation: light that reflected off the surface cannot also scatter through it. So
    // whatever Fresnel took goes to the highlight and the remainder is left for the diffuse.
    let diffuse = diffuse_colour * (vec3<f32>(1.0) - fresnel) / PI;

    // **The `PI` here is deliberate and is the reason existing scenes did not all go dark.**
    //
    // The BRDF above is energy-correct, which puts a `1 / PI` on the diffuse term. Applied
    // literally, every surface in the engine would drop to about a third of its previous brightness
    // and every authored `intensity` would need retuning. So `light_colour` is treated as carrying
    // `PI` times the irradiance -- the light's units absorb the constant, which is what most
    // real-time renderers do. The *relative* weighting of diffuse against specular, which is what
    // actually decides whether a material reads correctly, is unaffected.
    let direct = (diffuse + specular) * view.light_colour.rgb * lambert * shadow * PI;

    // **The ambient half, and it now has two parts where it used to have none.**
    //
    // Diffuse: the light a matte surface facing this way receives from the whole environment,
    // already summed into `irradiance_map` at load. A surface under a blue sky picks up blue; one
    // facing a red wall picks up red. This is what the flat constant could never do.
    let ambient_diffuse = diffuse_colour
        * textureSample(irradiance_map, environment_sampler, normal).rgb;

    // Specular: what the surface *reflects*. The direction is the view mirrored about the normal,
    // and the roughness picks how blurred a copy of the environment to read -- level 0 is a mirror,
    // the last level is fully diffuse scattering.
    //
    // `textureSampleLevel` rather than `textureSample`, because the level is chosen from the
    // material rather than from how far away the surface is. Letting the hardware pick would make a
    // polished floor go rough in the distance.
    let reflected = reflect(-towards_eye, normal);
    let blurred = textureSampleLevel(
        specular_map,
        environment_sampler,
        reflected,
        roughness * (SPECULAR_LEVELS - 1.0)
    ).rgb;

    // How much of that reflection survives at this angle, and how white it goes. This is what makes
    // a metal reflect its own colour while a dielectric's reflection turns white at a glancing
    // angle -- and it is what stops a metal being black, because a metal's *only* ambient light is
    // this term.
    let env_brdf = environment_brdf(n_dot_v, roughness);
    let ambient_specular = blurred * (f0 * env_brdf.x + vec3<f32>(env_brdf.y));

    let ambient_light = ambient_diffuse + ambient_specular;

    // Emissive is added rather than multiplied, and is not affected by the light -- that is what
    // makes it emissive. Above 1.0 it pushes into the HDR range the post pass tonemaps (ADR 0034).
    return vec4<f32>(direct + ambient_light + in.emissive, albedo.a);
}
