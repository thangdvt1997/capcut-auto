//! `WhisperProvider` — the one concrete `TranscriptionProvider` this phase
//! ships, backed by `whisper-rs` (Rust bindings around whisper.cpp, chosen
//! per master prompt §14's own suggestion list: it's the standard,
//! actively-maintained wrapper, and its build script vendors/compiles
//! whisper.cpp's C++ source directly — no system-installed whisper.cpp
//! needed on a user's machine, only a C/C++ toolchain + cmake at *this
//! app's* build time, same as `rusqlite`'s `bundled` SQLite already does).
//!
//! ## GPU/CUDA: architecturally ready, not compiled or verified here
//!
//! `whisper-rs-sys` exposes a `cuda` Cargo feature (see `Cargo.toml`) that
//! links against a real CUDA toolkit (`nvcc` etc.) at *compile* time. That
//! toolkit is not installed in this WSL2 build environment, and it is not
//! confirmed installed on the target Windows machine either (Phase 6's
//! hardware detection only confirmed an NVIDIA GPU + driver, never the CUDA
//! SDK) — building with `cuda` here would be claiming GPU support that was
//! never actually compiled, let alone run. So: this build enables neither
//! the `cuda` feature nor `WhisperContextParameters::use_gpu(true)`, and
//! `WhisperProvider` is CPU-only, for real, verified in this pass (see this
//! module's tests). "Prefer GPU acceleration when available, fall back to
//! CPU" (master prompt §14) *requires* a correct CPU path to exist
//! regardless of GPU availability — this is that path. A future pass with
//! access to a real CUDA toolkit can flip `--features cuda` on and set
//! `use_gpu(true)` behind the same feature flag (left as a `cfg!` no-op
//! wire-up below, not deleted) and re-verify on real hardware before
//! calling GPU support done.
//!
//! ## Word-level timestamps
//!
//! whisper.cpp tokenizes with a BPE vocabulary, so one whisper "token" is
//! often a sub-word piece, not a whole word — grouping tokens into words is
//! this module's own job, not whisper.cpp's. `group_tokens_into_words`
//! below is the pure, whisper-rs-independent half of that logic (a token
//! whose decoded text starts with whitespace begins a new word; anything
//! else — a subword continuation or attached punctuation — extends the
//! current word), kept separate from `extract_words` (the thin glue that
//! pulls `RawToken`s out of a real `whisper_rs::WhisperSegment`) specifically
//! so it can be unit-tested against realistic fixture data without needing
//! a real model loaded (`whisper_rs::WhisperSegment`/`WhisperToken` borrow
//! from a live `WhisperState` and can't be constructed standalone in a
//! test). This mirrors `vad::provider::segments_from_scores` being pure and
//! separately tested from `vad::silero::SileroVadProvider`'s real-model
//! integration test.
//!
//! Timestamps: whisper.cpp reports segment/token times in centiseconds (one
//! unit = 10ms) — `CENTISECONDS_TO_US` converts to this crate's i64-µs
//! timebase at this module's boundary, per `docs/architecture.md`'s
//! "convert only at adapter boundaries" rule.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::pcm::{to_unit, PCM_SAMPLE_RATE};
use crate::project::Word;

use super::error::TranscriptionError;
use super::provider::{TranscriptSegment, TranscriptionProvider};

/// whisper.cpp segment/token timestamps are centiseconds (10ms ticks).
const CENTISECONDS_TO_US: i64 = 10_000;

pub struct WhisperProvider {
    ctx: WhisperContext,
    n_threads: i32,
}

impl WhisperProvider {
    /// Loads a `.bin` ggml model file from disk. CPU-only in this build —
    /// see module doc comment.
    pub fn load(model_path: &Path) -> Result<Self, TranscriptionError> {
        if !model_path.is_file() {
            return Err(TranscriptionError::ModelLoadFailed {
                path: model_path.display().to_string(),
                details: "file does not exist".to_string(),
            });
        }
        let mut ctx_params = WhisperContextParameters::default();
        // Explicit, not just "left at default": documents that GPU use is a
        // deliberate no-op in this build (module doc comment), not an
        // oversight. `cfg!(feature = "cuda")` is always `false` unless a
        // future build opts in with `--features cuda` on a machine with a
        // real CUDA toolkit.
        ctx_params.use_gpu(cfg!(feature = "cuda"));

        let ctx = WhisperContext::new_with_params(model_path, ctx_params).map_err(|e| {
            TranscriptionError::ModelLoadFailed {
                path: model_path.display().to_string(),
                details: e.to_string(),
            }
        })?;

        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4);
        Ok(Self { ctx, n_threads })
    }
}

impl TranscriptionProvider for WhisperProvider {
    fn transcribe(
        &self,
        samples: &[i16],
        sample_rate: u32,
        language: Option<&str>,
    ) -> Result<Vec<TranscriptSegment>, TranscriptionError> {
        self.transcribe_with_progress(samples, sample_rate, language, None, |_percent| {})
    }
}

impl WhisperProvider {
    /// The real entry point the Tauri command layer uses
    /// (`commands::transcription::transcribe_media`): wires whisper.cpp's
    /// own native progress/abort callbacks (module doc comment) so a
    /// background job gets real percent-complete progress events and can
    /// actually interrupt inference mid-computation — finer-grained than
    /// the "only checked between chunks" cancellation `vad`/`media::proxy`
    /// use, because whisper.cpp's abort callback is checked between
    /// individual ggml compute steps, not just between segments.
    ///
    /// `cancel` is `Arc`, not `&AtomicBool`, because whisper-rs's
    /// `set_abort_callback_safe`/`set_progress_callback_safe` closures are
    /// `'static` (they're handed to the C++ side and outlive this stack
    /// frame until `full()` returns) — a borrowed reference can't satisfy
    /// that, so this method (unlike `VadProvider::score_chunks`'s
    /// `Option<&AtomicBool>`) takes ownership-shareable `Arc` instead.
    pub fn transcribe_with_progress(
        &self,
        samples: &[i16],
        sample_rate: u32,
        language: Option<&str>,
        cancel: Option<Arc<AtomicBool>>,
        on_progress: impl FnMut(i32) + 'static,
    ) -> Result<Vec<TranscriptSegment>, TranscriptionError> {
        if sample_rate != PCM_SAMPLE_RATE {
            return Err(TranscriptionError::UnsupportedSampleRate {
                found: sample_rate,
                expected: PCM_SAMPLE_RATE,
            });
        }
        if samples.is_empty() {
            return Err(TranscriptionError::EmptyAudio);
        }
        if is_cancelled(cancel.as_deref()) {
            return Err(TranscriptionError::Cancelled);
        }

        // Converted a chunk at a time would need whisper.cpp's own
        // streaming API (out of scope here); `full()` takes the whole
        // buffer at once, so the one-shot float conversion cost is
        // unavoidable for this non-streaming path — same tradeoff
        // `vad::silero` avoids per-chunk but this module cannot.
        let floats: Vec<f32> = samples.iter().copied().map(to_unit).collect();

        let mut state =
            self.ctx
                .create_state()
                .map_err(|e| TranscriptionError::InferenceFailed {
                    details: format!("creating whisper state: {e}"),
                })?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.n_threads);
        params.set_translate(false);
        params.set_language(language);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);
        params.set_token_timestamps(true);
        params.set_progress_callback_safe(on_progress);
        if let Some(cancel) = cancel.clone() {
            params.set_abort_callback_safe(move || cancel.load(Ordering::SeqCst));
        }

        let full_result = state.full(params, &floats);
        // Checked regardless of `full_result`'s own outcome: if the abort
        // callback fired, this is a cancellation, not a generic inference
        // failure, even if whisper.cpp's own error surface doesn't
        // distinguish the two cleanly.
        if is_cancelled(cancel.as_deref()) {
            return Err(TranscriptionError::Cancelled);
        }
        full_result.map_err(|e| TranscriptionError::InferenceFailed {
            details: e.to_string(),
        })?;

        let n_segments = state.full_n_segments();
        let mut out = Vec::with_capacity(n_segments.max(0) as usize);
        for i in 0..n_segments {
            let Some(segment) = state.get_segment(i) else {
                continue;
            };
            let Ok(text) = segment.to_str_lossy() else {
                continue;
            };
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let start_us = segment.start_timestamp() * CENTISECONDS_TO_US;
            let end_us = segment.end_timestamp() * CENTISECONDS_TO_US;
            let raw_tokens = collect_raw_tokens(&segment);
            let confidence = mean_probability(&raw_tokens);
            let words = group_tokens_into_words(&raw_tokens);
            out.push(TranscriptSegment {
                text,
                start_us,
                end_us,
                confidence,
                words,
            });
        }
        Ok(out)
    }
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel
        .map(|flag| flag.load(Ordering::SeqCst))
        .unwrap_or(false)
}

/// One whisper.cpp token's decoded text + timing + probability, independent
/// of whisper-rs's borrowed `WhisperToken` type — the pure boundary between
/// real FFI glue (`collect_raw_tokens`) and testable logic
/// (`group_tokens_into_words`).
#[derive(Debug, Clone, PartialEq)]
struct RawToken {
    text: String,
    start_us: i64,
    end_us: i64,
    probability: f32,
}

/// whisper.cpp special/control tokens look like `[_BEG_]`, `[_TT_123]`, or
/// `<|startoftranscript|>`/`<|en|>`/`<|endoftext|>` — never real transcript
/// text, and must be excluded from both word-grouping and the confidence
/// average (a run of confident special tokens would otherwise dilute a
/// segment's reported confidence with numbers that don't describe
/// recognition quality at all).
fn is_special_token(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with("[_") && t.ends_with(']')) || (t.starts_with("<|") && t.ends_with("|>"))
}

fn collect_raw_tokens(segment: &whisper_rs::WhisperSegment<'_>) -> Vec<RawToken> {
    let n = segment.n_tokens();
    let mut out = Vec::with_capacity(n.max(0) as usize);
    for i in 0..n {
        let Some(token) = segment.get_token(i) else {
            continue;
        };
        let Ok(text) = token.to_str_lossy() else {
            continue;
        };
        if is_special_token(&text) {
            continue;
        }
        let data = token.token_data();
        out.push(RawToken {
            text: text.into_owned(),
            start_us: data.t0 * CENTISECONDS_TO_US,
            end_us: data.t1 * CENTISECONDS_TO_US,
            probability: token.token_probability(),
        });
    }
    out
}

fn mean_probability(tokens: &[RawToken]) -> f32 {
    if tokens.is_empty() {
        return 0.0;
    }
    tokens.iter().map(|t| t.probability).sum::<f32>() / tokens.len() as f32
}

/// Groups whisper.cpp's sub-word tokens into whole words (module doc
/// comment: "Word-level timestamps"). A token whose decoded text starts
/// with whitespace begins a new word (whisper.cpp's BPE vocabulary encodes
/// a leading space as part of the token, e.g. `" Hello"`); everything else —
/// a sub-word continuation piece or attached punctuation with no leading
/// space, e.g. `","` after `" world"` — extends the word currently being
/// built. Per-word confidence is the mean of its constituent tokens'
/// probabilities (same "mean, not min" reasoning as
/// `vad::provider::segments_from_scores`'s segment confidence).
fn group_tokens_into_words(tokens: &[RawToken]) -> Vec<Word> {
    struct WordAccum {
        text: String,
        start_us: i64,
        end_us: i64,
        prob_sum: f32,
        prob_count: u32,
    }

    let mut acc: Vec<WordAccum> = Vec::new();
    for token in tokens {
        if token.text.trim().is_empty() {
            continue;
        }
        let starts_new_word = acc.is_empty() || token.text.starts_with(char::is_whitespace);
        let piece = token.text.trim_start();
        if starts_new_word {
            acc.push(WordAccum {
                text: piece.to_string(),
                start_us: token.start_us,
                end_us: token.end_us,
                prob_sum: token.probability,
                prob_count: 1,
            });
        } else if let Some(last) = acc.last_mut() {
            last.text.push_str(piece);
            last.end_us = token.end_us;
            last.prob_sum += token.probability;
            last.prob_count += 1;
        }
    }

    acc.into_iter()
        .map(|w| Word {
            text: w.text,
            start_us: w.start_us,
            end_us: w.end_us,
            confidence: if w.prob_count > 0 {
                w.prob_sum / w.prob_count as f32
            } else {
                0.0
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(text: &str, start_us: i64, end_us: i64, probability: f32) -> RawToken {
        RawToken {
            text: text.to_string(),
            start_us,
            end_us,
            probability,
        }
    }

    // -- is_special_token -------------------------------------------------

    #[test]
    fn recognizes_bracketed_and_angle_bracket_special_tokens() {
        assert!(is_special_token("[_BEG_]"));
        assert!(is_special_token("[_TT_123]"));
        assert!(is_special_token("<|startoftranscript|>"));
        assert!(is_special_token("<|en|>"));
        assert!(is_special_token(" <|endoftext|> "));
        assert!(!is_special_token(" Hello"));
        assert!(!is_special_token(","));
    }

    // -- group_tokens_into_words: realistic whisper.cpp-shaped fixtures --

    #[test]
    fn groups_leading_space_tokens_into_separate_words() {
        // Realistic whisper.cpp BPE output for "Hello world.": the
        // sentence-final punctuation is its own token with NO leading
        // space, so it attaches to the previous word.
        let tokens = vec![
            tok(" Hello", 0, 500_000, 0.95),
            tok(" world", 500_000, 900_000, 0.9),
            tok(".", 900_000, 950_000, 0.99),
        ];
        let words = group_tokens_into_words(&tokens);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[0].start_us, 0);
        assert_eq!(words[0].end_us, 500_000);
        assert_eq!(words[1].text, "world.");
        assert_eq!(words[1].start_us, 500_000);
        assert_eq!(words[1].end_us, 950_000);
    }

    #[test]
    fn reassembles_a_word_split_across_multiple_bpe_subword_tokens() {
        // "Unbelievable" split into BPE pieces, only the first carrying the
        // leading space — a realistic whisper.cpp tokenization shape for a
        // longer/rarer word.
        let tokens = vec![
            tok(" Unbel", 0, 200_000, 0.7),
            tok("iev", 200_000, 300_000, 0.8),
            tok("able", 300_000, 450_000, 0.9),
        ];
        let words = group_tokens_into_words(&tokens);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Unbelievable");
        assert_eq!(words[0].start_us, 0);
        assert_eq!(words[0].end_us, 450_000);
        // Mean of 0.7/0.8/0.9.
        assert!((words[0].confidence - 0.8).abs() < 1e-6);
    }

    #[test]
    fn first_token_with_no_leading_space_still_starts_a_word() {
        let tokens = vec![tok("Hi", 0, 100_000, 1.0)];
        let words = group_tokens_into_words(&tokens);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Hi");
    }

    #[test]
    fn whitespace_only_tokens_are_skipped_entirely() {
        let tokens = vec![
            tok(" Hi", 0, 100_000, 1.0),
            tok(" ", 100_000, 110_000, 0.5),
            tok(" there", 110_000, 250_000, 0.9),
        ];
        let words = group_tokens_into_words(&tokens);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hi");
        assert_eq!(words[1].text, "there");
    }

    #[test]
    fn empty_token_list_produces_no_words() {
        assert!(group_tokens_into_words(&[]).is_empty());
    }

    #[test]
    fn per_word_confidence_is_the_mean_of_its_own_tokens_only() {
        let tokens = vec![
            tok(" Yes", 0, 100_000, 1.0),
            tok(" no", 100_000, 200_000, 0.0),
        ];
        let words = group_tokens_into_words(&tokens);
        assert_eq!(words.len(), 2);
        assert!((words[0].confidence - 1.0).abs() < 1e-6);
        assert!((words[1].confidence - 0.0).abs() < 1e-6);
    }

    // -- mean_probability ---------------------------------------------------

    #[test]
    fn mean_probability_of_empty_tokens_is_zero() {
        assert_eq!(mean_probability(&[]), 0.0);
    }

    #[test]
    fn mean_probability_averages_every_token() {
        let tokens = vec![
            tok(" a", 0, 1, 0.4),
            tok(" b", 1, 2, 0.6),
            tok(" c", 2, 3, 1.0),
        ];
        assert!((mean_probability(&tokens) - (2.0 / 3.0)).abs() < 1e-6);
    }

    // -- real end-to-end transcription -------------------------------------

    /// The single most important verification for this half of Phase 7:
    /// real synthesized speech (via `espeak-ng`, NOT a sine tone — a pure
    /// tone carries no phonetic content and would prove nothing about
    /// recognition, same reasoning the Phase 5 VAD work documented) fed
    /// through the real `WhisperProvider`/whisper.cpp pipeline, asserting
    /// the returned text contains real recognizable words. `#[ignore]`
    /// because it downloads a real ~74MB `ggml-tiny.bin` model on first run
    /// (cached under the OS temp dir afterward) and needs `espeak-ng` on
    /// `PATH` — impractical to run on every `cargo test --lib` invocation /
    /// in an offline CI runner. Run explicitly:
    /// `cargo test --lib -- --ignored transcribes_real_synthesized_speech`.
    #[test]
    #[ignore = "downloads a real whisper model + needs espeak-ng; run explicitly with --ignored"]
    fn transcribes_real_synthesized_speech_into_recognizable_text() {
        use crate::transcription::{catalog_entry, download_model, ModelId};

        // Real tiny model, cached across repeated runs of this ignored test
        // rather than re-downloaded (~74MB) every time.
        let model_dir = std::env::temp_dir().join("ave-whisper-test-models");
        std::fs::create_dir_all(&model_dir).unwrap();
        let entry = catalog_entry(ModelId::Tiny);
        let model_path = model_dir.join(&entry.filename);
        if !model_path.is_file() {
            download_model(&entry, &model_dir, None, |_| {})
                .expect("downloading the real tiny model for this test");
        }

        let dir =
            std::env::temp_dir().join(format!("ave-whisper-speech-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav_path = dir.join("speech.wav");
        let phrase = "The quick brown fox jumps over the lazy dog";
        let status = std::process::Command::new("espeak-ng")
            .args(["-w", wav_path.to_str().unwrap(), phrase])
            .status()
            .expect("running espeak-ng (apt install espeak-ng)");
        assert!(status.success(), "espeak-ng exited with {status}");

        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let samples = crate::audio::pcm::extract_pcm(&ffmpeg, &wav_path)
            .expect("extracting real 16kHz mono PCM from the synthesized speech");

        let provider = WhisperProvider::load(&model_path).expect("loading the real whisper model");
        let segments = provider
            .transcribe(&samples, PCM_SAMPLE_RATE, Some("en"))
            .expect("real transcription succeeds");

        assert!(
            !segments.is_empty(),
            "expected at least one transcribed segment"
        );
        let full_text = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        assert!(
            full_text.contains("fox") || full_text.contains("dog") || full_text.contains("quick"),
            "expected recognizable words from \"{phrase}\" in the transcription, got: {full_text:?}"
        );
        assert!(
            segments.iter().any(|s| !s.words.is_empty()),
            "expected at least one segment with word-level timestamps"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
