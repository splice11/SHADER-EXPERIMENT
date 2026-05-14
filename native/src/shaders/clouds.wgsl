// Port of Nimitz's "Protean Clouds" (Shadertoy 3l23Rh, CC BY-NC-SA 3.0)
// to WGSL, with audio-reactive uniforms surfaced.

struct Params {
    resolution: vec2<f32>,
    time: f32,
    _pad0: f32,

    bass: f32,
    mid: f32,
    treble: f32,
    centroid: f32,

    rms: f32,
    punch: f32,
    _pad1: f32,
    _pad2: f32,

    speed: f32,
    morph: f32,
    density_mul: f32,
    hue_shift: f32,

    bass_to_speed: f32,
    bass_to_morph: f32,
    centroid_to_hue: f32,
    rms_to_density: f32,
};

@group(0) @binding(0) var<uniform> P: Params;

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

fn rot(a: f32) -> mat2x2<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat2x2<f32>(c, s, -s, c);
}

// Nimitz's mat3 m3, pre-multiplied by 1.93. Stored as columns to match GLSL.
fn m3() -> mat3x3<f32> {
    return mat3x3<f32>(
        vec3<f32>( 0.6434234,  1.0814562, -1.3860681),
        vec3<f32>(-1.6962191,  0.6301643, -0.2957339),
        vec3<f32>( 0.2926266,  1.3432028,  1.1838427),
    );
}

fn mag2(p: vec2<f32>) -> f32 { return dot(p, p); }

fn linstep(mn: f32, mx: f32, x: f32) -> f32 {
    return clamp((x - mn) / (mx - mn), 0.0, 1.0);
}

fn disp(t: f32) -> vec2<f32> {
    return vec2<f32>(sin(t * 0.22), cos(t * 0.175)) * 2.0;
}

// HSV-ish hue rotation on linear RGB.
fn hue_rotate(c: vec3<f32>, a: f32) -> vec3<f32> {
    let k = vec3<f32>(0.57735026919);
    let co = cos(a);
    let si = sin(a);
    return c * co + cross(k, c) * si + k * dot(k, c) * (1.0 - co);
}

fn map_fn(p_in: vec3<f32>, prm1: f32, bs_mo: vec2<f32>, itime: f32) -> vec2<f32> {
    var p = p_in;
    let p2_xy = p.xy - disp(p.z).xy;
    let p2 = vec3<f32>(p2_xy, p.z);
    let r = rot(sin(p.z + itime) * (0.1 + prm1 * 0.05) + itime * 0.09);
    // GLSL: p.xy *= rot(...)  → v * M
    let xy = p.xy * r;
    p = vec3<f32>(xy, p.z);
    let cl = mag2(p2.xy);
    var d = 0.0;
    p = p * 0.61;
    var z = 1.0;
    var trk = 1.0;
    let dsp_amp = 0.1 + prm1 * 0.2;
    for (var i = 0; i < 5; i = i + 1) {
        p = p + sin(p.zxy * 0.75 * trk + itime * trk * 0.8) * dsp_amp;
        d = d - abs(dot(cos(p), sin(p.yzx)) * z);
        z = z * 0.57;
        trk = trk * 1.4;
        // GLSL: p = p * m3 → v * M
        p = p * m3();
    }
    d = abs(d + prm1 * 3.0) + prm1 * 0.3 - 2.5 + bs_mo.y;
    return vec2<f32>(d + cl * 0.2 + 0.25, cl);
}

fn render_clouds(
    ro: vec3<f32>,
    rd: vec3<f32>,
    prm1: f32,
    bs_mo: vec2<f32>,
    itime: f32,
    density_boost: f32,
) -> vec4<f32> {
    var rez = vec4<f32>(0.0);
    var t = 1.5;
    var fog_t = 0.0;
    for (var i = 0; i < 130; i = i + 1) {
        if (rez.a > 0.99) { break; }
        let pos = ro + t * rd;
        let mpv = map_fn(pos, prm1, bs_mo, itime);
        let den = clamp(mpv.x - 0.3, 0.0, 1.0) * 1.12 * density_boost;
        let dn = clamp(mpv.x + 2.0, 0.0, 3.0);

        var col = vec4<f32>(0.0);
        if (mpv.x > 0.6) {
            let base = sin(vec3<f32>(5.0, 0.4, 0.2)
                + mpv.y * 0.1
                + sin(pos.z * 0.4) * 0.5
                + 1.8) * 0.5 + 0.5;
            col = vec4<f32>(base, 0.08);
            col = col * (den * den * den);
            col = vec4<f32>(col.rgb * (linstep(4.0, -2.5, mpv.x) * 2.3), col.a);

            var dif = clamp((den - map_fn(pos + 0.8, prm1, bs_mo, itime).x) / 9.0, 0.001, 1.0);
            dif = dif + clamp((den - map_fn(pos + 0.35, prm1, bs_mo, itime).x) / 2.5, 0.001, 1.0);
            let shade = vec3<f32>(0.005, 0.045, 0.075)
                + 1.5 * vec3<f32>(0.033, 0.07, 0.03) * dif;
            col = vec4<f32>(col.xyz * den * shade, col.a);
        }

        let fog_c = exp(t * 0.2 - 2.2);
        col = col + vec4<f32>(0.06, 0.11, 0.11, 0.1) * clamp(fog_c - fog_t, 0.0, 1.0);
        fog_t = fog_c;
        rez = rez + col * (1.0 - rez.a);
        t = t + clamp(0.5 - dn * dn * 0.05, 0.09, 0.3);
    }
    return clamp(rez, vec4<f32>(0.0), vec4<f32>(1.0));
}

fn getsat(c: vec3<f32>) -> f32 {
    let mi = min(min(c.x, c.y), c.z);
    let ma = max(max(c.x, c.y), c.z);
    return (ma - mi) / (ma + 1e-7);
}

fn i_lerp(a: vec3<f32>, b: vec3<f32>, x: f32) -> vec3<f32> {
    var ic = mix(a, b, x) + vec3<f32>(1e-6, 0.0, 0.0);
    let sd = abs(getsat(ic) - mix(getsat(a), getsat(b), x));
    let dir = normalize(vec3<f32>(
        2.0 * ic.x - ic.y - ic.z,
        2.0 * ic.y - ic.x - ic.z,
        2.0 * ic.z - ic.y - ic.x,
    ));
    let lgt = dot(vec3<f32>(1.0), ic);
    let ff = dot(dir, normalize(ic));
    ic = ic + 1.5 * dir * sd * ff * lgt;
    return clamp(ic, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_clouds(in: VsOut) -> @location(0) vec4<f32> {
    let frag_coord = in.uv * P.resolution;
    let q = in.uv;
    let p = (frag_coord - 0.5 * P.resolution) / P.resolution.y;
    let bs_mo = vec2<f32>(0.0);

    let itime = P.time;
    let speed = P.speed + P.bass * P.bass_to_speed;
    let time = itime * speed;

    var ro = vec3<f32>(0.0, 0.0, time);
    ro = ro + vec3<f32>(sin(itime) * 0.5, 0.0, 0.0);

    let dsp_amp = 0.85;
    let d_xy = disp(ro.z) * dsp_amp;
    ro = vec3<f32>(ro.xy + d_xy, ro.z);

    let tgt_dst = 3.5;
    let tgt_disp = disp(time + tgt_dst) * dsp_amp;
    var target = normalize(ro - vec3<f32>(tgt_disp, time + tgt_dst));
    ro.x = ro.x - bs_mo.x * 2.0;

    var rightdir = normalize(cross(target, vec3<f32>(0.0, 1.0, 0.0)));
    let updir = normalize(cross(rightdir, target));
    rightdir = normalize(cross(updir, target));
    var rd = normalize((p.x * rightdir + p.y * updir) - target);

    let r2 = rot(-disp(time + 3.5).x * 0.2 + bs_mo.x);
    let rdxy = rd.xy * r2;
    rd = vec3<f32>(rdxy, rd.z);

    let base_prm = smoothstep(-0.4, 0.4, sin(itime * 0.3));
    let prm1 = clamp(base_prm + P.morph + P.bass * P.bass_to_morph, 0.0, 1.6);

    let density_boost = P.density_mul + P.rms * P.rms_to_density + P.punch * 0.4;

    let scn = render_clouds(ro, rd, prm1, bs_mo, itime, density_boost);

    var col = scn.rgb;
    col = i_lerp(col.bgr, col.rgb, clamp(1.0 - prm1, 0.05, 1.0));
    col = pow(col, vec3<f32>(0.55, 0.65, 0.6)) * vec3<f32>(1.0, 0.97, 0.9);

    let hue = P.hue_shift + (P.centroid - 0.5) * P.centroid_to_hue;
    col = hue_rotate(col, hue);

    // Vignette.
    col = col * (pow(16.0 * q.x * q.y * (1.0 - q.x) * (1.0 - q.y), 0.12) * 0.7 + 0.3);

    return vec4<f32>(col, 1.0);
}
