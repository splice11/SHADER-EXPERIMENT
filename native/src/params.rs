use crate::palettes::PALETTES;
use bytemuck::{Pod, Zeroable};

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

    // Lightning — flash injected as emissive into the volume.
    pub flash_pos: [f32; 3],
    pub flash_strength: f32,

    pub flash_color: [f32; 3],
    pub bolt_intensity: f32,

    pub bolt_anchor: [f32; 2],
    pub bolt_seed: f32,
    pub bolt_width: f32,

    pub palette_amount: f32, // 0 = original Nimitz grade, 1 = full palette
    pub palette_centroid_drive: f32,
    pub _pad3: f32,
    pub _pad4: f32,

    // 5 palette stops (vec3 + pad each so they're vec4-aligned).
    pub palette0: [f32; 3], pub _ps0: f32,
    pub palette1: [f32; 3], pub _ps1: f32,
    pub palette2: [f32; 3], pub _ps2: f32,
    pub palette3: [f32; 3], pub _ps3: f32,
    pub palette4: [f32; 3], pub _ps4: f32,
}

impl Default for CloudParams {
    fn default() -> Self {
        let p = PALETTES[0].stops;
        Self {
            resolution: [1.0, 1.0],
            time: 0.0,
            _pad0: 0.0,

            bass: 0.0, mid: 0.0, treble: 0.0, centroid: 0.5,
            rms: 0.0, punch: 0.0, _pad1: 0.0, _pad2: 0.0,

            speed: 3.0, morph: 0.0, density_mul: 1.0, hue_shift: 0.0,

            bass_to_speed: 3.0,
            bass_to_morph: 0.6,
            centroid_to_hue: 0.0,
            rms_to_density: 0.5,

            flash_pos: [0.0, 0.0, 0.0],
            flash_strength: 0.0,
            flash_color: [0.78, 0.88, 1.20],
            bolt_intensity: 2.5,
            bolt_anchor: [0.5, 0.5],
            bolt_seed: 0.0,
            bolt_width: 0.0025,

            palette_amount: 0.85,
            palette_centroid_drive: 0.25,
            _pad3: 0.0, _pad4: 0.0,

            palette0: p[0], _ps0: 0.0,
            palette1: p[1], _ps1: 0.0,
            palette2: p[2], _ps2: 0.0,
            palette3: p[3], _ps3: 0.0,
            palette4: p[4], _ps4: 0.0,
        }
    }
}

impl CloudParams {
    pub fn set_palette(&mut self, stops: &[[f32; 3]; 5]) {
        self.palette0 = stops[0];
        self.palette1 = stops[1];
        self.palette2 = stops[2];
        self.palette3 = stops[3];
        self.palette4 = stops[4];
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PostParams {
    pub threshold: f32,
    pub knee: f32,
    pub intensity: f32,
    pub exposure: f32,
}

impl Default for PostParams {
    fn default() -> Self {
        Self {
            threshold: 1.0,
            knee: 0.4,
            intensity: 0.45,
            exposure: 1.0,
        }
    }
}
