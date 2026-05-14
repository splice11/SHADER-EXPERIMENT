// Entry point: wires Renderer, AudioEngine, UI together.

import { Renderer } from './renderer.js';
import { AudioEngine } from './audio.js';
import { buildUI, downloadPreset, uploadPreset } from './ui.js';
import { makeState, applyPreset, PARAM_DEFS } from './params.js';

const $ = (sel) => document.querySelector(sel);
const canvas = $('#gl');
const hud    = $('#hud');
const drop   = $('#drop');
const start  = $('#start');

async function fetchText(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  return r.text();
}

const state = makeState();
const defaults = JSON.parse(JSON.stringify(state));

const renderer = new Renderer(canvas);
const audio = new AudioEngine();

const [sceneSrc, compositeSrc, vertSrc] = await Promise.all([
  fetchText('./src/shaders/scene.frag'),
  fetchText('./src/shaders/composite.frag'),
  fetchText('./src/shaders/fullscreen.vert'),
]);
await renderer.init(sceneSrc, compositeSrc, vertSrc);

function resize() {
  renderer.resize(window.innerWidth, window.innerHeight);
}
window.addEventListener('resize', resize);
resize();

// --- UI hooks ---------------------------------------------------------
const hooks = {
  pickFile: () => {
    const i = document.createElement('input');
    i.type = 'file';
    i.accept = 'audio/*';
    i.onchange = () => { const f = i.files?.[0]; if (f) audio.playFile(f).catch(err => alert(err.message)); };
    i.click();
  },
  useMic: () => audio.useMic().catch(err => alert('mic: ' + err.message)),
  stopAudio: () => audio._disconnectSource(),
  setBoost: (v) => { audio.audioBoost = v; },
  setSmoothing: (v) => audio.setSmoothing(v),
  savePreset: () => downloadPreset(state, 'cloud-tunnel'),
  loadPreset: async () => { if (await uploadPreset(state)) ui.syncUI(); },
  resetPreset: () => { applyPreset(state, defaults); ui.syncUI(); },
};
const ui = buildUI($('#ui'), state, hooks);

// --- drag & drop audio ------------------------------------------------
let dragDepth = 0;
window.addEventListener('dragenter', (e) => { e.preventDefault(); dragDepth++; drop.classList.add('active'); });
window.addEventListener('dragleave', (e) => { e.preventDefault(); if (--dragDepth <= 0) drop.classList.remove('active'); });
window.addEventListener('dragover',  (e) => { e.preventDefault(); });
window.addEventListener('drop',      (e) => {
  e.preventDefault(); dragDepth = 0; drop.classList.remove('active');
  const f = e.dataTransfer.files?.[0];
  if (f && f.type.startsWith('audio/')) audio.playFile(f).catch(err => alert(err.message));
});

// --- click-to-start (audio context unlock) ----------------------------
start.addEventListener('click', async () => {
  await audio.ensureCtx();
  start.classList.add('hidden');
}, { once: true });

// --- render loop ------------------------------------------------------
let last = performance.now();
let fpsAccum = 0, fpsCount = 0, fpsT = 0;
function frame(now) {
  const dt = Math.min(0.1, (now - last) * 0.001);
  last = now;
  const t = now * 0.001;
  const features = audio.update(dt);
  renderer.render(state, features, t);

  fpsAccum += dt; fpsCount++; fpsT += dt;
  if (fpsT >= 0.5) {
    const fps = fpsCount / fpsAccum;
    hud.textContent =
      `${fps.toFixed(0)} fps · ${renderer.w}×${renderer.h} · ${audio.sourceKind} · ` +
      `bass ${features.bass.toFixed(2)} mid ${features.mid.toFixed(2)} ` +
      `tre ${features.treble.toFixed(2)} rms ${features.rms.toFixed(2)} ` +
      `pun ${features.punch.toFixed(2)} bt ${features.beat.toFixed(2)}`;
    if (ui && hooks._setMon) hooks._setMon(features);
    fpsAccum = 0; fpsCount = 0; fpsT = 0;
  } else if (hooks._setMon) {
    hooks._setMon(features);
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

// Hot-reload presets via window.* for tinkering from devtools.
window.__shader = { state, defaults, PARAM_DEFS, audio, renderer, ui, applyPreset };
