mod analyzer;
mod bpm_detector;

#[cfg(feature = "ffi")]
mod ffi;

#[cfg(feature = "wasm")]
mod wasm;

pub use analyzer::{Analyzer, FFT_SIZE};
