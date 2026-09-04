//! `VadCache` — Tauri-managed state caching the expensive half of the
//! two-phase VAD design (`super::provider`'s `ChunkScores`), keyed by media
//! id, mirroring how `MediaLibrary`/`TimelineState` are managed elsewhere in
//! this crate (`db::MediaLibrary`, `timeline::session::TimelineState`).
//!
//! Scoring is model-dependent and expensive (an hour of audio is ~112k model
//! invocations); segmentation (`super::provider::segments_from_scores`) is
//! cheap and pure. Caching `ChunkScores` here — not `Vec<SpeechSegment>` —
//! is what lets `commands::vad::segment_scored_media` re-segment instantly
//! on every parameter change without ever touching the model again.

use std::collections::HashMap;
use std::sync::Mutex;

use super::provider::ChunkScores;

/// `None` until a media file has been scored at least once via
/// `commands::vad::score_media_silence`.
#[derive(Default)]
pub struct VadCache(pub Mutex<HashMap<String, ChunkScores>>);

impl VadCache {
    pub fn insert(&self, media_id: impl Into<String>, chunks: ChunkScores) {
        self.0
            .lock()
            .expect("vad cache mutex poisoned")
            .insert(media_id.into(), chunks);
    }

    pub fn get(&self, media_id: &str) -> Option<ChunkScores> {
        self.0
            .lock()
            .expect("vad cache mutex poisoned")
            .get(media_id)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_get_round_trips() {
        let cache = VadCache::default();
        assert!(cache.get("m1").is_none());
        cache.insert(
            "m1",
            ChunkScores {
                scores: vec![0.1, 0.9],
                chunk_duration_us: 32_000,
            },
        );
        let found = cache.get("m1").expect("cached entry");
        assert_eq!(found.scores, vec![0.1, 0.9]);
    }

    #[test]
    fn unknown_media_id_returns_none() {
        let cache = VadCache::default();
        assert!(cache.get("does-not-exist").is_none());
    }
}
