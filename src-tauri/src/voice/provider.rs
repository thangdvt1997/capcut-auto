//! `VoiceProvider` trait (`promt.md` §9 "VOICE / TTS CONFIGURATION") — a new
//! subsystem, deliberately built to the same shape as this codebase's other
//! swappable `*Provider` abstractions: `ai::provider::AIProvider` (a
//! vendor-agnostic external-service trait — HTTP-based, synchronous, no
//! provider-specific type in the signature), `transcription::provider::
//! TranscriptionProvider`/`vad::provider::VadProvider` (a trait + one real
//! implementation + room for more), and `render::audio_filters::
//! NoiseReductionProvider` (a trait "architecture", not a perfectly-tuned
//! implementation — exactly the same posture this module takes for
//! `NtsGenAi`/`GptSoVits`, see `voice::stub` module doc comment).
//!
//! ## What is real here, and what is not — read this before trusting
//! anything else in this module
//!
//! - **Real**: the trait shape itself, `CustomApiVoiceProvider`'s request-
//!   building/response-handling code (`voice::custom_api`), and
//!   `commands::voice::test_voice_connection`'s HTTP reachability check.
//!   All independently unit-tested against a real local mock HTTP server
//!   (`ai::test_http`) — never a live network call.
//! - **NOT real / never exercised against a live service**: actual
//!   text-to-speech synthesis. There is no TTS API key, no network-reachable
//!   voice-synthesis service, and no live "Custom API" TTS server available
//!   in this development/CI environment. `CustomApiVoiceProvider::synthesize`
//!   is structurally correct — it builds the documented request and would
//!   write whatever bytes a real server sent back to `output_path` — but
//!   nothing in this codebase has ever confirmed a byte it writes is
//!   playable audio. `NtsGenAiProvider`/`GptSoVitsProvider` do not exist at
//!   all — see `voice::stub`.
//!
//! ## No async runtime (matching `ai::provider`)
//!
//! Same rationale as `ai::provider::AIProvider`'s own doc comment: this
//! crate has no async runtime dependency for HTTP anywhere else (model
//! downloads, AI provider calls all run synchronous `ureq` calls, typically
//! on a background thread via `tauri::async_runtime::spawn_blocking`), so
//! `VoiceProvider` is a plain synchronous trait, not `async_trait`.
//!
//! ## Where synthesized audio goes
//!
//! `synthesize` takes an explicit `output_path: &Path` rather than deciding
//! a location itself — the same "the trait doesn't know about `AppHandle`"
//! discipline every other provider trait in this crate already follows
//! (`VadProvider`/`TranscriptionProvider` don't touch the filesystem at all;
//! `capcut::export::export_draft` takes an explicit `draft_output_dir: &Path`
//! for the same reason). The intended real caller (a future
//! `synthesize_speech` command — not built this pass, see `commands::voice`
//! module doc comment for exactly why) would resolve `output_path` the same
//! way every other generated/managed artifact directory in this crate is
//! resolved: `app_local_data_dir().join("voice_output")` (the exact pattern
//! `commands::media`'s `media_cache_dir`/`commands::assets::assets_dir`
//! already use for their own generated files), joined with a fresh filename
//! per synthesis call.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::error::VoiceError;

/// Playback/generation parameters (`promt.md` §9: Speed/Pitch/Volume/
/// Emotion/Language) — everything a `synthesize` call needs beyond the text
/// itself and which voice to use.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct VoiceSynthesisSettings {
    /// 1.0 = normal speed.
    pub speed: f32,
    /// 1.0 = normal pitch.
    pub pitch: f32,
    /// 1.0 = normal (unattenuated) volume.
    pub volume: f32,
    /// Provider-defined emotion label (e.g. `"neutral"`, `"happy"`,
    /// `"sad"`) — no fixed enum here since `promt.md` §9 names no closed
    /// set and different providers support different vocabularies.
    pub emotion: Option<String>,
    /// ISO 639-1 language code (e.g. `"en"`, `"vi"`). `None` lets the
    /// provider use its own default/auto-detected language.
    pub language: Option<String>,
}

impl Default for VoiceSynthesisSettings {
    fn default() -> Self {
        Self {
            speed: 1.0,
            pitch: 1.0,
            volume: 1.0,
            emotion: None,
            language: None,
        }
    }
}

/// The result of a successful `synthesize` call.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct VoiceSynthesisOutput {
    /// Absolute path to the written audio file — always exactly the
    /// `output_path` the caller passed in (module doc comment); repeated
    /// back here so a caller that only holds the `VoiceSynthesisOutput`
    /// (not the original call site) still has it.
    pub audio_path: String,
    /// Wall-clock duration of the synthesized audio, if the provider's
    /// response reports one. `None` when a provider's response carries no
    /// duration field at all (this codebase never estimates one from file
    /// size/bitrate — see `voice::custom_api` module doc comment for why).
    pub duration_us: Option<i64>,
}

/// `promt.md` §9's Male/Female/Narrator voice-role concept, used by
/// [`VoiceInfo::gender`] (for "Refresh Voices" listing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VoiceGender {
    Male,
    Female,
    /// Neither strictly male nor female — a provider's own "narrator" voice
    /// preset, or one it does not categorize at all.
    Other,
}

/// One selectable voice a provider can list ("Refresh Voices", `promt.md`
/// §9).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct VoiceInfo {
    pub voice_id: String,
    pub name: String,
    pub gender: Option<VoiceGender>,
    pub language: Option<String>,
    /// A provider-hosted audio sample URL, if any — lets a future "Test
    /// Voice" (`promt.md` §9) preview without a real synthesis call. Not
    /// currently consumed anywhere (no command surface calls `list_voices`
    /// and plays a preview yet); present so a provider that has one doesn't
    /// need to drop it.
    pub preview_url: Option<String>,
}

/// TTS/voice-synthesis backend. Deliberately minimal and free of any
/// provider-specific type (module doc comment) — the same "no vendor type
/// leaks into the trait signature" discipline as `AIProvider`/
/// `TranscriptionProvider`/`VadProvider`.
pub trait VoiceProvider: Send + Sync {
    /// Human-readable provider name, used only in error messages/
    /// diagnostics — never parsed by callers (same contract as
    /// `AIProvider::name`).
    fn name(&self) -> &'static str;

    /// Synthesizes `text` as `voice_id`, writing the resulting audio to
    /// `output_path` (module doc comment — the caller decides where; this
    /// trait never invents a location). Real, synchronous HTTP for
    /// `CustomApiVoiceProvider` (module doc comment); a caller running this
    /// from a Tauri command should do so on a background thread via
    /// `tauri::async_runtime::spawn_blocking`, the same pattern every other
    /// long-running call in this crate uses.
    fn synthesize(
        &self,
        text: &str,
        voice_id: &str,
        settings: &VoiceSynthesisSettings,
        output_path: &Path,
    ) -> Result<VoiceSynthesisOutput, VoiceError>;

    /// Lists every voice this provider currently offers ("Refresh Voices",
    /// `promt.md` §9).
    fn list_voices(&self) -> Result<Vec<VoiceInfo>, VoiceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_synthesis_settings_are_unity_speed_pitch_and_volume() {
        let settings = VoiceSynthesisSettings::default();
        assert_eq!(settings.speed, 1.0);
        assert_eq!(settings.pitch, 1.0);
        assert_eq!(settings.volume, 1.0);
        assert!(settings.emotion.is_none());
        assert!(settings.language.is_none());
    }
}
