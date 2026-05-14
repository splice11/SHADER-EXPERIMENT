use crate::app::{Camera, Director, DirectorFeel, Lightning};
use crate::palettes::PALETTES;
use crate::params::{CloudParams, PostParams};

pub fn build(
    ctx: &egui::Context,
    p: &mut CloudParams,
    post: &mut PostParams,
    lightning: &mut Lightning,
    director: &mut Director,
    camera: &mut Camera,
    palette_index: &mut usize,
    audio_source: &str,
) {
    egui::SidePanel::right("controls")
        .default_width(330.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("protean clouds");
                ui.small("F11 fullscreen · H hide ui · L manual lightning · Esc exit");
                ui.separator();

                egui::CollapsingHeader::new("audio").default_open(true).show(ui, |ui| {
                    ui.label(format!("source: {audio_source}"));
                    bar(ui, "bass", p.bass);
                    bar(ui, "mid", p.mid);
                    bar(ui, "treble", p.treble);
                    bar(ui, "centroid", p.centroid);
                    bar(ui, "rms", p.rms);
                    bar(ui, "punch", p.punch);
                });

                egui::CollapsingHeader::new("director").default_open(true).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("feel");
                        ui.selectable_value(&mut director.feel, DirectorFeel::Off, "Off");
                        ui.selectable_value(&mut director.feel, DirectorFeel::Subtle, "Subtle");
                        ui.selectable_value(&mut director.feel, DirectorFeel::Cinematic, "Cinema");
                        ui.selectable_value(&mut director.feel, DirectorFeel::Theatrical, "Theatre");
                    });
                    bar(ui, "swell", director.swell);
                    bar(ui, "drop", director.drop);
                    bar(ui, "lull", director.lull);
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
                    ui.label("palette amount");
                    ui.add(egui::Slider::new(&mut p.palette_amount, 0.0..=1.0));
                    ui.label("centroid → palette offset");
                    ui.add(egui::Slider::new(&mut p.palette_centroid_drive, -1.0..=1.0));
                    ui.label("hue shift");
                    ui.add(egui::Slider::new(&mut p.hue_shift, -3.14..=3.14));
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
                    ui.add(egui::Slider::new(&mut p.density_mul, 0.2..=3.0));
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
                    "H show ui · F11 fullscreen · L lightning · Esc exit",
                );
            });
        });
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
