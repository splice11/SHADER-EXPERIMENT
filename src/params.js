// Single source of truth for shader uniforms exposed to the UI / presets.
// Each entry: { type, value, min, max, step?, label?, group, uniform? }
// type is one of: 'float' | 'int' | 'color' | 'angle' | 'bool'
// `uniform` is the GLSL uniform name; defaults to 'u_' + key.

export const PARAM_DEFS = {
  // ---- motion ----------------------------------------------------------
  speed:        { group: 'motion', type: 'float', value: 3.0,  min: 0.0,  max: 12.0, step: 0.01 },
  speedBass:    { group: 'motion', type: 'float', value: 0.8,  min: 0.0,  max: 5.0,  step: 0.01, label: 'speed × bass' },
  fov:          { group: 'motion', type: 'float', value: 50.0, min: 20.0, max: 110.0, step: 0.5 },
  swayAmp:      { group: 'motion', type: 'float', value: 0.0,  min: 0.0,  max: 4.0,  step: 0.01, label: 'tunnel sway' },
  swayFreq:     { group: 'motion', type: 'float', value: 1.0,  min: 0.0,  max: 4.0,  step: 0.01 },
  extraSwayX:   { group: 'motion', type: 'float', value: 0.0,  min: 0.0,  max: 2.0,  step: 0.01, label: 'extra x-sway' },
  rollAmp:      { group: 'motion', type: 'float', value: 0.0,  min: 0.0,  max: 0.4,  step: 0.001, label: 'world roll' },
  rollFreq:     { group: 'motion', type: 'float', value: 0.09, min: 0.0,  max: 1.0,  step: 0.001 },

  // ---- field shape -----------------------------------------------------
  morph:        { group: 'field', type: 'float', value: 0.30, min: 0.0,  max: 1.0,  step: 0.001 },
  morphBass:    { group: 'field', type: 'float', value: 0.25, min: 0.0,  max: 1.0,  step: 0.001, label: 'morph × bass' },
  morphCentroid:{ group: 'field', type: 'float', value: 0.15, min: 0.0,  max: 1.0,  step: 0.001, label: 'morph × centroid' },
  density:      { group: 'field', type: 'float', value: 1.12, min: 0.0,  max: 3.0,  step: 0.001 },
  densityBass:  { group: 'field', type: 'float', value: 0.35, min: 0.0,  max: 2.0,  step: 0.001, label: 'density × bass' },
  noiseScale:   { group: 'field', type: 'float', value: 0.61, min: 0.1,  max: 2.5,  step: 0.001 },
  dispAmp:      { group: 'field', type: 'float', value: 0.10, min: 0.0,  max: 0.6,  step: 0.001 },
  octaveScale:  { group: 'field', type: 'float', value: 0.57, min: 0.3,  max: 0.95, step: 0.001 },
  octaves:      { group: 'field', type: 'int',   value: 5,    min: 2,    max: 7,    step: 1 },

  // ---- raymarch --------------------------------------------------------
  steps:        { group: 'raymarch', type: 'int',   value: 110, min: 24,  max: 220, step: 1 },
  stepSize:     { group: 'raymarch', type: 'float', value: 0.5, min: 0.1, max: 1.2, step: 0.001 },
  near:         { group: 'raymarch', type: 'float', value: 1.5, min: 0.1, max: 6.0, step: 0.01 },
  far:          { group: 'raymarch', type: 'float', value: 60.0,min: 10.0,max: 200.0,step: 0.5 },

  // ---- shading ---------------------------------------------------------
  colorA:       { group: 'shading', type: 'color', value: [0.005, 0.012, 0.030] },
  colorB:       { group: 'shading', type: 'color', value: [0.85,  0.92,  1.05]  },
  colorAccent:  { group: 'shading', type: 'color', value: [0.30,  0.42,  0.95]  },
  accentAmt:    { group: 'shading', type: 'float', value: 0.30,  min: 0.0, max: 2.0, step: 0.001 },
  hueShift:     { group: 'shading', type: 'angle', value: 0.0,   min: -3.1416, max: 3.1416, step: 0.001 },
  hueCentroid:  { group: 'shading', type: 'float', value: 0.5,   min: 0.0, max: 3.14,   step: 0.001, label: 'hue × centroid' },
  fogColor:     { group: 'shading', type: 'color', value: [0.06, 0.11, 0.13] },
  fogDensity:   { group: 'shading', type: 'float', value: 0.20,  min: 0.0, max: 1.0, step: 0.001 },
  lightDir:     { group: 'shading', type: 'angle', value: 0.7,   min: -3.1416, max: 3.1416, step: 0.001 },
  lightStrength:{ group: 'shading', type: 'float', value: 1.0,   min: 0.0, max: 3.0, step: 0.001 },
  hg:           { group: 'shading', type: 'float', value: 0.30,  min: -0.95, max: 0.95, step: 0.001, label: 'HG g' },

  // ---- post ------------------------------------------------------------
  exposure:       { group: 'post', type: 'float', value: 1.10, min: 0.1, max: 4.0, step: 0.001 },
  bloomAmt:       { group: 'post', type: 'float', value: 0.65, min: 0.0, max: 4.0, step: 0.001 },
  bloomThreshold: { group: 'post', type: 'float', value: 0.85, min: 0.0, max: 4.0, step: 0.001 },
  ca:             { group: 'post', type: 'float', value: 0.004, min: 0.0, max: 0.05, step: 0.0001 },
  vignette:       { group: 'post', type: 'float', value: 0.55, min: 0.0, max: 2.0, step: 0.001 },
  grain:          { group: 'post', type: 'float', value: 0.03, min: 0.0, max: 0.2, step: 0.001 },
  gamma:          { group: 'post', type: 'float', value: 2.2,  min: 1.0, max: 3.2, step: 0.001 },
  contrast:       { group: 'post', type: 'float', value: 1.05, min: 0.4, max: 2.0, step: 0.001 },
  saturation:     { group: 'post', type: 'float', value: 1.05, min: 0.0, max: 2.0, step: 0.001 },
};

export function makeState() {
  const s = {};
  for (const [k, def] of Object.entries(PARAM_DEFS)) {
    s[k] = def.type === 'color' ? def.value.slice() : def.value;
  }
  return s;
}

export function applyPreset(state, preset) {
  for (const [k, v] of Object.entries(preset)) {
    if (!(k in PARAM_DEFS)) continue;
    const def = PARAM_DEFS[k];
    if (def.type === 'color' && Array.isArray(v)) state[k] = v.slice();
    else state[k] = v;
  }
}

export function exportPreset(state, meta = {}) {
  const out = { _meta: { app: 'cloud-tunnel-shader', version: 1, ...meta }, params: {} };
  for (const k of Object.keys(PARAM_DEFS)) out.params[k] = state[k];
  return out;
}
