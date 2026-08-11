// Drawing the environment behind everything else — ADR 0049's second half.
//
// The environment map became a *light source* first: surfaces are lit by it, shadows are filled by
// it, metals reflect it. But nothing painted it, so the background stayed the flat clear colour and
// the sun lighting the world was invisible. This is that pass.
//
// # Why it draws last rather than first
//
// It runs after the opaque geometry, with depth testing on and depth writing **off**, at exactly the
// far plane. So a fragment survives only where nothing nearer was drawn — the sky fills the gaps and
// is never drawn over. Painting it first instead would mean every pixel of ground is shaded twice.
//
// # Why there is no vertex buffer
//
// Three vertices are derived from `vertex_index` alone, covering the screen with one oversized
// triangle rather than two that meet down the diagonal. A single triangle has no seam and no shared
// edge for the rasteriser to process twice — the same reason `quad.wgsl` derives its corners rather
// than reading them.

// `MeshView` and `view` come from `view.wgsl`, prepended at pipeline creation.
//
// **This file used to declare its own copy**, and that copy is what broke when cascades made
// `light_view_projection` an array: every field after it, including the three vectors below, was
// read from 192 bytes too early. Nothing failed — the sky was simply drawn facing the wrong way.

// The environment, at group **1** rather than the mesh shader's group 3 — this pipeline declares
// only the two groups it reads, because the sky has to draw for a camera with no meshes at all,
// where nothing has bound a shadow map or a material. The bindings within the group are the same
// layout, so the same bind group object serves both pipelines.
@group(1) @binding(1) var specular_map: texture_cube<f32>;
@group(1) @binding(2) var environment_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // Where this pixel is on screen, -1 to 1.
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    // (0,0), (2,0), (0,2) becomes (-1,-1), (3,-1), (-1,3): one triangle covering the whole screen
    // and overhanging it on two sides.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    let position = corner * 2.0 - 1.0;

    var out: VertexOutput;
    // z = w = 1 puts this exactly on the far plane, which is where the depth test wants it: the
    // pipeline compares with `LessEqual` and writes nothing, so the sky passes only where the depth
    // buffer is still at its cleared value.
    out.clip_position = vec4<f32>(position, 1.0, 1.0);
    out.ndc = position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let direction = normalize(
        view.sky_right.xyz * in.ndc.x
            + view.sky_up.xyz * in.ndc.y
            + view.sky_forward.xyz
    );

    // Level 0 of the specular chain, which is the environment unblurred. The levels below it are
    // progressively rougher copies for surfaces to reflect; looking at the sky directly is the one
    // case that wants the sharp original.
    //
    // `textureSampleLevel` rather than `textureSample` because a fragment shader may only pick its
    // own mip level explicitly — there is no surface here for the hardware to measure a slope
    // across, so it has nothing to derive one from.
    let sky = textureSampleLevel(specular_map, environment_sampler, direction, 0.0).rgb;

    // Straight into the HDR target, sun and all. A sun two hundred and fifty times brighter than the
    // sky beside it is exactly what the post pass's tonemapping exists to bring back into range
    // (ADR 0034) -- and what makes it read as a light source rather than a white circle.
    return vec4<f32>(sky, 1.0);
}
