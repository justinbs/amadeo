// The per-view uniform, declared once and textually shared by every pipeline that reads it.
//
// # Why this is its own file
//
// `mesh.wgsl` and `sky.wgsl` each used to declare their own copy of this struct, and the two had to
// agree byte for byte because they are the *same buffer* at the same binding. Nothing checked that
// they did.
//
// They stopped agreeing the moment cascades turned `light_view_projection` into an array of four
// (ADR 0055). `sky.wgsl` still had it as a single matrix, so every field after it — including the
// three vectors that turn a screen position into a world direction — was read 192 bytes early. The
// symptom was not a compile error or a validation failure: it was a sky drawn in the wrong
// direction, which read as a huge dark wedge across the horizon.
//
// So this is prepended to both at pipeline creation, and there is one declaration. It is the same
// answer `amadeo-snapshot` gives for borrowing `format_float` from `amadeo-scene`: two copies of one
// fact drift, and the fix is to have one.
//
// **The Rust side is `GpuMeshView` in `gpu.rs`**, and that one still has to be kept in step by hand
// — `#[repr(C)]` and this struct are two statements of one layout, and only a wrong picture says
// they disagree.

struct MeshView {
    // World to clip space: the camera's inverse transform, then its projection.
    view_projection: mat4x4<f32>,
    // World to each cascade's light clip space, nearest first (ADR 0038, ADR 0055). Unused slots
    // hold the identity.
    light_view_projection: array<mat4x4<f32>, 4>,
    // xyz = the direction light travels, normalised. w unused.
    light_direction: vec4<f32>,
    // rgb = light colour with intensity already folded in. a unused.
    light_colour: vec4<f32>,
    // x = one shadow-map texel in UV, y = 1 when a real shadow map is bound, z = how many cascades
    // are real. w unused.
    shadow_params: vec4<f32>,
    // Where each cascade stops covering, as a distance from the camera. Nearest first.
    cascade_far: vec4<f32>,
    // Each cascade's depth bias, in its own clip space. Nearest first — and it has to be per
    // cascade, because a near cascade's depth range is a fraction of the far one's, so the same
    // authored world-unit offset is a much larger share of it.
    cascade_bias: vec4<f32>,
    // xyz = the camera's world position. Needed only since PBR: diffuse light looks the same from
    // everywhere, but a specular highlight is a reflection and moves with the viewer.
    eye: vec4<f32>,
    // Turn a screen position into a world direction, for drawing the sky. The first two already
    // carry the field of view and the aspect ratio, so nothing needs trigonometry.
    sky_right: vec4<f32>,
    sky_up: vec4<f32>,
    sky_forward: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: MeshView;
