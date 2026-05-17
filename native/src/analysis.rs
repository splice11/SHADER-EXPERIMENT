// Offline pre-analysis of a decoded track. Produces a `CueTrack` that the
// bake-mode director consults each frame so events (lightning, palette swaps,
// whip-pans, tunnel-glow build) land *on* musical structure instead of
// chasing smoothed envelopes that lag a half-beat behind.
//
// Pipeline:
//   1. Per-hop magnitude spectra (Hann-windowed FFT, hop ≈ 11.6 ms @ 44.1 kHz)
//   2. Multi-band spectral flux → bass / mid / treble onset envelopes
//   3. Combined onset envelope (rhythm-band weighted)
//   4. Autocorrelation over a BPM-plausible lag range → beat period
//   5. Best-phase alignment of the period across the song → beat grid
//   6. Drop detection from RMS curve + bass onset (impact preceded by quiet)
//   7. Build segments: walk back from each drop to the local energy minimum

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::sync::Arc;

const FFT_SIZE: usize = 1024;
const HOP: usize = 512;

/// Output of the pre-analysis pass. All timestamps are in seconds from the
/// start of the track.
pub struct CueTrack {
    /// Seconds per onset-envelope sample.
    pub hop_secs: f32,

    pub onset_bass: Vec<f32>,
    pub onset_mid: Vec<f32>,
    pub onset_treble: Vec<f32>,
    pub onset_combined: Vec<f32>,

    pub bpm: f32,              // 0 if undetected
    pub beat_period_secs: f32, // 0 if undetected
    /// Confidence ∈ [0, 1] — best autocorr peak height over local mean.
    /// Below ~0.2 means "no usable beat" and cued events should fall back to
    /// reactive behaviour.
    pub beat_confidence: f32,
    pub beats: Vec<f32>,
    pub bar_length_beats: usize,
    pub phrase_marks: Vec<f32>,

    pub drops: Vec<f32>,
    pub builds: Vec<(f32, f32)>, // (start, end)
}

impl CueTrack {
    pub fn empty() -> Self {
        Self {
            hop_secs: HOP as f32 / 44_100.0,
            onset_bass: Vec::new(),
            onset_mid: Vec::new(),
            onset_treble: Vec::new(),
            onset_combined: Vec::new(),
            bpm: 0.0,
            beat_period_secs: 0.0,
            beat_confidence: 0.0,
            beats: Vec::new(),
            bar_length_beats: 4,
            phrase_marks: Vec::new(),
            drops: Vec::new(),
            builds: Vec::new(),
        }
    }
}

pub fn analyse(pcm: &[f32], sample_rate: u32) -> CueTrack {
    if pcm.len() < FFT_SIZE * 4 || sample_rate == 0 {
        return CueTrack::empty();
    }

    let hop_secs = HOP as f32 / sample_rate as f32;
    let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
    let bass = bin_range(30.0, 140.0, bin_hz);
    let mid = bin_range(200.0, 1800.0, bin_hz);
    let treble = bin_range(2500.0, 9000.0, bin_hz);

    let mut planner = FftPlanner::<f32>::new();
    let fft: Arc<dyn rustfft::Fft<f32>> = planner.plan_fft_forward(FFT_SIZE);

    let n_hops = (pcm.len() - FFT_SIZE) / HOP + 1;
    let mut buf = vec![Complex { re: 0.0_f32, im: 0.0 }; FFT_SIZE];
    let mut prev_bass = vec![0.0_f32; bass.1 - bass.0];
    let mut prev_mid = vec![0.0_f32; mid.1 - mid.0];
    let mut prev_treble = vec![0.0_f32; treble.1 - treble.0];

    let mut onset_bass = Vec::with_capacity(n_hops);
    let mut onset_mid = Vec::with_capacity(n_hops);
    let mut onset_treble = Vec::with_capacity(n_hops);
    let mut rms_curve = Vec::with_capacity(n_hops);

    for hop_idx in 0..n_hops {
        let start = hop_idx * HOP;
        if start + FFT_SIZE > pcm.len() {
            break;
        }
        let mut rms = 0.0_f32;
        for i in 0..FFT_SIZE {
            let s = pcm[start + i];
            rms += s * s;
            let w = 0.5
                - 0.5
                    * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos();
            buf[i] = Complex { re: s * w, im: 0.0 };
        }
        rms_curve.push((rms / FFT_SIZE as f32).sqrt());
        fft.process(&mut buf);

        onset_bass.push(band_flux(&buf, bass, &mut prev_bass));
        onset_mid.push(band_flux(&buf, mid, &mut prev_mid));
        onset_treble.push(band_flux(&buf, treble, &mut prev_treble));
    }

    // Local-mean subtraction + global normalisation. Catches both quiet songs
    // and songs with one freakishly loud transient that would otherwise dominate.
    let lm_window_hops = (0.6 / hop_secs).max(8.0) as usize;
    normalise_local_mean(&mut onset_bass, lm_window_hops);
    normalise_local_mean(&mut onset_mid, lm_window_hops);
    normalise_local_mean(&mut onset_treble, lm_window_hops);

    let onset_combined: Vec<f32> = (0..onset_bass.len())
        .map(|i| {
            (onset_bass[i] * 0.55 + onset_mid[i] * 0.30 + onset_treble[i] * 0.15)
                .clamp(0.0, 4.0)
        })
        .collect();

    // Tempo + phase.
    let (beat_period_hops, beat_confidence) =
        estimate_beat_period_hops(&onset_combined, hop_secs);
    let beat_period_secs = beat_period_hops as f32 * hop_secs;
    let bpm = if beat_period_hops > 0 { 60.0 / beat_period_secs } else { 0.0 };
    let beats = if beat_period_hops > 0 && beat_confidence > 0.18 {
        let phase = best_phase(&onset_combined, beat_period_hops);
        generate_beat_grid(phase, beat_period_hops, n_hops, hop_secs)
    } else {
        Vec::new()
    };

    let bar_length_beats = 4;
    let phrase_marks: Vec<f32> = beats
        .iter()
        .copied()
        .step_by(bar_length_beats * 4)
        .collect();

    // Drops + builds.
    let drops = detect_drops(&rms_curve, &onset_bass, hop_secs);
    let builds = derive_builds(&drops, &rms_curve, hop_secs);

    CueTrack {
        hop_secs,
        onset_bass,
        onset_mid,
        onset_treble,
        onset_combined,
        bpm,
        beat_period_secs,
        beat_confidence,
        beats,
        bar_length_beats,
        phrase_marks,
        drops,
        builds,
    }
}

// ---------- helpers ----------

fn bin_range(lo_hz: f32, hi_hz: f32, bin_hz: f32) -> (usize, usize) {
    let half = FFT_SIZE / 2;
    let lo = ((lo_hz / bin_hz) as usize).max(1).min(half - 2);
    let hi = ((hi_hz / bin_hz) as usize).max(lo + 1).min(half - 1);
    (lo, hi)
}

fn band_flux(spec: &[Complex<f32>], range: (usize, usize), prev: &mut [f32]) -> f32 {
    let mut sum = 0.0_f32;
    for i in range.0..range.1 {
        let c = spec[i];
        let mag = (c.re * c.re + c.im * c.im).sqrt();
        sum += (mag - prev[i - range.0]).max(0.0);
        prev[i - range.0] = mag;
    }
    sum / (range.1 - range.0).max(1) as f32
}

/// Subtract a running local mean, half-wave rectify, then normalise to peak.
fn normalise_local_mean(v: &mut [f32], window: usize) {
    if v.is_empty() {
        return;
    }
    let mut sum = 0.0_f32;
    let mut means = vec![0.0_f32; v.len()];
    for i in 0..v.len() {
        sum += v[i];
        if i >= window {
            sum -= v[i - window];
        }
        let n = (i + 1).min(window) as f32;
        means[i] = sum / n;
    }
    let mut peak = 1e-6_f32;
    for i in 0..v.len() {
        v[i] = (v[i] - means[i]).max(0.0);
        if v[i] > peak {
            peak = v[i];
        }
    }
    for x in v.iter_mut() {
        *x /= peak;
    }
}

/// Returns (best lag in hops, confidence). Confidence ∈ [0, 1].
fn estimate_beat_period_hops(onset: &[f32], hop_secs: f32) -> (usize, f32) {
    if onset.is_empty() {
        return (0, 0.0);
    }
    // Plausible tempo range 60..200 BPM.
    let min_lag = (60.0 / 200.0 / hop_secs).max(2.0) as usize;
    let max_lag = (60.0 / 60.0 / hop_secs).max(min_lag as f32 + 1.0) as usize;
    if onset.len() < max_lag * 4 {
        return (0, 0.0);
    }
    let mut scores = vec![0.0_f32; max_lag + 1];
    for lag in min_lag..=max_lag {
        let mut s = 0.0_f32;
        let n = onset.len() - lag;
        for i in 0..n {
            s += onset[i] * onset[i + lag];
        }
        scores[lag] = s / n as f32;
    }
    // Pick the lag with the highest *normalised* score; bias is mild so we
    // prefer the actual beat (~0.5 s) over half-bar / bar harmonics (~2 s).
    let mut best = min_lag;
    let mut best_weighted = -f32::INFINITY;
    for lag in min_lag..=max_lag {
        let weighted = scores[lag] / (lag as f32).sqrt();
        if weighted > best_weighted {
            best_weighted = weighted;
            best = lag;
        }
    }
    // Confidence = peak score / median score (above 0 means there's a peak).
    let mut sorted: Vec<f32> = scores[min_lag..=max_lag].to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2].max(1e-6);
    let confidence = (scores[best] / median / 4.0).clamp(0.0, 1.0);
    (best, confidence)
}

/// Pick the phase offset 0..period that maximises summed onset energy at the
/// resulting beat positions.
fn best_phase(onset: &[f32], period: usize) -> usize {
    if period == 0 || onset.is_empty() {
        return 0;
    }
    let mut best = 0;
    let mut best_score = -f32::INFINITY;
    for phase in 0..period {
        let mut s = 0.0_f32;
        let mut i = phase;
        while i < onset.len() {
            s += onset[i];
            i += period;
        }
        if s > best_score {
            best_score = s;
            best = phase;
        }
    }
    best
}

fn generate_beat_grid(
    phase: usize,
    period: usize,
    n_hops: usize,
    hop_secs: f32,
) -> Vec<f32> {
    let mut beats = Vec::with_capacity(n_hops / period + 1);
    let mut i = phase;
    while i < n_hops {
        beats.push(i as f32 * hop_secs);
        i += period;
    }
    beats
}

/// A drop = a frame where current RMS is well above the recent floor and the
/// bass band has a strong onset. Min gap 3.5 s between drops.
fn detect_drops(rms: &[f32], bass_onset: &[f32], hop_secs: f32) -> Vec<f32> {
    if rms.is_empty() {
        return Vec::new();
    }
    let s_lookback = (4.0 / hop_secs) as usize;
    let s_lead = (0.8 / hop_secs) as usize;
    let global_avg: f32 = rms.iter().sum::<f32>() / rms.len() as f32;
    let avg_safe = global_avg.max(1e-3);

    let mut score = vec![0.0_f32; rms.len()];
    for i in s_lookback..rms.len() {
        let mut pre = 0.0_f32;
        let lookback_start = i.saturating_sub(s_lookback);
        let lookback_end = i.saturating_sub(s_lead);
        let mut n = 0;
        for j in lookback_start..lookback_end {
            pre += rms[j];
            n += 1;
        }
        if n > 0 {
            pre /= n as f32;
        }
        let pre_quiet = (1.0 - pre / avg_safe).clamp(0.0, 1.0);
        let here = (rms[i] / avg_safe - 1.0).max(0.0);
        let bass_kick = bass_onset[i];
        score[i] = here * pre_quiet * (0.7 + bass_kick * 2.5);
    }

    // Peak-pick with a min-gap of ~3.5 s.
    let min_gap_hops = (3.5 / hop_secs) as usize;
    let local_win = (0.6 / hop_secs) as usize;
    let threshold = 0.55_f32;
    let mut drops = Vec::new();
    let mut i = 0;
    while i < score.len() {
        if score[i] >= threshold {
            let win_end = (i + local_win).min(score.len());
            let mut peak = i;
            for j in i..win_end {
                if score[j] > score[peak] {
                    peak = j;
                }
            }
            drops.push(peak as f32 * hop_secs);
            i = peak + min_gap_hops;
        } else {
            i += 1;
        }
    }
    drops
}

/// For each drop, the build is the energy ramp leading into it. Walk back
/// from the drop until we find the recent RMS minimum (within an 8 s window).
fn derive_builds(drops: &[f32], rms: &[f32], hop_secs: f32) -> Vec<(f32, f32)> {
    let mut builds = Vec::with_capacity(drops.len());
    let max_back_hops = (8.0 / hop_secs) as usize;
    for &drop_t in drops {
        let drop_idx = ((drop_t / hop_secs) as usize).min(rms.len().saturating_sub(1));
        let lo = drop_idx.saturating_sub(max_back_hops);
        let mut min_idx = drop_idx;
        let mut min_val = f32::INFINITY;
        for j in lo..drop_idx {
            if rms[j] < min_val {
                min_val = rms[j];
                min_idx = j;
            }
        }
        let start_t = min_idx as f32 * hop_secs;
        if drop_t - start_t > 0.7 {
            builds.push((start_t, drop_t));
        }
    }
    builds
}
