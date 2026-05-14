use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CloudParams {
    pub resolution: [f32; 2],
    pub time: f32,
    pub _pad0: f32,

    pub sun_dir: [f32; 3],
    pub coverage: f32,

    pub density: f32,
    pub noise_scale: f32,
    pub steps: f32,
    pub light_steps: f32,

    pub hg_g: f32,
    pub absorption: f32,
    pub wind_speed: f32,
    pub cloud_height: f32,
}

impl Default for CloudParams {
    fn default() -> Self {
        Self {
            resolution: [1.0, 1.0],
            time: 0.0,
            _pad0: 0.0,
            sun_dir: normalize3([0.3, 0.7, 0.4]),
            coverage: 0.55,
            density: 0.8,
            noise_scale: 0.6,
            steps: 64.0,
            light_steps: 6.0,
            hg_g: 0.5,
            absorption: 0.08,
            wind_speed: 0.15,
            cloud_height: 1.0,
        }
    }
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}
