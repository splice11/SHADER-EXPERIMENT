// Final composite: HDR scene + bloom → tonemap → swapchain (sRGB target
// handles the gamma conversion).

struct PostParams {
    threshold: f32,
    knee: f32,
    intensity: f32,
    exposure: f32,
};

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_smp: sampler;
@group(0) @binding(2) var bloom_tex: texture_2d<f32>;
@group(0) @binding(3) var bloom_smp: sampler;
@group(0) @binding(4) var<uniform> P: PostParams;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vid: u32) -> VsOut {
    let x = f32((vid << 1u) & 2u);
    let y = f32(vid & 2u);
    var out: VsOut;
    out.pos = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, 1.0 - y);
    return out;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e),
                 vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let scn = textureSample(scene_tex, scene_smp, in.uv).rgb;
    let blm = textureSample(bloom_tex, bloom_smp, in.uv).rgb;
    let exposed = (scn + blm * P.intensity) * P.exposure;
    return vec4<f32>(aces(exposed), 1.0);
}
