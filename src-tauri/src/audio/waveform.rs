//! Downsample decoded PCM into the amplitude envelope the timeline draws.
//!
//! Ported from `vendor/autocut/src-tauri/src/waveform.rs` (reuse permitted,
//! `docs/upstream.md`); the peak-per-bin algorithm itself has no timebase to
//! rewrite (it only ever produced bin amplitudes, not timestamps — the
//! caller derives bin↔time externally). What this project adds beyond
//! autocut's version is `WaveformResult`, which now — since the project has
//! a canonical i64-microsecond timebase to be exact about — carries
//! `bin_duration_us` alongside the peaks, so a caller can map bin index to a
//! timeline position without recomputing `duration_us / bins` itself and
//! risking an off-by-one against the value this module actually used.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use specta::Type;

use crate::media::error::MediaError;

#[derive(Debug, Clone, Serialize, Type)]
pub struct WaveformResult {
    /// Peak `|sample|` per bin, normalized to `[0, 1]`.
    pub peaks: Vec<f32>,
    /// Microseconds spanned by one bin (`source_duration_us / peaks.len()`,
    /// rounded). `0` if `peaks` is empty.
    pub bin_duration_us: i64,
}

pub fn waveform_from_samples(
    samples: &[i16],
    target_bins: usize,
    sample_rate: u32,
    cancel: Option<&AtomicBool>,
) -> Result<WaveformResult, MediaError> {
    let peaks = peaks_per_bin(samples, target_bins, cancel)?;
    let bin_duration_us = if peaks.is_empty() || sample_rate == 0 {
        0
    } else {
        let total_us = (samples.len() as i64 * 1_000_000) / sample_rate as i64;
        total_us / peaks.len() as i64
    };
    Ok(WaveformResult {
        peaks,
        bin_duration_us,
    })
}

fn peaks_per_bin(
    samples: &[i16],
    target_bins: usize,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<f32>, MediaError> {
    if samples.is_empty() || target_bins == 0 {
        return Ok(Vec::new());
    }
    let bins = target_bins.min(samples.len());
    let bin_size = (samples.len() as f64 / bins as f64).max(1.0);
    let mut out = Vec::with_capacity(bins);
    for i in 0..bins {
        if i % 512 == 0 && is_cancelled(cancel) {
            return Err(MediaError::WaveformFailed {
                path: String::new(),
                details: "waveform extraction cancelled".into(),
            });
        }
        let start = (i as f64 * bin_size) as usize;
        let end = (((i + 1) as f64 * bin_size) as usize).min(samples.len());
        if end <= start {
            out.push(0.0);
            continue;
        }
        // unsigned_abs, not abs: i16::MIN has no positive counterpart, so
        // `-32768i16.abs()` overflows.
        let peak = samples[start..end]
            .iter()
            .map(|s| s.unsigned_abs())
            .max()
            .unwrap_or(0);
        out.push(peak as f32 / 32768.0);
    }
    Ok(out)
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel
        .map(|flag| flag.load(Ordering::SeqCst))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_samples_produce_an_empty_waveform() {
        let result = waveform_from_samples(&[], 100, 16_000, None).unwrap();
        assert!(result.peaks.is_empty());
        assert_eq!(result.bin_duration_us, 0);
    }

    #[test]
    fn peak_per_bin_finds_the_loudest_sample_in_each_bin() {
        let samples = vec![0, 100, -200, 50, 0, 5000, -1, 2];
        let result = waveform_from_samples(&samples, 2, 16_000, None).unwrap();
        assert_eq!(result.peaks.len(), 2);
        assert!((result.peaks[0] - 200.0 / 32768.0).abs() < 1e-6);
        assert!((result.peaks[1] - 5000.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn i16_min_does_not_panic_via_unsigned_abs() {
        let samples = vec![i16::MIN, 0, 0, 0];
        let result = waveform_from_samples(&samples, 1, 16_000, None).unwrap();
        assert!((result.peaks[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn bin_duration_reflects_the_real_sample_rate() {
        // 32000 samples @ 16kHz = 2,000,000us total, split into 10 bins.
        let samples = vec![0i16; 32_000];
        let result = waveform_from_samples(&samples, 10, 16_000, None).unwrap();
        assert_eq!(result.peaks.len(), 10);
        assert_eq!(result.bin_duration_us, 200_000);
    }

    #[test]
    fn caps_bins_at_the_sample_count_for_very_short_clips() {
        let samples = vec![1i16, 2, 3];
        let result = waveform_from_samples(&samples, 1000, 16_000, None).unwrap();
        assert_eq!(result.peaks.len(), 3);
    }
}
