use bytemuck::{Pod, Zeroable};

// Std140 uniform layout. All fields are f32-aligned; vec3 is aligned to 16.
// Mirror exactly in clouds.wgsl.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CloudParams {
    pub resolution: [f32; 2],
    pub time: f32,
    pub _pad0: f32,

    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub centroid: f32,

    pub rms: f32,
    pub punch: f32,
    pub _pad1: f32,
    pub _pad2: f32,

    pub speed: f32,
    pub morph: f32,
    pub density_mul: f32,
    pub hue_shift: f32,

    pub bass_to_speed: f32,
    pub bass_to_morph: f32,
    pub centroid_to_hue: f32,
    pub rms_to_density: f32,
}

impl Default for CloudParams {
    fn default() -> Self {
        Self {
            resolution: [1.0, 1.0],
            time: 0.0,
            _pad0: 0.0,

            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            centroid: 0.5,

            rms: 0.0,
            punch: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,

            speed: 3.0,
            morph: 0.0,
            density_mul: 1.0,
            hue_shift: 0.0,

            bass_to_speed: 3.0,
            bass_to_morph: 0.6,
            centroid_to_hue: 1.2,
            rms_to_density: 0.5,
        }
    }
}
