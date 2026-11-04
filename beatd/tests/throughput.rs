use beatd::Analyzer;
use std::{hint::black_box, time::Instant};

#[test]
#[ignore = "manual release throughput measurement"]
fn streaming_throughput() {
    let samples: Vec<_> = (0..48000).map(|i| (i as f32 * 0.057).sin()).collect();
    for bpm in [false, true] {
        let mut analyzer = Analyzer::new(48000);
        analyzer.set_bpm_enabled(bpm);
        let start = Instant::now();
        for _ in 0..120 {
            black_box(analyzer.push(black_box(&samples)));
        }
        eprintln!("BPM {bpm}: {:?} for 120 seconds of audio", start.elapsed());
        assert!(analyzer.level().is_finite());
    }
}
