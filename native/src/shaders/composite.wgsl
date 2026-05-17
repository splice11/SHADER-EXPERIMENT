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
    radial_blur: f32,

    black_point: f32,
    highlight_softness: f32,
    _pad_color0: f32,
    _pad_color1: f32,
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

// Standard ACES rational approximation. Applied per-channel it preserves
// nothing about chrominance — bright reds rendered with it desaturate fast
// because the red channel hits 1.0 while green/blue are still climbing.
fn aces_per_channel(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e),
                 vec3<f32>(0.0), vec3<f32>(1.0));
}

// Same curve applied to luminance only. Chrominance is preserved (the colour
// stays exactly as saturated as it was) but values above ~1.0 stack onto a
// luminance ceiling of 1.0 — i.e. saturated peaks read as "hot colour" rather
// than "blown white" but can't exceed sRGB gamut.
fn tonemap_luma_preserving(c: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let cc = 2.43; let d = 0.59; let e = 0.14;
    let l = max(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-6);
    let l_tm = clamp((l * (a * l + b)) / (l * (cc * l + d) + e), 0.0, 1.0);
    return clamp(c * (l_tm / l), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Blend the two tonemaps: at moderate input keep colour saturated (luma
// variant), at very high input lean on per-channel so blown highlights
// desaturate cleanly toward white instead of staying neon-saturated. Reads
// as filmic highlight roll-off without the standard "everything becomes
// white" ACES failure mode on saturated peaks.
fn tonemap(c: vec3<f32>, softness: f32) -> vec3<f32> {
    let luma_pres = tonemap_luma_preserving(c);
    let per_ch = aces_per_channel(c);
    let peak = max(c.r, max(c.g, c.b));
    let weight = smoothstep(0.8, 2.5, peak) * clamp(softness, 0.0, 1.0);
    return mix(luma_pres, per_ch, weight);
}

// Inky-blacks shadow crush. Everything below `black_point` becomes literally
// zero; values above it are remapped to [0, 1] linearly so we don't lose
// midtones. Applied after tonemap so it works in display-referred space.
fn shadow_crush(c: vec3<f32>, black_point: f32) -> vec3<f32> {
    let bp = clamp(black_point, 0.0, 0.4);
    return max((c - vec3<f32>(bp)) / max(1.0 - bp, 1e-4), vec3<f32>(0.0));
}

// Selective saturation: bell curve peaked at mid-luma, zero at the extremes.
// Pushing saturation here makes mids pop without amplifying shadow noise or
// destroying highlight detail. `amount > 1` saturates mids; `< 1` desaturates.
fn selective_saturation(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    let bell = 4.0 * l * (1.0 - l); // peak 1.0 at l=0.5, zero at 0 and 1
    let strength = 1.0 + (amount - 1.0) * bell;
    return mix(vec3<f32>(l), c, strength);
}

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Radial "speed line" blur: average several samples along the line from this
// pixel toward the centre of the screen. Reads as a hyperdrive/zoom-streak
// effect during big drops without sampling outside the frame (so no ugly
// black halo on the corners the way barrel/pincushion warp does).
fn sample_scene_radial(uv: vec2<f32>) -> vec3<f32> {
    let center = vec2<f32>(0.5);
    let dir = uv - center;
    let amt = P.aberration * 0.012;

    // Chromatic aberration baseline samples (radial offsets per channel).
    let uv_r = uv - dir * amt;
    let uv_g = uv;
    let uv_b = uv + dir * amt;

    let blur = clamp(P.radial_blur, 0.0, 0.10);
    if (blur < 1e-4) {
        let r = textureSample(scene_tex, scene_smp, uv_r).r;
        let g = textureSample(scene_tex, scene_smp, uv_g).g;
        let b = textureSample(scene_tex, scene_smp, uv_b).b;
        return vec3<f32>(r, g, b);
    }

    // 6 samples toward centre, geometrically spaced — heavier weight near the
    // original pixel so the result still reads as a sharp image with a streak.
    var rgb = vec3<f32>(0.0);
    var w_sum = 0.0;
    let taps = 6;
    for (var i = 0; i < taps; i = i + 1) {
        let s = f32(i) / f32(taps - 1);          // 0..1
        let push = s * blur;                     // 0..blur
        let w = exp(-f32(i) * 0.6);
        let r = textureSample(scene_tex, scene_smp, uv_r - dir * push).r;
        let g = textureSample(scene_tex, scene_smp, uv_g - dir * push).g;
        let b = textureSample(scene_tex, scene_smp, uv_b - dir * push).b;
        rgb = rgb + vec3<f32>(r, g, b) * w;
        w_sum = w_sum + w;
    }
    return rgb / w_sum;
}

fn sample_bloom_anamorphic(uv: vec2<f32>) -> vec3<f32> {
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

    let scn = sample_scene_radial(uv);
    let blm = sample_bloom_anamorphic(uv);

    // HDR composite + exposure → tonemap → LDR.
    var hdr = (scn + blm * P.intensity) * P.exposure;
    var col = tonemap(hdr, P.highlight_softness);

    // Inky-black crush (LDR).
    col = shadow_crush(col, P.black_point);

    // Saturation only fires in midtones, so heavy boost here doesn't trash
    // shadow noise or highlight detail.
    col = selective_saturation(col, P.saturation);

    // Linear contrast about mid-grey, kept gentle since tonemap already
    // contributes the curve's shape.
    col = (col - vec3<f32>(0.5)) * P.contrast + vec3<f32>(0.5);
    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));

    // Final luma used for grain weighting.
    let luma = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (P.grain > 0.0001) {
        let g = (hash21(uv * P.resolution + vec2<f32>(P.time * 137.31, P.time * 91.7)) - 0.5);
        let scale = mix(1.4, 0.6, smoothstep(0.0, 0.7, luma));
        col = col + vec3<f32>(g) * P.grain * scale;
    }

    let final_col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)) * clamp(P.fade_in, 0.0, 1.0);
    return vec4<f32>(final_col, 1.0);
}
