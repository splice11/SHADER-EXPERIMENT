// Music-video bake: deterministic offline render from the loaded audio file.
// Reuses the existing scene/bloom/composite pipelines but routes the composite
// output to a capture texture, copies it to a staging buffer, and pipes the
// bytes into a spawned ffmpeg process that muxes in the source mp3.

use crate::analysis::{analyse, CueTrack};
use crate::app::{build_bolt_path, hash_u32, Camera, Director, Lightning, PaletteCrossfade, Scene};
use crate::audio::Features;
use crate::palettes::PALETTES;
use crate::params::{CloudParams, PostParams, BOLT_PATH_LEN};
use crate::renderer::Renderer;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub struct BakeJob {
    pub frame_index: u32,
    pub total_frames: u32,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub output_path: PathBuf,

    // ffmpeg child + its stdin (taken out at start).
    ffmpeg: Child,
    stdin: Option<ChildStdin>,

    // GPU readback resources.
    pub capture_tex: wgpu::Texture,
    pub capture_view: wgpu::TextureView,
    pub staging_buf: wgpu::Buffer,
    pub bytes_per_row: u32,

    // Deterministic per-bake simulation state.
    pub params: CloudParams,
    pub post: PostParams,
    pub director: Director,
    pub camera: Camera,
    pub lightning: Lightning,
    pub palette_index: usize,
    pub scene: Scene,
    pub use_palette_accent: bool,
    pub palette_crossfade: PaletteCrossfade,

    // Pre-analysis: timestamped musical cues. Cursors track which cues have
    // already been consumed so we can fire each event exactly once on the
    // frame when bake-time crosses it.
    pub cues: CueTrack,
    pub use_cues: bool,
    next_drop_idx: usize,
    next_beat_idx: usize,
    next_phrase_idx: usize,
}

/// Length of the start-from-black fade at the head of every bake. Anything
/// past this and the composite shader passes through unchanged.
const BAKE_FADE_IN_SECS: f32 = 1.4;

impl BakeJob {
    pub fn start(
        renderer: &Renderer,
        audio_path: &Path,
        output_path: PathBuf,
        duration_secs: f32,
        fps: u32,
        live_params: &CloudParams,
        live_post: &PostParams,
        director_enabled: bool,
        director_strength: f32,
        scene: Scene,
        palette_index: usize,
        use_palette_accent: bool,
        auto_palette: bool,
        use_cues: bool,
        cues: CueTrack,
    ) -> Result<Self> {
        if !ffmpeg_available() {
            anyhow::bail!("`ffmpeg` not found on PATH — install it to enable baking");
        }
        let width = renderer.config.width;
        let height = renderer.config.height;
        let total_frames = (duration_secs * fps as f32).ceil() as u32;

        // BGRA capture target (matches the swapchain format on Linux, no
        // separate composite pipeline needed). RENDER_ATTACHMENT so the
        // composite pass can target it, COPY_SRC so we can read it back.
        let capture_tex = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bake-capture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let capture_view = capture_tex.create_view(&Default::default());

        // Per-row alignment for copy_texture_to_buffer is 256 bytes.
        let bpr_unpadded = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bytes_per_row = (bpr_unpadded + align - 1) / align * align;
        let staging_size = (bytes_per_row * height) as u64;
        let staging_buf = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bake-staging"),
            size: staging_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ffmpeg encode settings: `-preset veryfast` roughly halves encode CPU
        // time vs `fast` for ~1-2 % bigger files at the same CRF, which is the
        // best trade for "I want my render done" workflows. `-threads 0` lets
        // libx264 use every core. CRF 18 keeps clouds visually lossless.
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel", "warning",
            "-f", "rawvideo",
            "-pixel_format", "bgra",
            "-video_size", &format!("{}x{}", width, height),
            "-framerate", &fps.to_string(),
            "-i", "-",
            "-i", audio_path.to_str().context("audio path not utf-8")?,
            "-map", "0:v",
            "-map", "1:a",
            "-c:v", "libx264",
            "-preset", "veryfast",
            "-tune", "film",
            "-threads", "0",
            "-crf", "18",
            "-pix_fmt", "yuv420p",
            "-c:a", "aac",
            "-b:a", "192k",
            "-shortest",
            "-movflags", "+faststart",
            output_path.to_str().context("output path not utf-8")?,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
        let mut ffmpeg = cmd.spawn().context("spawn ffmpeg — is it on PATH?")?;
        let stdin = ffmpeg.stdin.take().context("ffmpeg stdin")?;

        // Deterministic simulation state — director / camera / lightning all
        // start fresh, but keep the user's feel and palette choice.
        let mut director = Director::default();
        director.enabled = director_enabled;
        director.strength = director_strength;
        director.auto_palette = auto_palette;

        let mut params = *live_params;
        params.set_palette(&PALETTES[palette_index].stops);
        if use_palette_accent {
            params.flash_color = PALETTES[palette_index].accent;
        }
        params.resolution = [width as f32, height as f32];
        // Render-only quality bump: more loop iterations + a smaller per-step
        // floor produce noticeably crisper wisps at 1440p / 2160p. ~70% extra
        // shader cost, but the bake doesn't need to hit interactive frame rate.
        params.quality_steps = 240.0;
        params.quality_step_floor = 0.055;

        Ok(Self {
            frame_index: 0,
            total_frames,
            fps,
            width,
            height,
            output_path,
            ffmpeg,
            stdin: Some(stdin),
            capture_tex,
            capture_view,
            staging_buf,
            bytes_per_row,
            params,
            post: *live_post,
            director,
            camera: Camera::default(),
            lightning: Lightning::default(),
            palette_index,
            scene,
            use_palette_accent,
            palette_crossfade: PaletteCrossfade::default(),
            cues,
            use_cues,
            next_drop_idx: 0,
            next_beat_idx: 0,
            next_phrase_idx: 0,
        })
    }

    /// Lightweight summary the UI can read mid-bake.
    pub fn cue_summary(&self) -> String {
        if self.cues.beats.is_empty() && self.cues.drops.is_empty() {
            return "cues: —".to_string();
        }
        format!(
            "cues: {:.0} BPM (conf {:.2}) · {} beats · {} phrases · {} drops · {} builds",
            self.cues.bpm,
            self.cues.beat_confidence,
            self.cues.beats.len(),
            self.cues.phrase_marks.len(),
            self.cues.drops.len(),
            self.cues.builds.len(),
        )
    }

    /// Apply pre-analysis cues that fall in (prev_t, now_t]. Mutates
    /// `self.director` and the in-flight `tick` so the downstream logic
    /// (lightning trigger, palette swap, scaled_swell/drop) reads the
    /// cued values instead of the reactive ones.
    fn consume_cues(&mut self, prev_t: f32, now_t: f32, tick: &mut crate::app::DirectorTick) {
        // --- drops: force a hard drop envelope + drop_trigger so lightning,
        // camera kick, and the existing per-drop side effects all fire.
        while self.next_drop_idx < self.cues.drops.len() {
            let dt = self.cues.drops[self.next_drop_idx];
            if dt > now_t {
                break;
            }
            if dt > prev_t {
                self.director.drop = 1.0;
                tick.drop_trigger = tick.drop_trigger.max(0.85);
            }
            self.next_drop_idx += 1;
        }

        // --- builds: while inside one, force `swell` to ramp linearly to 1.0
        // across the build. The tunnel-glow crush + cam pull-back ride this.
        for &(start_t, end_t) in &self.cues.builds {
            if now_t >= start_t && now_t < end_t {
                let span = (end_t - start_t).max(0.05);
                let progress = ((now_t - start_t) / span).clamp(0.0, 1.0);
                // Bias toward the end of the build (quadratic) so the tension
                // really stacks just before the impact.
                let shaped = progress * progress;
                if shaped > self.director.swell {
                    self.director.swell = shaped;
                }
            }
        }

        // --- beats: advance cursor to current bake-time. The roll phase is
        // anchored to the beat grid so the slow swell-roll oscillation lines
        // up with the music's underlying pulse (not a free-running sine).
        let confident_grid = !self.cues.beats.is_empty()
            && self.cues.beat_confidence > 0.2
            && self.cues.beat_period_secs > 0.05;
        if confident_grid {
            while self.next_beat_idx < self.cues.beats.len()
                && self.cues.beats[self.next_beat_idx] <= now_t
            {
                self.next_beat_idx += 1;
            }
            let last_beat_idx = self.next_beat_idx.saturating_sub(1);
            let last_beat_t = self.cues.beats[last_beat_idx];
            let beat_progress = ((now_t - last_beat_t) / self.cues.beat_period_secs)
                .clamp(0.0, 1.0);
            // Two beats per full roll cycle (so peaks oscillate L/R on the bar).
            self.director.roll_phase =
                (last_beat_idx as f32 + beat_progress) * std::f32::consts::PI;
        }

        // --- phrase boundaries: act like a forced section change → palette
        // crossfade (if auto_palette) + whip-pan kick. Skip the very first
        // phrase mark since the bake starts faded-in from black.
        while self.next_phrase_idx < self.cues.phrase_marks.len() {
            let pm = self.cues.phrase_marks[self.next_phrase_idx];
            if pm > now_t {
                break;
            }
            if pm > prev_t && self.next_phrase_idx > 0 {
                tick.section_changed = true;
                // Whip kick lives in the director update via section_changed,
                // but update already ran this frame — apply it directly here.
                self.director.whip_velocity += 6.0 * self.director.whip_dir;
                self.director.whip_dir = -self.director.whip_dir;
            }
            self.next_phrase_idx += 1;
        }
    }

    pub fn progress(&self) -> f32 {
        if self.total_frames == 0 {
            1.0
        } else {
            self.frame_index as f32 / self.total_frames as f32
        }
    }

    pub fn done(&self) -> bool {
        self.frame_index >= self.total_frames
    }

    /// Render and pipe one frame. Returns Ok(true) when the bake just finished.
    pub fn step(
        &mut self,
        renderer: &Renderer,
        features: Features,
    ) -> Result<bool> {
        let frame_dt = 1.0 / self.fps as f32;
        let t = self.frame_index as f32 * frame_dt;

        // ---- feed audio + step simulation ----
        self.params.time = t;
        self.params.bass = features.bass;
        self.params.mid = features.mid;
        self.params.treble = features.treble;
        self.params.centroid = features.centroid;
        self.params.rms = features.rms;
        self.params.punch = features.punch;

        let mut tick = self.director.update(&features, t, frame_dt);
        // Authored cues from pre-analysis: when bake-time crosses a known
        // drop/phrase/beat we *force* the corresponding director envelopes so
        // lightning + palette + whip-pan + tunnel-glow build land *on* the
        // music instead of half a beat after the reactive smoothing notices.
        if self.use_cues {
            let prev_t = (self.frame_index as f32 - 1.0).max(0.0) * frame_dt;
            self.consume_cues(prev_t, t, &mut tick);
        }
        let amt = self.director.amount();
        let scaled_swell = (self.director.swell * amt).clamp(0.0, 1.5);
        let scaled_drop = (self.director.drop * amt).clamp(0.0, 1.5);

        if self.director.auto_palette
            && tick.section_changed
            && self.director.palette_cooldown > 6.0
        {
            self.palette_crossfade.start(&self.params);
            self.palette_index = (self.palette_index + 1) % PALETTES.len();
            self.director.palette_cooldown = 0.0;
        }
        let target_pal = PALETTES[self.palette_index];
        self.palette_crossfade.step(
            &mut self.params,
            &target_pal.stops,
            target_pal.accent,
            self.use_palette_accent,
            frame_dt,
        );

        // Lightning trigger (deterministic — uses self.lightning seed).
        if self.lightning.maybe_trigger(features.punch, frame_dt) {
            let cam_x = self.camera.world_pos()[0];
            self.params.bolt_path =
                build_bolt_path(self.lightning.seed_counter, self.camera.z, cam_x);
            self.params.bolt_count = BOLT_PATH_LEN as f32;
        }
        self.lightning.decay(frame_dt);
        self.params.flash_strength = self.lightning.strength;
        if self.params.flash_strength <= 0.0 {
            self.params.bolt_count = 0.0;
        }

        // Camera — same pipeline as the live path: lull drags speed down,
        // swell pushes it up, reverse_strength flips sign occasionally.
        let lull_drag = self.director.lull * amt * 1.4;
        let speed_swell = scaled_swell * 1.8;
        let base_speed_mod = (self.params.speed - lull_drag).max(0.0);
        let reverse_factor = 1.0 - 2.0 * self.director.reverse_strength * amt;
        let speed = ((base_speed_mod
            + self.params.bass * self.params.bass_to_speed
            + speed_swell) * reverse_factor).clamp(-14.0, 18.0);
        self.camera.integrate(speed, frame_dt);

        let base_sway = self.camera.sway_amp;
        let base_follow = self.camera.follow_secs;
        self.camera.sway_amp = base_sway * (1.0 + scaled_swell * 0.6 + scaled_drop * 0.25);
        self.camera.follow_secs =
            (base_follow * (1.0 + self.director.slingshot * amt * 4.0)).min(2.5);
        self.camera.smooth_follow(frame_dt);
        self.camera.sway_amp = base_sway;
        self.camera.follow_secs = base_follow;

        if tick.drop_trigger > 0.05 {
            let r1 = hash_u32(self.director.seed, 11) - 0.5;
            let r2 = hash_u32(self.director.seed, 23) - 0.5;
            self.director.seed = self.director.seed.wrapping_add(1);
            let mag = tick.drop_trigger * amt * 1.4;
            self.camera.add_kick([r1 * mag, r2 * mag, 0.0]);
        }
        self.camera.apply_kick_spring(frame_dt);
        self.camera.roll = self.director.roll_phase.sin() * scaled_swell * 0.035
            + self.director.whip_angle * amt;

        // Slowly rotate the cloud-shading light direction (mirrors live).
        self.params.light_phase += frame_dt * 0.09;

        // Director modulation.
        let base_intensity = self.post.intensity;
        let base_aberration = self.post.aberration;
        let base_contrast = self.post.contrast;
        let base_saturation = self.post.saturation;
        let base_lens_warp = self.post.lens_warp;
        let base_tunnel_glow = self.params.tunnel_glow;
        let base_cam_zoom = self.params.cam_zoom;
        let base_density_mul = self.params.density_mul;
        let base_color_variance = self.params.color_variance;

        let silence = self.director.silence;
        self.post.intensity = base_intensity + scaled_drop * 0.25 + scaled_swell * 0.08;
        self.post.aberration = (base_aberration
            + scaled_drop * 0.55
            + self.params.bass * amt * 0.25).clamp(0.0, 1.5);
        self.post.contrast = base_contrast + scaled_drop * 0.08;
        self.post.saturation =
            (base_saturation - self.director.lull * amt * 0.25).max(0.0);
        self.post.lens_warp = (base_lens_warp
            + scaled_drop * 0.35
            + self.params.bass * amt * 0.12).clamp(-0.6, 0.9);
        // Build → crush tunnel glow to ~0 (tension); drop → snap brighter.
        self.params.tunnel_glow = (base_tunnel_glow
            * (1.0 - self.director.lull * amt * 0.30)
            * (1.0 - silence * 0.95)
            * (1.0 - scaled_swell * 0.85).max(0.0)
            * (1.0 + scaled_drop * 0.55))
            .max(0.0);
        // Density: silence empties the tunnel; swell/drop swell it up so peaks
        // get visibly thicker clouds; lull pulls density down to "thin haze".
        self.params.density_mul = (base_density_mul
            * (1.0 - silence * 0.95)
            * (1.0 + scaled_swell * 0.35 + scaled_drop * 0.20)
            * (1.0 - self.director.lull * amt * 0.30))
            .max(0.0);
        // Pull-back zoom on swell/drop (mirrors the live path).
        self.params.cam_zoom =
            base_cam_zoom * (1.0 + scaled_swell * 0.22 + scaled_drop * 0.08);
        // Colour variance follows the energy curve.
        self.params.color_variance = (base_color_variance
            * (1.0 + scaled_swell * 0.55 + scaled_drop * 0.20)
            * (1.0 - self.director.lull * amt * 0.40))
            .clamp(0.0, 1.5);
        // Start the video from black: ramp fade_in from 0 → 1 over the first
        // BAKE_FADE_IN_SECS seconds so the simulation isn't visibly mid-stride
        // when the music kicks in.
        self.post.fade_in = (t / BAKE_FADE_IN_SECS).clamp(0.0, 1.0);
        self.post.time = self.params.time;
        self.post.resolution = self.params.resolution;

        // Camera basis.
        self.params.cam_pos = self.camera.world_pos();
        let (right, up, fwd) = self.camera.basis();
        self.params.cam_right = right;
        self.params.cam_up = up;
        self.params.cam_fwd = fwd;

        renderer.write_cloud_params(&self.params);
        renderer.write_post_params(&self.post);

        // Restore user bases.
        self.post.intensity = base_intensity;
        self.post.aberration = base_aberration;
        self.post.contrast = base_contrast;
        self.post.saturation = base_saturation;
        self.post.lens_warp = base_lens_warp;
        self.params.tunnel_glow = base_tunnel_glow;
        self.params.cam_zoom = base_cam_zoom;
        self.params.density_mul = base_density_mul;
        self.params.color_variance = base_color_variance;
        // Note: we intentionally don't restore post.fade_in — it's a derived
        // bake-only knob the live UI never reads.

        // ---- render passes ----
        let mut enc = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bake-frame"),
            });

        // Scene → HDR
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bake-scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &renderer.targets.scene_view,
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
            let pipeline = match self.scene {
                Scene::Clouds => &renderer.scene_pipeline,
                Scene::Cube => &renderer.cube_pipeline,
            };
            rp.set_pipeline(pipeline);
            rp.set_bind_group(0, &renderer.scene_bind_group, &[]);
            rp.draw(0..3, 0..1);
        }
        // Bloom extract
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bake-bloom-extract"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &renderer.targets.bloom_views[0],
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
            rp.set_pipeline(&renderer.extract_pipeline);
            rp.set_bind_group(0, &renderer.targets.bloom_bind_groups[0], &[]);
            rp.draw(0..3, 0..1);
        }
        for i in 1..renderer.targets.bloom_views.len() {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bake-bloom-down"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &renderer.targets.bloom_views[i],
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
            rp.set_pipeline(&renderer.downsample_pipeline);
            rp.set_bind_group(0, &renderer.targets.bloom_bind_groups[i], &[]);
            rp.draw(0..3, 0..1);
        }
        let levels = renderer.targets.bloom_views.len();
        for i in (1..levels).rev() {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bake-bloom-up"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &renderer.targets.bloom_views[i - 1],
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
            rp.set_pipeline(&renderer.upsample_pipeline);
            rp.set_bind_group(0, &renderer.targets.bloom_bind_groups[i + 1], &[]);
            rp.draw(0..3, 0..1);
        }
        // Composite → capture texture
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bake-composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.capture_view,
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
            rp.set_pipeline(&renderer.composite_pipeline);
            rp.set_bind_group(0, &renderer.targets.composite_bind_group, &[]);
            rp.draw(0..3, 0..1);
        }
        // Texture → staging buffer
        enc.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.capture_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &self.staging_buf,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(self.bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        renderer.queue.submit(Some(enc.finish()));

        // ---- map staging, strip per-row padding, write to ffmpeg ----
        let slice = self.staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        renderer.device.poll(wgpu::Maintain::Wait);
        rx.recv().context("staging map_async dropped")??;

        {
            let data = slice.get_mapped_range();
            let bpr_unpadded = (self.width * 4) as usize;
            let bpr_padded = self.bytes_per_row as usize;
            if let Some(stdin) = self.stdin.as_mut() {
                for row in 0..self.height as usize {
                    let start = row * bpr_padded;
                    let end = start + bpr_unpadded;
                    stdin
                        .write_all(&data[start..end])
                        .context("write frame to ffmpeg stdin")?;
                }
            }
        }
        self.staging_buf.unmap();

        self.frame_index += 1;
        Ok(self.done())
    }

    /// Close stdin, wait for ffmpeg to finish encoding, return success status.
    pub fn finish(mut self) -> Result<PathBuf> {
        drop(self.stdin.take());
        let status = self.ffmpeg.wait().context("wait for ffmpeg")?;
        if !status.success() {
            anyhow::bail!("ffmpeg exited with status {:?}", status.code());
        }
        Ok(self.output_path)
    }

    /// If the user aborts mid-bake: kill ffmpeg, drop everything.
    pub fn abort(mut self) {
        drop(self.stdin.take());
        let _ = self.ffmpeg.kill();
        let _ = self.ffmpeg.wait();
    }
}
