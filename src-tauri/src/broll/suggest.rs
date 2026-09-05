//! The one B-roll piece that genuinely needs an AI call (master prompt §34:
//! "AI can suggest: keyword, start, end, duration, reason"): asking the
//! configured `AIProvider` to propose B-roll insertion points directly from
//! a transcript. Kept entirely separate from `broll::provider` (the real,
//! local, no-AI-needed half) so that module stays independently testable and
//! useful with zero AI provider configured at all — nothing in `provider`
//! imports this module or `ai::provider`, and nothing here talks to the
//! media library (mirrors `highlights::signals`/`highlights::semantic`'s own
//! split, module doc comment there).
//!
//! [`parse_and_validate`] mirrors `ai::smart_edit::parse_and_validate`'s own
//! "never partially succeed; reject the whole response on any malformed
//! entry" discipline. `end`/`duration` are redundant with each other (master
//! prompt §34's own worked example only ever gives `start` + `duration`, not
//! `end`), so this schema keeps just `insertion_time_us` + `duration_us` —
//! the exact closed schema this pass's brief specifies — rather than
//! carrying a derivable third field that could disagree with the other two.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ai::provider::AiRequest;
use crate::project::TranscriptEntry;

use super::error::BRollSuggestError;

/// The only schema version this module understands today — same
/// "exact match, no migration logic" convention as
/// `ai::edit_plan::CURRENT_VERSION`/`ai::smart_edit::CURRENT_VERSION`.
pub const CURRENT_VERSION: u32 = 1;

/// One AI-suggested B-roll insertion point. Closed, strictly typed, specta-
/// typed for eventual frontend consumption — this is a *proposal* a caller
/// reviews (and, per [`super::combine::suggest_and_search`], pairs with real
/// local search results), never something that inserts a clip on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct BRollSuggestion {
    pub id: String,
    pub insertion_time_us: i64,
    pub duration_us: i64,
    /// The search term to look for local B-roll with (master prompt §34's
    /// worked example: `"bitcoin price chart"`) — fed directly into
    /// `BRollQuery::keyword` by the combined suggest-then-search flow.
    pub keyword: String,
    pub reason: String,
}

/// The wire-format wrapper an `AIProvider`'s raw response text must parse
/// as — versioned for the same reason `ai::smart_edit::SmartEditResponse` is.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
struct BRollSuggestResponse {
    version: u32,
    suggestions: Vec<BRollSuggestion>,
}

/// Parses `raw` (whatever text an `AIProvider::complete` returned) as JSON
/// and validates it against the strict B-roll suggestion schema, returning
/// the validated suggestion list — or a specific `BRollSuggestError` and
/// *nothing* else, mirroring `ai::smart_edit::parse_and_validate` exactly.
///
/// Validation, in order:
/// 1. `raw` must parse as valid JSON matching `BRollSuggestResponse`'s shape
///    at all (`BRollSuggestError::MalformedJson`).
/// 2. `version` must equal [`CURRENT_VERSION`] exactly
///    (`BRollSuggestError::UnsupportedVersion`).
/// 3. Every suggestion's fields must be in-range
///    (`BRollSuggestError::InvalidSuggestion`, carrying the offending index):
///    - `insertion_time_us` must be `>= 0`.
///    - `duration_us` must be strictly positive (rejects zero/negative).
///    - `insertion_time_us + duration_us` must not exceed `total_duration_us`
///      — i.e. the suggested insertion must fall entirely within the
///      transcript's own span, per this pass's brief ("reject ... an
///      insertion time outside the transcript's own span"). `total_duration_us`
///      is caller-supplied (the same media duration the transcript itself
///      was generated against), not part of the AI's own JSON — an AI
///      response has no way to know or lie about it.
///    - `keyword` must be non-empty after trimming (an empty keyword can
///      never produce a meaningful local search).
pub fn parse_and_validate(
    raw: &str,
    total_duration_us: i64,
) -> Result<Vec<BRollSuggestion>, BRollSuggestError> {
    let parsed: BRollSuggestResponse =
        serde_json::from_str(raw).map_err(|e| BRollSuggestError::MalformedJson {
            details: e.to_string(),
        })?;

    if parsed.version != CURRENT_VERSION {
        return Err(BRollSuggestError::UnsupportedVersion {
            version: parsed.version,
        });
    }

    for (index, suggestion) in parsed.suggestions.iter().enumerate() {
        validate_suggestion(index, suggestion, total_duration_us)?;
    }

    Ok(parsed.suggestions)
}

fn validate_suggestion(
    index: usize,
    suggestion: &BRollSuggestion,
    total_duration_us: i64,
) -> Result<(), BRollSuggestError> {
    let invalid = |details: String| BRollSuggestError::InvalidSuggestion { index, details };

    if suggestion.insertion_time_us < 0 {
        return Err(invalid(format!(
            "insertion_time_us must be >= 0, got {}",
            suggestion.insertion_time_us
        )));
    }
    if suggestion.duration_us <= 0 {
        return Err(invalid(format!(
            "duration_us must be > 0, got {}",
            suggestion.duration_us
        )));
    }
    let end_us = suggestion
        .insertion_time_us
        .saturating_add(suggestion.duration_us);
    if end_us > total_duration_us {
        return Err(invalid(format!(
            "insertion_time_us + duration_us ({end_us}) must not exceed the transcript's own span ({total_duration_us})"
        )));
    }
    if suggestion.keyword.trim().is_empty() {
        return Err(invalid("keyword must not be empty".to_string()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Prompt construction
// ---------------------------------------------------------------------------

/// Formats one `TranscriptEntry` as one prompt line — same plain,
/// easy-to-eyeball format as `ai::smart_edit::format_entry` (not JSON, since
/// this is the *input* description to the model, not something this app
/// parses back).
fn format_entry(entry: &TranscriptEntry) -> String {
    format!(
        "[{} {} {}] {}",
        entry.id, entry.start_us, entry.end_us, entry.text
    )
}

/// Pure, testable string-building: given the current transcript, builds the
/// user-prompt text an `AIProvider` should receive to produce B-roll
/// suggestions, per master prompt §34's exact worked example (a transcript
/// mentioning "Bitcoin reached a new high..." should suggest a keyword like
/// "bitcoin price chart" timed at that moment).
pub fn build_broll_prompt(entries: &[TranscriptEntry], total_duration_us: i64) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are a B-roll planning assistant for a video editor. B-roll is supplementary \
         footage/imagery inserted alongside the main talking-head footage to illustrate what is \
         being said (master prompt example: when the speaker says \"Bitcoin reached a new \
         high...\", a chart showing the Bitcoin price would make good B-roll).\n\n\
         Read the transcript below and identify moments where a short piece of B-roll would \
         meaningfully illustrate the speaker's point.\n",
    );

    prompt.push_str(&format!(
        "\nTotal media duration: {total_duration_us} microseconds ({:.2} seconds).\n",
        total_duration_us as f64 / 1_000_000.0
    ));

    prompt.push_str("\nTranscript (one entry per line, format `[id start_us end_us] text`):\n");
    if entries.is_empty() {
        prompt.push_str("(no transcript entries)\n");
    }
    for entry in entries {
        prompt.push_str(&format_entry(entry));
        prompt.push('\n');
    }

    prompt.push_str(&format!(
        "\nRespond with a single JSON object and nothing else, exactly matching this schema:\n\
         {{\n\
         \x20\x20\"version\": {CURRENT_VERSION},\n\
         \x20\x20\"suggestions\": [\n\
         \x20\x20\x20\x20{{\n\
         \x20\x20\x20\x20\x20\x20\"id\": \"a unique string id\",\n\
         \x20\x20\x20\x20\x20\x20\"insertion_time_us\": 0,\n\
         \x20\x20\x20\x20\x20\x20\"duration_us\": 0,\n\
         \x20\x20\x20\x20\x20\x20\"keyword\": \"a short search term describing the B-roll to find, e.g. \\\"bitcoin price chart\\\"\",\n\
         \x20\x20\x20\x20\x20\x20\"reason\": \"a short human-readable reason this moment needs B-roll\"\n\
         \x20\x20\x20\x20}}\n\
         \x20\x20]\n\
         }}\n\
         `insertion_time_us`/`duration_us` are microseconds. `duration_us` must be a positive \
         number of microseconds, and `insertion_time_us + duration_us` must not exceed the total \
         media duration given above. `keyword` must not be empty. Return an empty \
         `suggestions` array if nothing in the transcript calls for B-roll."
    ));

    prompt
}

/// Builds the full `AiRequest` for a B-roll suggestion call: this module's
/// own system prompt plus [`build_broll_prompt`]'s user prompt, with
/// caller-supplied `temperature`/`timeout_ms` (the same per-call knobs every
/// other `AIProvider` caller in this crate threads through).
pub fn build_broll_request(
    entries: &[TranscriptEntry],
    total_duration_us: i64,
    temperature: f32,
    timeout_ms: u64,
) -> AiRequest {
    AiRequest {
        system_prompt: Some(
            "You are a B-roll planning assistant for a video editor. You only ever respond with \
             the exact JSON schema you are given — never prose, never markdown code fences, \
             never any other text."
                .to_string(),
        ),
        user_prompt: build_broll_prompt(entries, total_duration_us),
        temperature,
        timeout_ms,
        max_tokens: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, text: &str, start_us: i64, end_us: i64) -> TranscriptEntry {
        TranscriptEntry {
            id: id.to_string(),
            media_id: "m1".to_string(),
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.9,
            words: Vec::new(),
            is_filler: false,
        }
    }

    // -- prompt construction --------------------------------------------------

    #[test]
    fn prompt_contains_the_transcript_content_and_duration() {
        let entries = [entry(
            "e1",
            "Bitcoin reached a new high today",
            30_000_000,
            34_000_000,
        )];
        let prompt = build_broll_prompt(&entries, 60_000_000);
        assert!(prompt.contains("Bitcoin reached a new high today"));
        assert!(prompt.contains("e1"));
        assert!(prompt.contains("60000000 microseconds"));
    }

    #[test]
    fn prompt_carries_the_master_prompt_worked_example_style_guidance() {
        let prompt = build_broll_prompt(&[], 1_000_000);
        assert!(prompt.contains("bitcoin price chart"));
        assert!(prompt.contains("\"keyword\""));
        assert!(prompt.contains("\"insertion_time_us\""));
        assert!(prompt.contains("\"duration_us\""));
        assert!(prompt.contains("\"reason\""));
    }

    #[test]
    fn prompt_on_empty_transcript_still_builds_a_schema_prompt() {
        let prompt = build_broll_prompt(&[], 0);
        assert!(prompt.contains("no transcript entries"));
        assert!(prompt.contains("\"version\""));
    }

    #[test]
    fn build_broll_request_threads_temperature_and_timeout() {
        let entries = [entry("e1", "hello", 0, 1_000_000)];
        let request = build_broll_request(&entries, 10_000_000, 0.4, 9_000);
        assert_eq!(request.temperature, 0.4);
        assert_eq!(request.timeout_ms, 9_000);
        assert!(request.system_prompt.is_some());
        assert!(request.user_prompt.contains("hello"));
    }

    // -- parse_and_validate: happy path ---------------------------------------

    #[test]
    fn a_valid_response_parses() {
        let raw = r#"{
            "version": 1,
            "suggestions": [
                {"id": "s1", "insertion_time_us": 32500000, "duration_us": 3000000, "keyword": "bitcoin price chart", "reason": "speaker mentions Bitcoin's new high"}
            ]
        }"#;
        let suggestions = parse_and_validate(raw, 60_000_000).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].keyword, "bitcoin price chart");
        assert_eq!(suggestions[0].insertion_time_us, 32_500_000);
        assert_eq!(suggestions[0].duration_us, 3_000_000);
    }

    #[test]
    fn an_empty_suggestions_list_is_valid() {
        let suggestions =
            parse_and_validate(r#"{"version": 1, "suggestions": []}"#, 1_000_000).unwrap();
        assert!(suggestions.is_empty());
    }

    #[test]
    fn a_suggestion_exactly_at_the_total_duration_boundary_is_valid() {
        let raw = r#"{"version": 1, "suggestions": [
            {"id": "s1", "insertion_time_us": 7000000, "duration_us": 3000000, "keyword": "k", "reason": "r"}
        ]}"#;
        let suggestions = parse_and_validate(raw, 10_000_000).unwrap();
        assert_eq!(suggestions.len(), 1);
    }

    // -- parse_and_validate: rejection cases ----------------------------------

    #[test]
    fn malformed_json_is_rejected() {
        let err = parse_and_validate("not json at all", 1_000_000).unwrap_err();
        assert!(matches!(err, BRollSuggestError::MalformedJson { .. }));
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let raw = r#"{"version": 2, "suggestions": []}"#;
        assert!(matches!(
            parse_and_validate(raw, 1_000_000).unwrap_err(),
            BRollSuggestError::UnsupportedVersion { version: 2 }
        ));
    }

    #[test]
    fn negative_insertion_time_is_rejected() {
        let raw = r#"{"version": 1, "suggestions": [
            {"id": "s1", "insertion_time_us": -1, "duration_us": 1000, "keyword": "k", "reason": "r"}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw, 1_000_000).unwrap_err(),
            BRollSuggestError::InvalidSuggestion { index: 0, .. }
        ));
    }

    #[test]
    fn zero_duration_is_rejected() {
        let raw = r#"{"version": 1, "suggestions": [
            {"id": "s1", "insertion_time_us": 0, "duration_us": 0, "keyword": "k", "reason": "r"}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw, 1_000_000).unwrap_err(),
            BRollSuggestError::InvalidSuggestion { index: 0, .. }
        ));
    }

    #[test]
    fn negative_duration_is_rejected() {
        let raw = r#"{"version": 1, "suggestions": [
            {"id": "s1", "insertion_time_us": 0, "duration_us": -5, "keyword": "k", "reason": "r"}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw, 1_000_000).unwrap_err(),
            BRollSuggestError::InvalidSuggestion { index: 0, .. }
        ));
    }

    #[test]
    fn an_insertion_time_outside_the_transcript_span_is_rejected() {
        let raw = r#"{"version": 1, "suggestions": [
            {"id": "s1", "insertion_time_us": 9000000, "duration_us": 2000000, "keyword": "k", "reason": "r"}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw, 10_000_000).unwrap_err(),
            BRollSuggestError::InvalidSuggestion { index: 0, .. }
        ));
    }

    #[test]
    fn an_empty_keyword_is_rejected() {
        let raw = r#"{"version": 1, "suggestions": [
            {"id": "s1", "insertion_time_us": 0, "duration_us": 1000, "keyword": "   ", "reason": "r"}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw, 1_000_000).unwrap_err(),
            BRollSuggestError::InvalidSuggestion { index: 0, .. }
        ));
    }

    #[test]
    fn the_second_suggestion_index_is_reported_when_the_first_is_valid() {
        let raw = r#"{"version": 1, "suggestions": [
            {"id": "s1", "insertion_time_us": 0, "duration_us": 100, "keyword": "ok", "reason": "r"},
            {"id": "s2", "insertion_time_us": 0, "duration_us": -1, "keyword": "bad", "reason": "r"}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw, 1_000_000).unwrap_err(),
            BRollSuggestError::InvalidSuggestion { index: 1, .. }
        ));
    }
}
