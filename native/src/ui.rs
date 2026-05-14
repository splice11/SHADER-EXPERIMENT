use crate::params::CloudParams;

pub fn build(ctx: &egui::Context, p: &mut CloudParams) {
    egui::SidePanel::right("controls")
        .default_width(300.0)
        .show(ctx, |ui| {
            ui.heading("clouds");
            ui.separator();

            ui.label("Coverage");
            ui.add(egui::Slider::new(&mut p.coverage, 0.0..=1.0));

            ui.label("Density");
            ui.add(egui::Slider::new(&mut p.density, 0.0..=4.0));

            ui.label("Noise scale");
            ui.add(egui::Slider::new(&mut p.noise_scale, 0.1..=3.0));

            ui.label("Primary steps");
            ui.add(egui::Slider::new(&mut p.steps, 16.0..=256.0).integer());

            ui.label("Light march steps");
            ui.add(egui::Slider::new(&mut p.light_steps, 1.0..=16.0).integer());

            ui.label("Henyey–Greenstein g");
            ui.add(egui::Slider::new(&mut p.hg_g, -0.99..=0.99));

            ui.label("Absorption");
            ui.add(egui::Slider::new(&mut p.absorption, 0.0..=0.5));

            ui.label("Wind speed");
            ui.add(egui::Slider::new(&mut p.wind_speed, 0.0..=2.0));

            ui.label("Cloud layer thickness");
            ui.add(egui::Slider::new(&mut p.cloud_height, 0.2..=4.0));

            ui.separator();
            ui.label("Sun direction");
            let mut sd = p.sun_dir;
            ui.add(egui::Slider::new(&mut sd[0], -1.0..=1.0).text("x"));
            ui.add(egui::Slider::new(&mut sd[1], -1.0..=1.0).text("y"));
            ui.add(egui::Slider::new(&mut sd[2], -1.0..=1.0).text("z"));
            let l = (sd[0] * sd[0] + sd[1] * sd[1] + sd[2] * sd[2])
                .sqrt()
                .max(1e-6);
            p.sun_dir = [sd[0] / l, sd[1] / l, sd[2] / l];

            ui.separator();
            ui.small("Placeholder marcher. Replace with Schneider-style base+detail noise, weather map, cone-sampled light march.");
        });
}
