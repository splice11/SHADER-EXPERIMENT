use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const FFT_SIZE: usize = 1024;
const FEATURE_HOP: usize = 1024; // ~23 ms at 44.1 kHz

#[derive(Clone, Copy, Debug, Default)]
pub struct Features {
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub centroid: f32,
    pub rms: f32,
    pub punch: f32,
}

pub struct FilePlayback {
    pub path: PathBuf,
    pub pcm: Arc<Vec<f32>>, // mono PCM at the file's native sample rate
    pub sample_rate: u32,
    pub features: Arc<Vec<Features>>,
    pub hop_samples: usize,
    pub state: Arc<Mutex<PlaybackState>>,
}

#[derive(Default)]
pub struct PlaybackState {
    pub position_samples: u64,
    pub playing: bool,
    pub finished: bool,
}

enum InnerMode {
    None,
    Live(cpal::Stream),
    File { playback: FilePlayback, _stream: cpal::Stream },
}

pub struct Audio {
    pub source_name: String,
    last_features: Arc<Mutex<Features>>,
    inner: InnerMode,
}

impl Audio {
    pub fn start() -> Self {
        let features = Arc::new(Mutex::new(Features::default()));
        match build_live_stream(features.clone()) {
            Ok((stream, name)) => {
                log::info!("audio capture: {name}");
                Self {
                    source_name: name,
                    last_features: features,
                    inner: InnerMode::Live(stream),
                }
            }
            Err(e) => {
                log::warn!("audio capture disabled: {e:#}");
                Self {
                    source_name: "none".into(),
                    last_features: features,
                    inner: InnerMode::None,
                }
            }
        }
    }

    pub fn read(&self) -> Features {
        *self.last_features.lock().unwrap()
    }

    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        let playback = decode_and_analyze(path)?;
        let last = self.last_features.clone();
        let stream = build_file_output_stream(&playback, last)?;
        self.source_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        self.inner = InnerMode::File { playback, _stream: stream };
        Ok(())
    }

    pub fn unload_file(&mut self) {
        let features = self.last_features.clone();
        *features.lock().unwrap() = Features::default();
        match build_live_stream(features.clone()) {
            Ok((stream, name)) => {
                self.source_name = name;
                self.inner = InnerMode::Live(stream);
            }
            Err(_) => {
                self.source_name = "none".into();
                self.inner = InnerMode::None;
            }
        }
    }

    pub fn is_file_mode(&self) -> bool {
        matches!(self.inner, InnerMode::File { .. })
    }

    pub fn file_playback(&self) -> Option<&FilePlayback> {
        match &self.inner {
            InnerMode::File { playback, .. } => Some(playback),
            _ => None,
        }
    }

    pub fn play(&self) {
        if let InnerMode::File { playback, .. } = &self.inner {
            let mut s = playback.state.lock().unwrap();
            if s.finished {
                s.position_samples = 0;
                s.finished = false;
            }
            s.playing = true;
        }
    }
    pub fn pause(&self) {
        if let InnerMode::File { playback, .. } = &self.inner {
            playback.state.lock().unwrap().playing = false;
        }
    }
    pub fn seek_secs(&self, t: f32) {
        if let InnerMode::File { playback, .. } = &self.inner {
            let mut s = playback.state.lock().unwrap();
            s.position_samples = ((t.max(0.0) * playback.sample_rate as f32) as u64)
                .min(playback.pcm.len() as u64);
            s.finished = false;
        }
    }
    pub fn position_secs(&self) -> Option<f32> {
        self.file_playback().map(|p| {
            let pos = p.state.lock().unwrap().position_samples;
            pos as f32 / p.sample_rate as f32
        })
    }
    pub fn duration_secs(&self) -> Option<f32> {
        self.file_playback()
            .map(|p| p.pcm.len() as f32 / p.sample_rate as f32)
    }
    pub fn is_playing(&self) -> bool {
        match &self.inner {
            InnerMode::File { playback, .. } => playback.state.lock().unwrap().playing,
            _ => true,
        }
    }

    /// Look up features at an absolute time in the loaded file. In live mode
    /// this falls back to the current frame's features.
    pub fn features_at_secs(&self, t: f32) -> Features {
        if let Some(p) = self.file_playback() {
            let i = ((t.max(0.0) * p.sample_rate as f32) as usize / p.hop_samples)
                .min(p.features.len().saturating_sub(1));
            p.features[i]
        } else {
            self.read()
        }
    }
}

// ---------- decode + pre-analysis ----------

fn decode_and_analyze(path: &Path) -> Result<FilePlayback> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).context("open audio file")?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("probe audio format")?;
    let mut format = probed.format;
    let track = format.default_track().context("no default audio track")?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params.sample_rate.unwrap_or(44100);
    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .context("create decoder")?;

    let mut mono = Vec::<f32>::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        let mut sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        sb.copy_interleaved_ref(decoded);
        let inter = sb.samples();
        let ch = spec.channels.count();
        if ch == 0 {
            continue;
        }
        let frames = inter.len() / ch;
        mono.reserve(frames);
        for f in 0..frames {
            let mut s = 0.0;
            for c in 0..ch {
                s += inter[f * ch + c];
            }
            mono.push(s / ch as f32);
        }
    }

    log::info!(
        "decoded {} samples ({:.2}s) at {} Hz",
        mono.len(),
        mono.len() as f32 / sample_rate as f32,
        sample_rate
    );

    let mut planner = rustfft::FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut extractor = FeatureExtractor::new(sample_rate as f32, fft);
    let mut ring = vec![0.0_f32; FFT_SIZE];
    let mut features = Vec::with_capacity(mono.len() / FEATURE_HOP + 1);
    let mut i = 0;
    while i + FEATURE_HOP <= mono.len() {
        ring.copy_within(FEATURE_HOP.., 0);
        ring[FFT_SIZE - FEATURE_HOP..].copy_from_slice(&mono[i..i + FEATURE_HOP]);
        features.push(extractor.process_window(&ring));
        i += FEATURE_HOP;
    }
    if features.is_empty() {
        features.push(Features::default());
    }

    Ok(FilePlayback {
        path: path.to_path_buf(),
        pcm: Arc::new(mono),
        sample_rate,
        features: Arc::new(features),
        hop_samples: FEATURE_HOP,
        state: Arc::new(Mutex::new(PlaybackState::default())),
    })
}

fn build_file_output_stream(
    playback: &FilePlayback,
    last_features: Arc<Mutex<Features>>,
) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no default audio output device"))?;
    let supported = device.default_output_config()?;
    let sample_format = supported.sample_format();
    let device_sample_rate = supported.sample_rate().0;
    let device_channels = supported.channels() as usize;
    let config: cpal::StreamConfig = supported.into();

    let pcm = playback.pcm.clone();
    let features_buf = playback.features.clone();
    let state = playback.state.clone();
    let file_sample_rate = playback.sample_rate;
    let hop_samples = playback.hop_samples;
    let pcm_len = pcm.len();
    let rate_ratio = file_sample_rate as f64 / device_sample_rate as f64;

    let err_fn = |e| log::warn!("audio output stream error: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config,
            move |out: &mut [f32], _| {
                fill_output_f32(
                    out, device_channels, rate_ratio, pcm_len, &pcm,
                    &features_buf, hop_samples, &state, &last_features,
                );
            },
            err_fn, None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config,
            move |out: &mut [i16], _| {
                let mut scratch = vec![0.0f32; out.len()];
                fill_output_f32(
                    &mut scratch, device_channels, rate_ratio, pcm_len, &pcm,
                    &features_buf, hop_samples, &state, &last_features,
                );
                for i in 0..out.len() {
                    out[i] = (scratch[i].clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                }
            },
            err_fn, None,
        )?,
        cpal::SampleFormat::U16 => device.build_output_stream(
            &config,
            move |out: &mut [u16], _| {
                let mut scratch = vec![0.0f32; out.len()];
                fill_output_f32(
                    &mut scratch, device_channels, rate_ratio, pcm_len, &pcm,
                    &features_buf, hop_samples, &state, &last_features,
                );
                for i in 0..out.len() {
                    let v = (scratch[i].clamp(-1.0, 1.0) + 1.0) * 0.5 * u16::MAX as f32;
                    out[i] = v as u16;
                }
            },
            err_fn, None,
        )?,
        fmt => anyhow::bail!("unsupported output sample format {:?}", fmt),
    };
    stream.play()?;
    Ok(stream)
}

#[allow(clippy::too_many_arguments)]
fn fill_output_f32(
    out: &mut [f32],
    device_channels: usize,
    rate_ratio: f64,
    pcm_len: usize,
    pcm: &[f32],
    features_buf: &[Features],
    hop_samples: usize,
    state: &Mutex<PlaybackState>,
    last_features: &Mutex<Features>,
) {
    let frames = out.len() / device_channels;
    let mut state = state.lock().unwrap();
    let playing = state.playing && !state.finished;
    let start_pos = state.position_samples;

    for f in 0..frames {
        let sample = if playing {
            let s = start_pos as f64 + f as f64 * rate_ratio;
            let idx = s as usize;
            if idx >= pcm_len { 0.0 } else { pcm[idx] }
        } else {
            0.0
        };
        for c in 0..device_channels {
            out[f * device_channels + c] = sample;
        }
    }

    if playing {
        let new_pos = (start_pos as f64 + frames as f64 * rate_ratio) as u64;
        if new_pos as usize >= pcm_len {
            state.position_samples = pcm_len as u64;
            state.finished = true;
            state.playing = false;
        } else {
            state.position_samples = new_pos;
        }
        let feat_idx = (state.position_samples as usize / hop_samples)
            .min(features_buf.len().saturating_sub(1));
        if let Ok(mut lf) = last_features.lock() {
            *lf = features_buf[feat_idx];
        }
    }
}

// ---------- feature extractor (shared) ----------

struct FeatureExtractor {
    sample_rate: f32,
    fft: Arc<dyn rustfft::Fft<f32>>,
    bass_fast: f32,
    bass_slow: f32,
    punch_env: f32,
    smoothed: Features,
}

impl FeatureExtractor {
    fn new(sample_rate: f32, fft: Arc<dyn rustfft::Fft<f32>>) -> Self {
        Self {
            sample_rate, fft,
            bass_fast: 0.0, bass_slow: 0.0, punch_env: 0.0,
            smoothed: Features::default(),
        }
    }

    fn process_window(&mut self, window: &[f32]) -> Features {
        let n = window.len();
        let rms = (window.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();

        let mut buf: Vec<rustfft::num_complex::Complex<f32>> = window
            .iter().enumerate().map(|(i, s)| {
                let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n - 1) as f32).cos();
                rustfft::num_complex::Complex { re: *s * w, im: 0.0 }
            }).collect();
        self.fft.process(&mut buf);

        let bin_hz = self.sample_rate / n as f32;
        let mut bass = 0.0; let mut bass_n = 0;
        let mut mid = 0.0; let mut mid_n = 0;
        let mut treble = 0.0; let mut treble_n = 0;
        let mut spec_sum = 0.0;
        let mut weighted = 0.0;

        for i in 1..n / 2 {
            let c = buf[i];
            let mag = (c.re * c.re + c.im * c.im).sqrt();
            let f = i as f32 * bin_hz;
            spec_sum += mag;
            weighted += mag * f;
            if f >= 30.0 && f < 140.0 { bass += mag; bass_n += 1; }
            else if f >= 200.0 && f < 1800.0 { mid += mag; mid_n += 1; }
            else if f >= 2500.0 && f < 9000.0 { treble += mag; treble_n += 1; }
        }

        let norm = |x: f32, n: usize| if n > 0 { (x / n as f32 * 0.01).tanh() } else { 0.0 };
        let bass = norm(bass, bass_n);
        let mid = norm(mid, mid_n);
        let treble = norm(treble, treble_n);
        let centroid = if spec_sum > 1e-6 {
            (weighted / spec_sum / (self.sample_rate * 0.5)).clamp(0.0, 1.0)
        } else { 0.5 };
        let rms_n = (rms * 2.0).tanh();

        let dt = n as f32 / self.sample_rate;
        let a_fast = (dt * 12.0).min(1.0);
        let a_slow = (dt * 1.5).min(1.0);
        self.bass_fast += (bass - self.bass_fast) * a_fast;
        self.bass_slow += (bass - self.bass_slow) * a_slow;
        let ratio = self.bass_fast / self.bass_slow.max(0.05);
        let onset = (ratio - 1.25).max(0.0);
        self.punch_env = (self.punch_env * (-dt * 4.0).exp() + onset * 0.4).max(0.0);

        let a = 0.35;
        self.smoothed.bass = self.smoothed.bass * (1.0 - a) + bass * a;
        self.smoothed.mid = self.smoothed.mid * (1.0 - a) + mid * a;
        self.smoothed.treble = self.smoothed.treble * (1.0 - a) + treble * a;
        self.smoothed.centroid = self.smoothed.centroid * (1.0 - a) + centroid * a;
        self.smoothed.rms = self.smoothed.rms * (1.0 - a) + rms_n * a;
        self.smoothed.punch = self.punch_env.min(1.0);
        self.smoothed
    }
}

// ---------- live capture ----------

fn build_live_stream(features: Arc<Mutex<Features>>) -> Result<(cpal::Stream, String)> {
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
            let mut state = LiveProcState::new(channels, sample_rate, fft, features);
            device.build_input_stream(
                &config,
                move |data: &[f32], _| state.feed_f32(data),
                err_fn, None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let mut state = LiveProcState::new(channels, sample_rate, fft, features);
            device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    let buf: Vec<f32> = data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                    state.feed_f32(&buf);
                },
                err_fn, None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let mut state = LiveProcState::new(channels, sample_rate, fft, features);
            device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    let half = u16::MAX as f32 * 0.5;
                    let buf: Vec<f32> = data.iter().map(|s| (*s as f32 - half) / half).collect();
                    state.feed_f32(&buf);
                },
                err_fn, None,
            )?
        }
        fmt => anyhow::bail!("unsupported sample format {:?}", fmt),
    };
    stream.play()?;
    Ok((stream, name))
}

struct LiveProcState {
    channels: usize,
    ring: Vec<f32>,
    extractor: FeatureExtractor,
    out: Arc<Mutex<Features>>,
}

impl LiveProcState {
    fn new(
        channels: usize,
        sample_rate: f32,
        fft: Arc<dyn rustfft::Fft<f32>>,
        out: Arc<Mutex<Features>>,
    ) -> Self {
        Self {
            channels: channels.max(1),
            ring: vec![0.0; FFT_SIZE],
            extractor: FeatureExtractor::new(sample_rate, fft),
            out,
        }
    }

    fn feed_f32(&mut self, data: &[f32]) {
        let frames = data.len() / self.channels;
        if frames == 0 { return; }
        let mut mono = Vec::with_capacity(frames);
        for i in 0..frames {
            let mut s = 0.0;
            for c in 0..self.channels {
                s += data[i * self.channels + c];
            }
            mono.push(s / self.channels as f32);
        }
        let n = mono.len();
        if n >= FFT_SIZE {
            self.ring.copy_from_slice(&mono[n - FFT_SIZE..]);
        } else {
            self.ring.copy_within(n.., 0);
            self.ring[FFT_SIZE - n..].copy_from_slice(&mono);
        }
        let f = self.extractor.process_window(&self.ring);
        if let Ok(mut o) = self.out.lock() {
            *o = f;
        }
    }
}
