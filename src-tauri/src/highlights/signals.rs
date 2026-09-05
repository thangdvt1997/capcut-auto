//! Real, local, no-AI-needed highlight signals (Phase 10 follow-up, master
//! prompt §21): speech density and audio energy over a caller-given time
//! window. Both are pure functions of already-computed data this codebase
//! already extracts elsewhere — `vad::provider::SpeechSegment`s (real Silero
//! VAD scores, `vad::provider` module doc comment) and raw PCM samples
//! (`audio::pcm::extract_pcm`) — so this module stays independently useful
//! and testable with zero AI provider configured at all (this pass's
//! brief), and is never the thing that talks to an `AIProvider`
//! (`highlights::semantic` is the only place that does).
//!
//! Both signals are normalized to `0.0..=1.0`, matching
//! `vad::provider::SpeechSegment::confidence`'s own convention (not
//! `Highlight::score`'s `0.0..=100.0` — `highlights::combine` is where the
//! two scales meet).

use crate::audio::pcm::to_unit;
use crate::vad::provider::SpeechSegment;

/// Fraction of `[window_start_us, window_end_us)` covered by real detected
/// speech (`segments`, from `vad::provider::segments_from_scores`) —
/// `0.0..=1.0`. A window entirely inside one long speech segment scores
/// `1.0`; a window with no overlapping speech at all scores `0.0`.
pub fn windowed_speech_density(
    segments: &[SpeechSegment],
    window_start_us: i64,
    window_end_us: i64,
) -> f32 {
    let window_len = (window_end_us - window_start_us).max(1) as f64;
    let mut speech_us: i64 = 0;
    for seg in segments {
        let overlap_start = seg.start_us.max(window_start_us);
        let overlap_end = seg.end_us.min(window_end_us);
        if overlap_end > overlap_start {
            speech_us += overlap_end - overlap_start;
        }
    }
    ((speech_us as f64 / window_len) as f32).clamp(0.0, 1.0)
}

/// Root-mean-square amplitude of `samples` (already-extracted PCM,
/// `audio::pcm::extract_pcm`, at `sample_rate`) over
/// `[window_start_us, window_end_us)`, normalized to `0.0..=1.0` via
/// `audio::pcm::to_unit`'s own `[-1.0, 1.0)` scaling — real audio energy,
/// not a peak (`audio::waveform::peaks_per_bin`'s own metric), since a
/// sustained loud passage should score higher than one single clipped
/// sample surrounded by silence.
pub fn windowed_rms_energy(
    samples: &[i16],
    sample_rate: u32,
    window_start_us: i64,
    window_end_us: i64,
) -> f32 {
    if sample_rate == 0 || window_end_us <= window_start_us || samples.is_empty() {
        return 0.0;
    }
    let start_idx = us_to_sample_index(window_start_us, sample_rate).min(samples.len());
    let end_idx = us_to_sample_index(window_end_us, sample_rate).min(samples.len());
    if start_idx >= end_idx {
        return 0.0;
    }
    let slice = &samples[start_idx..end_idx];
    let sum_sq: f64 = slice
        .iter()
        .map(|&s| {
            let f = to_unit(s) as f64;
            f * f
        })
        .sum();
    let mean_sq = sum_sq / slice.len() as f64;
    (mean_sq.sqrt() as f32).clamp(0.0, 1.0)
}

fn us_to_sample_index(us: i64, sample_rate: u32) -> usize {
    if us <= 0 {
        return 0;
    }
    ((us as i128 * sample_rate as i128) / 1_000_000).max(0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(start_us: i64, end_us: i64) -> SpeechSegment {
        SpeechSegment {
            start_us,
            end_us,
            confidence: 0.9,
        }
    }

    #[test]
    fn a_window_fully_covered_by_one_speech_segment_scores_one() {
        let segments = vec![segment(0, 10_000_000)];
        assert_eq!(
            windowed_speech_density(&segments, 2_000_000, 5_000_000),
            1.0
        );
    }

    #[test]
    fn a_window_with_no_overlapping_speech_scores_zero() {
        let segments = vec![segment(0, 1_000_000)];
        assert_eq!(
            windowed_speech_density(&segments, 5_000_000, 6_000_000),
            0.0
        );
    }

    #[test]
    fn a_window_half_covered_by_speech_scores_one_half() {
        // Window is [0, 4s); speech only covers [0, 2s).
        let segments = vec![segment(0, 2_000_000)];
        let density = windowed_speech_density(&segments, 0, 4_000_000);
        assert!((density - 0.5).abs() < 1e-6, "{density}");
    }

    #[test]
    fn multiple_segments_in_one_window_sum_their_overlap() {
        // Window [0, 10s); two 2s speech segments inside it -> 4s / 10s = 0.4.
        let segments = vec![segment(0, 2_000_000), segment(5_000_000, 7_000_000)];
        let density = windowed_speech_density(&segments, 0, 10_000_000);
        assert!((density - 0.4).abs() < 1e-6, "{density}");
    }

    #[test]
    fn silence_produces_zero_rms_energy() {
        let samples = vec![0i16; 16_000];
        let energy = windowed_rms_energy(&samples, 16_000, 0, 1_000_000);
        assert_eq!(energy, 0.0);
    }

    #[test]
    fn a_constant_amplitude_signal_reports_its_own_amplitude_as_rms() {
        // RMS of a constant-value signal equals the value itself, so this is
        // exact (no waveform/tone approximation needed): 16384 / 32768 = 0.5.
        let samples = vec![16_384i16; 16_000];
        let energy = windowed_rms_energy(&samples, 16_000, 0, 1_000_000);
        assert!((energy - 0.5).abs() < 1e-4, "{energy}");
    }

    #[test]
    fn only_the_windowed_slice_of_samples_contributes() {
        // First half loud, second half silent; a window over only the
        // second half must report ~0 energy even though the full buffer is
        // loud.
        let mut samples = vec![32_000i16; 16_000];
        samples.extend(vec![0i16; 16_000]);
        // Samples are at 16kHz: the second 1s spans [1_000_000, 2_000_000)us.
        let energy = windowed_rms_energy(&samples, 16_000, 1_000_000, 2_000_000);
        assert!(energy < 0.01, "{energy}");
    }

    #[test]
    fn an_inverted_or_empty_window_reports_zero_without_panicking() {
        let samples = vec![32_000i16; 100];
        assert_eq!(windowed_rms_energy(&samples, 16_000, 500_000, 500_000), 0.0);
        assert_eq!(windowed_rms_energy(&samples, 16_000, 900_000, 500_000), 0.0);
        assert_eq!(windowed_rms_energy(&[], 16_000, 0, 1_000_000), 0.0);
    }

    #[test]
    fn a_zero_sample_rate_reports_zero_without_dividing_by_zero() {
        let samples = vec![32_000i16; 100];
        assert_eq!(windowed_rms_energy(&samples, 0, 0, 1_000_000), 0.0);
    }
}
