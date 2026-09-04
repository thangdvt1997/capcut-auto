//! `SileroVadProvider` — the one concrete `VadProvider` this phase ships,
//! using the same `voice_activity_detector` crate autocut depends on
//! (`vendor/autocut/src-tauri/src/vad.rs`), ported to this project's
//! `VadProvider`/`ChunkScores` shapes (`super::provider`).

use std::sync::atomic::{AtomicBool, Ordering};

use voice_activity_detector::VoiceActivityDetector;

use crate::audio::pcm::to_unit;

use super::error::VadError;
use super::provider::{ChunkScores, VadProvider};

/// Silero V5's fixed input window. Not configurable — the ONNX graph this
/// crate embeds was exported for exactly this chunk size (matches autocut's
/// own `CHUNK_SIZE` constant).
const CHUNK_SIZE: usize = 512;

/// Silero-VAD-via-ONNX, run locally (master prompt §13's "strong local VAD
/// implementation").
pub struct SileroVadProvider;

impl VadProvider for SileroVadProvider {
    fn score_chunks(
        &self,
        samples: &[i16],
        sample_rate: u32,
        cancel: Option<&AtomicBool>,
    ) -> Result<ChunkScores, VadError> {
        let mut vad = VoiceActivityDetector::builder()
            .sample_rate(sample_rate as i64)
            .chunk_size(CHUNK_SIZE)
            .build()
            .map_err(|e| VadError::ModelInitFailed {
                details: e.to_string(),
            })?;

        let mut scores: Vec<f32> = Vec::with_capacity(samples.len() / CHUNK_SIZE + 1);
        for (i, chunk) in samples.chunks(CHUNK_SIZE).enumerate() {
            if i % 64 == 0 && is_cancelled(cancel) {
                return Err(VadError::Cancelled);
            }
            // Converted a chunk at a time, straight into the model's
            // iterator, so no float copy of the whole track ever exists.
            scores.push(vad.predict(chunk.iter().copied().map(to_unit)));
        }

        let chunk_duration_us = (CHUNK_SIZE as i64 * 1_000_000) / sample_rate.max(1) as i64;
        Ok(ChunkScores {
            scores,
            chunk_duration_us,
        })
    }
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel
        .map(|flag| flag.load(Ordering::SeqCst))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::pcm::PCM_SAMPLE_RATE;

    #[test]
    fn scoring_silence_produces_one_score_per_chunk_and_low_probabilities() {
        // A real ONNX model run (not mocked): a buffer of exact silence
        // should score consistently low across every chunk. This is the
        // integration point the pure segmentation-logic tests in
        // `super::provider` deliberately don't cover.
        let samples = vec![0i16; CHUNK_SIZE * 5];
        let provider = SileroVadProvider;
        let chunks = provider
            .score_chunks(&samples, PCM_SAMPLE_RATE, None)
            .expect("silero model should initialize and score");
        assert_eq!(chunks.scores.len(), 5);
        assert_eq!(chunks.chunk_duration_us, 32_000);
        for &score in &chunks.scores {
            assert!(
                score < 0.5,
                "expected low speech probability for silence, got {score}"
            );
        }
    }

    #[test]
    fn cancellation_stops_scoring_early() {
        let samples = vec![0i16; CHUNK_SIZE * 200];
        let cancelled = AtomicBool::new(true);
        let provider = SileroVadProvider;
        let err = provider
            .score_chunks(&samples, PCM_SAMPLE_RATE, Some(&cancelled))
            .unwrap_err();
        assert!(matches!(err, VadError::Cancelled));
    }
}
