#[derive(Copy, Clone)]
pub struct Palette {
    pub name: &'static str,
    pub stops: [[f32; 3]; 5],
    /// Contrasting accent used for lightning by default.
    pub accent: [f32; 3],
}

pub const PALETTES: &[Palette] = &[
    Palette {
        name: "Protean",
        stops: [
            [0.02, 0.04, 0.08],
            [0.10, 0.22, 0.35],
            [0.35, 0.50, 0.65],
            [0.90, 0.78, 0.62],
            [1.00, 0.97, 0.88],
        ],
        accent: [0.78, 0.88, 1.20],
    },
    Palette {
        name: "Sunset",
        stops: [
            [0.04, 0.02, 0.10],
            [0.45, 0.10, 0.30],
            [0.95, 0.35, 0.20],
            [1.00, 0.75, 0.30],
            [1.00, 0.97, 0.85],
        ],
        accent: [0.20, 0.95, 1.10], // teal/cyan contrast
    },
    Palette {
        name: "Cyberpunk",
        stops: [
            [0.04, 0.00, 0.12],
            [0.22, 0.00, 0.52],
            [0.60, 0.10, 0.90],
            [1.00, 0.30, 0.80],
            [1.00, 0.85, 0.95],
        ],
        accent: [0.85, 1.20, 0.30], // acid green/yellow
    },
    Palette {
        name: "Aurora",
        stops: [
            [0.00, 0.02, 0.05],
            [0.05, 0.20, 0.35],
            [0.10, 0.65, 0.55],
            [0.55, 1.00, 0.60],
            [0.92, 1.00, 0.92],
        ],
        accent: [1.20, 0.35, 0.95], // hot magenta
    },
    Palette {
        name: "Ember",
        stops: [
            [0.02, 0.00, 0.00],
            [0.22, 0.05, 0.00],
            [0.70, 0.18, 0.05],
            [1.00, 0.55, 0.10],
            [1.00, 0.95, 0.65],
        ],
        accent: [0.30, 0.95, 1.20], // ice blue
    },
    Palette {
        name: "Mono",
        stops: [
            [0.00, 0.00, 0.00],
            [0.15, 0.15, 0.16],
            [0.45, 0.45, 0.48],
            [0.75, 0.75, 0.78],
            [1.00, 1.00, 1.00],
        ],
        accent: [1.00, 1.00, 1.00],
    },
    Palette {
        name: "Ice",
        stops: [
            [0.02, 0.05, 0.10],
            [0.08, 0.20, 0.40],
            [0.30, 0.55, 0.85],
            [0.70, 0.90, 1.00],
            [0.97, 0.99, 1.00],
        ],
        accent: [1.20, 0.55, 0.10], // warm orange
    },
];
