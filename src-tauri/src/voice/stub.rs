//! Honest, structurally-real stand-ins for `NTSGenAI` and `GPT-SoVITS`
//! (`promt.md` §9's own three-provider list — "NTSGenAI / GPT-SoVITS /
//! Custom API"; `voice::custom_api` implements the third). Neither of these
//! two names corresponds to a vendor SDK or documented HTTP API this
//! codebase has ever integrated with — there is nothing to build a real
//! adapter against yet, so this module deliberately does not pretend one
//! exists. Both providers below are real, selectable `VoiceProvider`
//! implementations (a user can pick "NTSGenAI" or "GPT-SoVITS" in Voice
//! Settings and the app will not crash or silently no-op) whose every method
//! always returns `VoiceError::NotImplemented` with a clear, honest reason —
//! exactly the "architecture, not a fake implementation" posture
//! `render::audio_filters::NoiseReductionProvider`'s own module doc comment
//! documents for a trait with only one real implementation so far, applied
//! here to a trait with *zero* real implementations for these two variants.
//!
//! If a real integration for either provider is ever built, it replaces the
//! matching stub here (or a new module, if it grows its own request/
//! response machinery like `voice::custom_api` did) — nothing else in this
//! codebase needs to change, since callers only ever see `Box<dyn
//! VoiceProvider>` (`voice::provider` module doc comment: no vendor-specific
//! type leaks into the trait signature).

use std::path::Path;

use super::error::VoiceError;
use super::provider::{VoiceInfo, VoiceProvider, VoiceSynthesisOutput, VoiceSynthesisSettings};

fn not_implemented(provider: &'static str) -> VoiceError {
    VoiceError::NotImplemented {
        provider: provider.to_string(),
        reason: format!(
            "{provider} integration has not been built in this codebase yet; choose Custom API instead."
        ),
    }
}

/// Stand-in for `promt.md` §9's "NTSGenAI" provider entry. No NTSGenAI SDK
/// or documented HTTP API is integrated anywhere in this codebase (module
/// doc comment) — every call is a real, immediate `NotImplemented` error.
pub struct NtsGenAiProvider;

impl VoiceProvider for NtsGenAiProvider {
    fn name(&self) -> &'static str {
        "NTSGenAI"
    }

    fn synthesize(
        &self,
        _text: &str,
        _voice_id: &str,
        _settings: &VoiceSynthesisSettings,
        _output_path: &Path,
    ) -> Result<VoiceSynthesisOutput, VoiceError> {
        Err(not_implemented(self.name()))
    }

    fn list_voices(&self) -> Result<Vec<VoiceInfo>, VoiceError> {
        Err(not_implemented(self.name()))
    }
}

/// Stand-in for `promt.md` §9's "GPT-SoVITS" provider entry. Same posture as
/// [`NtsGenAiProvider`] above — no real integration exists yet, every call
/// is a real, immediate `NotImplemented` error.
pub struct GptSoVitsProvider;

impl VoiceProvider for GptSoVitsProvider {
    fn name(&self) -> &'static str {
        "GPT-SoVITS"
    }

    fn synthesize(
        &self,
        _text: &str,
        _voice_id: &str,
        _settings: &VoiceSynthesisSettings,
        _output_path: &Path,
    ) -> Result<VoiceSynthesisOutput, VoiceError> {
        Err(not_implemented(self.name()))
    }

    fn list_voices(&self) -> Result<Vec<VoiceInfo>, VoiceError> {
        Err(not_implemented(self.name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> VoiceSynthesisSettings {
        VoiceSynthesisSettings::default()
    }

    #[test]
    fn nts_gen_ai_synthesize_is_honestly_not_implemented() {
        let p = NtsGenAiProvider;
        let path = std::path::PathBuf::from("unused.wav");
        let err = p.synthesize("hi", "v1", &settings(), &path).unwrap_err();
        match err {
            VoiceError::NotImplemented { provider, reason } => {
                assert_eq!(provider, "NTSGenAI");
                assert!(!reason.is_empty());
            }
            other => panic!("expected NotImplemented, got {other:?}"),
        }
    }

    #[test]
    fn nts_gen_ai_list_voices_is_honestly_not_implemented() {
        let p = NtsGenAiProvider;
        assert!(matches!(
            p.list_voices(),
            Err(VoiceError::NotImplemented { .. })
        ));
    }

    #[test]
    fn gpt_so_vits_synthesize_is_honestly_not_implemented() {
        let p = GptSoVitsProvider;
        let path = std::path::PathBuf::from("unused.wav");
        let err = p.synthesize("hi", "v1", &settings(), &path).unwrap_err();
        match err {
            VoiceError::NotImplemented { provider, reason } => {
                assert_eq!(provider, "GPT-SoVITS");
                assert!(!reason.is_empty());
            }
            other => panic!("expected NotImplemented, got {other:?}"),
        }
    }

    #[test]
    fn gpt_so_vits_list_voices_is_honestly_not_implemented() {
        let p = GptSoVitsProvider;
        assert!(matches!(
            p.list_voices(),
            Err(VoiceError::NotImplemented { .. })
        ));
    }

    #[test]
    fn stub_providers_never_touch_the_filesystem() {
        // A path under a directory that does not exist: if either stub ever
        // tried to actually write a file before returning its error, this
        // would fail with an IO error instead of the expected
        // `NotImplemented` — proving the error really is returned
        // immediately, with no attempted synthesis work at all.
        let missing_dir_path = std::path::PathBuf::from("/definitely/does/not/exist/output.wav");
        assert!(matches!(
            NtsGenAiProvider.synthesize("hi", "v1", &settings(), &missing_dir_path),
            Err(VoiceError::NotImplemented { .. })
        ));
        assert!(matches!(
            GptSoVitsProvider.synthesize("hi", "v1", &settings(), &missing_dir_path),
            Err(VoiceError::NotImplemented { .. })
        ));
    }
}
