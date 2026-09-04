//! PCM audio extraction (`pcm`) and waveform peak-per-bin generation
//! (`waveform`). Ported from `vendor/autocut/src-tauri/src/audio.rs` and
//! `waveform.rs` (see each module's doc comment for what was and wasn't
//! rewritten for this project's i64-microsecond timebase — audit §2).

pub mod pcm;
pub mod waveform;
