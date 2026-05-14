use crate::{audio::Audio, params::CloudParams, renderer::Renderer, ui};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

pub struct AppState {
    pub renderer: Renderer,
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,
    pub params: CloudParams,
    pub start: Instant,
    pub audio: Audio,
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

        self.state = Some(AppState {
            renderer,
            egui_ctx,
            egui_state,
            egui_renderer,
            params: CloudParams::default(),
            start: Instant::now(),
            audio,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        let Some(s) = self.state.as_mut() else {
            return;
        };

        let _ = s.egui_state.on_window_event(&s.renderer.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(sz) => {
                s.renderer.resize(sz.width, sz.height);
            }
            WindowEvent::RedrawRequested => {
                render_frame(s);
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

fn render_frame(s: &mut AppState) {
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

    // UI pass
    let raw = s.egui_state.take_egui_input(&s.renderer.window);
    let audio_src = s.audio.source_name.clone();
    let full = s
        .egui_ctx
        .clone()
        .run(raw, |ctx| ui::build(ctx, &mut s.params, &audio_src));
    s.egui_state
        .handle_platform_output(&s.renderer.window, full.platform_output);

    let paint_jobs = s
        .egui_ctx
        .tessellate(full.shapes, full.pixels_per_point);
    let screen_desc = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [s.renderer.config.width, s.renderer.config.height],
        pixels_per_point: full.pixels_per_point,
    };

    for (id, delta) in &full.textures_delta.set {
        s.egui_renderer
            .update_texture(&s.renderer.device, &s.renderer.queue, *id, delta);
    }

    s.renderer.write_params(&s.params);

    let frame = match s.renderer.surface.get_current_texture() {
        Ok(f) => f,
        Err(wgpu::SurfaceError::Outdated) | Err(wgpu::SurfaceError::Lost) => {
            s.renderer
                .resize(s.renderer.config.width, s.renderer.config.height);
            return;
        }
        Err(e) => {
            log::warn!("surface error: {e:?}");
            return;
        }
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut enc = s
        .renderer
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });

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
                label: Some("scene+ui"),
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
        rp.set_pipeline(&s.renderer.pipeline);
        rp.set_bind_group(0, &s.renderer.bind_group, &[]);
        rp.draw(0..3, 0..1);

        s.egui_renderer.render(&mut rp, &paint_jobs, &screen_desc);
    }

    s.renderer.queue.submit(Some(enc.finish()));
    frame.present();

    for id in &full.textures_delta.free {
        s.egui_renderer.free_texture(id);
    }
}
