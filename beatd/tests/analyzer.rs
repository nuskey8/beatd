use beatd::{Analyzer, FFT_SIZE};

#[test]
fn streaming_chunks_produce_same_results_and_reset() {
    let rate = 48000;
    let audio: Vec<f32> = (0..FFT_SIZE * 3)
        .map(|i| (std::f32::consts::TAU * 1000.0 * i as f32 / rate as f32).sin() * 0.5)
        .collect();
    let mut whole = Analyzer::new(rate);
    let mut chunks = Analyzer::new(rate);
    assert!(whole.push(&audio));
    for chunk in audio.chunks(137) {
        chunks.push(chunk);
    }
    assert_eq!(whole.spectrum(), chunks.spectrum());
    assert!((whole.level() - 0.5 / 2.0_f32.sqrt()).abs() < 0.01);
    assert!(whole.band_level(900.0, 1100.0) > whole.band_level(2000.0, 3000.0) * 10.0);
    assert!(!whole.waveform().is_empty());
    whole.reset();
    assert!(whole.spectrum().is_empty());
    assert!(whole.waveform().is_empty());
    assert_eq!(whole.level(), 0.0);
}

#[test]
fn spectrum_gain_and_compression_lift_quiet_bins() {
    let rate = 48000;
    let audio: Vec<f32> = (0..FFT_SIZE)
        .map(|i| (std::f32::consts::TAU * 1000.0 * i as f32 / rate as f32).sin() * 0.01)
        .collect();
    let mut analyzer = Analyzer::new(rate);
    analyzer.push(&audio);
    let base = analyzer.band_level(900.0, 1100.0);
    analyzer.set_spectrum_gain(2.0);
    analyzer.push(&audio);
    let gained = analyzer.band_level(900.0, 1100.0);
    analyzer.set_spectrum_compression(3.0);
    analyzer.push(&audio);
    let compressed = analyzer.band_level(900.0, 1100.0);
    assert!(gained > base * 1.5);
    assert!(compressed > gained);
}

#[test]
fn fft_input_reuse_does_not_keep_previous_frame() {
    let mut analyzer = Analyzer::new(48000);
    analyzer.set_bpm_enabled(false);
    analyzer.push(&vec![0.5; FFT_SIZE]);
    analyzer.push(&vec![0.0; FFT_SIZE - 1]);
    assert!(!analyzer.spectrum().iter().all(|&v| v == 0.0));
    assert!(analyzer.push(&[f32::NAN]));
    assert_eq!(analyzer.level(), 0.0);
    assert!(analyzer.spectrum().iter().all(|&v| v == 0.0));
}
