//! C interface. See `beatd.h` for ownership and buffer contracts.
use crate::Analyzer;

#[unsafe(no_mangle)]
pub extern "C" fn beatd_new(rate: u32) -> *mut Analyzer {
    if rate < 100 {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(Analyzer::new(rate)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_free(a: *mut Analyzer) {
    if !a.is_null() {
        unsafe {
            drop(Box::from_raw(a));
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_push(a: *mut Analyzer, samples: *const f32, len: usize) -> bool {
    if a.is_null()
        || (samples.is_null() && len != 0)
        || len > isize::MAX as usize / size_of::<f32>()
    {
        return false;
    }
    let samples = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(samples, len) }
    };
    unsafe { (*a).push(samples) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_level(a: *const Analyzer) -> f32 {
    unsafe { a.as_ref() }.map_or(0.0, Analyzer::level)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_bpm(a: *const Analyzer) -> f32 {
    unsafe { a.as_ref() }.map_or(0.0, Analyzer::bpm)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_spectrum(
    a: *const Analyzer,
    out: *mut f32,
    capacity: usize,
) -> usize {
    let Some(a) = (unsafe { a.as_ref() }) else {
        return 0;
    };
    let spectrum = a.spectrum();
    let count = capacity.min(spectrum.len());
    if !out.is_null() && count != 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(spectrum.as_ptr(), out, count);
        }
    }
    spectrum.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_reset(a: *mut Analyzer) {
    if let Some(a) = unsafe { a.as_mut() } {
        a.reset();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_sample_rate(a: *const Analyzer) -> u32 {
    unsafe { a.as_ref() }.map_or(0, Analyzer::sample_rate)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_band_level(a: *const Analyzer, low: f32, high: f32) -> f32 {
    unsafe { a.as_ref() }.map_or(0.0, |a| a.band_level(low, high))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_bpm_range_min(a: *const Analyzer) -> f32 {
    unsafe { a.as_ref() }.map_or(0.0, Analyzer::bpm_range_min)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_set_bpm_range_min(a: *mut Analyzer, min: f32) -> bool {
    let Some(a) = (unsafe { a.as_mut() }) else {
        return false;
    };
    if min != 0.0 && !(25.0..=300.0).contains(&min) {
        return false;
    }
    a.set_bpm_range_min(min);
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_set_bpm_enabled(a: *mut Analyzer, enabled: bool) {
    if let Some(a) = unsafe { a.as_mut() } {
        a.set_bpm_enabled(enabled);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_set_spectrum_gain(a: *mut Analyzer, gain: f32) {
    if let Some(a) = unsafe { a.as_mut() } {
        a.set_spectrum_gain(gain);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_set_spectrum_compression(a: *mut Analyzer, compression: f32) {
    if let Some(a) = unsafe { a.as_mut() } {
        a.set_spectrum_compression(compression);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_waveform(
    a: *const Analyzer,
    out: *mut f32,
    capacity: usize,
) -> usize {
    let Some(a) = (unsafe { a.as_ref() }) else {
        return 0;
    };
    let waveform = a.waveform();
    if !out.is_null() {
        for (i, &value) in waveform.iter().take(capacity).enumerate() {
            unsafe {
                out.add(i).write(value);
            }
        }
    }
    waveform.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn beatd_beat_offsets(
    a: *const Analyzer,
    bpm: f32,
    out: *mut f32,
    capacity: usize,
) -> usize {
    let Some(a) = (unsafe { a.as_ref() }) else {
        return 0;
    };
    let offsets = a.beat_offsets(bpm);
    let count = offsets.len().min(capacity);
    if !out.is_null() && count != 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(offsets.as_ptr(), out, count);
        }
    }
    offsets.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_and_output_buffers_match_analyzer() {
        unsafe {
            let a = beatd_new(48_000);
            assert_eq!(beatd_sample_rate(a), 48_000);
            assert!(!beatd_set_bpm_range_min(a, f32::NAN));
            assert!(beatd_set_bpm_range_min(a, 90.0));
            assert_eq!(beatd_bpm_range_min(a), 90.0);
            beatd_set_spectrum_gain(a, 2.0);
            beatd_set_spectrum_compression(a, 2.0);
            assert!(beatd_push(a, [0.5; 4096].as_ptr(), 4096));
            assert!(beatd_band_level(a, 0.0, 100.0) > 0.0);
            let waveform = (*a).waveform();
            let mut short = [0.0; 1];
            assert_eq!(beatd_waveform(a, short.as_mut_ptr(), 1), waveform.len());
            assert_eq!(short[0], waveform[0]);
            let expected = (*a).beat_offsets(120.0);
            let mut offsets = vec![0.0; expected.len()];
            assert!(!expected.is_empty());
            assert_eq!(
                beatd_beat_offsets(a, 120.0, offsets.as_mut_ptr(), offsets.len()),
                expected.len()
            );
            assert_eq!(offsets, expected);
            assert_eq!(beatd_beat_offsets(a, f32::NAN, std::ptr::null_mut(), 0), 0);
            beatd_set_bpm_enabled(a, false);
            assert_eq!(beatd_bpm(a), 0.0);
            beatd_free(a);
        }
    }
}
