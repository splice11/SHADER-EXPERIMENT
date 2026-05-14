use crate::app::Lightning;
use crate::palettes::PALETTES;
use crate::params::{CloudParams, PostParams};

pub fn build(
    ctx: &egui::Context,
    p: &mut CloudParams,
    post: &mut PostParams,
    lightning: &mut Lightning,
    palette_index: &mut usize,
    audio_source: &str,
) {
    egui::SidePanel::right("controls")
        .default_width(320.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("protean clouds");
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

                egui::CollapsingHeader::new("palette").default_open(true).show(ui, |ui| {
                    egui::ComboBox::from_label("preset")
                        .selected_text(PALETTES[*palette_index].name)
                        .show_ui(ui, |ui| {
                            for (i, pal) in PALETTES.iter().enumerate() {
                                ui.selectable_value(palette_index, i, pal.name);
                            }
                        });
                    ui.label("palette amount (0 = Nimitz grade, 1 = full)");
                    ui.add(egui::Slider::new(&mut p.palette_amount, 0.0..=1.0));
                    ui.label("centroid → palette offset");
                    ui.add(egui::Slider::new(&mut p.palette_centroid_drive, -1.0..=1.0));
                    ui.label("hue shift");
                    ui.add(egui::Slider::new(&mut p.hue_shift, -3.14..=3.14));
                });

                egui::CollapsingHeader::new("lightning").default_open(true).show(ui, |ui| {
                    ui.checkbox(&mut lightning.auto, "trigger on audio onsets");
                    ui.label("punch threshold");
                    ui.add(egui::Slider::new(&mut lightning.threshold, 0.05..=1.5));
                    ui.label("cooldown (s)");
                    ui.add(egui::Slider::new(&mut lightning.cooldown_secs, 0.05..=2.0));
                    ui.label("peak intensity");
                    ui.add(egui::Slider::new(&mut lightning.peak_intensity, 0.1..=4.0));
                    ui.label("bolt width");
                    ui.add(egui::Slider::new(&mut p.bolt_width, 0.0005..=0.02).logarithmic(true));
                    ui.label("bolt intensity");
                    ui.add(egui::Slider::new(&mut p.bolt_intensity, 0.0..=8.0));
                    color_picker(ui, "flash colour", &mut p.flash_color);
                    if ui.button("⚡ trigger now").clicked() {
                        lightning.timer = 0.0;
                        lightning.strength = lightning.peak_intensity;
                        lightning.cooldown = lightning.cooldown_secs;
                        lightning.seed_counter = lightning.seed_counter.wrapping_add(1);
                        p.bolt_seed = lightning.seed_counter as f32;
                        p.bolt_anchor = [0.4 + (p.bolt_seed.fract() * 0.2), 0.25];
                        p.flash_pos = [0.0, 0.0, p.time * (p.speed + p.bass * p.bass_to_speed) + 8.0];
                    }
                });

                egui::CollapsingHeader::new("bloom + tonemap").default_open(true).show(ui, |ui| {
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
                    "Audio capture uses your default ALSA/PipeWire input. \
                     For system audio, point pavucontrol Recording at \
                     'Monitor of <output sink>'.",
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
