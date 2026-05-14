// Web Audio FFT + a small feature extractor.
// Exposes a single Features object that the render loop reads each frame.

const SR_FALLBACK = 48000;

export class AudioEngine {
  constructor() {
    this.ctx = null;
    this.analyser = null;
    this.source = null;
    this.sourceKind = 'none';     // 'file' | 'mic' | 'none'
    this.element = null;
    this.fft = new Uint8Array(0);
    this.time = new Uint8Array(0);

    // Smoothed / derived features. Range roughly 0..1, except centroid 0..1.
    this.f = {
      bass: 0, mid: 0, treble: 0,
      centroid: 0.5, rms: 0, punch: 0,
      beat: 0,
    };

    // Internal envelopes.
    this._bassFast = 0; this._bassSlow = 0; this._lastBeat = -1; this._beatPhase = 0;
    this._punch = 0;
    this._manualGain = 1.0;
    this.audioBoost = 1.0;
    this.smoothing = 0.78;
  }

  async ensureCtx() {
    if (this.ctx) return;
    this.ctx = new (window.AudioContext || window.webkitAudioContext)();
    this.analyser = this.ctx.createAnalyser();
    this.analyser.fftSize = 2048;
    this.analyser.smoothingTimeConstant = this.smoothing;
    this.fft  = new Uint8Array(this.analyser.frequencyBinCount);
    this.time = new Uint8Array(this.analyser.fftSize);
    this.gain = this.ctx.createGain();
    this.gain.gain.value = 1.0;
    this.gain.connect(this.analyser);
    this.analyser.connect(this.ctx.destination);
  }

  async _disconnectSource() {
    if (this.source) {
      try { this.source.disconnect(); } catch (_) {}
      this.source = null;
    }
    if (this.element) {
      try { this.element.pause(); } catch (_) {}
      if (this.element.src && this.element.src.startsWith('blob:')) URL.revokeObjectURL(this.element.src);
      this.element = null;
    }
    this.sourceKind = 'none';
  }

  async playFile(file) {
    await this.ensureCtx();
    await this._disconnectSource();
    const url = URL.createObjectURL(file);
    const el = new Audio();
    el.src = url;
    el.crossOrigin = 'anonymous';
    el.loop = false;
    this.element = el;
    const src = this.ctx.createMediaElementSource(el);
    src.connect(this.gain);
    this.source = src;
    this.sourceKind = 'file';
    await el.play();
  }

  async useMic() {
    await this.ensureCtx();
    await this._disconnectSource();
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false },
    });
    const src = this.ctx.createMediaStreamSource(stream);
    // Mic should NOT play through speakers, route only to analyser.
    src.connect(this.analyser);
    this.source = src;
    this.sourceKind = 'mic';
  }

  setSmoothing(v) {
    this.smoothing = v;
    if (this.analyser) this.analyser.smoothingTimeConstant = v;
  }

  // Pulls a frame of FFT, derives features. Called every frame.
  // dt: seconds since last frame.
  update(dt) {
    if (!this.analyser) return this.f;
    this.analyser.getByteFrequencyData(this.fft);
    this.analyser.getByteTimeDomainData(this.time);

    const sr = this.ctx?.sampleRate || SR_FALLBACK;
    const nyq = sr * 0.5;
    const bin = nyq / this.fft.length;
    const idxOf = (hz) => Math.max(1, Math.min(this.fft.length - 1, Math.round(hz / bin)));
    const avgRange = (lo, hi) => {
      const a = idxOf(lo), b = idxOf(hi);
      let s = 0; for (let i = a; i <= b; i++) s += this.fft[i];
      return s / (b - a + 1) / 255;
    };

    const bass   = avgRange(30,  140);
    const mid    = avgRange(200, 1800);
    const treble = avgRange(2500, 9000);

    // spectral centroid (normalized 0..1 over the bin range)
    let num = 0, den = 0;
    for (let i = 1; i < this.fft.length; i++) {
      const a = this.fft[i] / 255;
      num += i * a; den += a;
    }
    const centroid = den > 1e-6
      ? Math.min(1, (num / den) / this.fft.length)
      : 0.5;

    // RMS from time domain
    let rms = 0;
    for (let i = 0; i < this.time.length; i++) {
      const v = (this.time[i] - 128) / 128;
      rms += v * v;
    }
    rms = Math.sqrt(rms / this.time.length);

    // Beat detector: two-rate envelope on bass, transient when fast >> slow.
    this._bassFast += (bass - this._bassFast) * Math.min(1, dt * 12.0);
    this._bassSlow += (bass - this._bassSlow) * Math.min(1, dt * 1.5);
    const ratio = this._bassFast / Math.max(this._bassSlow, 0.05);
    const onset = Math.max(0, ratio - 1.25);
    this._punch += (onset * 1.8 - this._punch) * Math.min(1, dt * 8.0);
    this._punch = Math.max(0, this._punch * Math.exp(-dt * 4.0) + onset * 0.4);

    // crude beat phase: when onset spikes, restart phase; otherwise advance by ~2 Hz.
    this._beatPhase += dt * 2.0;
    if (onset > 0.4 && performance.now() - this._lastBeat > 220) {
      this._beatPhase = 0; this._lastBeat = performance.now();
    }
    if (this._beatPhase > 1) this._beatPhase = 1.0;

    const g = this.audioBoost;
    this.f.bass     = Math.min(1, bass * g);
    this.f.mid      = Math.min(1, mid * g);
    this.f.treble   = Math.min(1, treble * g);
    this.f.centroid = centroid;
    this.f.rms      = Math.min(1, rms * 2.0 * g);
    this.f.punch    = Math.min(1, this._punch);
    this.f.beat     = this._beatPhase;
    return this.f;
  }
}
