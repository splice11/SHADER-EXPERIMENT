// Bloom passes: extract (with soft-knee threshold), downsample, upsample.

struct PostParams {
    threshold: f32,
    knee: f32,
    intensity: f32,
    exposure: f32,
};

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_smp: sampler;
@group(0) @binding(2) var<uniform> P: PostParams;

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

fn soft_threshold(c: vec3<f32>) -> vec3<f32> {
    let br = max(max(c.r, c.g), c.b);
    let knee = max(P.knee, 1e-4);
    let q = clamp(br - P.threshold + knee, 0.0, 2.0 * knee);
    let soft = q * q / (4.0 * knee);
    let contribution = max(soft, br - P.threshold) / max(br, 1e-4);
    return c * contribution;
}

@fragment
fn fs_extract(in: VsOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(src_tex));
    let o = texel * 0.5;
    let s0 = textureSample(src_tex, src_smp, in.uv + vec2<f32>(-o.x, -o.y)).rgb;
    let s1 = textureSample(src_tex, src_smp, in.uv + vec2<f32>( o.x, -o.y)).rgb;
    let s2 = textureSample(src_tex, src_smp, in.uv + vec2<f32>(-o.x,  o.y)).rgb;
    let s3 = textureSample(src_tex, src_smp, in.uv + vec2<f32>( o.x,  o.y)).rgb;
    let avg = (s0 + s1 + s2 + s3) * 0.25;
    return vec4<f32>(soft_threshold(avg), 1.0);
}

@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(src_tex));
    let o = texel * 0.5;
    let s0 = textureSample(src_tex, src_smp, in.uv + vec2<f32>(-o.x, -o.y)).rgb;
    let s1 = textureSample(src_tex, src_smp, in.uv + vec2<f32>( o.x, -o.y)).rgb;
    let s2 = textureSample(src_tex, src_smp, in.uv + vec2<f32>(-o.x,  o.y)).rgb;
    let s3 = textureSample(src_tex, src_smp, in.uv + vec2<f32>( o.x,  o.y)).rgb;
    return vec4<f32>((s0 + s1 + s2 + s3) * 0.25, 1.0);
}

@fragment
fn fs_upsample(in: VsOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(src_tex));
    let r = texel;
    var sum = vec3<f32>(0.0);
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>(-r.x, -r.y)).rgb * 1.0;
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>( 0.0, -r.y)).rgb * 2.0;
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>( r.x, -r.y)).rgb * 1.0;
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>(-r.x,  0.0)).rgb * 2.0;
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>( 0.0,  0.0)).rgb * 4.0;
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>( r.x,  0.0)).rgb * 2.0;
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>(-r.x,  r.y)).rgb * 1.0;
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>( 0.0,  r.y)).rgb * 2.0;
    sum = sum + textureSample(src_tex, src_smp, in.uv + vec2<f32>( r.x,  r.y)).rgb * 1.0;
    return vec4<f32>(sum / 16.0, 1.0);
}
