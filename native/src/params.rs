use crate::palettes::PALETTES;
use bytemuck::{Pod, Zeroable};

pub const BOLT_PATH_LEN: usize = 8;

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

    // Camera basis is computed CPU-side (smoothed follow + director kicks).
    pub cam_pos: [f32; 3],
    pub cam_zoom: f32,

    pub cam_right: [f32; 3],
    pub _pad3: f32,

    pub cam_up: [f32; 3],
    pub _pad4: f32,

    pub cam_fwd: [f32; 3],
    pub vignette: f32,

    // Lightning
    pub flash_color: [f32; 3],
    pub flash_strength: f32,

    pub bolt_intensity: f32,
    pub bolt_width: f32,
    pub bolt_glow: f32,
    pub bolt_count: f32,

    // 8 path control points (xyz + unused).
    pub bolt_path: [[f32; 4]; BOLT_PATH_LEN],

    // Palette
    pub palette_amount: f32,
    pub palette_centroid_drive: f32,
    pub _pad5: f32,
    pub _pad6: f32,

    pub palette0: [f32; 3], pub _ps0: f32,
    pub palette1: [f32; 3], pub _ps1: f32,
    pub palette2: [f32; 3], pub _ps2: f32,
    pub palette3: [f32; 3], pub _ps3: f32,
    pub palette4: [f32; 3], pub _ps4: f32,

    // Aesthetic knobs added in the lightning/colour/detail pass.
    pub tunnel_glow: f32,
    pub morph_cap: f32,
    pub color_variance: f32,
    pub bolt_saturation: f32,

    // Render-only quality knobs. The live app keeps these modest; the bake
    // job bumps them so the recorded video gets crisp clouds at 1440p+.
    pub quality_steps: f32,       // max raymarch iterations (cap 320 in shader)
    pub quality_step_floor: f32,  // minimum per-step distance
    pub bolt_invert: f32,         // 0 = bright bolts, 1 = "shadow" bolts that darken clouds
    pub light_phase: f32,         // radians — CPU-rotated so shading highlights don't stay on the same screen quadrant
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

            // Bass-route gains default to 0 — the music director already
            // adds intensity via swell/drop, and double-modulation from
            // bass-on-top-of-bass tended to feel busy.
            bass_to_speed: 0.0,
            bass_to_morph: 0.0,
            centroid_to_hue: 0.0,
            rms_to_density: 0.45,

            cam_pos: [0.0, 0.0, 0.0],
            // Wide default zoom — the airy "high FOV, pulled back" look reads
            // better at 1440p than the tight 1.0 default.
            cam_zoom: 2.0,
            cam_right: [1.0, 0.0, 0.0],
            _pad3: 0.0,
            cam_up: [0.0, 1.0, 0.0],
            _pad4: 0.0,
            cam_fwd: [0.0, 0.0, 1.0],
            vignette: 0.7,

            flash_color: [0.78, 0.88, 1.20],
            flash_strength: 0.0,
            bolt_intensity: 2.4,
            bolt_width: 0.24,
            bolt_glow: 1.4,
            bolt_count: 0.0,

            bolt_path: [[0.0; 4]; BOLT_PATH_LEN],

            palette_amount: 0.85,
            palette_centroid_drive: 0.25,
            _pad5: 0.0, _pad6: 0.0,

            palette0: p[0], _ps0: 0.0,
            palette1: p[1], _ps1: 0.0,
            palette2: p[2], _ps2: 0.0,
            palette3: p[3], _ps3: 0.0,
            palette4: p[4], _ps4: 0.0,

            tunnel_glow: 1.0,
            morph_cap: 0.95,
            color_variance: 0.40,
            bolt_saturation: 1.6,
            quality_steps: 140.0,
            quality_step_floor: 0.085,
            bolt_invert: 0.0,
            light_phase: 0.0,
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

    pub contrast: f32,
    pub saturation: f32,
    pub grain: f32,
    pub time: f32,

    pub aberration: f32,
    pub letterbox_aspect: f32, // 0 = off; >0 = target aspect (e.g. 2.39)
    pub anamorphic: f32,
    pub vignette: f32,

    pub resolution: [f32; 2],
    pub fade_in: f32,        // 1.0 = normal; <1 = darken final image (used for bake start-from-black)
    pub radial_blur: f32,    // 0 = none; ~0.04 is a strong "hyperdrive streak" toward centre

    // Colour grading additions:
    pub black_point: f32,          // crushes anything below this to true 0 after tonemap (inky shadows)
    pub highlight_softness: f32,   // 0..1 — how aggressively peaks desaturate toward white at the tonemap shoulder
    pub _pad_color0: f32,
    pub _pad_color1: f32,
}

impl Default for PostParams {
    fn default() -> Self {
        Self {
            threshold: 1.1,
            knee: 0.5,
            intensity: 0.38,
            // Bumped from 1.0 → 1.5 so HDR values actually reach the new
            // tonemap shoulder (where the creamy roll-off happens).
            exposure: 1.5,

            // Contrast is now 1.0 by default — the chroma-preserving tonemap
            // gives the image a natural S-shape, so adding linear contrast
            // on top just crushed things. User can dial back in if wanted.
            contrast: 1.0,
            // Selective saturation only fires in midtones, so we can push
            // this higher without nuking shadow noise / highlight detail.
            saturation: 1.30,
            grain: 0.0,
            time: 0.0,

            aberration: 0.6,
            letterbox_aspect: 0.0,
            anamorphic: 0.25,
            vignette: 0.0,

            resolution: [1.0, 1.0],
            fade_in: 1.0,
            radial_blur: 0.0,

            // Inky shadows: anything below this becomes 0 in LDR.
            black_point: 0.045,
            // 0 = peaks stay fully saturated (often blow ugly), 1 = peaks
            // desaturate to pure white at the shoulder. 0.55 gives film-like
            // creamy highlights without losing hue on saturated peaks.
            highlight_softness: 0.55,
            _pad_color0: 0.0,
            _pad_color1: 0.0,
        }
    }
}
