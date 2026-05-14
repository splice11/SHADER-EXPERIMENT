// Placeholder volumetric cloud raymarcher.
// Grow into a Schneider-style pipeline: base 3D Worley+Perlin, detail Worley
// erosion, 2D weather map (coverage/type/height), cone-sampled secondary
// light march, Beer-Powder, temporal reprojection.

struct Params {
    resolution: vec2<f32>,
    time: f32,
    _pad0: f32,

    sun_dir: vec3<f32>,
    coverage: f32,

    density: f32,
    noise_scale: f32,
    steps: f32,
    light_steps: f32,

    hg_g: f32,
    absorption: f32,
    wind_speed: f32,
    cloud_height: f32,
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

// ---- noise --------------------------------------------------------------
fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q = q + dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = hash3(i + vec3<f32>(0.0, 0.0, 0.0));
    let n100 = hash3(i + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = hash3(i + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = hash3(i + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = hash3(i + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = hash3(i + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = hash3(i + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = hash3(i + vec3<f32>(1.0, 1.0, 1.0));
    let nx00 = mix(n000, n100, u.x);
    let nx10 = mix(n010, n110, u.x);
    let nx01 = mix(n001, n101, u.x);
    let nx11 = mix(n011, n111, u.x);
    let nxy0 = mix(nx00, nx10, u.y);
    let nxy1 = mix(nx01, nx11, u.y);
    return mix(nxy0, nxy1, u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var q = p;
    var amp = 0.5;
    var sum = 0.0;
    for (var i = 0; i < 5; i = i + 1) {
        sum = sum + amp * value_noise(q);
        q = q * 2.03;
        amp = amp * 0.5;
    }
    return sum;
}

// ---- cloud field --------------------------------------------------------
const CLOUD_BASE: f32 = 1.5;

fn cloud_density(pos: vec3<f32>) -> f32 {
    let thickness = P.cloud_height;
    let top = CLOUD_BASE + thickness;
    if (pos.y < CLOUD_BASE || pos.y > top) {
        return 0.0;
    }
    let wind = vec3<f32>(P.time * P.wind_speed, 0.0, P.time * P.wind_speed * 0.3);
    let q = (pos + wind) * P.noise_scale;
    let n = fbm(q);
    let h = (pos.y - CLOUD_BASE) / thickness;
    let shape = smoothstep(0.0, 0.2, h) * smoothstep(1.0, 0.7, h);
    let d = max(n - (1.0 - P.coverage), 0.0) * shape * P.density;
    return d;
}

fn hg_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = 1.0 + g2 - 2.0 * g * cos_theta;
    return (1.0 - g2) / (12.566370614 * pow(max(denom, 1e-4), 1.5));
}

fn light_march(pos: vec3<f32>) -> f32 {
    let n = i32(P.light_steps);
    var step_len = 0.15;
    var t = step_len;
    var dens = 0.0;
    for (var i = 0; i < n; i = i + 1) {
        let p = pos + P.sun_dir * t;
        dens = dens + cloud_density(p) * step_len;
        t = t + step_len;
        step_len = step_len * 1.3;
    }
    return exp(-dens * P.absorption * 8.0);
}

@fragment
fn fs_clouds(in: VsOut) -> @location(0) vec4<f32> {
    let aspect = P.resolution.x / max(P.resolution.y, 1.0);
    let ndc = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, 1.0);

    let cam_pos = vec3<f32>(0.0, 1.0, 0.0);
    let ray_dir = normalize(vec3<f32>(ndc.x, ndc.y * 0.6 + 0.2, 1.0));

    let sky_top = vec3<f32>(0.40, 0.58, 0.85);
    let sky_horizon = vec3<f32>(0.85, 0.88, 0.95);
    let sun = max(dot(ray_dir, P.sun_dir), 0.0);
    var sky = mix(sky_horizon, sky_top, clamp(ray_dir.y, 0.0, 1.0));
    sky = sky + vec3<f32>(1.0, 0.95, 0.85) * pow(sun, 32.0) * 0.7;

    let n = i32(P.steps);
    let t_max = 30.0;
    let dt = t_max / f32(n);

    var T = 1.0;
    var col = vec3<f32>(0.0);
    let phase = hg_phase(dot(ray_dir, P.sun_dir), P.hg_g);

    let jitter = fract(sin(dot(in.uv, vec2<f32>(12.9898, 78.233))) * 43758.5453);

    for (var i = 0; i < n; i = i + 1) {
        let t = (f32(i) + jitter) * dt;
        let p = cam_pos + ray_dir * t;
        if (p.y > CLOUD_BASE + P.cloud_height + 0.01 && ray_dir.y > 0.0) {
            break;
        }
        let d = cloud_density(p);
        if (d > 0.001) {
            let lt = light_march(p);
            let scatter = lt * phase * d * dt;
            let absorbed = exp(-d * dt * (P.absorption + 1.0));
            col = col + T * scatter * vec3<f32>(1.0, 0.95, 0.9);
            T = T * absorbed;
            if (T < 0.02) {
                break;
            }
        }
    }

    let out_col = sky * T + col;
    return vec4<f32>(pow(out_col, vec3<f32>(1.0 / 2.2)), 1.0);
}
