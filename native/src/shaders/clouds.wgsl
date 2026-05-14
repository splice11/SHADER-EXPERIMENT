// Port of Nimitz's "Protean Clouds" (Shadertoy 3l23Rh, CC BY-NC-SA 3.0)
// with audio-reactive uniforms, lightning injection, and palette grading.
// Writes scene-linear HDR; bloom + tonemap happen in post.wgsl.

struct Params {
    resolution: vec2<f32>,
    time: f32,
    _pad0: f32,

    bass: f32, mid: f32, treble: f32, centroid: f32,
    rms: f32, punch: f32, _pad1: f32, _pad2: f32,

    speed: f32, morph: f32, density_mul: f32, hue_shift: f32,
    bass_to_speed: f32, bass_to_morph: f32, centroid_to_hue: f32, rms_to_density: f32,

    flash_pos: vec3<f32>,
    flash_strength: f32,

    flash_color: vec3<f32>,
    bolt_intensity: f32,

    bolt_anchor: vec2<f32>,
    bolt_seed: f32,
    bolt_width: f32,

    palette_amount: f32,
    palette_centroid_drive: f32,
    _pad3: f32,
    _pad4: f32,

    palette0: vec3<f32>, _ps0: f32,
    palette1: vec3<f32>, _ps1: f32,
    palette2: vec3<f32>, _ps2: f32,
    palette3: vec3<f32>, _ps3: f32,
    palette4: vec3<f32>, _ps4: f32,
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
    let c = cos(a); let s = sin(a);
    return mat2x2<f32>(c, s, -s, c);
}

fn m3() -> mat3x3<f32> {
    return mat3x3<f32>(
        vec3<f32>( 0.6434234,  1.0814562, -1.3860681),
        vec3<f32>(-1.6962191,  0.6301643, -0.2957339),
        vec3<f32>( 0.2926266,  1.3432028,  1.1838427),
    );
}

fn mag2(p: vec2<f32>) -> f32 { return dot(p, p); }
fn linstep(mn: f32, mx: f32, x: f32) -> f32 { return clamp((x - mn) / (mx - mn), 0.0, 1.0); }
fn disp(t: f32) -> vec2<f32> { return vec2<f32>(sin(t * 0.22), cos(t * 0.175)) * 2.0; }

fn hash11(n: f32) -> f32 {
    return fract(sin(n * 12.9898) * 43758.5453);
}

fn hue_rotate(c: vec3<f32>, a: f32) -> vec3<f32> {
    let k = vec3<f32>(0.57735026919);
    let co = cos(a); let si = sin(a);
    return c * co + cross(k, c) * si + k * dot(k, c) * (1.0 - co);
}

fn map_fn(p_in: vec3<f32>, prm1: f32, bs_mo: vec2<f32>, itime: f32) -> vec2<f32> {
    var p = p_in;
    let p2_xy = p.xy - disp(p.z).xy;
    let p2 = vec3<f32>(p2_xy, p.z);
    let r = rot(sin(p.z + itime) * (0.1 + prm1 * 0.05) + itime * 0.09);
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
    let flash_on = P.flash_strength > 0.0001;
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

            // Lightning emissive injection — inverse-square falloff from the
            // flash position, modulated by local density so it lights the cloud.
            if (flash_on) {
                let to_flash = P.flash_pos - pos;
                let d2 = dot(to_flash, to_flash);
                let att = 1.0 / (1.0 + 0.18 * d2);
                let emissive = P.flash_color * P.flash_strength * att * den * 1.2;
                col = vec4<f32>(col.rgb + emissive, col.a);
            }
        }

        let fog_c = exp(t * 0.2 - 2.2);
        col = col + vec4<f32>(0.06, 0.11, 0.11, 0.1) * clamp(fog_c - fog_t, 0.0, 1.0);
        fog_t = fog_c;
        rez = rez + col * (1.0 - rez.a);
        t = t + clamp(0.5 - dn * dn * 0.05, 0.09, 0.3);
    }
    return clamp(rez, vec4<f32>(0.0), vec4<f32>(4.0));
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

// 5-stop palette lookup; x in [0,1].
fn palette_lookup(x: f32) -> vec3<f32> {
    let xc = clamp(x, 0.0, 1.0) * 4.0;
    let i = floor(xc);
    let f = xc - i;
    var a: vec3<f32>;
    var b: vec3<f32>;
    if (i < 0.5)      { a = P.palette0; b = P.palette1; }
    else if (i < 1.5) { a = P.palette1; b = P.palette2; }
    else if (i < 2.5) { a = P.palette2; b = P.palette3; }
    else              { a = P.palette3; b = P.palette4; }
    let ft = f * f * (3.0 - 2.0 * f);
    return mix(a, b, ft);
}

fn segment_dist(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let t = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    return length(pa - ba * t);
}

// Jagged bolt from top of screen down to anchor, jittered by seed.
// uv is in aspect-corrected space (x in [-aspect/2, aspect/2], y in [-0.5, 0.5]).
fn bolt(uv: vec2<f32>, anchor: vec2<f32>, seed: f32) -> f32 {
    let start = vec2<f32>(anchor.x + (hash11(seed) - 0.5) * 0.4, 0.6);
    var a = start;
    var d = 9999.0;
    let segments = 14;
    for (var i = 0; i < segments; i = i + 1) {
        let t = f32(i + 1) / f32(segments);
        let off = (hash11(seed + f32(i) * 7.3) - 0.5) * 0.16 * (1.0 - t);
        let drift = (hash11(seed + f32(i) * 3.1 + 11.0) - 0.5) * 0.04;
        let b = mix(start, anchor, t) + vec2<f32>(off, drift);
        d = min(d, segment_dist(uv, a, b));
        // Tiny branch every few segments.
        if ((i == 4) || (i == 9)) {
            let branch_end = b + vec2<f32>(
                (hash11(seed + f32(i) * 2.1) - 0.5) * 0.3,
                -hash11(seed + f32(i) * 4.7) * 0.2,
            );
            d = min(d, segment_dist(uv, b, branch_end));
        }
        a = b;
    }
    return d;
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
    var tgt = normalize(ro - vec3<f32>(tgt_disp, time + tgt_dst));
    ro.x = ro.x - bs_mo.x * 2.0;

    var rightdir = normalize(cross(tgt, vec3<f32>(0.0, 1.0, 0.0)));
    let updir = normalize(cross(rightdir, tgt));
    rightdir = normalize(cross(updir, tgt));
    var rd = normalize((p.x * rightdir + p.y * updir) - tgt);

    let r2 = rot(-disp(time + 3.5).x * 0.2 + bs_mo.x);
    let rdxy = rd.xy * r2;
    rd = vec3<f32>(rdxy, rd.z);

    let base_prm = smoothstep(-0.4, 0.4, sin(itime * 0.3));
    let prm1 = clamp(base_prm + P.morph + P.bass * P.bass_to_morph, 0.0, 1.6);
    let density_boost = P.density_mul + P.rms * P.rms_to_density + P.punch * 0.4;

    let scn = render_clouds(ro, rd, prm1, bs_mo, itime, density_boost);

    var col = scn.rgb;
    col = i_lerp(col.bgr, col.rgb, clamp(1.0 - prm1, 0.05, 1.0));
    // Nimitz's original grade — kept and crossfaded with palette.
    let nimitz = pow(col, vec3<f32>(0.55, 0.65, 0.6)) * vec3<f32>(1.0, 0.97, 0.9);

    // Palette lookup keyed on luminance with centroid offset.
    let lum = dot(col, vec3<f32>(0.299, 0.587, 0.114));
    let pal_x = clamp(
        lum + (P.centroid - 0.5) * P.palette_centroid_drive,
        0.0, 1.0,
    );
    let graded = palette_lookup(pal_x) * (0.5 + 0.8 * lum);

    col = mix(nimitz, graded, P.palette_amount);
    col = hue_rotate(col, P.hue_shift + (P.centroid - 0.5) * P.centroid_to_hue);

    // Vignette.
    col = col * (pow(16.0 * q.x * q.y * (1.0 - q.x) * (1.0 - q.y), 0.12) * 0.7 + 0.3);

    // 2D bolt overlay — additive emissive.
    if (P.flash_strength > 0.0001) {
        let aspect = P.resolution.x / max(P.resolution.y, 1.0);
        let uv = vec2<f32>((q.x - 0.5) * aspect, q.y - 0.5);
        let anchor = vec2<f32>((P.bolt_anchor.x - 0.5) * aspect, P.bolt_anchor.y - 0.5);
        let bd = bolt(uv, anchor, P.bolt_seed);
        let core = exp(-bd / max(P.bolt_width, 1e-4));
        let halo = exp(-bd / max(P.bolt_width * 9.0, 1e-4)) * 0.35;
        let bolt_emit = (core + halo) * P.flash_strength * P.bolt_intensity;
        col = col + P.flash_color * bolt_emit;
    }

    return vec4<f32>(col, 1.0);
}
