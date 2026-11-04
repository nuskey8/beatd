use crate::bpm_detector::BpmDetector;
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::{
    collections::VecDeque,
    sync::{Arc, OnceLock},
};

/// Number of samples in the spectrum analysis window.
pub const FFT_SIZE: usize = 2048;

pub(crate) fn hann_window() -> &'static [f32; FFT_SIZE] {
    static WINDOW: OnceLock<[f32; FFT_SIZE]> = OnceLock::new();
    WINDOW.get_or_init(|| {
        std::array::from_fn(|i| {
            0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos()
        })
    })
}

pub(crate) fn fft_plan() -> Arc<dyn Fft<f32>> {
    static FFT: OnceLock<Arc<dyn Fft<f32>>> = OnceLock::new();
    Arc::clone(FFT.get_or_init(|| FftPlanner::new().plan_fft_forward(FFT_SIZE)))
}

/// Streaming analysis of mono audio sampled at a fixed rate.
pub struct Analyzer {
    rate: u32,
    sample_count: usize,
    waveform: VecDeque<f32>,
    wave_peak: f32,
    wave_count: usize,
    fft: Arc<dyn Fft<f32>>,
    spectrum: Vec<f32>,
    fft_buffer: Vec<Complex<f32>>,
    fft_scratch: Vec<Complex<f32>>,
    level: f32,
    bpm_detector: BpmDetector,
    bpm: f32,
    bpm_enabled: bool,
    spectrum_gain: f32,
    spectrum_compression: f32,
}

impl Analyzer {
    /// Creates new analyzer. Sample rate must be at least 100 Hz.
    pub fn new(sample_rate: u32) -> Self {
        assert!(sample_rate >= 100, "sample rate must be at least 100 Hz");
        let fft = fft_plan();
        let fft_scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        Self {
            rate: sample_rate,
            sample_count: 0,
            waveform: VecDeque::new(),
            wave_peak: 0.0,
            wave_count: 0,
            fft,
            fft_scratch,
            fft_buffer: vec![Complex::default(); FFT_SIZE],
            spectrum: Vec::new(),
            level: 0.0,
            bpm_detector: BpmDetector::default(),
            bpm: 0.0,
            bpm_enabled: true,
            spectrum_gain: 1.0,
            spectrum_compression: 1.0,
        }
    }

    /// Adds mono samples. Returns true when a new spectrum is available.
    pub fn push(&mut self, samples: &[f32]) -> bool {
        let window = hann_window();
        let mut updated = false;
        for &sample in samples {
            let sample = if sample.is_finite() { sample } else { 0.0 };
            if self.bpm_enabled {
                if let Some(bpm) = self.bpm_detector.push(sample, self.rate as f32) {
                    self.bpm = bpm;
                }
            }
            self.wave_peak = self.wave_peak.max(sample.abs());
            self.wave_count += 1;
            if self.wave_count >= (self.rate / 50) as usize {
                self.waveform.push_back(self.wave_peak);
                if self.waveform.len() > 250 {
                    self.waveform.pop_front();
                }
                self.wave_peak = 0.0;
                self.wave_count = 0;
            }
            self.fft_buffer[self.sample_count] = Complex::new(sample, 0.0);
            self.sample_count += 1;
            if self.sample_count == FFT_SIZE {
                self.level = (self.fft_buffer.iter().map(|v| v.re * v.re).sum::<f32>()
                    / FFT_SIZE as f32)
                    .sqrt();
                for (value, window) in self.fft_buffer.iter_mut().zip(window) {
                    value.re *= window;
                }
                self.fft
                    .process_with_scratch(&mut self.fft_buffer, &mut self.fft_scratch);
                let gain = self.spectrum_gain;
                let exponent = 1.0 / self.spectrum_compression.max(0.01);
                self.spectrum.clear();
                self.spectrum
                    .extend(self.fft_buffer[..FFT_SIZE / 2].iter().map(|c| {
                        (c.norm() * 4.0 / FFT_SIZE as f32 * gain)
                            .clamp(0.0, 1.0)
                            .powf(exponent)
                    }));
                self.sample_count = 0;
                updated = true;
            }
        }
        updated
    }

    /// Gets the sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Gets the RMS level of the latest analysis window.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Gets the linear magnitudes of bins from 0 Hz to just below Nyquist.
    pub fn spectrum(&self) -> &[f32] {
        &self.spectrum
    }

    /// Gets the peak magnitudes in approximately 20 ms windows, oldest first (up to five seconds).
    pub fn waveform(&self) -> &VecDeque<f32> {
        &self.waveform
    }

    /// Gets the estimated BPM, or zero while unavailable.
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Gets the estimated beat ages in seconds; `bpm` can be a manually tapped BPM.
    pub fn beat_offsets(&self, bpm: f32) -> Vec<f32> {
        self.bpm_detector.beat_offsets(bpm, self.rate as f32)
    }

    /// Gets the root-sum-square magnitude of FFT bins in the half-open frequency interval.
    pub fn band_level(&self, low: f32, high: f32) -> f32 {
        self.spectrum
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                let hz = *i as f32 * self.rate as f32 / FFT_SIZE as f32;
                hz >= low && hz < high
            })
            .map(|(_, v)| v * v)
            .sum::<f32>()
            .sqrt()
    }

    /// Gets the minimum BPM range.
    pub fn bpm_range_min(&self) -> f32 {
        self.bpm_detector.min_bpm
    }

    /// Sets the BPM range to [min, 2 * min), or 30–300 BPM when `min` is zero.
    pub fn set_bpm_range_min(&mut self, min: f32) {
        assert!(
            min == 0.0 || (25.0..=300.0).contains(&min),
            "invalid BPM range"
        );
        self.bpm_detector.min_bpm = min;
    }

    /// Sets whether BPM detection is enabled.
    pub fn set_bpm_enabled(&mut self, enabled: bool) {
        if self.bpm_enabled != enabled {
            self.bpm_enabled = enabled;
            let min = self.bpm_range_min();
            self.bpm_detector = BpmDetector::default();
            self.bpm_detector.min_bpm = min;
            self.bpm = 0.0;
        }
    }

    /// Sets the spectrum gain multiplier.
    pub fn set_spectrum_gain(&mut self, gain: f32) {
        self.spectrum_gain = gain.clamp(0.01, 100.0);
    }

    /// Sets the spectral compression amount: 1 is linear, higher values lift quiet bins.
    pub fn set_spectrum_compression(&mut self, compression: f32) {
        self.spectrum_compression = compression.clamp(0.01, 10.0);
    }

    /// Clears all history while retaining configuration.
    pub fn reset(&mut self) {
        let range = self.bpm_range_min();
        let enabled = self.bpm_enabled;
        let gain = self.spectrum_gain;
        let compression = self.spectrum_compression;
        *self = Self::new(self.rate);
        self.set_bpm_range_min(range);
        self.bpm_enabled = enabled;
        self.spectrum_gain = gain;
        self.spectrum_compression = compression;
    }
}
