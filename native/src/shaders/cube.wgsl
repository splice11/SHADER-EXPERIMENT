// Port of Jaenam's "MotionCube" (CC BY-NC-SA 4.0).
// Shares the SceneParams uniform layout with clouds.wgsl so it can plug into
// the same bind group / HDR target. Audio + director drive the unfold pace,
// roll, zoom, and palette grade.

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

    god_ray_strength: f32,
    _pad_extra0: f32,
    _pad_extra1: f32,
    _pad_extra2: f32,
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

const PI: f32 = 3.14159265;

// GLSL's `v * R(a)` rotates v by -a (row-vector convention). We reproduce that
// here so the unfold animation matches the original Shadertoy.
fn rrot(v: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a); let s = sin(a);
    return vec2<f32>(c * v.x + s * v.y, -s * v.x + c * v.y);
}

fn box_sdf(p: vec3<f32>, b: vec3<f32>) -> f32 {
    return length(max(abs(p) - b, vec3<f32>(0.0)));
}

fn hash3u(s_in: vec3<u32>) -> vec3<u32> {
    var s = s_in * vec3<u32>(1145141919u) + vec3<u32>(1919810u);
    s.x = s.x + s.y * s.z;
    s.y = s.y + s.z * s.x;
    s.z = s.z + s.x * s.y;
    s = s ^ (s >> vec3<u32>(16u));
    s.x = s.x + s.y * s.z;
    s.y = s.y + s.z * s.x;
    s.z = s.z + s.x * s.y;
    return s;
}

fn hash3f(f: vec3<f32>) -> vec3<f32> {
    let u = vec3<u32>(bitcast<u32>(f.x), bitcast<u32>(f.y), bitcast<u32>(f.z));
    let h = hash3u(u);
    let inv = 1.0 / 4294967295.0;
    return vec3<f32>(f32(h.x), f32(h.y), f32(h.z)) * inv;
}

var<private> texPos: vec3<f32>;

fn cube_unfold(p_in: vec3<f32>, a: f32) -> f32 {
    var p = p_in;
    let yz0 = rrot(p.yz, 0.2 * PI);   p = vec3<f32>(p.x, yz0.x, yz0.y);
    let xz0 = rrot(p.xz, -0.25 * PI); p = vec3<f32>(xz0.x, p.y, xz0.y);

    var d = 1.0e9;
    let sz = 4.0; let th = 0.2;
    let sx = sign(p.x);
    var q: vec3<f32>;
    var fd: f32;

    // Bottom face
    fd = box_sdf(p - vec3<f32>(0.0, -sz, 0.0), vec3<f32>(sz, th, sz));
    if (fd < d) { d = fd; texPos = p; }

    // Top
    q = p;
    q.y = q.y + sz; q.z = q.z + sz;
    let r1 = rrot(q.yz, -a); q = vec3<f32>(q.x, r1.x, r1.y);
    q.y = q.y - sz; q.z = q.z - sz;
    q.y = q.y - sz; q.z = q.z + sz;
    let r2 = rrot(q.yz, -a); q = vec3<f32>(q.x, r2.x, r2.y);
    q.y = q.y + sz; q.z = q.z - sz;
    fd = box_sdf(q - vec3<f32>(0.0, sz, 0.0), vec3<f32>(sz, th, sz));
    if (fd < d) { d = fd; texPos = q; }

    // Front
    q = p;
    q.y = q.y + sz; q.z = q.z + sz;
    let r3 = rrot(q.yz, -a); q = vec3<f32>(q.x, r3.x, r3.y);
    q.y = q.y - sz; q.z = q.z - sz;
    fd = box_sdf(q - vec3<f32>(0.0, 0.0, -sz), vec3<f32>(sz, sz, th));
    if (fd < d) { d = fd; texPos = q; }

    // Back
    q = p;
    q.y = q.y + sz; q.z = q.z - sz;
    let r4 = rrot(q.yz, a); q = vec3<f32>(q.x, r4.x, r4.y);
    q.y = q.y - sz; q.z = q.z + sz;
    fd = box_sdf(q - vec3<f32>(0.0, 0.0, sz), vec3<f32>(sz, sz, th));
    if (fd < d) { d = fd; texPos = q; }

    // Left / Right (mirror X)
    q = vec3<f32>(abs(p.x), p.y, p.z);
    q.y = q.y + sz; q.x = q.x - sz;
    let r5 = rrot(q.xy, -a); q = vec3<f32>(r5.x, r5.y, q.z);
    q.y = q.y - sz; q.x = q.x + sz;
    fd = box_sdf(q - vec3<f32>(sz, 0.0, 0.0), vec3<f32>(th, sz, sz));
    if (fd < d) { d = fd; texPos = vec3<f32>(sx * q.x, q.y, q.z); }

    return d;
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
fn fs_cube(in: VsOut) -> @location(0) vec4<f32> {
    let r = P.resolution;
    let t = P.time;

    // Audio drives unfold pace + add a bass kick offset.
    let bass_kick = P.bass * 0.7 + P.punch * 0.4;
    let e = sin(t * (0.6 + P.bass_to_speed * 0.05) + bass_kick) * 0.5;
    let a = clamp(e + P.morph * 0.5, 0.0, 1.0) * 1.57;

    // Director-friendly extra roll via centroid (already comes from audio).
    let roll = sin(t * 0.4) * 0.1 + (P.centroid - 0.5) * 0.4;
    let Rx_a = sin(t - PI * 0.5) + roll;
    let Ry_a = sin(t + PI * 0.5) - roll * 0.5;

    var O = vec3<f32>(0.0);
    var d = 0.0;

    let frag = in.uv * r;
    let iters = 100;
    for (var i: i32 = 1; i <= iters; i = i + 1) {
        let fi = f32(i);
        let uv = (frag + frag - r) / r.y;
        let fl = 20.0;
        let ro = vec3<f32>(0.0, 0.0, 9.5 * fl);
        let rd = normalize(vec3<f32>(uv * 1.2 * P.cam_zoom, -fl));
        var p = ro + rd * d;

        let pxy = rrot(p.xy, Rx_a); p = vec3<f32>(pxy.x, pxy.y, p.z);
        let pyz = rrot(p.yz, Ry_a); p = vec3<f32>(p.x, pyz.x, pyz.y);

        let z = -tanh(e * 8.0 - fi * 0.005);
        let zoom = 0.9 + 0.3 * z;
        p = p * zoom;

        let cube = cube_unfold(p, a);
        var tex = texPos * 0.8;

        // Holographic ring pattern
        let g = floor(tex * 2.0);
        let f = fract(tex * 2.0) - vec3<f32>(0.5);
        let rnd = hash3f(g);
        let ang = rnd.y * 6.28;
        let h = smoothstep(0.08, 0.0, abs(length(f) - (rnd.x * 0.3 + 0.1)));

        // Turbulence
        for (var n: f32 = 1.0; n <= 3.0; n = n + 1.0) {
            tex = tex + (abs(fract(tex.zyx * n / 6.28 + vec3<f32>(0.75)) - vec3<f32>(0.5)) * 4.0 - vec3<f32>(1.0)) * 1.57 / n;
        }

        let s_step = (0.005 + 0.1 * abs(dot(abs(fract(tex) - vec3<f32>(0.5)), vec3<f32>(0.6)) - cube * 2.0 - fi / 300.0)) / zoom;
        d = d + s_step;
        if (d > 300.0 || s_step < 0.0001) { break; }

        let sf = smoothstep(0.02, 0.01, s_step);
        let base = (0.5 + 0.5 * sin(fi * 0.3 + vec3<f32>(-1.0, 0.0, 1.0) * 5.0)) / s_step;
        let ring = sf * 4.0 * h * (0.5 + 0.5 * sin(ang + fi * 0.1 + vec3<f32>(1.0, 2.0, 3.0))) / s_step;
        O = O + (base + ring) / zoom;
    }

    // Soft compress to keep values sane, then expose linear HDR for bloom.
    let compressed = tanh(O * O / vec3<f32>(1.0e7));

    // Palette grade: blend the rainbow output with a palette-keyed remap.
    let lum = dot(compressed, vec3<f32>(0.299, 0.587, 0.114));
    let pal_x = clamp(lum + (P.centroid - 0.5) * P.palette_centroid_drive, 0.0, 1.0);
    let graded = palette_lookup(pal_x) * (0.4 + 1.2 * lum);
    var col = mix(compressed, graded, P.palette_amount * 0.6);

    // Vignette
    let q = in.uv;
    let v = clamp(P.vignette, 0.0, 1.0);
    let vfac = pow(16.0 * q.x * q.y * (1.0 - q.x) * (1.0 - q.y), 0.12);
    col = col * mix(vec3<f32>(1.0), vec3<f32>(vfac), v);

    // Lightning bloom on the cube — bright flash on screen during strikes,
    // tinted by the palette accent (flash_color).
    if (P.flash_strength > 0.0001) {
        col = col + P.flash_color * P.flash_strength * 0.6;
    }

    return vec4<f32>(col, 1.0);
}
