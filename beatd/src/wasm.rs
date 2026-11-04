use crate::Analyzer;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = Analyzer)]
pub struct WasmAnalyzer(Analyzer);

#[wasm_bindgen(js_class = Analyzer)]
impl WasmAnalyzer {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: u32) -> Result<WasmAnalyzer, JsError> {
        if sample_rate < 100 {
            return Err(JsError::new("sample rate must be at least 100 Hz"));
        }
        Ok(Self(Analyzer::new(sample_rate)))
    }

    pub fn push(&mut self, samples: &[f32]) -> bool {
        self.0.push(samples)
    }

    pub fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }

    pub fn level(&self) -> f32 {
        self.0.level()
    }

    pub fn bpm(&self) -> f32 {
        self.0.bpm()
    }

    pub fn spectrum(&self) -> Vec<f32> {
        self.0.spectrum().to_vec()
    }

    pub fn waveform(&self) -> Vec<f32> {
        self.0.waveform().iter().copied().collect()
    }

    pub fn beat_offsets(&self, bpm: f32) -> Vec<f32> {
        if !bpm.is_finite() || bpm <= 0.0 {
            return Vec::new();
        }
        self.0.beat_offsets(bpm)
    }

    pub fn band_level(&self, low: f32, high: f32) -> f32 {
        self.0.band_level(low, high)
    }

    pub fn bpm_range_min(&self) -> f32 {
        self.0.bpm_range_min()
    }

    pub fn set_bpm_range_min(&mut self, min: f32) -> Result<(), JsError> {
        if min != 0.0 && !(25.0..=300.0).contains(&min) {
            return Err(JsError::new("invalid BPM range"));
        }
        self.0.set_bpm_range_min(min);
        Ok(())
    }

    pub fn set_bpm_enabled(&mut self, enabled: bool) {
        self.0.set_bpm_enabled(enabled);
    }

    pub fn set_spectrum_gain(&mut self, gain: f32) {
        self.0.set_spectrum_gain(gain);
    }

    pub fn set_spectrum_compression(&mut self, compression: f32) {
        self.0.set_spectrum_compression(compression);
    }

    pub fn reset(&mut self) {
        self.0.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzes_and_resets_samples() {
        let mut analyzer = WasmAnalyzer::new(48_000).unwrap();
        assert!(analyzer.push(&[0.5; 2048]));
        assert_eq!(analyzer.level(), 0.5);
        assert_eq!(analyzer.spectrum().len(), 1024);
        analyzer.reset();
        assert!(analyzer.spectrum().is_empty());
    }
}
