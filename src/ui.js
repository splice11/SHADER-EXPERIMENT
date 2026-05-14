// Tweakpane UI + preset import/export.
import { Pane } from 'tweakpane';
import { PARAM_DEFS, exportPreset, applyPreset } from './params.js';

function groupTitle(g) {
  return ({ motion: 'motion', field: 'field shape', raymarch: 'raymarch',
            shading: 'shading', post: 'post' })[g] || g;
}

function colorObj(arr) { return { r: arr[0], g: arr[1], b: arr[2] }; }
function colorBack(state, key, obj) { state[key][0] = obj.r; state[key][1] = obj.g; state[key][2] = obj.b; }

export function buildUI(rootEl, state, hooks) {
  const pane = new Pane({ container: rootEl, title: 'cloud tunnel — shader lab' });

  // --- audio source bar -------------------------------------------------
  const audioFolder = pane.addFolder({ title: 'audio source', expanded: true });
  audioFolder.addButton({ title: 'pick audio file…' }).on('click', () => hooks.pickFile());
  audioFolder.addButton({ title: 'use microphone' }).on('click', () => hooks.useMic());
  audioFolder.addButton({ title: 'stop audio' }).on('click', () => hooks.stopAudio());
  const audioCfg = { boost: 1.0, smoothing: 0.78 };
  audioFolder.addBinding(audioCfg, 'boost', { min: 0.1, max: 6.0, step: 0.01 })
             .on('change', e => hooks.setBoost(e.value));
  audioFolder.addBinding(audioCfg, 'smoothing', { min: 0.0, max: 0.99, step: 0.01 })
             .on('change', e => hooks.setSmoothing(e.value));

  // --- live audio readout (read-only graphs) ---------------------------
  const monitor = audioFolder.addFolder({ title: 'monitor', expanded: false });
  const mon = { bass: 0, mid: 0, treble: 0, rms: 0, punch: 0, centroid: 0.5 };
  for (const k of ['bass','mid','treble','rms','punch','centroid']) {
    monitor.addBinding(mon, k, { readonly: true, view: 'graph', min: 0, max: 1, interval: 33 });
  }
  hooks._setMon = (f) => {
    mon.bass = f.bass; mon.mid = f.mid; mon.treble = f.treble;
    mon.rms = f.rms;   mon.punch = f.punch; mon.centroid = f.centroid;
    monitor.refresh();
  };

  // --- presets ---------------------------------------------------------
  const pf = pane.addFolder({ title: 'presets', expanded: true });
  pf.addButton({ title: 'save preset .json' }).on('click', () => hooks.savePreset());
  pf.addButton({ title: 'load preset .json' }).on('click', () => hooks.loadPreset());
  pf.addButton({ title: 'reset to defaults' }).on('click', () => hooks.resetPreset());

  // --- param folders by group ------------------------------------------
  const folders = {};
  const bindings = {};
  const groups = ['motion', 'field', 'raymarch', 'shading', 'post'];
  for (const g of groups) folders[g] = pane.addFolder({ title: groupTitle(g), expanded: g === 'motion' });

  for (const [k, def] of Object.entries(PARAM_DEFS)) {
    const folder = folders[def.group];
    const label = def.label || k;
    if (def.type === 'color') {
      const obj = { [k]: colorObj(state[k]) };
      const b = folder.addBinding(obj, k, { label, color: { type: 'float' } })
        .on('change', e => colorBack(state, k, e.value));
      bindings[k] = { obj, binding: b, isColor: true };
    } else {
      const proxy = { [k]: state[k] };
      const opts = { label, min: def.min, max: def.max, step: def.step };
      const b = folder.addBinding(proxy, k, opts)
        .on('change', e => { state[k] = e.value; });
      bindings[k] = { obj: proxy, binding: b, isColor: false };
    }
  }

  // External code can call this to refresh widgets after a preset load.
  function syncUI() {
    for (const [k, def] of Object.entries(PARAM_DEFS)) {
      const b = bindings[k];
      if (!b) continue;
      if (b.isColor) b.obj[k] = colorObj(state[k]);
      else b.obj[k] = state[k];
      b.binding.refresh();
    }
  }

  return { pane, syncUI };
}

// --- preset file I/O --------------------------------------------------
export function downloadPreset(state, name = 'preset') {
  const blob = new Blob([JSON.stringify(exportPreset(state, { name }), null, 2)],
                        { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  a.href = url; a.download = `${name}-${stamp}.json`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function uploadPreset(state) {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.onchange = async () => {
      const f = input.files?.[0];
      if (!f) return resolve(false);
      try {
        const obj = JSON.parse(await f.text());
        applyPreset(state, obj.params || obj);
        resolve(true);
      } catch (e) {
        console.error(e); resolve(false);
      }
    };
    input.click();
  });
}
