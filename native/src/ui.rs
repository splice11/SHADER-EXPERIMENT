use crate::app::{BakeSize, Camera, Director, Lightning, Scene};
use crate::audio::Audio;
use crate::palettes::PALETTES;
use crate::params::{CloudParams, PostParams};
use std::path::PathBuf;

pub struct UiCtx<'a> {
    pub p: &'a mut CloudParams,
    pub post: &'a mut PostParams,
    pub lightning: &'a mut Lightning,
    pub director: &'a mut Director,
    pub camera: &'a mut Camera,
    pub palette_index: &'a mut usize,
    pub use_palette_accent: &'a mut bool,
    pub scene: &'a mut Scene,
    pub audio: &'a Audio,
    pub audio_source: &'a str,
    pub ffmpeg_present: bool,
    pub bake_fps: &'a mut u32,
    pub bake_size: &'a mut BakeSize,
    pub use_cues: &'a mut bool,
    pub show_hud: &'a mut bool,
    pub pending_audio_load: &'a mut Option<PathBuf>,
    pub pending_bake: &'a mut Option<PathBuf>,
    pub bake_message: &'a Option<String>,
}

pub fn build_ctx(ctx: &egui::Context, c: UiCtx<'_>) {
    let UiCtx {
        p, post, lightning, director, camera,
        palette_index, use_palette_accent, scene,
        audio, audio_source, ffmpeg_present, bake_fps, bake_size, use_cues, show_hud,
        pending_audio_load, pending_bake, bake_message,
    } = c;
    egui::SidePanel::right("controls")
        .default_width(330.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("shader-experiment");
                ui.small("F11 fullscreen · H hide ui · C cycle scene · L lightning · Esc exit");
                ui.separator();

                egui::CollapsingHeader::new("scene").default_open(true).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("show");
                        ui.selectable_value(scene, Scene::Clouds, "Clouds");
                        ui.selectable_value(scene, Scene::Cube, "Cube");
                    });
                });

                egui::CollapsingHeader::new("audio").default_open(true).show(ui, |ui| {
                    ui.label(format!("source: {audio_source}"));
                    ui.horizontal(|ui| {
                        if ui.button("Load track…").clicked() {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("audio", &["mp3", "wav", "flac", "ogg", "m4a", "aac"])
                                .pick_file()
                            {
                                *pending_audio_load = Some(p);
                            }
                        }
                        if audio.is_file_mode() && ui.button("Use mic / system").clicked() {
                            // unloading goes back to live capture — we can't
                            // mutate Audio here (immutable ref); flag and let
                            // app.rs handle it. (See note below.)
                            // For simplicity we won't implement this for now;
                            // user can restart the app.
                            ui.label("(restart app to switch back)");
                        }
                    });
                    if audio.is_file_mode() {
                        let pos = audio.position_secs().unwrap_or(0.0);
                        let dur = audio.duration_secs().unwrap_or(0.0);
                        let playing = audio.is_playing();
                        ui.horizontal(|ui| {
                            if playing {
                                if ui.button("⏸").clicked() { audio.pause(); }
                            } else if ui.button("▶").clicked() { audio.play(); }
                            if ui.button("⏮").clicked() { audio.seek_secs(0.0); }
                            ui.label(format!("{} / {}", fmt_time(pos), fmt_time(dur)));
                        });
                        let mut seek_pos = pos;
                        let resp = ui.add(egui::Slider::new(&mut seek_pos, 0.0..=dur.max(0.1))
                            .show_value(false));
                        if resp.dragged() || resp.changed() {
                            audio.seek_secs(seek_pos);
                        }
                    }
                    bar(ui, "bass", p.bass);
                    bar(ui, "mid", p.mid);
                    bar(ui, "treble", p.treble);
                    bar(ui, "centroid", p.centroid);
                    bar(ui, "rms", p.rms);
                    bar(ui, "punch", p.punch);
                });

                egui::CollapsingHeader::new("bake video").default_open(true).show(ui, |ui| {
                    if !ffmpeg_present {
                        ui.colored_label(egui::Color32::from_rgb(220, 130, 100),
                            "ffmpeg not found on PATH.");
                        ui.small("Install ffmpeg (apt/brew/pacman) and restart.");
                    } else if !audio.is_file_mode() {
                        ui.small("Load a track first to enable bake.");
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("fps");
                            ui.selectable_value(bake_fps, 30, "30");
                            ui.selectable_value(bake_fps, 60, "60");
                        });
                        ui.horizontal(|ui| {
                            ui.label("size");
                            ui.selectable_value(bake_size, BakeSize::Window, BakeSize::Window.label());
                            ui.selectable_value(bake_size, BakeSize::P1080, BakeSize::P1080.label());
                            ui.selectable_value(bake_size, BakeSize::P1440, BakeSize::P1440.label());
                            ui.selectable_value(bake_size, BakeSize::P2160, BakeSize::P2160.label());
                        });
                        let win_w = p.resolution[0] as u32;
                        let win_h = p.resolution[1] as u32;
                        let (bw, bh) = bake_size.dimensions((win_w, win_h));
                        if (bw, bh) == (win_w, win_h) {
                            ui.small(format!("output: {}×{} (window)", bw, bh));
                        } else {
                            ui.small(format!(
                                "output: {}×{} (render targets resize during bake)", bw, bh,
                            ));
                        }
                        ui.small("bake uses higher cloud detail than the live preview.");
                        ui.checkbox(use_cues, "pre-analyse music for cued events");
                        ui.small("pre-analysis adds a few-second pause before bake \
                                  begins to detect beats / drops / phrases / builds, \
                                  then aligns events to them.");
                        ui.checkbox(show_hud, "burn-in info overlay (track / BPM / palette)");
                        if ui.button("Bake to MP4…").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("clouds.mp4")
                                .add_filter("MP4", &["mp4"])
                                .save_file()
                            {
                                *pending_bake = Some(path);
                            }
                        }
                    }
                    if let Some(msg) = bake_message {
                        ui.small(msg);
                    }
                });

                egui::CollapsingHeader::new("director").default_open(true).show(ui, |ui| {
                    ui.checkbox(&mut director.enabled, "director enabled");
                    ui.horizontal(|ui| {
                        ui.label("strength");
                        ui.add(egui::Slider::new(&mut director.strength, 0.0..=2.0));
                    });
                    bar(ui, "swell", director.swell);
                    bar(ui, "drop", director.drop);
                    bar(ui, "lull", director.lull);
                    bar(ui, "silence", director.silence);
                    let bpm = director.bpm();
                    ui.label(format!(
                        "section: {:?}   bpm: {}",
                        director.section,
                        if bpm > 30.0 { format!("{bpm:.0}") } else { "—".to_string() }
                    ));
                    ui.checkbox(&mut director.auto_palette,
                        "auto-rotate palettes on section changes");
                    ui.checkbox(&mut director.allow_reverse,
                        "occasional backward sections on peaks");
                    ui.small("director also drives: speed (faster on peaks, \
                              slower on lulls), camera sway/zoom, follow inertia \
                              slingshot on drops, density, colour variance, \
                              tunnel glow.");
                });

                egui::CollapsingHeader::new("camera").default_open(true).show(ui, |ui| {
                    ui.label("sway amplitude");
                    ui.add(egui::Slider::new(&mut camera.sway_amp, 0.0..=1.5));
                    ui.label("follow inertia (s)");
                    ui.add(egui::Slider::new(&mut camera.follow_secs, 0.02..=1.5).logarithmic(true));
                    ui.label("base zoom");
                    ui.add(egui::Slider::new(&mut p.cam_zoom, 0.4..=2.0));
                    ui.label("vignette");
                    ui.add(egui::Slider::new(&mut p.vignette, 0.0..=1.0));
                });

                egui::CollapsingHeader::new("palette").default_open(true).show(ui, |ui| {
                    egui::ComboBox::from_label("preset")
                        .selected_text(PALETTES[*palette_index].name)
                        .show_ui(ui, |ui| {
                            for (i, pal) in PALETTES.iter().enumerate() {
                                ui.selectable_value(palette_index, i, pal.name);
                            }
                        });
                    ui.checkbox(use_palette_accent,
                        "palette accent → lightning colour");
                    ui.label("palette amount");
                    ui.add(egui::Slider::new(&mut p.palette_amount, 0.0..=1.0));
                    ui.label("centroid → palette offset");
                    ui.add(egui::Slider::new(&mut p.palette_centroid_drive, -1.0..=1.0));
                    ui.label("hue shift");
                    ui.add(egui::Slider::new(&mut p.hue_shift, -3.14..=3.14));
                });

                egui::CollapsingHeader::new("look").default_open(true).show(ui, |ui| {
                    ui.label("tunnel glow (end-of-tunnel brightness)");
                    ui.add(egui::Slider::new(&mut p.tunnel_glow, 0.0..=2.0));
                    ui.small("director lull dims this on quiet sections.");
                    ui.label("plumey cap (limits how closed the tunnel gets)");
                    ui.add(egui::Slider::new(&mut p.morph_cap, 0.30..=1.6));
                    ui.label("colour variance (per-puff hue spread — adds depth)");
                    ui.add(egui::Slider::new(&mut p.color_variance, 0.0..=1.5));
                });

                egui::CollapsingHeader::new("lightning").default_open(true).show(ui, |ui| {
                    ui.checkbox(&mut lightning.auto, "auto-trigger on audio");
                    ui.label("punch threshold");
                    ui.add(egui::Slider::new(&mut lightning.threshold, 0.05..=1.5));
                    ui.label("cooldown (s)");
                    ui.add(egui::Slider::new(&mut lightning.cooldown_secs, 0.05..=2.0));
                    ui.label("peak intensity");
                    ui.add(egui::Slider::new(&mut lightning.peak_intensity, 0.1..=4.0));
                    ui.label("bolt core width");
                    ui.add(egui::Slider::new(&mut p.bolt_width, 0.02..=1.5).logarithmic(true));
                    ui.label("bolt core intensity");
                    ui.add(egui::Slider::new(&mut p.bolt_intensity, 0.0..=12.0));
                    ui.label("bolt cloud glow");
                    ui.add(egui::Slider::new(&mut p.bolt_glow, 0.0..=4.0));
                    ui.label("bolt colour saturation");
                    ui.add(egui::Slider::new(&mut p.bolt_saturation, 0.0..=3.0));
                    ui.label("invert (shadow bolts that darken clouds)");
                    ui.add(egui::Slider::new(&mut p.bolt_invert, 0.0..=1.0));
                    color_picker(ui, "flash colour", &mut p.flash_color);
                });

                egui::CollapsingHeader::new("cinematic").default_open(true).show(ui, |ui| {
                    ui.label("letterbox aspect (0 = off)");
                    ui.add(egui::Slider::new(&mut post.letterbox_aspect, 0.0..=3.0));
                    ui.horizontal(|ui| {
                        if ui.small_button("off").clicked() { post.letterbox_aspect = 0.0; }
                        if ui.small_button("16:9").clicked() { post.letterbox_aspect = 16.0/9.0; }
                        if ui.small_button("2.00").clicked() { post.letterbox_aspect = 2.00; }
                        if ui.small_button("2.39").clicked() { post.letterbox_aspect = 2.39; }
                        if ui.small_button("2.76").clicked() { post.letterbox_aspect = 2.76; }
                    });
                    ui.label("film grain");
                    ui.add(egui::Slider::new(&mut post.grain, 0.0..=0.2));
                    ui.label("contrast");
                    ui.add(egui::Slider::new(&mut post.contrast, 0.5..=2.0));
                    ui.label("saturation");
                    ui.add(egui::Slider::new(&mut post.saturation, 0.0..=2.0));
                    ui.label("anamorphic streak");
                    ui.add(egui::Slider::new(&mut post.anamorphic, 0.0..=1.5));
                    ui.label("chromatic aberration (base)");
                    ui.add(egui::Slider::new(&mut post.aberration, 0.0..=1.5));
                    ui.label("radial speed blur (hyperdrive streaks)");
                    ui.add(egui::Slider::new(&mut post.radial_blur, 0.0..=0.10));
                    ui.small("director adds a streak on drops + ambient bass.");
                });

                egui::CollapsingHeader::new("bloom + tonemap").default_open(false).show(ui, |ui| {
                    ui.label("threshold");
                    ui.add(egui::Slider::new(&mut post.threshold, 0.0..=4.0));
                    ui.label("knee");
                    ui.add(egui::Slider::new(&mut post.knee, 0.01..=2.0));
                    ui.label("intensity");
                    ui.add(egui::Slider::new(&mut post.intensity, 0.0..=2.0));
                    ui.label("exposure");
                    ui.add(egui::Slider::new(&mut post.exposure, 0.1..=4.0));
                });

                egui::CollapsingHeader::new("motion").default_open(false).show(ui, |ui| {
                    ui.label("base speed");
                    ui.add(egui::Slider::new(&mut p.speed, 0.0..=12.0));
                    ui.label("morph (prm1)");
                    ui.add(egui::Slider::new(&mut p.morph, -0.5..=1.5));
                    ui.label("density mul");
                    ui.add(egui::Slider::new(&mut p.density_mul, 0.2..=2.0));
                    ui.small("density is hard-capped at 1.45 inside the shader to \
                              prevent the camera being smothered in fog.");
                });

                egui::CollapsingHeader::new("audio routing").default_open(false).show(ui, |ui| {
                    ui.label("bass → speed");
                    ui.add(egui::Slider::new(&mut p.bass_to_speed, 0.0..=10.0));
                    ui.label("bass → morph");
                    ui.add(egui::Slider::new(&mut p.bass_to_morph, 0.0..=2.0));
                    ui.label("centroid → hue");
                    ui.add(egui::Slider::new(&mut p.centroid_to_hue, -3.14..=3.14));
                    ui.label("rms → density");
                    ui.add(egui::Slider::new(&mut p.rms_to_density, 0.0..=2.0));
                });

                ui.separator();
                ui.small(
                    "For system audio, point pavucontrol Recording at \
                     'Monitor of <output sink>'.",
                );
            });
        });
}

pub fn hint_overlay(ctx: &egui::Context) {
    egui::Area::new("hint".into())
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0))
        .interactable(false)
        .show(ctx, |ui| {
            let frame = egui::Frame::none()
                .fill(egui::Color32::from_black_alpha(140))
                .inner_margin(egui::Margin::symmetric(8.0, 5.0))
                .rounding(4.0);
            frame.show(ui, |ui| {
                ui.colored_label(
                    egui::Color32::from_white_alpha(200),
                    "H show ui · F11 fullscreen · C scene · L lightning · Esc exit",
                );
            });
        });
}

fn fmt_time(t: f32) -> String {
    let m = (t / 60.0) as u32;
    let s = (t - (m as f32) * 60.0) as u32;
    format!("{m}:{s:02}")
}

fn bar(ui: &mut egui::Ui, label: &str, value: f32) {
    ui.horizontal(|ui| {
        ui.label(format!("{label:<9}"));
        ui.add(egui::ProgressBar::new(value.clamp(0.0, 1.0)).desired_width(180.0));
        ui.label(format!("{value:.2}"));
    });
}

fn color_picker(ui: &mut egui::Ui, label: &str, c: &mut [f32; 3]) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.color_edit_button_rgb(c);
    });
}
