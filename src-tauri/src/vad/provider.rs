//! `VadProvider` trait (master prompt §13) and the pure, model-independent
//! segmentation half of the two-phase VAD design ported from
//! `vendor/autocut/src-tauri/src/vad.rs` (`docs/architecture-audit.md` §2/§3),
//! rewritten to this project's i64-microsecond timebase.
//!
//! Two phases, deliberately kept apart (see each function's doc comment for
//! why): `VadProvider::score_chunks` is expensive and model-dependent —
//! score once per file, cache the result. `segments_from_scores` is cheap,
//! pure post-processing of an already-scored, fixed probability array —
//! threshold / min-silence / min-speech / hysteresis are all applied here,
//! so re-segmenting on a slider change never re-runs the model.
//!
//! `VadProvider` itself carries no model-specific types (`ChunkScores` is
//! just `Vec<f32>` + a chunk duration) so a second implementation — WebRTC
//! VAD, autocut's own detector, anything else — can be added later without
//! touching any call site (master prompt §13: "Do NOT tightly couple
//! application logic to one model").

use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::error::VadError;

/// A detected speech region, master prompt §13's `{start, end, confidence}`
/// return shape.
///
/// `confidence` does not exist in autocut's own `SpeechSegment` (its version
/// is `{start, end}` only in f64 seconds) — master prompt §13 explicitly
/// asks for one, so this is a new derived value: the mean of the per-chunk
/// probabilities that fell inside this segment after hysteresis grouping
/// (computed in `segments_from_scores`). Mean rather than min was chosen so
/// a single marginal chunk sitting right at the hysteresis release threshold
/// doesn't tank an otherwise-confident segment's reported score.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct SpeechSegment {
    pub start_us: i64,
    pub end_us: i64,
    pub confidence: f32,
}

/// Segmentation parameters — everything `segments_from_scores` needs, and
/// nothing `score_chunks` needs (see module doc comment). i64-microsecond
/// counterpart of autocut's `VadParams` (which used `u32` milliseconds).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub struct VadParams {
    pub threshold: f32,
    pub min_silence_us: i64,
    pub min_speech_us: i64,
}

impl Default for VadParams {
    /// Same tuning rationale as autocut's default: favor recall (short
    /// silences merged, short utterances kept) so combined with hysteresis
    /// this avoids the most common "cut my word in half" complaint, at the
    /// cost of slightly less aggressive silence removal.
    fn default() -> Self {
        Self {
            threshold: 0.5,
            min_silence_us: 100_000,
            min_speech_us: 150_000,
        }
    }
}

/// The output of the expensive scoring phase: one probability per
/// fixed-duration chunk, plus that fixed duration (so a caller can convert
/// chunk indices back to microseconds without knowing the model's internal
/// chunk size). Model-dependent but `VadParams`-independent — this is what
/// gets cached, keyed by media id, not the segments (module doc comment).
#[derive(Debug, Clone)]
pub struct ChunkScores {
    pub scores: Vec<f32>,
    pub chunk_duration_us: i64,
}

/// Voice activity detection backend. Deliberately minimal and free of any
/// model-specific type so a second implementation can be swapped in without
/// touching call sites (master prompt §13).
pub trait VadProvider: Send + Sync {
    /// Score every fixed-size chunk of `samples` (already resampled to mono
    /// `sample_rate`). Expensive — one model inference per chunk over the
    /// whole file. Does not depend on `VadParams`: threshold / min-silence /
    /// min-speech are all applied afterwards by `segments_from_scores`,
    /// which is what lets a caller score once and re-segment for free on
    /// every slider change. `cancel`, if provided, is polled periodically so
    /// a long analysis can be aborted.
    fn score_chunks(
        &self,
        samples: &[i16],
        sample_rate: u32,
        cancel: Option<&AtomicBool>,
    ) -> Result<ChunkScores, VadError>;

    /// Convenience one-shot entry point: score then segment. Most real
    /// callers should instead call `score_chunks` once (caching the result
    /// by media id) and `segments_from_scores` separately, so adjusting
    /// `VadParams` never re-runs the model.
    fn analyze(
        &self,
        samples: &[i16],
        sample_rate: u32,
        params: VadParams,
    ) -> Result<Vec<SpeechSegment>, VadError> {
        let chunks = self.score_chunks(samples, sample_rate, None)?;
        Ok(segments_from_scores(&chunks, params, 0))
    }
}

/// Turns per-chunk probabilities into speech segments: hysteresis, then
/// group runs, then merge across short silences, then drop short utterances.
/// Pure function of `chunks`/`params` — no model involved, cheap enough to
/// call on every parameter change.
///
/// Speech *starts* at `params.threshold` but *continues* down to
/// `threshold - 0.15` (matching Silero's reference implementation, ported
/// faithfully from `vendor/autocut/src-tauri/src/vad.rs`'s own hysteresis —
/// the fix for a real "cuts mid-word" bug class). Without it, marginal
/// chunks in the middle of an utterance flicker on/off and manufacture
/// silences mid-word.
///
/// `time_offset_us` is added to every timestamp so results stay
/// source-relative when the caller scored a windowed slice of the audio.
pub fn segments_from_scores(
    chunks: &ChunkScores,
    params: VadParams,
    time_offset_us: i64,
) -> Vec<SpeechSegment> {
    let release = (params.threshold - 0.15).max(0.05);
    let mut in_speech = false;
    let mut chunk_is_speech: Vec<bool> = Vec::with_capacity(chunks.scores.len());
    for &prob in &chunks.scores {
        if !in_speech && prob >= params.threshold {
            in_speech = true;
        } else if in_speech && prob < release {
            in_speech = false;
        }
        chunk_is_speech.push(in_speech);
    }

    let raw = group_runs(&chunk_is_speech);
    let merged = merge_close(
        raw,
        us_to_chunks(params.min_silence_us, chunks.chunk_duration_us),
    );
    let filtered = drop_short(
        merged,
        us_to_chunks(params.min_speech_us, chunks.chunk_duration_us),
    );

    filtered
        .into_iter()
        .map(|(s, e)| {
            let slice = &chunks.scores[s..e];
            let confidence = if slice.is_empty() {
                0.0
            } else {
                slice.iter().sum::<f32>() / slice.len() as f32
            };
            SpeechSegment {
                start_us: time_offset_us + s as i64 * chunks.chunk_duration_us,
                end_us: time_offset_us + e as i64 * chunks.chunk_duration_us,
                confidence,
            }
        })
        .collect()
}

fn us_to_chunks(us: i64, chunk_duration_us: i64) -> usize {
    if chunk_duration_us <= 0 || us <= 0 {
        return 0;
    }
    ((us as f64 / chunk_duration_us as f64).ceil()) as usize
}

fn group_runs(flags: &[bool]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &is_speech) in flags.iter().enumerate() {
        match (start, is_speech) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                out.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, flags.len()));
    }
    out
}

fn merge_close(regions: Vec<(usize, usize)>, min_gap: usize) -> Vec<(usize, usize)> {
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(regions.len());
    for (s, e) in regions {
        if let Some(last) = merged.last_mut() {
            if s.saturating_sub(last.1) < min_gap {
                last.1 = e.max(last.1);
                continue;
            }
        }
        merged.push((s, e));
    }
    merged
}

fn drop_short(regions: Vec<(usize, usize)>, min_len: usize) -> Vec<(usize, usize)> {
    regions
        .into_iter()
        .filter(|(s, e)| e.saturating_sub(*s) >= min_len)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32ms chunks (Silero-at-16kHz's real chunk duration), matching
    /// autocut's own test constant so ported scenarios read the same way.
    const CHUNK_US: i64 = 32_000;

    fn chunks(scores: &[f32]) -> ChunkScores {
        ChunkScores {
            scores: scores.to_vec(),
            chunk_duration_us: CHUNK_US,
        }
    }

    fn params(threshold: f32, min_silence_us: i64, min_speech_us: i64) -> VadParams {
        VadParams {
            threshold,
            min_silence_us,
            min_speech_us,
        }
    }

    /// Segment boundaries in chunk-index units, which is what every
    /// assertion below actually cares about — asserting on raw
    /// microseconds would just restate every expectation multiplied by
    /// `CHUNK_US`.
    fn chunk_spans(scores: &[f32], params: VadParams) -> Vec<(i64, i64)> {
        segments_from_scores(&chunks(scores), params, 0)
            .into_iter()
            .map(|s| (s.start_us / CHUNK_US, s.end_us / CHUNK_US))
            .collect()
    }

    #[test]
    fn a_run_of_confident_chunks_becomes_one_segment() {
        let spans = chunk_spans(&[0.0, 0.9, 0.9, 0.9, 0.0], params(0.5, 0, 0));
        assert_eq!(spans, vec![(1, 4)]);
    }

    #[test]
    fn hysteresis_holds_through_a_dip_above_the_release_threshold() {
        // Speech starts at `threshold` but only stops below threshold-0.15.
        // Without that, marginal frames mid-utterance flicker and
        // manufacture silences in the middle of a word.
        let spans = chunk_spans(&[0.9, 0.4, 0.9], params(0.5, 0, 0));
        assert_eq!(spans, vec![(0, 3)]);
    }

    #[test]
    fn a_dip_below_the_release_threshold_ends_the_segment() {
        let spans = chunk_spans(&[0.9, 0.2, 0.9], params(0.5, 0, 0));
        assert_eq!(spans, vec![(0, 1), (2, 3)]);
    }

    #[test]
    fn the_same_scores_resegment_under_a_different_threshold() {
        // The property the whole score/segment split rests on: every knob
        // in VadParams is post-processing over a fixed probability array,
        // so moving a slider never needs the model to run again.
        let scores = [0.9, 0.4, 0.9];
        assert_eq!(chunk_spans(&scores, params(0.5, 0, 0)), vec![(0, 3)]);
        assert_eq!(
            chunk_spans(&scores, params(0.9, 0, 0)),
            vec![(0, 1), (2, 3)]
        );
    }

    #[test]
    fn min_speech_drops_a_burst_that_is_too_short() {
        // 128_000us == 4 chunks; the leading single-chunk burst doesn't survive.
        let spans = chunk_spans(
            &[0.9, 0.0, 0.9, 0.9, 0.9, 0.9, 0.9],
            params(0.5, 0, 4 * CHUNK_US),
        );
        assert_eq!(spans, vec![(2, 7)]);
    }

    #[test]
    fn min_silence_merges_bursts_across_a_short_gap() {
        let spans = chunk_spans(&[0.9, 0.0, 0.9], params(0.5, 4 * CHUNK_US, 0));
        assert_eq!(spans, vec![(0, 3)]);
    }

    #[test]
    fn the_time_offset_shifts_every_timestamp() {
        let segs = segments_from_scores(&chunks(&[0.9]), params(0.5, 0, 0), 10_000_000);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start_us, 10_000_000);
        assert_eq!(segs[0].end_us, 10_000_000 + CHUNK_US);
    }

    #[test]
    fn silence_throughout_produces_no_segments() {
        assert!(chunk_spans(&[0.0, 0.1, 0.0], params(0.5, 0, 0)).is_empty());
    }

    #[test]
    fn confidence_is_the_mean_of_the_segments_own_chunk_scores() {
        let segs = segments_from_scores(&chunks(&[0.6, 1.0, 0.8]), params(0.5, 0, 0), 0);
        assert_eq!(segs.len(), 1);
        assert!(
            (segs[0].confidence - 0.8).abs() < 1e-6,
            "{}",
            segs[0].confidence
        );
    }

    #[test]
    fn group_runs_basic() {
        let flags = vec![false, true, true, false, false, true, false];
        assert_eq!(group_runs(&flags), vec![(1, 3), (5, 6)]);
    }

    #[test]
    fn group_runs_trailing_speech() {
        let flags = vec![false, true, true];
        assert_eq!(group_runs(&flags), vec![(1, 3)]);
    }

    #[test]
    fn merge_close_combines_short_gap() {
        // gap of 1 chunk, min_gap=2 -> merge
        let r = merge_close(vec![(0, 5), (6, 10)], 2);
        assert_eq!(r, vec![(0, 10)]);
    }

    #[test]
    fn merge_close_keeps_long_gap() {
        // gap of 5 chunks, min_gap=2 -> keep separate
        let r = merge_close(vec![(0, 5), (10, 15)], 2);
        assert_eq!(r, vec![(0, 5), (10, 15)]);
    }

    #[test]
    fn drop_short_filters_below_min() {
        let r = drop_short(vec![(0, 3), (10, 20)], 5);
        assert_eq!(r, vec![(10, 20)]);
    }

    #[test]
    fn us_to_chunks_rounds_up() {
        // 32_000us = exactly 1 chunk; 33_000us -> 2 (ceil).
        assert_eq!(us_to_chunks(32_000, CHUNK_US), 1);
        assert_eq!(us_to_chunks(33_000, CHUNK_US), 2);
        assert_eq!(us_to_chunks(0, CHUNK_US), 0);
    }
}
