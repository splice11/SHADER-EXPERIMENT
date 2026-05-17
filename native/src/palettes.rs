#[derive(Copy, Clone)]
pub struct Palette {
    pub name: &'static str,
    /// 5 colour stops sampled by luma — index 0 ≈ deepest shadow, index 4 ≈
    /// hottest core. Values can exceed 1.0; the HDR scene buffer + ACES
    /// tonemap will roll them off, so pushing peaks above 1.0 is the way to
    /// get real "bloom-eating" highlights instead of pastel cores.
    pub stops: [[f32; 3]; 5],
    /// Contrasting accent used for lightning by default.
    pub accent: [f32; 3],
}

pub const PALETTES: &[Palette] = &[
    Palette {
        // Protean is the reference look — five distinct hues across the
        // fluffiness range. Left as-is.
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
        // Cold violet shadow → blood magenta core → solar gold rim, with a
        // hot near-white peak for cores so dense puffs actually bloom.
        name: "Sunset",
        stops: [
            [0.03, 0.01, 0.10],
            [0.32, 0.05, 0.28],
            [0.95, 0.18, 0.18],
            [1.25, 0.65, 0.18],
            [1.45, 1.15, 0.55],
        ],
        accent: [0.20, 0.95, 1.20],
    },
    Palette {
        // Heavy contrast: indigo void, electric magenta, then a cool cyan
        // swing for variety, then a hot acid-yellow core (no more pastel pink).
        name: "Cyberpunk",
        stops: [
            [0.04, 0.00, 0.16],
            [0.30, 0.00, 0.72],
            [0.95, 0.10, 0.95],
            [0.25, 0.95, 1.30],
            [1.35, 1.05, 0.45],
        ],
        accent: [1.10, 1.30, 0.30],
    },
    Palette {
        // Inky teal, emerald body, chartreuse rim, hot ivory core.
        name: "Aurora",
        stops: [
            [0.00, 0.03, 0.06],
            [0.04, 0.22, 0.42],
            [0.10, 0.85, 0.55],
            [0.55, 1.25, 0.45],
            [1.25, 1.10, 0.70],
        ],
        accent: [1.30, 0.30, 1.00],
    },
    Palette {
        // True coal shadow into orange forge into a hot white-yellow core.
        // Adds a magenta-tinted middle so peaks don't collapse to one hue.
        name: "Ember",
        stops: [
            [0.02, 0.00, 0.00],
            [0.35, 0.04, 0.06],
            [0.95, 0.18, 0.04],
            [1.30, 0.65, 0.10],
            [1.50, 1.25, 0.60],
        ],
        accent: [0.30, 0.95, 1.30],
    },
    Palette {
        // High-contrast monochrome with a touch of warmth in the highlights.
        name: "Mono",
        stops: [
            [0.00, 0.00, 0.00],
            [0.06, 0.06, 0.08],
            [0.38, 0.38, 0.42],
            [0.85, 0.85, 0.88],
            [1.30, 1.25, 1.18],
        ],
        accent: [1.20, 1.20, 1.20],
    },
    Palette {
        // Deep navy void through saturated cobalt to a glacier-white hot core.
        name: "Ice",
        stops: [
            [0.01, 0.03, 0.10],
            [0.04, 0.18, 0.55],
            [0.18, 0.55, 1.05],
            [0.65, 0.98, 1.25],
            [1.25, 1.30, 1.35],
        ],
        accent: [1.30, 0.55, 0.10],
    },
    Palette {
        // New: vine — deep oxblood through plum and amethyst to a hot cream.
        // Wanted another option with a strong purple body and warm highlights.
        name: "Vine",
        stops: [
            [0.05, 0.01, 0.04],
            [0.30, 0.05, 0.20],
            [0.65, 0.15, 0.55],
            [0.95, 0.55, 0.85],
            [1.35, 1.15, 0.95],
        ],
        accent: [1.10, 1.30, 0.55],
    },
];
