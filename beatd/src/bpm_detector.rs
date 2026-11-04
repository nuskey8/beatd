use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::{collections::VecDeque, sync::Arc};
const N: usize = 2048;
const HOP: usize = 256;

// Spectral-flux history follows Sound2Light's 2048/256 analysis geometry.
// Periodicity is scored over the history rather than adjacent onset intervals.
pub struct BpmDetector {
    samples: VecDeque<f32>,
    previous: Vec<f32>,
    flux: VecDeque<f32>,
    fft: Arc<dyn Fft<f32>>,
    frames: usize,
    candidates: VecDeque<f32>,
    pub min_bpm: f32,
}

impl Default for BpmDetector {
    fn default() -> Self {
        Self {
            samples: VecDeque::with_capacity(N),
            previous: vec![0.0; N / 2],
            flux: VecDeque::new(),
            fft: FftPlanner::new().plan_fft_forward(N),
            frames: 0,
            candidates: VecDeque::new(),
            min_bpm: 75.0,
        }
    }
}
impl BpmDetector {
    pub fn beat_offsets(&self, bpm: f32, rate: f32) -> Vec<f32> {
        if !bpm.is_finite() || bpm <= 0.0 || self.flux.len() < 4 {
            return Vec::new();
        }
        let period = 60.0 * rate / (bpm * HOP as f32);
        let flux: Vec<_> = self.flux.iter().copied().collect();
        let phases = (period.round() as usize).min(flux.len());
        if !period.is_finite() || phases == 0 {
            return Vec::new();
        }
        let phase = (0..phases)
            .max_by(|a, b| {
                let score = |phase: usize| -> f32 {
                    let mut sum = 0.0;
                    let mut pos = phase as f32;
                    while (pos as usize) < flux.len() {
                        let i = pos.round() as usize;
                        if i >= flux.len() {
                            break;
                        }
                        sum += flux[i];
                        pos += period;
                    }
                    sum
                };
                score(*a).total_cmp(&score(*b))
            })
            .unwrap_or(0);
        let mut result = Vec::new();
        let mut pos = phase as f32;
        while pos < flux.len() as f32 {
            result.push(((flux.len() as f32 - 1.0 - pos) * HOP as f32 + (N / 2) as f32) / rate);
            pos += period;
        }
        result
    }

    pub fn push(&mut self, sample: f32, rate: f32) -> Option<f32> {
        self.samples
            .push_back(if sample.is_finite() { sample } else { 0.0 });
        if self.samples.len() < N {
            return None;
        }
        let mut buffer: Vec<_> = self
            .samples
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Complex::new(
                    s * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (N - 1) as f32).cos()),
                    0.0,
                )
            })
            .collect();
        self.fft.process(&mut buffer);
        let mut flux = 0.0;
        for (bin, previous) in buffer.iter().zip(&mut self.previous) {
            let magnitude = bin.norm();
            flux += (magnitude - *previous).max(0.0);
            *previous = magnitude;
        }
        self.samples.drain(..HOP);
        self.flux.push_back(flux);
        let capacity = (rate * 5.0 / HOP as f32) as usize;
        if self.flux.len() > capacity {
            self.flux.pop_front();
        }
        self.frames += 1;
        if self.flux.len() < capacity || self.frames % 32 != 0 {
            return None;
        }
        let mean = self.flux.iter().sum::<f32>() / capacity as f32;
        if mean < 0.001 {
            self.candidates.clear();
            return Some(0.0);
        }
        let values: Vec<_> = self.flux.iter().map(|v| (v - mean).max(0.0)).collect();
        let (min_bpm, max_bpm) = if self.min_bpm > 0.0 {
            (self.min_bpm, self.min_bpm * 2.0)
        } else {
            (30.0, 300.0)
        };
        let min_lag = (60.0 * rate / (max_bpm * HOP as f32)).ceil() as usize;
        let max_lag = (60.0 * rate / (min_bpm * HOP as f32)).floor() as usize;
        let correlation = |lag: usize| -> f32 {
            if lag >= values.len() {
                return 0.0;
            }
            let (mut dot, mut a, mut b) = (0.0, 0.0, 0.0);
            for i in lag..values.len() {
                dot += values[i] * values[i - lag];
                a += values[i] * values[i];
                b += values[i - lag] * values[i - lag];
            }
            dot / (a * b).sqrt().max(1e-12)
        };
        let score_at = |lag: usize| correlation(lag) + 0.25 * correlation(lag * 2);
        let (lag, score) = (min_lag..=max_lag)
            .map(|lag| (lag, score_at(lag)))
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        if score < 0.2 {
            self.candidates.clear();
            return Some(0.0);
        }
        // Prefer a shorter submultiple when it scores nearly as well,
        // so wide ranges do not settle on a slow multiple of the beat.
        let mut lag = lag;
        for divisor in 2..=4 {
            let center = lag as f32 / divisor as f32;
            let lo = (center - 2.0).round() as usize;
            let hi = (center + 2.0).round() as usize;
            if hi < min_lag {
                break;
            }
            if let Some(candidate) = (lo.max(min_lag)..=hi.min(max_lag))
                .max_by(|a, b| score_at(*a).total_cmp(&score_at(*b)))
            {
                if score_at(candidate) >= score * 0.85 {
                    lag = candidate;
                    break;
                }
            }
        }
        let (left, center, right) = (correlation(lag - 1), correlation(lag), correlation(lag + 1));
        let denominator = left - 2.0 * center + right;
        let offset = if denominator.abs() > 1e-6 {
            (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        let bpm = 60.0 * rate / ((lag as f32 + offset) * HOP as f32);
        self.candidates.push_back(bpm);
        if self.candidates.len() > 16 {
            self.candidates.pop_front();
        }
        if self.candidates.len() < 4 {
            return None;
        }
        let mut sorted: Vec<_> = self.candidates.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let median = sorted[sorted.len() / 2];
        let cluster: Vec<_> = sorted
            .into_iter()
            .filter(|v| (v - median).abs() < 2.0)
            .collect();
        if cluster.len() * 4 < self.candidates.len() * 3 {
            return None;
        }
        Some(cluster.iter().sum::<f32>() / cluster.len() as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracks_beats_over_continuous_tone_and_subdivisions() {
        for rate in [44100, 48000] {
            for bpm in [90.0, 120.0, 145.0] {
                let mut detector = BpmDetector::default();
                let mut result = 0.0;
                for i in 0..rate * 10 {
                    let t = i as f32 / rate as f32;
                    let beat = (t % (60.0 / bpm)) / (60.0 / bpm);
                    let eighth = (t % (30.0 / bpm)) / (30.0 / bpm);
                    let sample = 0.12 * (t * 440.0 * std::f32::consts::TAU).sin()
                        + 0.6 * (-beat * 50.0).exp() * (t * 80.0 * std::f32::consts::TAU).sin()
                        + 0.025
                            * (-eighth * 80.0).exp()
                            * (t * 7000.0 * std::f32::consts::TAU).sin();
                    if let Some(value) = detector.push(sample, rate as f32) {
                        result = value;
                    }
                }
                assert!(
                    (result - bpm).abs() < 1.0,
                    "{rate}: expected {bpm}, got {result}"
                );
            }
        }
    }
    #[test]
    fn beat_offsets_handles_extreme_bpm() {
        let mut detector = BpmDetector::default();
        for _ in 0..4096 {
            detector.push(0.5, 48_000.0);
        }
        for bpm in [f32::NAN, f32::INFINITY, f32::MIN_POSITIVE, f32::MAX, 0.0] {
            assert!(detector.beat_offsets(bpm, 48_000.0).len() <= detector.flux.len());
        }
    }
    #[test]
    fn silence_has_no_bpm() {
        let mut detector = BpmDetector::default();
        for _ in 0..44100 * 6 {
            assert!(detector.push(0.0, 44100.0).unwrap_or(0.0) == 0.0);
        }
    }
    #[test]
    fn auto_range_covers_wide_bpm() {
        let rate = 44100;
        for bpm in [60.0, 200.0] {
            let mut detector = BpmDetector::default();
            detector.min_bpm = 0.0;
            let mut result = 0.0;
            for i in 0..rate * 10 {
                let t = i as f32 / rate as f32;
                let beat = (t % (60.0 / bpm)) / (60.0 / bpm);
                let sample = 0.6 * (-beat * 50.0).exp() * (t * 80.0 * std::f32::consts::TAU).sin();
                if let Some(value) = detector.push(sample, rate as f32) {
                    result = value;
                }
            }
            assert!((result - bpm).abs() < 1.0, "expected {bpm}, got {result}");
        }
    }
}
