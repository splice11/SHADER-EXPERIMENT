use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default)]
pub struct Features {
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub centroid: f32,
    pub rms: f32,
    pub punch: f32,
}

pub struct Audio {
    features: Arc<Mutex<Features>>,
    _stream: Option<cpal::Stream>,
    pub source_name: String,
}

impl Audio {
    pub fn start() -> Self {
        let features = Arc::new(Mutex::new(Features::default()));
        match build_stream(features.clone()) {
            Ok((stream, name)) => {
                log::info!("audio capture: {name}");
                Self {
                    features,
                    _stream: Some(stream),
                    source_name: name,
                }
            }
            Err(e) => {
                log::warn!("audio capture disabled: {e:#}");
                Self {
                    features,
                    _stream: None,
                    source_name: "none".to_string(),
                }
            }
        }
    }

    pub fn read(&self) -> Features {
        *self.features.lock().unwrap()
    }
}

const FFT_SIZE: usize = 1024;

fn build_stream(features: Arc<Mutex<Features>>) -> Result<(cpal::Stream, String)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no default audio input device"))?;
    let name = device.name().unwrap_or_else(|_| "input".into());
    let supported = device.default_input_config()?;
    let sample_rate = supported.sample_rate().0 as f32;
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let mut planner = rustfft::FftPlanner::<f32>::new();
    let fft: Arc<dyn rustfft::Fft<f32>> = planner.plan_fft_forward(FFT_SIZE);

    let err_fn = |e| log::warn!("audio stream error: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let mut state = ProcState::new(channels, sample_rate, fft, features);
            device.build_input_stream(
                &config,
                move |data: &[f32], _| state.feed_f32(data),
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let mut state = ProcState::new(channels, sample_rate, fft, features);
            device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    let buf: Vec<f32> = data
                        .iter()
                        .map(|s| *s as f32 / i16::MAX as f32)
                        .collect();
                    state.feed_f32(&buf);
                },
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let mut state = ProcState::new(channels, sample_rate, fft, features);
            device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    let half = u16::MAX as f32 * 0.5;
                    let buf: Vec<f32> =
                        data.iter().map(|s| (*s as f32 - half) / half).collect();
                    state.feed_f32(&buf);
                },
                err_fn,
                None,
            )?
        }
        fmt => anyhow::bail!("unsupported sample format {:?}", fmt),
    };

    stream.play()?;
    Ok((stream, name))
}

struct ProcState {
    channels: usize,
    sample_rate: f32,
    fft: Arc<dyn rustfft::Fft<f32>>,
    features: Arc<Mutex<Features>>,
    ring: Vec<f32>,
    bass_fast: f32,
    bass_slow: f32,
    punch_env: f32,
}

impl ProcState {
    fn new(
        channels: usize,
        sample_rate: f32,
        fft: Arc<dyn rustfft::Fft<f32>>,
        features: Arc<Mutex<Features>>,
    ) -> Self {
        Self {
            channels: channels.max(1),
            sample_rate,
            fft,
            features,
            ring: vec![0.0; FFT_SIZE],
            bass_fast: 0.0,
            bass_slow: 0.0,
            punch_env: 0.0,
        }
    }

    fn feed_f32(&mut self, data: &[f32]) {
        let frames = data.len() / self.channels;
        if frames == 0 {
            return;
        }

        // Downmix to mono.
        let mut mono = Vec::with_capacity(frames);
        for i in 0..frames {
            let mut s = 0.0;
            for c in 0..self.channels {
                s += data[i * self.channels + c];
            }
            mono.push(s / self.channels as f32);
        }

        // Shift into ring.
        let n = mono.len();
        if n >= FFT_SIZE {
            self.ring.copy_from_slice(&mono[n - FFT_SIZE..]);
        } else {
            self.ring.copy_within(n.., 0);
            self.ring[FFT_SIZE - n..].copy_from_slice(&mono);
        }

        // Time-domain RMS.
        let rms = (self.ring.iter().map(|s| s * s).sum::<f32>() / FFT_SIZE as f32).sqrt();

        // Windowed FFT.
        let mut buf: Vec<rustfft::num_complex::Complex<f32>> = self
            .ring
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let w = 0.5
                    - 0.5
                        * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos();
                rustfft::num_complex::Complex {
                    re: *s * w,
                    im: 0.0,
                }
            })
            .collect();
        self.fft.process(&mut buf);

        let bin_hz = self.sample_rate / FFT_SIZE as f32;
        let mut bass = 0.0;
        let mut bass_n = 0;
        let mut mid = 0.0;
        let mut mid_n = 0;
        let mut treble = 0.0;
        let mut treble_n = 0;
        let mut spec_sum = 0.0;
        let mut weighted = 0.0;

        for i in 1..FFT_SIZE / 2 {
            let c = buf[i];
            let mag = (c.re * c.re + c.im * c.im).sqrt();
            let f = i as f32 * bin_hz;
            spec_sum += mag;
            weighted += mag * f;
            if f >= 30.0 && f < 140.0 {
                bass += mag;
                bass_n += 1;
            } else if f >= 200.0 && f < 1800.0 {
                mid += mag;
                mid_n += 1;
            } else if f >= 2500.0 && f < 9000.0 {
                treble += mag;
                treble_n += 1;
            }
        }

        // 0.01 brings raw FFT magnitudes into ~0..1 before squashing.
        let norm = |x: f32, n: usize| {
            if n > 0 {
                (x / n as f32 * 0.01).tanh()
            } else {
                0.0
            }
        };
        let bass = norm(bass, bass_n);
        let mid = norm(mid, mid_n);
        let treble = norm(treble, treble_n);
        let centroid = if spec_sum > 1e-6 {
            (weighted / spec_sum / (self.sample_rate * 0.5)).clamp(0.0, 1.0)
        } else {
            0.5
        };

        // Two-rate envelope → onset → punch (mirrors audio.js).
        let dt = FFT_SIZE as f32 / self.sample_rate;
        let a_fast = (dt * 12.0).min(1.0);
        let a_slow = (dt * 1.5).min(1.0);
        self.bass_fast += (bass - self.bass_fast) * a_fast;
        self.bass_slow += (bass - self.bass_slow) * a_slow;
        let ratio = self.bass_fast / self.bass_slow.max(0.05);
        let onset = (ratio - 1.25).max(0.0);
        self.punch_env =
            (self.punch_env * (-dt * 4.0).exp() + onset * 0.4).max(0.0);

        if let Ok(mut f) = self.features.lock() {
            let a = 0.35;
            f.bass = f.bass * (1.0 - a) + bass * a;
            f.mid = f.mid * (1.0 - a) + mid * a;
            f.treble = f.treble * (1.0 - a) + treble * a;
            f.centroid = f.centroid * (1.0 - a) + centroid * a;
            f.rms = f.rms * (1.0 - a) + (rms * 2.0).tanh() * a;
            f.punch = self.punch_env.min(1.0);
        }
    }
}
