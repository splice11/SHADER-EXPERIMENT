use crate::params::CloudParams;

pub fn build(ctx: &egui::Context, p: &mut CloudParams, audio_source: &str) {
    egui::SidePanel::right("controls")
        .default_width(320.0)
        .show(ctx, |ui| {
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

            ui.separator();
            ui.label("base speed");
            ui.add(egui::Slider::new(&mut p.speed, 0.0..=12.0));
            ui.label("morph (prm1)");
            ui.add(egui::Slider::new(&mut p.morph, -0.5..=1.5));
            ui.label("density mul");
            ui.add(egui::Slider::new(&mut p.density_mul, 0.2..=3.0));
            ui.label("hue shift");
            ui.add(egui::Slider::new(&mut p.hue_shift, -3.14..=3.14));

            ui.separator();
            ui.label("audio routing");
            ui.label("bass → speed");
            ui.add(egui::Slider::new(&mut p.bass_to_speed, 0.0..=10.0));
            ui.label("bass → morph");
            ui.add(egui::Slider::new(&mut p.bass_to_morph, 0.0..=2.0));
            ui.label("centroid → hue");
            ui.add(egui::Slider::new(&mut p.centroid_to_hue, -3.14..=3.14));
            ui.label("rms → density");
            ui.add(egui::Slider::new(&mut p.rms_to_density, 0.0..=2.0));

            ui.separator();
            ui.small(
                "Audio capture uses your default ALSA/PipeWire input. \
                 To visualize system audio, set the input in pavucontrol \
                 to a 'Monitor of …' source for your output sink.",
            );
        });
}

fn bar(ui: &mut egui::Ui, label: &str, value: f32) {
    ui.horizontal(|ui| {
        ui.label(format!("{label:<9}"));
        ui.add(egui::ProgressBar::new(value.clamp(0.0, 1.0)).desired_width(180.0));
        ui.label(format!("{value:.2}"));
    });
}
