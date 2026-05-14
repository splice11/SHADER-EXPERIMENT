use crate::{
    audio::Audio,
    palettes::PALETTES,
    params::{CloudParams, PostParams},
    renderer::Renderer,
    ui,
};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

pub struct Lightning {
    pub strength: f32,
    pub timer: f32,
    pub cooldown: f32,
    pub auto: bool,
    pub threshold: f32,    // punch above this triggers a strike
    pub cooldown_secs: f32,
    pub peak_intensity: f32,
    pub seed_counter: u32,
}

impl Default for Lightning {
    fn default() -> Self {
        Self {
            strength: 0.0,
            timer: 0.0,
            cooldown: 0.0,
            auto: true,
            threshold: 0.45,
            cooldown_secs: 0.35,
            peak_intensity: 1.2,
            seed_counter: 0,
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
    pub lightning: Lightning,
    pub palette_index: usize,
}

#[derive(Default)]
pub struct App {
    state: Option<AppState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
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

        self.state = Some(AppState {
            renderer,
            egui_ctx,
            egui_state,
            egui_renderer,
            params,
            post: PostParams::default(),
            start: Instant::now(),
            last_frame: Instant::now(),
            audio,
            lightning: Lightning::default(),
            palette_index: 0,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(s) = self.state.as_mut() else { return; };
        let _ = s.egui_state.on_window_event(&s.renderer.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(sz) => s.renderer.resize(sz.width, sz.height),
            WindowEvent::RedrawRequested => render_frame(s),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(s) = self.state.as_ref() {
            s.renderer.window.request_redraw();
        }
    }
}

fn render_frame(s: &mut AppState) {
    let now = Instant::now();
    let dt = (now - s.last_frame).as_secs_f32().min(0.1);
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

    update_lightning(&mut s.lightning, &mut s.params, feat.punch, dt);

    // ---- UI ----
    let raw = s.egui_state.take_egui_input(&s.renderer.window);
    let audio_src = s.audio.source_name.clone();
    let prev_palette = s.palette_index;
    let full = s.egui_ctx.clone().run(raw, |ctx| {
        ui::build(
            ctx,
            &mut s.params,
            &mut s.post,
            &mut s.lightning,
            &mut s.palette_index,
            &audio_src,
        );
    });
    if s.palette_index != prev_palette {
        s.params.set_palette(&PALETTES[s.palette_index].stops);
    }
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

    s.renderer.write_cloud_params(&s.params);
    s.renderer.write_post_params(&s.post);

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

    let mut enc = s
        .renderer
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

    // -------- 1. Scene pass → HDR scene texture --------
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
        rp.set_pipeline(&s.renderer.scene_pipeline);
        rp.set_bind_group(0, &s.renderer.scene_bind_group, &[]);
        rp.draw(0..3, 0..1);
    }

    // -------- 2. Bloom: extract → downsample chain --------
    // Level 0 is produced by extracting from the scene.
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
        rp.set_bind_group(0, &s.renderer.targets.bloom_bind_groups[0], &[]); // from scene
        rp.draw(0..3, 0..1);
    }
    // Subsequent levels downsample from the previous.
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
        rp.set_bind_group(0, &s.renderer.targets.bloom_bind_groups[i], &[]); // from bloom[i-1]
        rp.draw(0..3, 0..1);
    }

    // -------- 3. Bloom: upsample chain (additive into the lower level) --------
    // Read from bloom[i], add into bloom[i-1] via the upsample pipeline's additive blend.
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
        // bloom_bind_groups index `i+1` reads bloom_views[i].
        rp.set_bind_group(0, &s.renderer.targets.bloom_bind_groups[i + 1], &[]);
        rp.draw(0..3, 0..1);
    }

    // -------- 4. Composite (scene + bloom) → swapchain, then egui on top --------
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

fn update_lightning(l: &mut Lightning, p: &mut CloudParams, punch: f32, dt: f32) {
    // Cooldown tick.
    l.cooldown = (l.cooldown - dt).max(0.0);

    // Trigger.
    if l.auto && punch > l.threshold && l.cooldown <= 0.0 {
        l.timer = 0.0;
        l.strength = l.peak_intensity * (1.0 + (punch - l.threshold) * 1.5);
        l.cooldown = l.cooldown_secs;
        l.seed_counter = l.seed_counter.wrapping_add(1);
        // Anchor on screen: random with a bias toward upper half.
        let r1 = hash_u32(l.seed_counter, 11);
        let r2 = hash_u32(l.seed_counter, 23);
        p.bolt_anchor = [0.15 + r1 * 0.70, 0.05 + r2 * 0.45];
        // World position inside the cloud volume, in front of the camera.
        let r3 = hash_u32(l.seed_counter, 37) - 0.5;
        let r4 = hash_u32(l.seed_counter, 53) - 0.5;
        let r5 = hash_u32(l.seed_counter, 71);
        let speed = p.speed + p.bass * p.bass_to_speed;
        let time = p.time * speed;
        p.flash_pos = [r3 * 4.0, r4 * 3.0, time + 6.0 + r5 * 6.0];
        p.bolt_seed = l.seed_counter as f32;
    }

    // Envelope: fast attack handled at trigger, then exponential decay over ~250ms.
    if l.strength > 0.0 {
        l.timer += dt;
        let decay_rate = 14.0; // ~70ms time constant
        l.strength *= (-decay_rate * dt).exp();
        if l.strength < 0.002 {
            l.strength = 0.0;
        }
    }

    p.flash_strength = l.strength;
}

fn hash_u32(seed: u32, salt: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E3779B1).wrapping_add(salt.wrapping_mul(0x85EBCA77));
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846CA68B);
    x ^= x >> 16;
    (x as f32 / u32::MAX as f32).clamp(0.0, 1.0)
}
