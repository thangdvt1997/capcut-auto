//! `TranscriptionProvider` trait (master prompt §14) — model-independent,
//! same design shape as `vad::provider::VadProvider`: no whisper-specific
//! type leaks into the trait signature, so a second backend (faster-whisper
//! via a sidecar process, a cloud fallback, etc.) could implement this same
//! trait without touching any call site.
//!
//! ## `TranscriptSegment` vs. `project::TranscriptEntry`
//!
//! `TranscriptSegment` is `TranscriptEntry` minus `id`/`media_id`/
//! `is_filler` — the three fields that only make sense once a transcript
//! entry is attached to a specific project's specific media item. A
//! provider has no notion of "project" or "media library id"; it only ever
//! sees a raw sample buffer and hands back text+timing+confidence+words.
//! The command layer (`commands::transcription::transcribe_media`) is what
//! assigns a fresh `id`/`media_id` and defaults `is_filler: false` when
//! converting a `Vec<TranscriptSegment>` into `Vec<TranscriptEntry>` ready
//! to merge into `ProjectV1::transcript`. Kept as a distinct type rather
//! than reusing `TranscriptEntry` directly with dummy id/media_id values —
//! a provider fabricating an `id` that looks like a real stable entity id
//! would be misleading (nothing about it is stable across re-transcription
//! runs).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::project::Word;

use super::error::TranscriptionError;

/// One transcribed segment (master prompt §14's `{text, start, end,
/// confidence}` schema, extended with `words` per that same section's
/// "Prefer word-level timestamps").
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscriptSegment {
    pub text: String,
    pub start_us: i64,
    pub end_us: i64,
    pub confidence: f32,
    pub words: Vec<Word>,
}

/// Local transcription backend. Deliberately minimal — the exact shape
/// master prompt §14 itself suggests (`text`/`start`/`end`/`confidence`,
/// here extended with `words`) — and, like `vad::provider::VadProvider`,
/// carries no model-specific type so it stays swappable (master prompt §14
/// lists Whisper/whisper.cpp/faster-whisper as options to *evaluate*, not a
/// single mandated implementation).
///
/// No cancellation/progress-callback parameter here on purpose: whisper.cpp
/// exposes a real native abort/progress *callback* pair (see
/// `WhisperProvider::transcribe_with_progress`), but a hypothetical second
/// provider (a REST-API-backed one, say) might only ever expose
/// cancellation as "stop polling", a completely different shape. Forcing
/// one cross-provider progress abstraction into this trait before a second
/// implementation exists to prove out its real shape would be premature —
/// the same "don't tightly couple to one model" principle master prompt
/// §13/§14 asks for cuts both ways. `WhisperProvider`'s extra capability is
/// a concrete, whisper-specific inherent method instead, and it is what the
/// real Tauri command layer (`commands::transcription::transcribe_media`)
/// actually calls for real background-job progress events + cancellation.
pub trait TranscriptionProvider: Send + Sync {
    /// Transcribe `samples` (already extracted to mono PCM at
    /// `sample_rate` — see `audio::pcm::extract_pcm`). `language`, if
    /// `Some`, forces recognition in that language (an ISO 639-1 code, e.g.
    /// `"en"`); `None` requests auto-detection.
    fn transcribe(
        &self,
        samples: &[i16],
        sample_rate: u32,
        language: Option<&str>,
    ) -> Result<Vec<TranscriptSegment>, TranscriptionError>;
}
