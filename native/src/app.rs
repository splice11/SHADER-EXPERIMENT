use crate::{
    audio::{Audio, Features as AudioFeatures},
    bake::{ffmpeg_available, BakeJob},
    palettes::PALETTES,
    params::{CloudParams, PostParams, BOLT_PATH_LEN},
    renderer::Renderer,
    ui,
};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

// ---------- math helpers ----------

fn v_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn v_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn v_scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn v_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
}
fn v_norm(v: [f32; 3]) -> [f32; 3] {
    let m = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt().max(1e-6);
    [v[0]/m, v[1]/m, v[2]/m]
}
fn v_lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0]-a[0])*t, a[1] + (b[1]-a[1])*t, a[2] + (b[2]-a[2])*t]
}

fn disp_xy(t: f32) -> [f32; 2] {
    [(t * 0.22).sin() * 2.0, (t * 0.175).cos() * 2.0]
}

pub fn run_bake_chunk(s: &mut AppState) {
    // Bake several frames per UI tick so the user isn't waiting at 60fps —
    // the GPU can usually do this much faster than realtime since we're not
    // vsync-bound. 8 frames per tick still lets the UI redraw at ~7 Hz.
    let frames_per_tick = 8u32;
    let mut finished = false;
    let mut bake_error: Option<String> = None;

    if let Some(job) = s.bake.as_mut() {
        for _ in 0..frames_per_tick {
            if job.done() {
                finished = true;
                break;
            }
            let t = job.frame_index as f32 / job.fps as f32;
            let feat = s.audio.features_at_secs(t);
            match job.step(&s.renderer, feat) {
                Ok(done) => {
                    if done {
                        finished = true;
                        break;
                    }
                }
                Err(e) => {
                    bake_error = Some(format!("bake error: {e:#}"));
                    finished = true;
                    break;
                }
            }
        }
    }

    if finished {
        if let Some(job) = s.bake.take() {
            if let Some(msg) = bake_error {
                job.abort();
                s.bake_message = Some(msg);
            } else {
                match job.finish() {
                    Ok(path) => {
                        s.bake_message = Some(format!("Bake complete: {}", path.display()));
                        log::info!("bake complete: {}", path.display());
                    }
                    Err(e) => {
                        s.bake_message = Some(format!("ffmpeg failed: {e:#}"));
                    }
                }
            }
        }
        // Restore the live render targets to the actual window size — the
        // bake may have temporarily resized them up to 4K.
        if let Some((w, h)) = s.pre_bake_window_size.take() {
            s.renderer.resize(w, h);
        }
    }

    // Draw a progress overlay to the swapchain so the user knows something
    // is happening.
    let frame = match s.renderer.surface.get_current_texture() {
        Ok(f) => f,
        Err(_) => return,
    };
    let view = frame.texture.create_view(&Default::default());

    let raw = s.egui_state.take_egui_input(&s.renderer.window);
    let bake_state = s.bake.as_ref().map(|b| (b.frame_index, b.total_frames, b.output_path.clone()));
    let full = s.egui_ctx.clone().run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Baking music video…");
            if let Some((done, total, path)) = bake_state.as_ref() {
                let frac = if *total > 0 { *done as f32 / *total as f32 } else { 0.0 };
                ui.add(egui::ProgressBar::new(frac).text(format!("{done} / {total}")));
                ui.label(format!("→ {}", path.display()));
                ui.small("Window may feel sluggish; ffmpeg encodes once frames are piped in.");
            } else {
                ui.label("Finalising…");
            }
        });
    });
    s.egui_state
        .handle_platform_output(&s.renderer.window, full.platform_output);
    let paint_jobs = s.egui_ctx.tessellate(full.shapes, full.pixels_per_point);
    let screen_desc = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [s.renderer.config.width, s.renderer.config.height],
        pixels_per_point: full.pixels_per_point,
    };
    for (id, delta) in &full.textures_delta.set {
        s.egui_renderer
            .update_texture(&s.renderer.device, &s.renderer.queue, *id, delta);
    }

    let mut enc = s.renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("bake-overlay"),
    });
    s.egui_renderer
        .update_buffers(&s.renderer.device, &s.renderer.queue, &mut enc, &paint_jobs, &screen_desc);
    {
        let mut rp = enc
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bake-overlay-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.03, g: 0.03, b: 0.04, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            })
            .forget_lifetime();
        s.egui_renderer.render(&mut rp, &paint_jobs, &screen_desc);
    }
    s.renderer.queue.submit(Some(enc.finish()));
    frame.present();
    for id in &full.textures_delta.free {
        s.egui_renderer.free_texture(id);
    }
}

pub fn hash_u32(seed: u32, salt: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E3779B1).wrapping_add(salt.wrapping_mul(0x85EBCA77));
    x ^= x >> 16; x = x.wrapping_mul(0x7FEB352D);
    x ^= x >> 15; x = x.wrapping_mul(0x846CA68B);
    x ^= x >> 16;
    (x as f32 / u32::MAX as f32).clamp(0.0, 1.0)
}

// ---------- camera ----------

pub struct Camera {
    pub pos: [f32; 3],
    pub lookat: [f32; 3],
    pub z: f32, // integrated forward distance
    pub sway_amp: f32,
    pub follow_secs: f32,
    pub kick_offset: [f32; 3],
    pub kick_vel: [f32; 3],
    pub roll: f32, // radians, around forward
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0, 0.0],
            lookat: [0.0, 0.0, 3.5],
            z: 0.0,
            sway_amp: 0.55,
            follow_secs: 0.32,
            kick_offset: [0.0; 3],
            kick_vel: [0.0; 3],
            roll: 0.0,
        }
    }
}

impl Camera {
    pub fn integrate(&mut self, speed: f32, dt: f32) {
        self.z += speed * dt;
    }

    fn target_pos(&self) -> [f32; 3] {
        let d = disp_xy(self.z);
        [d[0] * self.sway_amp, d[1] * self.sway_amp, self.z]
    }
    fn target_look(&self) -> [f32; 3] {
        let ahead = self.z + 3.5;
        let d = disp_xy(ahead);
        [d[0] * self.sway_amp, d[1] * self.sway_amp, ahead]
    }

    pub fn smooth_follow(&mut self, dt: f32) {
        let alpha = 1.0 - (-dt / self.follow_secs.max(1e-3)).exp();
        self.pos = v_lerp(self.pos, self.target_pos(), alpha);
        self.lookat = v_lerp(self.lookat, self.target_look(), alpha);
    }

    pub fn apply_kick_spring(&mut self, dt: f32) {
        // critically-damped spring back to zero offset
        let k = 60.0;
        let c = 14.0;
        for i in 0..3 {
            let accel = -self.kick_offset[i] * k - self.kick_vel[i] * c;
            self.kick_vel[i] += accel * dt;
            self.kick_offset[i] += self.kick_vel[i] * dt;
        }
    }

    pub fn add_kick(&mut self, v: [f32; 3]) {
        for i in 0..3 {
            self.kick_vel[i] += v[i];
        }
    }

    /// Compute right/up/fwd basis with roll. Returns (right, up, fwd).
    pub fn basis(&self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let pos = v_add(self.pos, self.kick_offset);
        let fwd = v_norm(v_sub(self.lookat, pos));
        let world_up = [0.0, 1.0, 0.0];
        let right0 = v_norm(v_cross(fwd, world_up));
        let up0 = v_norm(v_cross(right0, fwd));
        // Apply roll around fwd: rotate (right, up) by self.roll.
        let cr = self.roll.cos();
        let sr = self.roll.sin();
        let right = v_add(v_scale(right0, cr), v_scale(up0, sr));
        let up = v_add(v_scale(up0, cr), v_scale(right0, -sr));
        (right, up, fwd)
    }

    pub fn world_pos(&self) -> [f32; 3] {
        v_add(self.pos, self.kick_offset)
    }
}

// ---------- music director ----------

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DirectorFeel {
    Off,
    Subtle,
    Cinematic,
    Theatrical,
}

impl DirectorFeel {
    /// Multiplier applied at consumption sites (NOT fed back into smoothing).
    pub fn amount(self) -> f32 {
        match self {
            DirectorFeel::Off => 0.0,
            DirectorFeel::Subtle => 0.5,
            DirectorFeel::Cinematic => 1.0,
            DirectorFeel::Theatrical => 1.6,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Section {
    Lull,
    Cruise,
    Peak,
}

pub struct Director {
    pub feel: DirectorFeel,

    // smoothed audio
    pub e_short: f32,
    pub e_long: f32,
    pub e_very_long: f32,
    pub punch_baseline: f32,

    // raw (unscaled) detector outputs in [0, 1]
    pub swell: f32,
    pub drop: f32,
    pub lull: f32,
    /// Sustained-silence envelope: rises only after several seconds of
    /// near-zero RMS. Used to drive the "empty tunnel when no music" look.
    pub silence: f32,

    // section state w/ hysteresis
    pub section: Section,
    pub section_age: f32,
    pub section_changed_at: f32, // wall-clock time of last change (params.time)

    // beat / onset interval estimate
    pub last_onset_time: f32,
    pub onset_intervals: [f32; 6], // ring buffer of recent IBI samples
    pub onset_idx: usize,
    pub estimated_period: f32, // seconds; 0 if unknown

    pub roll_phase: f32,
    pub seed: u32,

    // palette auto-rotation
    pub auto_palette: bool,
    pub palette_cooldown: f32, // seconds since last auto-swap
}

impl Default for Director {
    fn default() -> Self {
        Self {
            feel: DirectorFeel::Subtle,
            e_short: 0.0, e_long: 0.0, e_very_long: 0.0,
            punch_baseline: 0.0,
            swell: 0.0, drop: 0.0, lull: 0.0, silence: 0.0,
            section: Section::Cruise,
            section_age: 0.0,
            section_changed_at: 0.0,
            last_onset_time: -1.0,
            onset_intervals: [0.0; 6],
            onset_idx: 0,
            estimated_period: 0.0,
            roll_phase: 0.0,
            seed: 1,
            auto_palette: false,
            palette_cooldown: 0.0,
        }
    }
}

pub struct DirectorTick {
    pub drop_trigger: f32, // raw transient magnitude (unscaled)
    pub section_changed: bool,
}

impl Director {
    pub fn update(&mut self, audio: &AudioFeatures, now: f32, dt: f32) -> DirectorTick {
        // Long/short/very-long energy EMAs.
        let alpha_s = 1.0 - (-dt / 0.20).exp();
        let alpha_l = 1.0 - (-dt / 3.0).exp();
        let alpha_vl = 1.0 - (-dt / 10.0).exp();
        let alpha_pb = 1.0 - (-dt / 1.5).exp();
        self.e_short += alpha_s * (audio.rms - self.e_short);
        self.e_long += alpha_l * (audio.rms - self.e_long);
        self.e_very_long += alpha_vl * (audio.rms - self.e_very_long);
        self.punch_baseline += alpha_pb * (audio.punch - self.punch_baseline);

        // Swell: short-term excess over long-term, smoothed. No multiplier
        // feedback here — scaling happens at consumption time.
        let swell_raw = ((self.e_short - self.e_long * 1.05) / (self.e_long + 0.05))
            .clamp(0.0, 1.5)
            * 0.4;
        self.swell += (1.0 - (-dt / 0.45).exp()) * (swell_raw - self.swell);
        self.swell = self.swell.clamp(0.0, 1.0);

        // Drop: large transient above its rolling baseline.
        let drop_trigger = (audio.punch - self.punch_baseline - 0.18).max(0.0);
        self.drop = (self.drop - dt * 2.4).max(0.0);
        self.drop = self.drop.max(drop_trigger * 1.4).min(1.0);

        // Lull: long-term energy near floor.
        let lull_raw = (1.0 - self.e_long * 5.0).clamp(0.0, 1.0);
        self.lull += (1.0 - (-dt / 1.0).exp()) * (lull_raw - self.lull);
        self.lull = self.lull.clamp(0.0, 1.0);

        // Silence: requires sustained near-zero RMS to engage. Drives the
        // "no clouds when no music" effect. Asymmetric attack/release: slow
        // to engage (so brief quiet passages don't dissolve the scene), fast
        // to release (clouds reappear as soon as the music kicks back in).
        let silent = self.e_long < 0.04 && self.e_very_long < 0.05;
        let silence_target = if silent { 1.0 } else { 0.0 };
        let silence_tau = if silent { 2.5 } else { 0.6 };
        self.silence += (1.0 - (-dt / silence_tau).exp()) * (silence_target - self.silence);
        self.silence = self.silence.clamp(0.0, 1.0);

        // Beat-ish onset interval estimate. Each big drop is treated as an
        // onset; we collect a rolling window of inter-onset intervals and use
        // the median as the period. Crude but enough for sectioning + display.
        if drop_trigger > 0.12 && self.last_onset_time >= 0.0 {
            let ibi = now - self.last_onset_time;
            if (0.20..1.20).contains(&ibi) {
                // plausible BPM range: 50-300
                self.onset_intervals[self.onset_idx] = ibi;
                self.onset_idx = (self.onset_idx + 1) % self.onset_intervals.len();
                let mut samples: Vec<f32> = self.onset_intervals
                    .iter().copied().filter(|x| *x > 0.0).collect();
                if samples.len() >= 3 {
                    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    self.estimated_period = samples[samples.len() / 2];
                }
            }
        }
        if drop_trigger > 0.05 {
            self.last_onset_time = now;
        }

        // Section with hysteresis: avoid flip-flopping when the very-long
        // energy hovers around a threshold.
        self.section_age += dt;
        let lo = self.e_very_long.min(self.e_long);
        let hi = self.e_very_long.max(self.e_long);
        let (low_th, high_th) = match self.section {
            Section::Lull => (0.10, 0.18),    // need to climb past 0.18 to leave lull
            Section::Cruise => (0.07, 0.32),  // need to drop below 0.07 or climb past 0.32
            Section::Peak => (0.24, 0.40),    // need to drop below 0.24 to leave peak
        };
        let new_section = if hi >= high_th {
            Section::Peak
        } else if lo <= low_th {
            Section::Lull
        } else {
            Section::Cruise
        };
        let mut section_changed = false;
        // Require at least 1.5 s in current section before switching.
        if new_section != self.section && self.section_age > 1.5 {
            self.section = new_section;
            self.section_age = 0.0;
            self.section_changed_at = now;
            section_changed = true;
        }

        self.roll_phase += dt * 0.13;
        self.palette_cooldown += dt;

        DirectorTick { drop_trigger, section_changed }
    }

    pub fn bpm(&self) -> f32 {
        if self.estimated_period > 0.05 {
            60.0 / self.estimated_period
        } else {
            0.0
        }
    }
}

// ---------- lightning ----------

pub struct Lightning {
    pub strength: f32,
    pub cooldown: f32,
    pub auto: bool,
    pub threshold: f32,
    pub cooldown_secs: f32,
    pub peak_intensity: f32,
    pub seed_counter: u32,
}

impl Default for Lightning {
    fn default() -> Self {
        Self {
            strength: 0.0,
            cooldown: 0.0,
            auto: true,
            threshold: 0.45,
            cooldown_secs: 0.40,
            peak_intensity: 1.1,
            seed_counter: 0,
        }
    }
}

impl Lightning {
    pub fn maybe_trigger(&mut self, punch: f32, dt: f32) -> bool {
        self.cooldown = (self.cooldown - dt).max(0.0);
        if self.auto && punch > self.threshold && self.cooldown <= 0.0 {
            self.cooldown = self.cooldown_secs;
            self.seed_counter = self.seed_counter.wrapping_add(1);
            self.strength = self.peak_intensity * (1.0 + (punch - self.threshold) * 1.2);
            true
        } else {
            false
        }
    }

    pub fn force_trigger(&mut self) {
        self.cooldown = self.cooldown_secs;
        self.seed_counter = self.seed_counter.wrapping_add(1);
        self.strength = self.peak_intensity;
    }

    pub fn decay(&mut self, dt: f32) {
        if self.strength > 0.0 {
            self.strength *= (-12.0 * dt).exp();
            if self.strength < 0.002 {
                self.strength = 0.0;
            }
        }
    }
}

pub fn build_bolt_path(seed: u32, cam_z: f32, cam_x: f32) -> [[f32; 4]; BOLT_PATH_LEN] {
    let mut path = [[0.0f32; 4]; BOLT_PATH_LEN];
    // Start above & in front of camera; end below & a bit further.
    let ahead = 6.0 + hash_u32(seed, 1) * 5.0;
    let lateral = (hash_u32(seed, 2) - 0.5) * 4.0 + cam_x * 0.5;
    let end_lateral = lateral + (hash_u32(seed, 3) - 0.5) * 3.5;
    let start = [lateral, 5.0, cam_z + ahead];
    let end = [end_lateral, -5.0, cam_z + ahead + (hash_u32(seed, 4) - 0.5) * 2.0];
    let n = BOLT_PATH_LEN;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let base = [
            start[0] + (end[0] - start[0]) * t,
            start[1] + (end[1] - start[1]) * t,
            start[2] + (end[2] - start[2]) * t,
        ];
        // Most jitter in the middle, less at ends.
        let bell = (1.0 - (2.0 * t - 1.0).abs()).max(0.0);
        let jx = (hash_u32(seed, 100 + i as u32) - 0.5) * 2.0 * bell;
        let jz = (hash_u32(seed, 200 + i as u32) - 0.5) * 1.4 * bell;
        path[i] = [base[0] + jx, base[1], base[2] + jz, 0.0];
    }
    path
}

// ---------- palette crossfade ----------

/// Holds the "from" snapshot of a palette swap so the live + bake paths can
/// smoothly lerp into a new palette over a couple of seconds instead of
/// snapping. `t` advances from 0 → 1 at rate dt / duration.
#[derive(Copy, Clone)]
pub struct PaletteCrossfade {
    pub from_stops: [[f32; 3]; 5],
    pub from_accent: [f32; 3],
    pub t: f32,
    pub duration: f32,
}

impl Default for PaletteCrossfade {
    fn default() -> Self {
        Self {
            from_stops: [[0.0; 3]; 5],
            from_accent: [0.0; 3],
            t: 1.0, // start "done" — no blend in progress
            duration: 2.0,
        }
    }
}

impl PaletteCrossfade {
    /// Snapshot the current palette as "from" and reset the blend so the
    /// caller can write the destination palette into params over time.
    pub fn start(&mut self, params: &crate::params::CloudParams) {
        self.from_stops = [
            params.palette0, params.palette1, params.palette2,
            params.palette3, params.palette4,
        ];
        self.from_accent = params.flash_color;
        self.t = 0.0;
    }

    /// Advance and write the blended palette to `params`. `target` is the
    /// destination palette stops; `target_accent` only used if `use_accent`.
    pub fn step(
        &mut self,
        params: &mut crate::params::CloudParams,
        target: &[[f32; 3]; 5],
        target_accent: [f32; 3],
        use_accent: bool,
        dt: f32,
    ) {
        if self.t >= 1.0 { return; }
        self.t = (self.t + dt / self.duration.max(0.05)).min(1.0);
        // Smoothstep for a gentler transition than linear.
        let s = self.t * self.t * (3.0 - 2.0 * self.t);
        let lerp = |a: [f32; 3], b: [f32; 3]| -> [f32; 3] {
            [a[0] + (b[0]-a[0]) * s, a[1] + (b[1]-a[1]) * s, a[2] + (b[2]-a[2]) * s]
        };
        params.palette0 = lerp(self.from_stops[0], target[0]);
        params.palette1 = lerp(self.from_stops[1], target[1]);
        params.palette2 = lerp(self.from_stops[2], target[2]);
        params.palette3 = lerp(self.from_stops[3], target[3]);
        params.palette4 = lerp(self.from_stops[4], target[4]);
        if use_accent {
            params.flash_color = lerp(self.from_accent, target_accent);
        }
    }
}

// ---------- app state ----------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Scene {
    Clouds,
    Cube,
}

/// Output resolution selector for the bake job. `Window` reuses the current
/// window size; the others temporarily resize the render targets so the
/// recorded video can be sharper than the visible window.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BakeSize {
    Window,
    P1080,
    P1440,
    P2160,
}

impl BakeSize {
    pub fn label(self) -> &'static str {
        match self {
            BakeSize::Window => "window",
            BakeSize::P1080 => "1080p",
            BakeSize::P1440 => "1440p",
            BakeSize::P2160 => "2160p",
        }
    }
    /// Returns (w, h) for the given selector, falling back to `window` when
    /// `Window` is requested.
    pub fn dimensions(self, window: (u32, u32)) -> (u32, u32) {
        match self {
            BakeSize::Window => window,
            BakeSize::P1080 => (1920, 1080),
            BakeSize::P1440 => (2560, 1440),
            BakeSize::P2160 => (3840, 2160),
        }
    }
}

pub struct AppState {
    pub renderer: Renderer,
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,
    pub params: CloudParams,
    pub post: PostParams,
    pub start: Instant,
    pub last_frame: Instant,
    pub audio: Audio,
    pub camera: Camera,
    pub director: Director,
    pub lightning: Lightning,
    pub palette_index: usize,
    pub use_palette_accent: bool, // when true, palette swap retargets flash colour
    pub show_ui: bool,
    pub scene: Scene,
    pub ffmpeg_present: bool,
    pub bake: Option<BakeJob>,
    pub bake_fps: u32,
    pub bake_size: BakeSize,
    pub pre_bake_window_size: Option<(u32, u32)>,
    pub palette_crossfade: PaletteCrossfade,
    pub pending_audio_load: Option<std::path::PathBuf>,
    pub pending_bake: Option<std::path::PathBuf>,
    pub bake_message: Option<String>,
}

#[derive(Default)]
pub struct App {
    state: Option<AppState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() { return; }
        let attrs = Window::default_attributes()
            .with_title("shader-experiment — clouds (wgpu)")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        let window = Arc::new(event_loop.create_window(attrs).expect("create_window"));

        let renderer =
            pollster::block_on(Renderer::new(window.clone())).expect("renderer init failed");

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::viewport::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&renderer.device, renderer.config.format, None, 1, false);

        let audio = Audio::start();

        let mut params = CloudParams::default();
        params.set_palette(&PALETTES[0].stops);
        params.flash_color = PALETTES[0].accent;

        let ffmpeg_present = ffmpeg_available();
        if !ffmpeg_present {
            log::warn!("ffmpeg not found on PATH — music-video bake will be disabled");
        }

        self.state = Some(AppState {
            renderer, egui_ctx, egui_state, egui_renderer,
            params,
            post: PostParams::default(),
            start: Instant::now(),
            last_frame: Instant::now(),
            audio,
            camera: Camera::default(),
            director: Director::default(),
            lightning: Lightning::default(),
            palette_index: 0,
            use_palette_accent: true,
            show_ui: true,
            scene: Scene::Clouds,
            ffmpeg_present,
            bake: None,
            bake_fps: 60,
            bake_size: BakeSize::Window,
            pre_bake_window_size: None,
            palette_crossfade: PaletteCrossfade::default(),
            pending_audio_load: None,
            pending_bake: None,
            bake_message: None,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(s) = self.state.as_mut() else { return; };
        let resp = s.egui_state.on_window_event(&s.renderer.window, &event);

        match &event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(sz) => {
                // Don't recreate render targets mid-bake — the capture texture
                // is sized to the original target dimensions.
                if s.bake.is_none() {
                    s.renderer.resize(sz.width, sz.height);
                }
            }
            WindowEvent::RedrawRequested => render_frame(s),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !resp.consumed =>
            {
                if let PhysicalKey::Code(code) = event.physical_key {
                    handle_key(s, event_loop, code);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(s) = self.state.as_ref() {
            s.renderer.window.request_redraw();
        }
    }
}

fn handle_key(s: &mut AppState, event_loop: &ActiveEventLoop, code: KeyCode) {
    match code {
        KeyCode::F11 => toggle_fullscreen(s),
        KeyCode::KeyH => s.show_ui = !s.show_ui,
        KeyCode::KeyL => {
            // manual lightning
            s.lightning.force_trigger();
            let bolt_x = s.camera.world_pos()[0];
            s.params.bolt_path = build_bolt_path(s.lightning.seed_counter, s.camera.z, bolt_x);
            s.params.bolt_count = BOLT_PATH_LEN as f32;
        }
        KeyCode::Escape => {
            if s.renderer.window.fullscreen().is_some() {
                s.renderer.window.set_fullscreen(None);
            } else {
                event_loop.exit();
            }
        }
        KeyCode::KeyC => {
            // cycle scene
            s.scene = match s.scene {
                Scene::Clouds => Scene::Cube,
                Scene::Cube => Scene::Clouds,
            };
        }
        _ => {}
    }
}

fn toggle_fullscreen(s: &AppState) {
    if s.renderer.window.fullscreen().is_some() {
        s.renderer.window.set_fullscreen(None);
    } else {
        s.renderer.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
    }
}

// ---------- frame ----------

fn render_frame(s: &mut AppState) {
    // Resolve pending file load before anything else (the UI just queues the
    // path; actually loading happens here on the main thread).
    if let Some(path) = s.pending_audio_load.take() {
        match s.audio.load_file(&path) {
            Ok(()) => {
                s.audio.play();
                s.bake_message = None;
            }
            Err(e) => {
                let msg = format!("Failed to load {}: {e:#}", path.display());
                log::warn!("{msg}");
                s.bake_message = Some(msg);
            }
        }
    }

    // Resolve pending bake-start request.
    if let Some(out_path) = s.pending_bake.take() {
        if let Some(playback) = s.audio.file_playback() {
            let audio_path = playback.path.clone();
            let duration = s.audio.duration_secs().unwrap_or(0.0);
            // Stash the live window resolution and resize render targets to
            // the chosen bake output. The window itself stays the same; only
            // the off-screen scene/bloom/composite targets grow.
            let window_size = (s.renderer.config.width, s.renderer.config.height);
            let (bw, bh) = s.bake_size.dimensions(window_size);
            if (bw, bh) != window_size {
                s.pre_bake_window_size = Some(window_size);
                s.renderer.resize(bw, bh);
                s.params.resolution = [bw as f32, bh as f32];
            }
            match BakeJob::start(
                &s.renderer,
                &audio_path,
                out_path,
                duration,
                s.bake_fps,
                &s.params,
                &s.post,
                s.director.feel,
                s.scene,
                s.palette_index,
                s.use_palette_accent,
                s.director.auto_palette,
            ) {
                Ok(job) => {
                    log::info!(
                        "baking {} frames @ {} fps to {}",
                        job.total_frames,
                        job.fps,
                        job.output_path.display()
                    );
                    s.audio.pause();
                    s.bake_message = None;
                    s.bake = Some(job);
                }
                Err(e) => {
                    s.bake_message = Some(format!("Bake failed to start: {e:#}"));
                    // Roll back the resize if the bake never got off the ground.
                    if let Some((w, h)) = s.pre_bake_window_size.take() {
                        s.renderer.resize(w, h);
                    }
                }
            }
        } else {
            s.bake_message = Some("Load a track first.".to_string());
        }
    }

    // If a bake is running, step it (a few frames per UI tick to keep the
    // window responsive) and short-circuit the normal scene render.
    if s.bake.is_some() {
        run_bake_chunk(s);
        return;
    }

    let now = Instant::now();
    let dt = (now - s.last_frame).as_secs_f32().clamp(1e-4, 0.1);
    s.last_frame = now;

    let size = s.renderer.window.inner_size();
    s.params.resolution = [size.width as f32, size.height as f32];
    s.params.time = s.start.elapsed().as_secs_f32();

    let feat = s.audio.read();
    s.params.bass = feat.bass;
    s.params.mid = feat.mid;
    s.params.treble = feat.treble;
    s.params.centroid = feat.centroid;
    s.params.rms = feat.rms;
    s.params.punch = feat.punch;

    let tick = s.director.update(&feat, s.params.time, dt);
    let amt = s.director.feel.amount();
    // All consumption sites scale by `amt` — never mutate director state by it.
    let scaled_swell = (s.director.swell * amt).clamp(0.0, 1.5);
    let scaled_drop = (s.director.drop * amt).clamp(0.0, 1.5);

    // Optional palette auto-rotation on section changes (beat-aware). Instead
    // of snapping, kick off a smooth crossfade — `palette_crossfade.step()`
    // below writes blended stops into `params.palette*` every frame until done.
    if s.director.auto_palette && tick.section_changed
        && s.director.palette_cooldown > 6.0
    {
        s.palette_crossfade.start(&s.params);
        s.palette_index = (s.palette_index + 1) % PALETTES.len();
        s.director.palette_cooldown = 0.0;
    }

    // ---- lightning trigger from audio onset ----
    if s.lightning.maybe_trigger(feat.punch, dt) {
        let cam_x = s.camera.world_pos()[0];
        s.params.bolt_path = build_bolt_path(s.lightning.seed_counter, s.camera.z, cam_x);
        s.params.bolt_count = BOLT_PATH_LEN as f32;
    }
    s.lightning.decay(dt);
    s.params.flash_strength = s.lightning.strength;
    if s.params.flash_strength <= 0.0 {
        s.params.bolt_count = 0.0;
    }

    // ---- UI first: user knob edits land on s.params/s.post BEFORE the
    // director-driven modulation overwrites them, so dragged sliders don't
    // snap back to base each frame. ----
    let raw = s.egui_state.take_egui_input(&s.renderer.window);
    let audio_src = s.audio.source_name.clone();
    let prev_palette = s.palette_index;
    let show_ui = s.show_ui;
    let ffmpeg_present = s.ffmpeg_present;
    let bake_msg = s.bake_message.clone();
    let mut pending_audio_load: Option<std::path::PathBuf> = None;
    let mut pending_bake: Option<std::path::PathBuf> = None;
    let full = s.egui_ctx.clone().run(raw, |ctx| {
        if show_ui {
            ui::build_ctx(ctx, ui::UiCtx {
                p: &mut s.params,
                post: &mut s.post,
                lightning: &mut s.lightning,
                director: &mut s.director,
                camera: &mut s.camera,
                palette_index: &mut s.palette_index,
                use_palette_accent: &mut s.use_palette_accent,
                scene: &mut s.scene,
                audio: &s.audio,
                audio_source: &audio_src,
                ffmpeg_present,
                bake_fps: &mut s.bake_fps,
                bake_size: &mut s.bake_size,
                pending_audio_load: &mut pending_audio_load,
                pending_bake: &mut pending_bake,
                bake_message: &bake_msg,
            });
        } else {
            ui::hint_overlay(ctx);
        }
    });
    if pending_audio_load.is_some() {
        s.pending_audio_load = pending_audio_load;
    }
    if pending_bake.is_some() {
        s.pending_bake = pending_bake;
    }
    if s.palette_index != prev_palette {
        // User-triggered palette change from the dropdown — crossfade too.
        s.palette_crossfade.start(&s.params);
    }
    // Drive the active crossfade (no-op once `t >= 1`).
    let target_pal = PALETTES[s.palette_index];
    s.palette_crossfade.step(
        &mut s.params,
        &target_pal.stops,
        target_pal.accent,
        s.use_palette_accent,
        dt,
    );
    s.egui_state
        .handle_platform_output(&s.renderer.window, full.platform_output);

    let paint_jobs = s.egui_ctx.tessellate(full.shapes, full.pixels_per_point);
    let screen_desc = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [s.renderer.config.width, s.renderer.config.height],
        pixels_per_point: full.pixels_per_point,
    };
    for (id, delta) in &full.textures_delta.set {
        s.egui_renderer
            .update_texture(&s.renderer.device, &s.renderer.queue, *id, delta);
    }

    // ---- camera (uses user-set s.params.speed, which is now the post-UI value) ----
    let speed = (s.params.speed + s.params.bass * s.params.bass_to_speed
        + scaled_swell * 0.9).max(0.0);
    s.camera.integrate(speed, dt);
    s.camera.smooth_follow(dt);

    if tick.drop_trigger > 0.05 {
        let r1 = hash_u32(s.director.seed, 11) - 0.5;
        let r2 = hash_u32(s.director.seed, 23) - 0.5;
        s.director.seed = s.director.seed.wrapping_add(1);
        let mag = tick.drop_trigger * amt * 1.4;
        s.camera.add_kick([r1 * mag, r2 * mag, 0.0]);
    }
    s.camera.apply_kick_spring(dt);
    s.camera.roll = s.director.roll_phase.sin() * scaled_swell * 0.035;

    // ---- save user bases, apply director modulation ----
    let base_intensity = s.post.intensity;
    let base_aberration = s.post.aberration;
    let base_contrast = s.post.contrast;
    let base_saturation = s.post.saturation;
    let base_tunnel_glow = s.params.tunnel_glow;
    let base_cam_zoom = s.params.cam_zoom;
    let base_density_mul = s.params.density_mul;

    s.post.intensity = base_intensity + scaled_drop * 0.25 + scaled_swell * 0.08;
    s.post.aberration = base_aberration + scaled_drop * 0.40;
    s.post.contrast = base_contrast + scaled_drop * 0.08;
    s.post.saturation = (base_saturation - s.director.lull * amt * 0.25).max(0.0);
    // Silence fades both the clouds and the end-of-tunnel glow toward zero
    // so a quiet bridge leaves us in an empty dark tunnel; lull contributes a
    // milder dim on top. Both knobs are restored at end-of-frame so the UI
    // sliders show the user's base value.
    let silence_amt = s.director.silence;
    s.params.tunnel_glow = (base_tunnel_glow
        * (1.0 - s.director.lull * amt * 0.30)
        * (1.0 - silence_amt * 0.95)).max(0.0);
    s.params.density_mul = base_density_mul * (1.0 - silence_amt * 0.95);
    // Pull-back zoom: swell/drop widen the FOV so peaks feel airy rather than
    // claustrophobic. (Previously this multiplied < 1.0, which zoomed *in* and
    // never read as the cam_zoom slider doing anything during peaks.)
    s.params.cam_zoom = base_cam_zoom * (1.0 + scaled_swell * 0.22 + scaled_drop * 0.08);
    // Live quality: keep the loop modest so weak GPUs stay above 60 fps. The
    // bake job overrides these (and grain-free fade-in) for the recorded video.
    s.params.quality_steps = 140.0;
    s.params.quality_step_floor = 0.085;
    s.post.fade_in = 1.0;
    s.post.time = s.params.time;
    s.post.resolution = s.params.resolution;

    // Camera basis (must happen after kick + roll, before GPU write).
    s.params.cam_pos = s.camera.world_pos();
    let (right, up, fwd) = s.camera.basis();
    s.params.cam_right = right;
    s.params.cam_up = up;
    s.params.cam_fwd = fwd;

    s.renderer.write_cloud_params(&s.params);
    s.renderer.write_post_params(&s.post);

    // Restore base values so the UI sliders read the user-set value next
    // frame instead of the director-modulated one.
    s.post.intensity = base_intensity;
    s.post.aberration = base_aberration;
    s.post.contrast = base_contrast;
    s.post.saturation = base_saturation;
    s.params.tunnel_glow = base_tunnel_glow;
    s.params.cam_zoom = base_cam_zoom;
    s.params.density_mul = base_density_mul;

    let frame = match s.renderer.surface.get_current_texture() {
        Ok(f) => f,
        Err(wgpu::SurfaceError::Outdated) | Err(wgpu::SurfaceError::Lost) => {
            s.renderer.resize(s.renderer.config.width, s.renderer.config.height);
            return;
        }
        Err(e) => {
            log::warn!("surface error: {e:?}");
            return;
        }
    };
    let view = frame.texture.create_view(&Default::default());

    let mut enc = s.renderer.device.create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("frame") });

    // -------- 1. Scene → HDR --------
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &s.renderer.targets.scene_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let pipeline = match s.scene {
            Scene::Clouds => &s.renderer.scene_pipeline,
            Scene::Cube => &s.renderer.cube_pipeline,
        };
        rp.set_pipeline(pipeline);
        rp.set_bind_group(0, &s.renderer.scene_bind_group, &[]);
        rp.draw(0..3, 0..1);
    }

    // -------- 2. Bloom: extract → downsample chain --------
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bloom-extract"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &s.renderer.targets.bloom_views[0],
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&s.renderer.extract_pipeline);
        rp.set_bind_group(0, &s.renderer.targets.bloom_bind_groups[0], &[]);
        rp.draw(0..3, 0..1);
    }
    for i in 1..s.renderer.targets.bloom_views.len() {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bloom-downsample"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &s.renderer.targets.bloom_views[i],
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&s.renderer.downsample_pipeline);
        rp.set_bind_group(0, &s.renderer.targets.bloom_bind_groups[i], &[]);
        rp.draw(0..3, 0..1);
    }

    // -------- 3. Bloom: upsample additively --------
    let levels = s.renderer.targets.bloom_views.len();
    for i in (1..levels).rev() {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bloom-upsample"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &s.renderer.targets.bloom_views[i - 1],
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&s.renderer.upsample_pipeline);
        rp.set_bind_group(0, &s.renderer.targets.bloom_bind_groups[i + 1], &[]);
        rp.draw(0..3, 0..1);
    }

    // -------- 4. Composite + UI --------
    s.egui_renderer.update_buffers(
        &s.renderer.device,
        &s.renderer.queue,
        &mut enc,
        &paint_jobs,
        &screen_desc,
    );
    {
        let mut rp = enc
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite+ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            })
            .forget_lifetime();
        rp.set_pipeline(&s.renderer.composite_pipeline);
        rp.set_bind_group(0, &s.renderer.targets.composite_bind_group, &[]);
        rp.draw(0..3, 0..1);
        s.egui_renderer.render(&mut rp, &paint_jobs, &screen_desc);
    }

    s.renderer.queue.submit(Some(enc.finish()));
    frame.present();

    for id in &full.textures_delta.free {
        s.egui_renderer.free_texture(id);
    }
}
