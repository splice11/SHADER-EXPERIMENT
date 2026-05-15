// Port of Nimitz's "Protean Clouds" (Shadertoy 3l23Rh, CC BY-NC-SA 3.0).
// Camera basis (pos / right / up / fwd / zoom) is provided by the CPU so we
// can do smooth follow + audio-driven kicks without re-deriving them in the
// shader. A 3D bolt path is accumulated as emissive inside the volume march.

struct Params {
    resolution: vec2<f32>,
    time: f32,
    _pad0: f32,

    bass: f32, mid: f32, treble: f32, centroid: f32,
    rms: f32, punch: f32, _pad1: f32, _pad2: f32,

    speed: f32, morph: f32, density_mul: f32, hue_shift: f32,
    bass_to_speed: f32, bass_to_morph: f32, centroid_to_hue: f32, rms_to_density: f32,

    cam_pos: vec3<f32>, cam_zoom: f32,
    cam_right: vec3<f32>, _pad3: f32,
    cam_up: vec3<f32>, _pad4: f32,
    cam_fwd: vec3<f32>, vignette: f32,

    flash_color: vec3<f32>, flash_strength: f32,
    bolt_intensity: f32, bolt_width: f32, bolt_glow: f32, bolt_count: f32,

    bolt_path: array<vec4<f32>, 8>,

    palette_amount: f32,
    palette_centroid_drive: f32,
    _pad5: f32, _pad6: f32,

    palette0: vec3<f32>, _ps0: f32,
    palette1: vec3<f32>, _ps1: f32,
    palette2: vec3<f32>, _ps2: f32,
    palette3: vec3<f32>, _ps3: f32,
    palette4: vec3<f32>, _ps4: f32,

    tunnel_glow: f32,
    morph_cap: f32,
    color_variance: f32,
    bolt_saturation: f32,
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

fn hue_rotate(c: vec3<f32>, a: f32) -> vec3<f32> {
    let k = vec3<f32>(0.57735026919);
    let co = cos(a); let si = sin(a);
    return c * co + cross(k, c) * si + k * dot(k, c) * (1.0 - co);
}

fn map_fn(p_in: vec3<f32>, prm1: f32, itime: f32) -> vec2<f32> {
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
    d = abs(d + prm1 * 3.0) + prm1 * 0.3 - 2.5;
    return vec2<f32>(d + cl * 0.2 + 0.25, cl);
}

fn segment_dist3(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let t = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    return length(pa - ba * t);
}

fn bolt_dist3(pos: vec3<f32>) -> f32 {
    let n = i32(P.bolt_count);
    if (n < 2) { return 9999.0; }
    var d = 9999.0;
    let last = min(n - 1, 7);
    for (var i = 0; i < 7; i = i + 1) {
        if (i >= last) { break; }
        let a = P.bolt_path[i].xyz;
        let b = P.bolt_path[i + 1].xyz;
        d = min(d, segment_dist3(pos, a, b));
    }
    return d;
}

fn saturate_color(c: vec3<f32>, sat: f32) -> vec3<f32> {
    let lum = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return max(mix(vec3<f32>(lum), c, sat), vec3<f32>(0.0));
}

fn render_clouds(
    ro: vec3<f32>,
    rd: vec3<f32>,
    prm1: f32,
    itime: f32,
    density_boost: f32,
) -> vec4<f32> {
    var rez = vec4<f32>(0.0);
    var t = 1.5;
    var fog_t = 0.0;
    let flash_on = P.flash_strength > 0.0001;
    // Tunnel glow base colour: original blue-grey crossfaded with a palette
    // mid-tone so dark palettes get a darker end-of-tunnel.
    let palette_mid = mix(P.palette1, P.palette3, 0.55);
    let tunnel_col_base = mix(vec3<f32>(0.06, 0.11, 0.11),
                              palette_mid * 0.35,
                              clamp(P.palette_amount, 0.0, 1.0));
    let tunnel_col = tunnel_col_base * P.tunnel_glow;

    // 160 steps with a tighter floor on the per-step distance — more wisp
    // detail in dense regions for ~25% more cost.
    for (var i = 0; i < 160; i = i + 1) {
        if (rez.a > 0.99) { break; }
        let pos = ro + t * rd;
        let mpv = map_fn(pos, prm1, itime);
        let den = clamp(mpv.x - 0.3, 0.0, 1.0) * 1.12 * density_boost;
        let dn = clamp(mpv.x + 2.0, 0.0, 3.0);

        var col = vec4<f32>(0.0);
        if (mpv.x > 0.6) {
            // Per-sample hue-phase variation: neighbouring puffs land on
            // different parts of the colour cycle. color_variance=0 reproduces
            // the original Nimitz colour pattern exactly.
            let v = P.color_variance;
            let phase_drift = (sin(pos.x * 0.35)
                             + cos(pos.z * 0.25 + 1.7)
                             + sin(pos.y * 0.55)) * 0.75 * v;
            let phases = vec3<f32>(
                5.0 + phase_drift,
                0.4 + phase_drift * 0.8,
                0.2 + phase_drift * 0.55,
            );
            let base = sin(phases
                + mpv.y * 0.1
                + sin(pos.z * 0.4) * 0.5
                + 1.8) * 0.5 + 0.5;
            col = vec4<f32>(base, 0.08);
            col = col * (den * den * den);
            col = vec4<f32>(col.rgb * (linstep(4.0, -2.5, mpv.x) * 2.3), col.a);

            var dif = clamp((den - map_fn(pos + 0.8, prm1, itime).x) / 9.0, 0.001, 1.0);
            dif = dif + clamp((den - map_fn(pos + 0.35, prm1, itime).x) / 2.5, 0.001, 1.0);
            let shade = vec3<f32>(0.005, 0.045, 0.075)
                + 1.5 * vec3<f32>(0.033, 0.07, 0.03) * dif;
            col = vec4<f32>(col.xyz * den * shade, col.a);
        }

        // 3D bolt: emissive accumulation. Apply saturation push so the core's
        // colour survives ACES tonemap instead of clipping to white.
        if (flash_on) {
            let bd = bolt_dist3(pos);
            let core = exp(-bd / max(P.bolt_width, 1e-4));
            let glow = exp(-bd * 0.55);
            let raw = P.flash_color * P.flash_strength
                * (core * P.bolt_intensity + glow * P.bolt_glow * den);
            let tinted = saturate_color(raw, P.bolt_saturation);
            col = vec4<f32>(col.rgb + tinted, max(col.a, core * 0.1));
        }

        let fog_c = exp(t * 0.2 - 2.2);
        col = col + vec4<f32>(tunnel_col, 0.1) * clamp(fog_c - fog_t, 0.0, 1.0);
        fog_t = fog_c;
        rez = rez + col * (1.0 - rez.a);
        t = t + clamp(0.5 - dn * dn * 0.05, 0.07, 0.28);
    }
    return clamp(rez, vec4<f32>(0.0), vec4<f32>(8.0));
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

@fragment
fn fs_clouds(in: VsOut) -> @location(0) vec4<f32> {
    let q = in.uv;
    let frag_coord = q * P.resolution;
    let p = (frag_coord - 0.5 * P.resolution) / P.resolution.y * P.cam_zoom;

    let itime = P.time;
    let prm1_base = smoothstep(-0.4, 0.4, sin(itime * 0.3));
    // morph_cap controls how plumey/closed the tunnel can get. The original
    // shader allowed prm1 up to 1.6 which produces wall-of-cloud moments —
    // capping ~1.0 keeps the tunnel breathable.
    let prm1 = clamp(prm1_base + P.morph + P.bass * P.bass_to_morph,
                     0.0, max(P.morph_cap, 0.1));
    // Density boost is hard-capped so transient hits / sustained bass can't
    // drown the camera in fog. Crank density_mul if you really want soup.
    let density_boost = clamp(
        P.density_mul + P.rms * P.rms_to_density + P.punch * 0.25,
        0.5, 1.45,
    );

    let ro = P.cam_pos;
    let rd = normalize(p.x * P.cam_right + p.y * P.cam_up + P.cam_fwd);

    let scn = render_clouds(ro, rd, prm1, itime, density_boost);
    var col = scn.rgb;
    col = i_lerp(col.bgr, col.rgb, clamp(1.0 - prm1, 0.05, 1.0));
    let nimitz = pow(col, vec3<f32>(0.55, 0.65, 0.6)) * vec3<f32>(1.0, 0.97, 0.9);

    let lum = dot(col, vec3<f32>(0.299, 0.587, 0.114));
    let pal_x = clamp(lum + (P.centroid - 0.5) * P.palette_centroid_drive, 0.0, 1.0);
    let graded = palette_lookup(pal_x) * (0.5 + 0.8 * lum);

    col = mix(nimitz, graded, P.palette_amount);
    col = hue_rotate(col, P.hue_shift + (P.centroid - 0.5) * P.centroid_to_hue);

    // Vignette. v=0 → no darkening, v=1 → full corners-go-to-zero.
    let v = clamp(P.vignette, 0.0, 1.0);
    let vfac = pow(16.0 * q.x * q.y * (1.0 - q.x) * (1.0 - q.y), 0.12);
    col = col * mix(vec3<f32>(1.0), vec3<f32>(vfac), v);

    return vec4<f32>(col, 1.0);
}
