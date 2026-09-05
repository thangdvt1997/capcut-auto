//! Combines highlight detection's real local signals (`highlights::signals`
//! — speech density, audio energy) and real scene-change boundaries
//! (`media::scene`) with the optional AI-proposed semantic candidates
//! (`highlights::semantic`) into the final `Vec<Highlight>`
//! `commands::highlights::detect_highlights` returns.
//!
//! ## Combination approach (explicit, per this pass's brief)
//!
//! Two paths, chosen by whether an `AIProvider` is configured at all:
//!
//! - **AI configured**: the LLM proposes its own candidate list (start/end/
//!   score/title/reason —
//!   `highlights::semantic::parse_and_validate_candidates`). Each
//!   candidate's `score` is then re-scored by blending it with that same
//!   time range's real local signal score:
//!   `final = clamp(0.7 * llm_score + 0.3 * local_score, 0, 100)`
//!   ([`blend_with_semantic`]) — the LLM supplies the semantic judgment
//!   (title/reason/why this moment matters), while local signals nudge the
//!   final ranking toward moments that are *also* measurably speech-dense
//!   and energetic, rather than the LLM's opinion alone deciding order.
//! - **No AI configured**: highlight candidates are generated directly from
//!   real, local signals only — candidate windows are the spans between
//!   consecutive detected scene changes ([`candidate_windows_from_scene_changes`],
//!   bounded by `[0, total_duration_us]`), each scored purely by
//!   [`local_signal_score`] ([`local_only_highlights`]), with a generic,
//!   honestly-labeled title/reason (no semantic judgment is available
//!   without an LLM — stated plainly rather than faked as if a model had
//!   judged it).
//!
//! Either path keeps `highlights::signals` completely independent of
//! `highlights::semantic` — the crate's own module doc comment requirement
//! that the real, no-AI-needed signals stay independently testable/useful
//! even with no provider configured at all.

use crate::vad::provider::SpeechSegment;

use super::signals::{windowed_rms_energy, windowed_speech_density};
use super::types::Highlight;

/// Real local signals for one candidate time range, each `0.0..=1.0`
/// (`highlights::signals`' own convention).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalSignals {
    pub speech_density: f32,
    pub audio_energy: f32,
}

impl LocalSignals {
    /// Computes both real local signals for `[start_us, end_us)` from
    /// already-scored VAD segments and already-extracted PCM samples — no
    /// I/O here, both inputs are expected to already exist server-side by
    /// the time a candidate is being scored.
    pub fn for_window(
        segments: &[SpeechSegment],
        samples: &[i16],
        sample_rate: u32,
        start_us: i64,
        end_us: i64,
    ) -> Self {
        Self {
            speech_density: windowed_speech_density(segments, start_us, end_us),
            audio_energy: windowed_rms_energy(samples, sample_rate, start_us, end_us),
        }
    }
}

/// `0.0..=100.0` score from local signals alone: speech density and audio
/// energy weighted equally, since neither reliably means "interesting" on
/// its own (a loud but silence-free stretch of filler chatter scores medium
/// on both — rewarding only measurable presence, not manufacturing false
/// confidence about *why* a moment matters, which is exactly the judgment
/// `highlights::semantic` exists to add when a provider is configured).
pub fn local_signal_score(local: LocalSignals) -> f32 {
    ((local.speech_density * 0.5 + local.audio_energy * 0.5) * 100.0).clamp(0.0, 100.0)
}

/// Blends an LLM-proposed candidate's own `score` with the same time
/// range's real local signal score — 70/30 favoring the LLM's semantic
/// judgment (module doc comment's combination approach).
pub fn blend_with_semantic(semantic_score: f32, local: LocalSignals) -> f32 {
    (semantic_score * 0.7 + local_signal_score(local) * 0.3).clamp(0.0, 100.0)
}

/// Re-scores every AI-proposed candidate in place using its own time
/// range's real local signals — the "AI configured" combination path.
/// `title`/`reason` (the LLM's semantic judgment) pass through unchanged;
/// only `score` is touched.
pub fn blend_semantic_candidates(
    candidates: Vec<Highlight>,
    segments: &[SpeechSegment],
    samples: &[i16],
    sample_rate: u32,
) -> Vec<Highlight> {
    candidates
        .into_iter()
        .map(|mut h| {
            let local =
                LocalSignals::for_window(segments, samples, sample_rate, h.start_us, h.end_us);
            h.score = blend_with_semantic(h.score, local);
            h
        })
        .collect()
}

/// Turns a sorted or unsorted list of detected scene-change timestamps
/// (`media::scene::detect_scene_changes`) into candidate `(start_us,
/// end_us)` windows spanning `[0, total_duration_us]` — the "no AI
/// configured" fallback's candidate boundaries (module doc comment).
/// Degenerate (empty/inverted, or out-of-range) windows are dropped.
pub fn candidate_windows_from_scene_changes(
    scene_cuts_us: &[i64],
    total_duration_us: i64,
) -> Vec<(i64, i64)> {
    if total_duration_us <= 0 {
        return Vec::new();
    }
    let mut bounds: Vec<i64> = Vec::with_capacity(scene_cuts_us.len() + 2);
    bounds.push(0);
    for &cut in scene_cuts_us {
        if cut > 0 && cut < total_duration_us {
            bounds.push(cut);
        }
    }
    bounds.push(total_duration_us);
    bounds.sort_unstable();
    bounds.dedup();

    bounds
        .windows(2)
        .filter(|w| w[1] > w[0])
        .map(|w| (w[0], w[1]))
        .collect()
}

/// The "no AI configured" fallback: scores every candidate window purely
/// from real local signals, synthesizes a generic (honestly-labeled, not
/// semantic) title/reason, sorts by score descending, and keeps the top
/// `max_highlights`.
pub fn local_only_highlights(
    windows: &[(i64, i64)],
    segments: &[SpeechSegment],
    samples: &[i16],
    sample_rate: u32,
    max_highlights: usize,
) -> Vec<Highlight> {
    let mut scored: Vec<Highlight> = windows
        .iter()
        .map(|&(start_us, end_us)| {
            let local = LocalSignals::for_window(segments, samples, sample_rate, start_us, end_us);
            let score = local_signal_score(local);
            Highlight {
                id: uuid::Uuid::new_v4().to_string(),
                start_us,
                end_us,
                score,
                title: format!("Highlight at {:.1}s", start_us as f64 / 1_000_000.0),
                reason: describe_local_signals(local),
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(max_highlights);
    scored
}

/// Speech-density/energy thresholds only used to pick which honest,
/// signal-derived sentence to show — not the score itself.
const DENSITY_DESCRIBE_THRESHOLD: f32 = 0.5;
const ENERGY_DESCRIBE_THRESHOLD: f32 = 0.3;

fn describe_local_signals(local: LocalSignals) -> String {
    let dense = local.speech_density >= DENSITY_DESCRIBE_THRESHOLD;
    let loud = local.audio_energy >= ENERGY_DESCRIBE_THRESHOLD;
    match (dense, loud) {
        (true, true) => {
            "High speech density and strong audio energy in this segment (no AI provider configured — signal-based only)."
                .to_string()
        }
        (true, false) => {
            "High speech density in this segment (no AI provider configured — signal-based only)."
                .to_string()
        }
        (false, true) => {
            "Strong audio energy in this segment (no AI provider configured — signal-based only)."
                .to_string()
        }
        (false, false) => {
            "Detected via a scene-change boundary; low measured speech/energy in this segment (no AI provider configured — signal-based only)."
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(speech_density: f32, audio_energy: f32) -> LocalSignals {
        LocalSignals {
            speech_density,
            audio_energy,
        }
    }

    #[test]
    fn local_signal_score_weights_both_signals_equally() {
        assert_eq!(local_signal_score(local(1.0, 1.0)), 100.0);
        assert_eq!(local_signal_score(local(0.0, 0.0)), 0.0);
        assert!((local_signal_score(local(1.0, 0.0)) - 50.0).abs() < 1e-4);
        assert!((local_signal_score(local(0.5, 0.5)) - 50.0).abs() < 1e-4);
    }

    #[test]
    fn blend_with_semantic_favors_the_llm_score_seventy_thirty() {
        // llm_score=100, local=0 -> 70.0; llm_score=0, local=100% -> 30.0.
        assert!((blend_with_semantic(100.0, local(0.0, 0.0)) - 70.0).abs() < 1e-4);
        assert!((blend_with_semantic(0.0, local(1.0, 1.0)) - 30.0).abs() < 1e-4);
        assert!((blend_with_semantic(100.0, local(1.0, 1.0)) - 100.0).abs() < 1e-4);
    }

    #[test]
    fn blend_semantic_candidates_rescoes_using_each_candidates_own_window() {
        use crate::vad::provider::SpeechSegment;

        let segments = vec![SpeechSegment {
            start_us: 0,
            end_us: 2_000_000,
            confidence: 0.9,
        }];
        // Full-scale samples for 2s @ 16kHz -> max audio energy over that span.
        let samples = vec![32_000i16; 32_000];

        let candidates = vec![Highlight {
            id: "c1".to_string(),
            start_us: 0,
            end_us: 2_000_000,
            score: 80.0,
            title: "Great moment".to_string(),
            reason: "the LLM said so".to_string(),
        }];
        let blended = blend_semantic_candidates(candidates, &segments, &samples, 16_000);
        assert_eq!(blended.len(), 1);
        // Full speech coverage + near-max energy -> local_score ~= 100, so
        // blended should land near 0.7*80 + 0.3*100 = 86.
        assert!(
            (blended[0].score - 86.0).abs() < 2.0,
            "{}",
            blended[0].score
        );
        // Title/reason (the LLM's semantic judgment) pass through untouched.
        assert_eq!(blended[0].title, "Great moment");
        assert_eq!(blended[0].reason, "the LLM said so");
    }

    #[test]
    fn candidate_windows_from_scene_changes_splits_on_every_cut_within_range() {
        let windows = candidate_windows_from_scene_changes(&[3_000_000, 7_000_000], 10_000_000);
        assert_eq!(
            windows,
            vec![
                (0, 3_000_000),
                (3_000_000, 7_000_000),
                (7_000_000, 10_000_000)
            ]
        );
    }

    #[test]
    fn candidate_windows_from_scene_changes_ignores_out_of_range_and_duplicate_cuts() {
        let windows = candidate_windows_from_scene_changes(
            &[0, 5_000_000, 5_000_000, 10_000_000, 20_000_000],
            10_000_000,
        );
        assert_eq!(windows, vec![(0, 5_000_000), (5_000_000, 10_000_000)]);
    }

    #[test]
    fn no_scene_cuts_yields_one_window_spanning_the_whole_duration() {
        let windows = candidate_windows_from_scene_changes(&[], 10_000_000);
        assert_eq!(windows, vec![(0, 10_000_000)]);
    }

    #[test]
    fn a_zero_or_negative_duration_yields_no_windows() {
        assert!(candidate_windows_from_scene_changes(&[1], 0).is_empty());
        assert!(candidate_windows_from_scene_changes(&[], -1).is_empty());
    }

    #[test]
    fn local_only_highlights_sorts_by_score_descending_and_truncates() {
        use crate::vad::provider::SpeechSegment;

        // Window A: no speech, silence -> score 0. Window B: full speech,
        // full energy -> score 100.
        let segments = vec![SpeechSegment {
            start_us: 5_000_000,
            end_us: 10_000_000,
            confidence: 0.9,
        }];
        let mut samples = vec![0i16; 5 * 16_000];
        samples.extend(vec![32_000i16; 5 * 16_000]);

        let windows = vec![(0, 5_000_000), (5_000_000, 10_000_000)];
        let highlights = local_only_highlights(&windows, &segments, &samples, 16_000, 1);

        assert_eq!(highlights.len(), 1, "max_highlights=1 truncates to one");
        assert_eq!(highlights[0].start_us, 5_000_000);
        assert!(highlights[0].score > 50.0, "{}", highlights[0].score);
        assert!(highlights[0].reason.contains("no AI provider configured"));
    }
}
