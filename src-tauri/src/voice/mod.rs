//! Voice/TTS provider abstraction (`promt.md` §9 "VOICE / TTS
//! CONFIGURATION") — a new subsystem, structurally parallel to `ai::` (this
//! module's own `error`/`provider` doc comments spell out exactly which
//! precedent each piece follows).
//!
//! - [`error`] — `VoiceError`, this subsystem's slice of the standardized
//!   error model, deliberately close to `ai::error::AiProviderError`'s own
//!   shape (that module's doc comment explains why).
//! - [`provider`] — the `VoiceProvider` trait plus its shared value types
//!   (`VoiceSynthesisSettings`/`VoiceSynthesisOutput`/`VoiceInfo`/
//!   `VoiceGender`). No vendor-specific type leaks into the trait signature.
//! - [`custom_api`] — `CustomApiVoiceProvider`, the one real, working
//!   implementation: real HTTP request-building/response-parsing against a
//!   user-hosted server speaking this module's own documented protocol
//!   (that module's doc comment is the protocol spec). Independently unit-
//!   tested against a real local mock HTTP server (`ai::test_http`) — never
//!   a live network call.
//! - [`stub`] — `NtsGenAiProvider`/`GptSoVitsProvider`, honest
//!   `VoiceError::NotImplemented`-returning stand-ins for `promt.md` §9's
//!   other two named providers. Real, selectable enum-adjacent types (a user
//!   can pick them without the app crashing or silently no-op'ing), with no
//!   working backing at all — see that module's doc comment.
//!
//! `commands::voice` is the thin Tauri command surface in front of all of
//! this (`test_voice_connection`/`list_voices` this pass; see that module's
//! own doc comment for exactly why a full `synthesize_speech` command is
//! deliberately deferred).
//!
//! ## Honest status, read literally before trusting anything else here
//!
//! **Real and tested against a local mock HTTP server**: the trait shape,
//! `CustomApiVoiceProvider`'s request-building and response-parsing (both
//! the synthesize and list-voices endpoints), `commands::voice::
//! test_voice_connection`'s reachability check, and `commands::voice::
//! list_voices`. **Not real / never exercised against a live service**:
//! actual text-to-speech synthesis end-to-end — there is no TTS API key, no
//! network-reachable voice-synthesis service, and no live "Custom API" TTS
//! server available in this development/CI environment, so
//! `CustomApiVoiceProvider::synthesize` has never confirmed a byte it writes
//! is playable audio, only that it faithfully decodes and writes back
//! whatever a (mock) server sent. `NtsGenAiProvider`/`GptSoVitsProvider` have
//! no real backing at all — see `voice::stub` module doc comment.

pub mod custom_api;
pub mod error;
pub mod provider;
pub mod stub;

pub use custom_api::CustomApiVoiceProvider;
pub use error::VoiceError;
pub use provider::{
    VoiceGender, VoiceInfo, VoiceProvider, VoiceSynthesisOutput, VoiceSynthesisSettings,
};
pub use stub::{GptSoVitsProvider, NtsGenAiProvider};
