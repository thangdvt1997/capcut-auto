//! Transcription (master prompt §14/§15/§16). Split into independent
//! sub-areas so two concurrent work-streams don't collide on the same
//! files:
//!
//! - `filler`: filler-word detection (§16) — dictionary-based matching over
//!   `project::types::TranscriptEntry`, producing proposed
//!   `Cut { reason: FillerWord, applied: false }` candidates, the same
//!   "pure, stateless, cheap to re-run on every slider change" shape as
//!   `vad::cutlist::build_cuts_from_speech_segments`. Self-contained: it
//!   only needs the transcript *data shape* (already in the schema), not
//!   any actual Whisper integration.
//! - `provider`: the model-independent `TranscriptionProvider` trait +
//!   `TranscriptSegment` (§14), mirroring `vad::provider::VadProvider`'s
//!   design.
//! - `whisper`: `WhisperProvider`, the one concrete `TranscriptionProvider`
//!   this phase ships (`whisper-rs`, CPU-only this build — see that
//!   module's doc comment for the GPU/CUDA story), plus the pure
//!   token-into-word grouping logic.
//! - `models`: the Model Manager catalog (§60) — the 5 standard ggml model
//!   sizes, real metadata/URLs, installed/available listing, delete.
//! - `download`: resumable model downloads (§60) — `.part` file, `Range`
//!   requests, verify, atomic rename.
//! - `error`: `TranscriptionError` (the inference pipeline) and `ModelError`
//!   (the Model Manager) — two enums, see that module's doc comment for why.

pub mod download;
pub mod error;
pub mod filler;
pub mod models;
pub mod provider;
pub mod whisper;

pub use download::{download_model, part_path, DownloadProgress};
pub use error::{ModelError, TranscriptionError};
pub use models::{
    catalog, catalog_entry, delete_model, is_installed, list_installed, InstalledModel,
    ModelCatalogEntry, ModelId,
};
pub use provider::{TranscriptSegment, TranscriptionProvider};
pub use whisper::WhisperProvider;
