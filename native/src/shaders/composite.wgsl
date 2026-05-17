// Composite: HDR scene + bloom → tonemap → contrast/saturation → grain →
// letterbox → swapchain (sRGB on the way out). Adds anamorphic-stretched
// bloom contribution and per-pixel chromatic aberration.

struct PostParams {
    threshold: f32,
    knee: f32,
    intensity: f32,
    exposure: f32,

    contrast: f32,
    saturation: f32,
    grain: f32,
    time: f32,

    aberration: f32,
    letterbox_aspect: f32,
    anamorphic: f32,
    vignette: f32,

    resolution: vec2<f32>,
    fade_in: f32,
    lens_warp: f32,
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

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Radial lens distortion. `amount` > 0 = barrel (corners pushed out, fisheye),
// `amount` < 0 = pincushion (corners pulled in). Range tested ±0.5.
fn lens_warp_uv(uv: vec2<f32>, amount: f32) -> vec2<f32> {
    let c = uv - vec2<f32>(0.5);
    let r2 = dot(c, c);
    return uv + c * r2 * amount;
}

fn sample_scene_aberrated(uv_in: vec2<f32>) -> vec3<f32> {
    let uv = lens_warp_uv(uv_in, P.lens_warp);
    if (P.aberration <= 0.0001) {
        return textureSample(scene_tex, scene_smp, uv).rgb;
    }
    let center = vec2<f32>(0.5);
    let dir = uv - center;
    let amt = P.aberration * 0.012;
    let r = textureSample(scene_tex, scene_smp, uv - dir * amt).r;
    let g = textureSample(scene_tex, scene_smp, uv).g;
    let b = textureSample(scene_tex, scene_smp, uv + dir * amt).b;
    return vec3<f32>(r, g, b);
}

fn sample_bloom_anamorphic(uv_in: vec2<f32>) -> vec3<f32> {
    let uv = lens_warp_uv(uv_in, P.lens_warp);
    let base = textureSample(bloom_tex, bloom_smp, uv).rgb;
    if (P.anamorphic <= 0.0001) {
        return base;
    }
    var streak = vec3<f32>(0.0);
    var w_sum = 0.0;
    for (var i = -5; i <= 5; i = i + 1) {
        let off = f32(i) * 0.012;
        let w = exp(-f32(i * i) * 0.18);
        streak = streak + textureSample(bloom_tex, bloom_smp, uv + vec2<f32>(off, 0.0)).rgb * w;
        w_sum = w_sum + w;
    }
    streak = streak / w_sum;
    return base + streak * P.anamorphic;
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;

    // Letterbox bars (drawn early so we don't waste GPU on later steps where
    // they'd be overwritten anyway — but cost is negligible either way).
    let aspect_screen = P.resolution.x / max(P.resolution.y, 1.0);
    if (P.letterbox_aspect > 0.5) {
        let bar = max(0.0, 1.0 - aspect_screen / P.letterbox_aspect) * 0.5;
        if (uv.y < bar || uv.y > 1.0 - bar) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
    }

    let scn = sample_scene_aberrated(uv);
    let blm = sample_bloom_anamorphic(uv);

    var col = (scn + blm * P.intensity) * P.exposure;
    col = aces(col);

    // Saturation around luma.
    let luma = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    col = mix(vec3<f32>(luma), col, P.saturation);
    // Contrast around mid-grey.
    col = (col - vec3<f32>(0.5)) * P.contrast + vec3<f32>(0.5);
    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));

    // Animated film grain. Scale by 1/luma a bit so it shows in the dark areas
    // more than in highlights — that's how real film stock looks.
    if (P.grain > 0.0001) {
        let g = (hash21(uv * P.resolution + vec2<f32>(P.time * 137.31, P.time * 91.7)) - 0.5);
        let scale = mix(1.4, 0.6, smoothstep(0.0, 0.7, luma));
        col = col + vec3<f32>(g) * P.grain * scale;
    }

    let final_col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)) * clamp(P.fade_in, 0.0, 1.0);
    return vec4<f32>(final_col, 1.0);
}
