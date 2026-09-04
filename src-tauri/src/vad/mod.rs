//! `VadProvider` trait and Silero-VAD implementation (master prompt §13),
//! reimplemented from `vendor/autocut/src-tauri/src/vad.rs`'s two-phase
//! score/segment design (score once, cache; cheap re-segmentation on
//! parameter change — `docs/architecture-audit.md` §2/§3), using the same
//! `voice_activity_detector` crate, rewritten to i64-microsecond timebase.
//!
//! Module layout:
//! - `provider`: the `VadProvider` trait, `SpeechSegment`/`VadParams`/
//!   `ChunkScores`, and the pure `segments_from_scores` post-processing
//!   (hysteresis/threshold/min-silence/min-speech).
//! - `silero`: `SileroVadProvider`, the one concrete implementation shipped
//!   this phase.
//! - `cutlist`: speech segments → proposed `Cut` list (padding, "merge
//!   nearby speech"), feeding the timeline engine.
//! - `cache`: `VadCache`, Tauri-managed state caching the expensive scoring
//!   phase by media id.
//! - `error`: `VadError`.

pub mod cache;
pub mod cutlist;
pub mod error;
pub mod provider;
pub mod silero;

pub use cache::VadCache;
pub use cutlist::{build_cuts_from_speech_segments, CutParams};
pub use error::VadError;
pub use provider::{segments_from_scores, ChunkScores, SpeechSegment, VadParams, VadProvider};
pub use silero::SileroVadProvider;
