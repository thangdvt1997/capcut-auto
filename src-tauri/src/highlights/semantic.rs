//! The one highlight-detection signal that genuinely needs an AI call
//! (master prompt §21's "semantic importance"): asking the configured
//! `AIProvider` to propose its own highlight candidates — time range, score,
//! title, and reason — directly from the transcript. Kept entirely separate
//! from `highlights::signals` (the real, local, no-AI-needed half) so that
//! module stays independently testable and useful even with no provider
//! configured at all (this pass's brief) — nothing in `signals` imports this
//! module or `ai::provider`, and nothing here computes speech
//! density/audio energy itself.
//!
//! `parse_and_validate_candidates` mirrors `ai::edit_plan::parse_and_validate`'s
//! own "never partially succeed; reject the whole response on any malformed
//! entry" discipline, producing the exact same `Highlight` schema
//! (`highlights::types`) the rest of this feature uses — not a second,
//! parallel ad hoc shape that `highlights::combine` would then have to
//! reconcile.

use serde::Deserialize;

use crate::ai::provider::AiRequest;
use crate::project::TranscriptEntry;

use super::error::HighlightError;
use super::types::Highlight;

/// A constructed two-part prompt, ready to become a real [`AiRequest`] once
/// a caller supplies the provider-call knobs (`temperature`/`timeout_ms`/
/// `max_tokens`) — same split as `ai::nl_command::EditPlanPrompt`.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightPrompt {
    pub system_prompt: String,
    pub user_prompt: String,
}

impl HighlightPrompt {
    pub fn into_request(
        self,
        temperature: f32,
        timeout_ms: u64,
        max_tokens: Option<u32>,
    ) -> AiRequest {
        AiRequest {
            system_prompt: Some(self.system_prompt),
            user_prompt: self.user_prompt,
            temperature,
            timeout_ms,
            max_tokens,
        }
    }
}

/// The exact candidate JSON schema, spelled out for the model rather than
/// hoping it infers the shape (same discipline as
/// `ai::nl_command::EDIT_PLAN_SCHEMA_INSTRUCTIONS`).
const CANDIDATE_SCHEMA_INSTRUCTIONS: &str = r#"Respond with ONLY a single JSON array (no markdown code fences, no commentary before or after) of highlight candidates, each matching exactly this schema:

{"start_us": <integer microseconds, >= 0>, "end_us": <integer microseconds, must be > start_us>, "score": <float 0-100, higher means more interesting/highlight-worthy>, "title": "<short punchy title, a few words>", "reason": "<one sentence explaining why this moment is a highlight>"}

Return an empty array [] if nothing in the transcript stands out as a highlight. Do not include any field not listed above."#;

/// Builds the full grounding prompt for semantic highlight-candidate
/// proposal: a system prompt carrying the exact candidate schema (above),
/// plus a user prompt carrying the transcript (with timestamps), the
/// media's total duration, and a cap on how many candidates to propose.
pub fn build_highlight_prompt(
    transcript: &[TranscriptEntry],
    total_duration_us: i64,
    max_candidates: usize,
) -> HighlightPrompt {
    let system_prompt = format!(
        "You are a highlight-detection assistant embedded in a desktop video editor. Identify \
         the most engaging, interesting, or important moments in the transcript below so they \
         can be suggested to a human editor as highlight clips. Propose at most {max_candidates} \
         candidates, ranked by how highlight-worthy they are.\n\n{CANDIDATE_SCHEMA_INSTRUCTIONS}"
    );

    let mut user_prompt = String::new();
    user_prompt.push_str(&format!(
        "Total media duration: {total_duration_us} microseconds ({:.2} seconds).\n\n",
        total_duration_us as f64 / 1_000_000.0
    ));
    if transcript.is_empty() {
        user_prompt.push_str("Transcript: (none available for this media).\n");
    } else {
        user_prompt.push_str("Transcript (start_us, end_us, text):\n");
        for entry in transcript {
            user_prompt.push_str(&format!(
                "- [{}..{}]: {}\n",
                entry.start_us, entry.end_us, entry.text
            ));
        }
    }
    user_prompt.push_str("\nPropose highlight candidates grounded in these timestamps.");

    HighlightPrompt {
        system_prompt,
        user_prompt,
    }
}

/// Wire shape of one candidate before validation — deliberately not
/// [`Highlight`] itself (which also carries a server-generated `id` no LLM
/// should be trusted to invent uniquely).
#[derive(Debug, Deserialize)]
struct RawCandidate {
    start_us: i64,
    end_us: i64,
    score: f32,
    title: String,
    reason: String,
}

/// Parses `raw` (whatever text an `AIProvider::complete` returned) as a JSON
/// array of highlight candidates and validates each one, assigning a real
/// `id` to every surviving candidate. Never partially succeeds: any
/// malformed JSON or out-of-range candidate rejects the *entire* response
/// (`HighlightError`), matching `ai::edit_plan::parse_and_validate`'s own
/// discipline.
///
/// Validation, in order per candidate:
/// - `start_us` must be `>= 0`.
/// - `end_us` must be strictly greater than `start_us`.
/// - `score` must be within `0.0..=100.0` (`Highlight::score`'s documented
///   range).
pub fn parse_and_validate_candidates(raw: &str) -> Result<Vec<Highlight>, HighlightError> {
    let candidates: Vec<RawCandidate> =
        serde_json::from_str(raw).map_err(|e| HighlightError::MalformedJson {
            details: e.to_string(),
        })?;

    let mut out = Vec::with_capacity(candidates.len());
    for (index, c) in candidates.into_iter().enumerate() {
        let invalid = |details: String| HighlightError::InvalidCandidate { index, details };
        if c.start_us < 0 {
            return Err(invalid(format!(
                "start_us must be >= 0, got {}",
                c.start_us
            )));
        }
        if c.end_us <= c.start_us {
            return Err(invalid(format!(
                "end_us ({}) must be greater than start_us ({})",
                c.end_us, c.start_us
            )));
        }
        if !(0.0..=100.0).contains(&c.score) {
            return Err(invalid(format!(
                "score must be within 0.0..=100.0, got {}",
                c.score
            )));
        }
        out.push(Highlight {
            id: uuid::Uuid::new_v4().to_string(),
            start_us: c.start_us,
            end_us: c.end_us,
            score: c.score,
            title: c.title,
            reason: c.reason,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, start_us: i64, end_us: i64, text: &str) -> TranscriptEntry {
        TranscriptEntry {
            id: id.to_string(),
            media_id: "m1".to_string(),
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.9,
            words: vec![],
            is_filler: false,
        }
    }

    #[test]
    fn the_system_prompt_carries_the_exact_candidate_schema() {
        let prompt = build_highlight_prompt(&[], 10_000_000, 5);
        assert!(prompt.system_prompt.contains("start_us"));
        assert!(prompt.system_prompt.contains("end_us"));
        assert!(prompt.system_prompt.contains("\"score\""));
        assert!(prompt.system_prompt.contains("\"title\""));
        assert!(prompt.system_prompt.contains("\"reason\""));
        assert!(prompt.system_prompt.contains("at most 5"));
    }

    #[test]
    fn the_user_prompt_grounds_every_transcript_entry_with_its_timestamps() {
        let transcript = vec![entry("t1", 0, 2_000_000, "and that's when it happened")];
        let prompt = build_highlight_prompt(&transcript, 2_000_000, 3);
        assert!(prompt.user_prompt.contains("and that's when it happened"));
        assert!(prompt.user_prompt.contains("[0..2000000]"));
    }

    #[test]
    fn an_empty_transcript_still_produces_a_usable_prompt() {
        let prompt = build_highlight_prompt(&[], 1_000_000, 3);
        assert!(prompt.user_prompt.contains("none available"));
    }

    #[test]
    fn a_valid_candidate_array_parses_and_gets_real_ids() {
        let raw = r#"[
            {"start_us": 0, "end_us": 2000000, "score": 92.0, "title": "The reveal", "reason": "big surprise"},
            {"start_us": 5000000, "end_us": 8000000, "score": 60.5, "title": "Good joke", "reason": "audience laughs"}
        ]"#;
        let highlights = parse_and_validate_candidates(raw).expect("valid candidates parse");
        assert_eq!(highlights.len(), 2);
        assert_eq!(highlights[0].title, "The reveal");
        assert_eq!(highlights[0].score, 92.0);
        assert!(!highlights[0].id.is_empty());
        assert_ne!(highlights[0].id, highlights[1].id);
    }

    #[test]
    fn an_empty_candidate_array_is_valid() {
        assert!(parse_and_validate_candidates("[]").unwrap().is_empty());
    }

    #[test]
    fn malformed_json_is_rejected() {
        let err = parse_and_validate_candidates("not json at all").unwrap_err();
        assert!(matches!(err, HighlightError::MalformedJson { .. }));
    }

    #[test]
    fn an_inverted_time_range_is_rejected() {
        let raw =
            r#"[{"start_us": 100, "end_us": 50, "score": 50.0, "title": "x", "reason": "x"}]"#;
        assert!(matches!(
            parse_and_validate_candidates(raw).unwrap_err(),
            HighlightError::InvalidCandidate { index: 0, .. }
        ));
    }

    #[test]
    fn a_negative_start_us_is_rejected() {
        let raw =
            r#"[{"start_us": -1, "end_us": 100, "score": 50.0, "title": "x", "reason": "x"}]"#;
        assert!(matches!(
            parse_and_validate_candidates(raw).unwrap_err(),
            HighlightError::InvalidCandidate { index: 0, .. }
        ));
    }

    #[test]
    fn an_out_of_range_score_is_rejected() {
        let raw =
            r#"[{"start_us": 0, "end_us": 100, "score": 150.0, "title": "x", "reason": "x"}]"#;
        assert!(matches!(
            parse_and_validate_candidates(raw).unwrap_err(),
            HighlightError::InvalidCandidate { index: 0, .. }
        ));
    }

    #[test]
    fn the_second_candidates_index_is_reported_when_the_first_is_valid() {
        let raw = r#"[
            {"start_us": 0, "end_us": 100, "score": 50.0, "title": "ok", "reason": "ok"},
            {"start_us": 0, "end_us": 100, "score": -1.0, "title": "bad", "reason": "bad"}
        ]"#;
        assert!(matches!(
            parse_and_validate_candidates(raw).unwrap_err(),
            HighlightError::InvalidCandidate { index: 1, .. }
        ));
    }
}
