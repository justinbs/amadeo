// The camera's `Environment` (ADR 0034), declared once and prepended to every pass that reads it.
//
// # Why this is its own file
//
// `post.wgsl` and `bloom.wgsl` read the **same buffer at the same binding**, so their declarations of
// it have to agree byte for byte. Two hand-written copies of one layout drift — `view.wgsl` exists
// because exactly that happened between the mesh and sky shaders, and the symptom was a sky drawn
// facing the wrong way with nothing failing to compile.
//
// **The Rust side is `GpuPost` in `gpu.rs`.** That copy cannot be removed this way, because
// `#[repr(C)]` and a WGSL struct are two statements of one layout in two languages.

struct Post {
    // x = exposure, y = tonemap operator, z = vignette intensity, w = vignette radius.
    //
    // Packed into vec4s rather than named scalars because a WGSL uniform pads every member to 16
    // bytes; four separate f32s would occupy 64 bytes and read no more clearly than this does with
    // the comment next to it.
    controls: vec4<f32>,
    // x = contrast, y = saturation. zw unused.
    grade: vec4<f32>,
    // rgb = tint. a unused.
    tint: vec4<f32>,
    // x = bloom threshold, y = bloom intensity. zw unused.
    //
    // Both are read by `bloom.wgsl`; only the intensity is read by `post.wgsl`, which is what adds
    // the finished blur back in.
    bloom: vec4<f32>,
};

@group(1) @binding(0) var<uniform> post: Post;
