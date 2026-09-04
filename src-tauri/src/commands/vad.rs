//! Silence Detector Tauri command surface (master prompt §12/§13): score →
//! segment → cutlist, kept as three separate thin commands so the frontend
//! can re-segment/re-build-cuts on every parameter slider change without
//! ever re-running the VAD model (the whole point of the two-phase design —
//! see `crate::vad::provider` module doc comment). Thin per master prompt
//! §66 — all real logic lives in `crate::vad`.

use std::path::Path;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

use crate::audio::pcm::{self, PCM_SAMPLE_RATE};
use crate::commands::media::resolve_ffmpeg;
use crate::error::AppErrorPayload;
use crate::media::error::MediaError;
use crate::project::Cut;
use crate::vad::{self, ChunkScores, CutParams, SpeechSegment, VadCache, VadParams, VadProvider};

/// Result of `score_media_silence` — deliberately does not carry the raw
/// `Vec<f32>` scores back over IPC (potentially ~100k+ floats for a long
/// file); they stay server-side in `VadCache`, keyed by `media_id`, and
/// `segment_media_silence` reads them back by id.
#[derive(Debug, Clone, Serialize, Type)]
pub struct VadScoreSummary {
    pub media_id: String,
    pub chunk_count: usize,
    pub chunk_duration_us: i64,
    pub sample_count: usize,
}

/// **Analyze** (master prompt §12): extracts 16kHz mono PCM from
/// `media_path` and scores it with `SileroVadProvider`, caching the result
/// in `VadCache` under `media_id`. This is the expensive, model-dependent
/// half of the two-phase design — call it once per media file; every
/// subsequent parameter tweak should call `segment_media_silence` instead,
/// not this command again.
#[tauri::command]
#[specta::specta]
pub fn score_media_silence(
    app: AppHandle,
    cache: State<'_, VadCache>,
    media_id: String,
    media_path: String,
) -> Result<VadScoreSummary, AppErrorPayload> {
    let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let samples = pcm::extract_pcm(&ffmpeg, Path::new(&media_path))
        .map_err(|e: MediaError| AppErrorPayload::from(&e))?;

    let provider = vad::SileroVadProvider;
    let chunks: ChunkScores = provider
        .score_chunks(&samples, PCM_SAMPLE_RATE, None)
        .map_err(|e| AppErrorPayload::from(&e))?;

    let summary = VadScoreSummary {
        media_id: media_id.clone(),
        chunk_count: chunks.scores.len(),
        chunk_duration_us: chunks.chunk_duration_us,
        sample_count: samples.len(),
    };
    cache.insert(media_id, chunks);
    Ok(summary)
}

/// **Preview Cuts' VAD half**: re-segments the *already-cached* chunk scores
/// for `media_id` under new `params` — cheap, pure post-processing, never
/// touches the model. Errors with `VAD_NOT_SCORED` if `score_media_silence`
/// hasn't been called for this `media_id` yet.
#[tauri::command]
#[specta::specta]
pub fn segment_media_silence(
    cache: State<'_, VadCache>,
    media_id: String,
    params: VadParams,
) -> Result<Vec<SpeechSegment>, AppErrorPayload> {
    let chunks = cache
        .get(&media_id)
        .ok_or_else(|| vad::VadError::NotScored {
            media_id: media_id.clone(),
        })
        .map_err(|e| AppErrorPayload::from(&e))?;
    Ok(vad::segments_from_scores(&chunks, params, 0))
}

/// **Preview Cuts' cutlist half**: builds the proposed (`applied: false`)
/// `Remove` cuts from `segments` per `cut_params` — pure, stateless, no
/// caching needed (cheap enough to call on every padding/merge-gap slider
/// change).
#[tauri::command]
#[specta::specta]
pub fn build_silence_cutlist(
    source_media_id: String,
    media_duration_us: i64,
    segments: Vec<SpeechSegment>,
    cut_params: CutParams,
) -> Vec<Cut> {
    vad::build_cuts_from_speech_segments(&segments, &source_media_id, media_duration_us, cut_params)
}
