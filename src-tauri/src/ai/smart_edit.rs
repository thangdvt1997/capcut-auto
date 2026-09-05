//! Smart Edit / AI semantic editing (master prompt §19), the follow-up pass
//! `IMPLEMENTATION_PLAN.md` Phase 10 calls out on top of this phase's own
//! `AIProvider`/`EditPlan` foundation (`ai::provider`/`ai::edit_plan`).
//!
//! Same overall shape as `transcription::filler`/`vad::cutlist` (module doc
//! comments there): "a detector produces reviewable candidates with a time
//! range + reason, the user picks which to apply, then the accepted subset
//! converts into real `Cut`s applied through the existing timeline
//! machinery" — except the *detector* here is an LLM call over the
//! transcript rather than a signal-processing algorithm. The pipeline is:
//!
//! 1. [`build_smart_edit_request`] turns `Vec<TranscriptEntry>` into a real
//!    `AiRequest` — a prompt describing the exact master-prompt §19 category
//!    list plus this module's JSON response schema, so the model is told the
//!    shape it must answer in rather than just hoped to guess it.
//! 2. A caller (`commands::ai::analyze_smart_edit`) sends that request
//!    through the configured `AIProvider::complete`.
//! 3. [`parse_and_validate`] turns the raw response text into a strict
//!    `Vec<SmartEditRecommendation>` — the *only* way raw AI text becomes
//!    something this app acts on, same rigor and "never partially succeeds"
//!    discipline as `ai::edit_plan::parse_and_validate` (master prompt §53:
//!    AI output is a proposal, never directly executed).
//! 4. The frontend (a later pass) shows recommendations to the user, who
//!    may accept a subset and override any `suggested_action` (e.g.
//!    downgrade a suggested `Remove` to `Keep`).
//! 5. [`recommendations_to_cuts`] converts the accepted, possibly-overridden
//!    subset into real `Cut`s, applied through the *existing*
//!    `commands::timeline::apply_silence_cuts`/`apply_silence_cuts_to_track`
//!    path — never a parallel mutation path (`commands::ai::
//!    apply_smart_edit_recommendations_to_clip`/`_to_track`).
//!
//! ## `SmartEditAction::Highlight` vs. Phase 10's highlight-*detection*
//!
//! These are deliberately unrelated features that happen to share a word.
//! `SmartEditAction::Highlight` here is Smart Edit's own judgment that a
//! transcript span is worth calling out to the user (a purely informational,
//! non-mutating verdict, exactly like `Keep` — nothing in this module turns
//! it into a `Cut`). The separate highlight-*detection* feature
//! (`IMPLEMENTATION_PLAN.md` Phase 10, master prompt §21 — built by a
//! different, concurrently-running work stream) is about finding candidate
//! short-form clips from speech-density/audio-energy/scene-change signals
//! (`start`/`end`/`score`/`title`/`reason`). Do not conflate the two: this
//! module has no dependency on that one and vice versa.
//!
//! ## `Shorten`'s exact timeline interpretation
//!
//! `SmartEditAction::Shorten { target_duration_us }` is modeled as trimming
//! the recommended span down to `target_duration_us` by **keeping the
//! beginning** of the span and removing the remainder: the resulting `Cut`
//! spans `(start_us + target_duration_us)..end_us`. This is a deliberate,
//! simple, documented choice among several plausible ones (keep the end,
//! keep the middle, ...) — "keep the beginning" was picked because a
//! recommended span's opening words are, in practice, more likely to carry
//! the point being made (a long-winded restatement, a rambling aside) than
//! its tail, and it requires no further AI input (a "which part matters"
//! judgment) to apply mechanically. `target_duration_us` is validated
//! (`validate_recommendation` below) to be strictly within
//! `0..(end_us - start_us)`, so this always produces a real, non-empty `Cut`
//! for a validated recommendation.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::project::{Cut, TranscriptEntry};

use super::edit_plan::ai_suggested_cut;
use super::error::SmartEditError;
use super::provider::AiRequest;

/// The only schema version this module understands today — same
/// "exact match, no migration logic" convention as
/// `ai::edit_plan::CURRENT_VERSION`.
pub const CURRENT_VERSION: u32 = 1;

/// Master prompt §19's exact category list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SmartEditCategory {
    Repetition,
    FalseStart,
    OffTopic,
    WeakSentence,
    LongPause,
    FillerWord,
    UnnecessaryIntro,
    DuplicateIdea,
    BoringSection,
}

impl SmartEditCategory {
    /// Every variant, in master prompt §19's listed order — used both to
    /// build the prompt's category list and to enumerate every variant in
    /// tests.
    pub const ALL: [SmartEditCategory; 9] = [
        SmartEditCategory::Repetition,
        SmartEditCategory::FalseStart,
        SmartEditCategory::OffTopic,
        SmartEditCategory::WeakSentence,
        SmartEditCategory::LongPause,
        SmartEditCategory::FillerWord,
        SmartEditCategory::UnnecessaryIntro,
        SmartEditCategory::DuplicateIdea,
        SmartEditCategory::BoringSection,
    ];

    /// The human-readable phrase master prompt §19 uses for this category
    /// (e.g. `Repetition` -> "repetition removal") — used to build the
    /// prompt's category list in the model's own words, and in error/UI
    /// messages elsewhere.
    pub fn description(self) -> &'static str {
        match self {
            SmartEditCategory::Repetition => "repetition removal",
            SmartEditCategory::FalseStart => "false starts",
            SmartEditCategory::OffTopic => "off-topic sections",
            SmartEditCategory::WeakSentence => "weak sentences",
            SmartEditCategory::LongPause => "long pauses",
            SmartEditCategory::FillerWord => "filler words",
            SmartEditCategory::UnnecessaryIntro => "unnecessary introductions",
            SmartEditCategory::DuplicateIdea => "duplicate ideas",
            SmartEditCategory::BoringSection => "boring sections",
        }
    }

    /// The wire/JSON tag this category serializes/deserializes as (its
    /// `#[serde(rename_all = "snake_case")]` name) — used to spell out the
    /// exact literal the model must answer with in the prompt's schema
    /// instructions, so the two never drift apart.
    fn wire_name(self) -> &'static str {
        match self {
            SmartEditCategory::Repetition => "repetition",
            SmartEditCategory::FalseStart => "false_start",
            SmartEditCategory::OffTopic => "off_topic",
            SmartEditCategory::WeakSentence => "weak_sentence",
            SmartEditCategory::LongPause => "long_pause",
            SmartEditCategory::FillerWord => "filler_word",
            SmartEditCategory::UnnecessaryIntro => "unnecessary_intro",
            SmartEditCategory::DuplicateIdea => "duplicate_idea",
            SmartEditCategory::BoringSection => "boring_section",
        }
    }
}

/// Master prompt §19's exact action list. Closed enum, `#[serde(tag =
/// "type")]` — same "no free-form string ever gets pattern-matched or
/// interpreted at runtime" discipline as `ai::edit_plan::EditOperation`
/// (that module's doc comment explains the master prompt §53 threat model
/// this defends against; identical reasoning applies here).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SmartEditAction {
    /// No-op on the timeline: the user/AI judged this span should stay
    /// exactly as-is.
    Keep,
    /// Maps to one `Cut` spanning the recommendation's whole
    /// `start_us..end_us` — see [`recommendations_to_cuts`].
    Remove,
    /// Maps to one `Cut` trimming the recommendation's span down to
    /// `target_duration_us` — see module doc comment ("Shorten's exact
    /// timeline interpretation") and [`recommendations_to_cuts`].
    Shorten { target_duration_us: i64 },
    /// No-op on the timeline: a user-facing "this span is worth calling
    /// out" verdict, distinct from Phase 10's separate highlight-detection
    /// feature (module doc comment).
    Highlight,
}

/// A single Smart Edit recommendation. Every field master prompt §19
/// requires ("time range, transcript, reason, confidence, suggested
/// action") plus `id` (so a frontend/caller can reference one recommendation
/// across the propose -> user-reviews -> apply round trip) and `category`
/// (which of §19's nine categories this recommendation falls under).
///
/// Closed, strictly typed, specta-typed for eventual frontend consumption —
/// this is a *proposal the user reviews*, never auto-applied (master prompt
/// §53's discipline, same as `EditPlan`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SmartEditRecommendation {
    pub id: String,
    pub start_us: i64,
    pub end_us: i64,
    /// The actual transcript text this recommendation is about — not just a
    /// time range, so a user can judge the recommendation without having to
    /// scrub the timeline first.
    pub transcript: String,
    pub category: SmartEditCategory,
    pub reason: String,
    pub confidence: f32,
    pub suggested_action: SmartEditAction,
}

/// The wire-format wrapper an `AIProvider`'s raw response text must parse
/// as — versioned for the same reason `ai::edit_plan::EditPlan` is: a future
/// schema change gets a new version number rather than silently
/// reinterpreting old fields.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
struct SmartEditResponse {
    version: u32,
    recommendations: Vec<SmartEditRecommendation>,
}

/// Parses `raw` (whatever text an `AIProvider::complete` returned) as JSON
/// and validates it against the strict Smart Edit schema, returning the
/// validated recommendation list — or a specific `SmartEditError` and
/// *nothing* else (never a partially-populated or partially-validated
/// result), mirroring `ai::edit_plan::parse_and_validate` exactly.
///
/// Validation, in order:
/// 1. `raw` must parse as valid JSON matching `SmartEditResponse`'s shape at
///    all — a malformed document, or one naming a `category`/`suggested_action`
///    outside their closed enums, fails here (`SmartEditError::MalformedJson`).
/// 2. `version` must equal [`CURRENT_VERSION`] exactly
///    (`SmartEditError::UnsupportedVersion`).
/// 3. Every recommendation's fields must be in-range
///    (`SmartEditError::InvalidRecommendation`, carrying the offending
///    index):
///    - `start_us` must be `>= 0`.
///    - `end_us` must be strictly greater than `start_us`.
///    - `confidence` must be within `0.0..=1.0`.
///    - `Shorten.target_duration_us` must be strictly within
///      `0..(end_us - start_us)` — i.e. positive and strictly less than the
///      recommendation's own span, so it always produces a real, non-empty
///      `Cut` (never `>=` the span, which would remove nothing or invert
///      the trim).
pub fn parse_and_validate(raw: &str) -> Result<Vec<SmartEditRecommendation>, SmartEditError> {
    let parsed: SmartEditResponse =
        serde_json::from_str(raw).map_err(|e| SmartEditError::MalformedJson {
            details: e.to_string(),
        })?;

    if parsed.version != CURRENT_VERSION {
        return Err(SmartEditError::UnsupportedVersion {
            version: parsed.version,
        });
    }

    for (index, rec) in parsed.recommendations.iter().enumerate() {
        validate_recommendation(index, rec)?;
    }

    Ok(parsed.recommendations)
}

fn validate_recommendation(
    index: usize,
    rec: &SmartEditRecommendation,
) -> Result<(), SmartEditError> {
    let invalid = |details: String| SmartEditError::InvalidRecommendation { index, details };

    if rec.start_us < 0 {
        return Err(invalid(format!(
            "start_us must be >= 0, got {}",
            rec.start_us
        )));
    }
    if rec.end_us <= rec.start_us {
        return Err(invalid(format!(
            "end_us ({}) must be greater than start_us ({})",
            rec.end_us, rec.start_us
        )));
    }
    if !(0.0..=1.0).contains(&rec.confidence) {
        return Err(invalid(format!(
            "confidence must be within 0.0..=1.0, got {}",
            rec.confidence
        )));
    }
    if let SmartEditAction::Shorten { target_duration_us } = rec.suggested_action {
        let span = rec.end_us - rec.start_us;
        if target_duration_us <= 0 || target_duration_us >= span {
            return Err(invalid(format!(
                "shorten target_duration_us ({target_duration_us}) must be within 0..{span} (the recommendation's own span)"
            )));
        }
    }
    Ok(())
}

/// Converts a caller-selected (and possibly action-overridden — a user may
/// downgrade an AI-suggested `Remove` to `Keep`, or vice versa) subset of
/// `SmartEditRecommendation`s into real, unapplied `Cut`s against
/// `source_media_id`:
///
/// - `Remove` -> one `Cut` spanning the recommendation's whole
///   `start_us..end_us`.
/// - `Shorten { target_duration_us }` -> one `Cut` spanning
///   `(start_us + target_duration_us)..end_us` (module doc comment,
///   "Shorten's exact timeline interpretation"). Defensively skipped (not a
///   panic) if `target_duration_us` is out of the validated range — this can
///   only happen if a caller hand-builds a recommendation bypassing
///   [`parse_and_validate`] entirely.
/// - `Keep`/`Highlight` -> no `Cut`. Not an error: there is nothing to
///   "apply" for a no-op judgment.
///
/// Mirrors `ai::edit_plan::plan_to_remove_cuts` exactly — `source_media_id`
/// is not part of the schema itself (same reasoning: a recommendation's
/// `start_us`/`end_us` are source-media-relative, but which media that is
/// comes from whichever caller generated the recommendation, not the
/// recommendation itself).
pub fn recommendations_to_cuts(
    recommendations: &[SmartEditRecommendation],
    source_media_id: &str,
) -> Vec<Cut> {
    recommendations
        .iter()
        .filter_map(|rec| match rec.suggested_action {
            SmartEditAction::Remove => {
                Some(ai_suggested_cut(source_media_id, rec.start_us, rec.end_us))
            }
            SmartEditAction::Shorten { target_duration_us } => {
                let trimmed_start = rec.start_us.saturating_add(target_duration_us);
                (trimmed_start < rec.end_us)
                    .then(|| ai_suggested_cut(source_media_id, trimmed_start, rec.end_us))
            }
            SmartEditAction::Keep | SmartEditAction::Highlight => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Prompt construction
// ---------------------------------------------------------------------------

/// Formats one `TranscriptEntry` as one prompt line: `[id start_us end_us]
/// text`. A plain, easy-to-eyeball format (not JSON) since this is the
/// *input* description to the model, not something this app parses back —
/// only the model's JSON *response* goes through strict parsing.
fn format_entry(entry: &TranscriptEntry) -> String {
    format!(
        "[{} {} {}] {}",
        entry.id, entry.start_us, entry.end_us, entry.text
    )
}

/// Pure, testable string-building: given the current transcript, builds the
/// user-prompt text an `AIProvider` should receive to produce Smart Edit
/// recommendations. Includes the master prompt §19 category list, the
/// action list, the transcript content itself, and this module's JSON
/// response schema/format instructions — a real production system tells the
/// model the exact shape to answer in rather than hoping it infers one.
pub fn build_smart_edit_prompt(entries: &[TranscriptEntry]) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are an expert video editor performing a Smart Edit pass over a transcript.\n\
         Analyze the transcript below and identify spans that fall into any of the following categories:\n",
    );
    for category in SmartEditCategory::ALL {
        prompt.push_str(&format!(
            "- {} (category: \"{}\")\n",
            category.description(),
            category.wire_name()
        ));
    }

    prompt.push_str(
        "\nFor each span you flag, choose exactly one suggested action:\n\
         - KEEP: the span is fine as-is, but worth noting.\n\
         - REMOVE: the whole span should be cut.\n\
         - SHORTEN: the span should be trimmed down to a shorter target duration (in microseconds).\n\
         - HIGHLIGHT: the span is worth calling out to the editor (not necessarily for removal).\n",
    );

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
         \x20\x20\"recommendations\": [\n\
         \x20\x20\x20\x20{{\n\
         \x20\x20\x20\x20\x20\x20\"id\": \"a unique string id\",\n\
         \x20\x20\x20\x20\x20\x20\"start_us\": 0,\n\
         \x20\x20\x20\x20\x20\x20\"end_us\": 0,\n\
         \x20\x20\x20\x20\x20\x20\"transcript\": \"the exact transcript text this recommendation is about\",\n\
         \x20\x20\x20\x20\x20\x20\"category\": \"one of the category tags listed above, e.g. \\\"filler_word\\\"\",\n\
         \x20\x20\x20\x20\x20\x20\"reason\": \"a short human-readable reason\",\n\
         \x20\x20\x20\x20\x20\x20\"confidence\": 0.0,\n\
         \x20\x20\x20\x20\x20\x20\"suggested_action\": {{\"type\": \"keep\" | \"remove\" | \"highlight\"}} or {{\"type\": \"shorten\", \"target_duration_us\": 0}}\n\
         \x20\x20\x20\x20}}\n\
         \x20\x20]\n\
         }}\n\
         `start_us`/`end_us` are microseconds and must satisfy `end_us > start_us`. `confidence` must be within 0.0..=1.0. \
         For `shorten`, `target_duration_us` must be a positive number of microseconds strictly less than `end_us - start_us`. \
         Only use the category and action tags listed above — do not invent new ones."
    ));

    prompt
}

/// Builds the full `AiRequest` for a Smart Edit analysis call: this
/// module's own system prompt plus [`build_smart_edit_prompt`]'s user
/// prompt, with caller-supplied `temperature`/`timeout_ms` (the same
/// per-call knobs every other `AIProvider` caller in this crate threads
/// through, e.g. `commands::ai::test_ai_connection`).
pub fn build_smart_edit_request(
    entries: &[TranscriptEntry],
    temperature: f32,
    timeout_ms: u64,
) -> AiRequest {
    AiRequest {
        system_prompt: Some(
            "You are a Smart Edit assistant for a video editor. You only ever respond with the \
             exact JSON schema you are given — never prose, never markdown code fences, never \
             any other text."
                .to_string(),
        ),
        user_prompt: build_smart_edit_prompt(entries),
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

    fn recommendation(
        id: &str,
        start_us: i64,
        end_us: i64,
        category: SmartEditCategory,
        confidence: f32,
        action: SmartEditAction,
    ) -> SmartEditRecommendation {
        SmartEditRecommendation {
            id: id.to_string(),
            start_us,
            end_us,
            transcript: "some text".to_string(),
            category,
            reason: "because".to_string(),
            confidence,
            suggested_action: action,
        }
    }

    fn valid_response_json() -> &'static str {
        r#"{
            "version": 1,
            "recommendations": [
                {
                    "id": "r1",
                    "start_us": 1000000,
                    "end_us": 2000000,
                    "transcript": "so, so, so it was great",
                    "category": "repetition",
                    "reason": "the phrase 'so' repeats three times",
                    "confidence": 0.87,
                    "suggested_action": {"type": "remove"}
                },
                {
                    "id": "r2",
                    "start_us": 3000000,
                    "end_us": 6000000,
                    "transcript": "let me start over, um, okay so basically",
                    "category": "false_start",
                    "reason": "speaker restarts the sentence",
                    "confidence": 0.7,
                    "suggested_action": {"type": "shorten", "target_duration_us": 1000000}
                },
                {
                    "id": "r3",
                    "start_us": 7000000,
                    "end_us": 8000000,
                    "transcript": "this part is genuinely great",
                    "category": "boring_section",
                    "reason": "surprisingly strong moment worth keeping",
                    "confidence": 0.6,
                    "suggested_action": {"type": "highlight"}
                },
                {
                    "id": "r4",
                    "start_us": 9000000,
                    "end_us": 9500000,
                    "transcript": "fine as-is",
                    "category": "weak_sentence",
                    "reason": "borderline, but acceptable",
                    "confidence": 0.4,
                    "suggested_action": {"type": "keep"}
                }
            ]
        }"#
    }

    // -- prompt construction ------------------------------------------------

    #[test]
    fn prompt_contains_the_transcript_content() {
        let entries = [
            entry("e1", "um so anyway", 0, 1_000_000),
            entry("e2", "it was great, great, great", 1_000_000, 3_000_000),
        ];
        let prompt = build_smart_edit_prompt(&entries);
        assert!(prompt.contains("um so anyway"));
        assert!(prompt.contains("it was great, great, great"));
        assert!(prompt.contains("e1"));
        assert!(prompt.contains("e2"));
    }

    #[test]
    fn prompt_contains_every_master_prompt_category() {
        let prompt = build_smart_edit_prompt(&[]);
        for description in [
            "repetition removal",
            "false starts",
            "off-topic sections",
            "weak sentences",
            "long pauses",
            "filler words",
            "unnecessary introductions",
            "duplicate ideas",
            "boring sections",
        ] {
            assert!(
                prompt.contains(description),
                "expected prompt to mention {description:?}"
            );
        }
        for wire_name in [
            "repetition",
            "false_start",
            "off_topic",
            "weak_sentence",
            "long_pause",
            "filler_word",
            "unnecessary_intro",
            "duplicate_idea",
            "boring_section",
        ] {
            assert!(
                prompt.contains(wire_name),
                "expected prompt to mention wire tag {wire_name:?}"
            );
        }
    }

    #[test]
    fn prompt_contains_every_action() {
        let prompt = build_smart_edit_prompt(&[]);
        for action in ["KEEP", "REMOVE", "SHORTEN", "HIGHLIGHT"] {
            assert!(prompt.contains(action));
        }
    }

    #[test]
    fn prompt_on_empty_transcript_still_builds_a_schema_prompt() {
        let prompt = build_smart_edit_prompt(&[]);
        assert!(prompt.contains("no transcript entries"));
        assert!(prompt.contains("\"version\""));
    }

    #[test]
    fn build_smart_edit_request_threads_temperature_and_timeout() {
        let entries = [entry("e1", "hello", 0, 1_000_000)];
        let request = build_smart_edit_request(&entries, 0.3, 12_345);
        assert_eq!(request.temperature, 0.3);
        assert_eq!(request.timeout_ms, 12_345);
        assert!(request.system_prompt.is_some());
        assert!(request.user_prompt.contains("hello"));
    }

    // -- parse_and_validate: happy path -------------------------------------

    #[test]
    fn a_valid_response_parses_with_all_four_action_kinds() {
        let recs = parse_and_validate(valid_response_json()).unwrap();
        assert_eq!(recs.len(), 4);
        assert!(matches!(recs[0].suggested_action, SmartEditAction::Remove));
        assert!(matches!(
            recs[1].suggested_action,
            SmartEditAction::Shorten {
                target_duration_us: 1_000_000
            }
        ));
        assert!(matches!(
            recs[2].suggested_action,
            SmartEditAction::Highlight
        ));
        assert!(matches!(recs[3].suggested_action, SmartEditAction::Keep));
    }

    #[test]
    fn an_empty_recommendations_list_is_valid() {
        let recs = parse_and_validate(r#"{"version": 1, "recommendations": []}"#).unwrap();
        assert!(recs.is_empty());
    }

    // -- parse_and_validate: rejection cases --------------------------------

    #[test]
    fn malformed_json_is_rejected() {
        let err = parse_and_validate("not json at all").unwrap_err();
        assert!(matches!(err, SmartEditError::MalformedJson { .. }));
    }

    #[test]
    fn an_unknown_category_fails_to_deserialize() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "x", "category": "not_a_real_category",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "keep"}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::MalformedJson { .. }
        ));
    }

    #[test]
    fn an_unknown_action_type_fails_to_deserialize() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "x", "category": "filler_word",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "delete_everything"}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::MalformedJson { .. }
        ));
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let raw = r#"{"version": 2, "recommendations": []}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::UnsupportedVersion { version: 2 }
        ));
    }

    #[test]
    fn negative_start_us_is_rejected() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": -1, "end_us": 100, "transcript": "x", "category": "filler_word",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "keep"}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));
    }

    #[test]
    fn inverted_time_range_is_rejected() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 200, "end_us": 100, "transcript": "x", "category": "filler_word",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "keep"}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));
    }

    #[test]
    fn zero_length_time_range_is_rejected() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 100, "end_us": 100, "transcript": "x", "category": "filler_word",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "keep"}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));
    }

    #[test]
    fn out_of_range_confidence_is_rejected() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "x", "category": "filler_word",
             "reason": "x", "confidence": 1.5, "suggested_action": {"type": "keep"}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));
    }

    #[test]
    fn negative_confidence_is_rejected() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "x", "category": "filler_word",
             "reason": "x", "confidence": -0.1, "suggested_action": {"type": "keep"}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));
    }

    #[test]
    fn shorten_target_duration_equal_to_the_span_is_rejected() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "x", "category": "false_start",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "shorten", "target_duration_us": 100}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));
    }

    #[test]
    fn shorten_target_duration_greater_than_the_span_is_rejected() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "x", "category": "false_start",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "shorten", "target_duration_us": 500}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));
    }

    #[test]
    fn shorten_target_duration_zero_or_negative_is_rejected() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "x", "category": "false_start",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "shorten", "target_duration_us": 0}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));

        let raw_negative = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "x", "category": "false_start",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "shorten", "target_duration_us": -10}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw_negative).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 0, .. }
        ));
    }

    #[test]
    fn the_second_recommendation_index_is_reported_when_the_first_is_valid() {
        let raw = r#"{"version": 1, "recommendations": [
            {"id": "r1", "start_us": 0, "end_us": 100, "transcript": "ok", "category": "filler_word",
             "reason": "x", "confidence": 0.5, "suggested_action": {"type": "keep"}},
            {"id": "r2", "start_us": 0, "end_us": 100, "transcript": "bad", "category": "filler_word",
             "reason": "x", "confidence": 2.0, "suggested_action": {"type": "keep"}}
        ]}"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            SmartEditError::InvalidRecommendation { index: 1, .. }
        ));
    }

    // -- recommendations_to_cuts ---------------------------------------------

    #[test]
    fn remove_produces_a_cut_spanning_the_whole_recommendation() {
        let recs = [recommendation(
            "r1",
            1_000_000,
            2_000_000,
            SmartEditCategory::Repetition,
            0.9,
            SmartEditAction::Remove,
        )];
        let cuts = recommendations_to_cuts(&recs, "m1");
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].start_us, 1_000_000);
        assert_eq!(cuts[0].end_us, 2_000_000);
        assert_eq!(cuts[0].source_media_id, "m1");
        assert_eq!(cuts[0].kind, crate::project::CutKind::Remove);
        assert_eq!(cuts[0].reason, crate::project::CutReason::AiSuggested);
        assert!(!cuts[0].applied);
    }

    #[test]
    fn shorten_produces_a_cut_trimming_the_tail() {
        let recs = [recommendation(
            "r1",
            1_000_000,
            5_000_000,
            SmartEditCategory::FalseStart,
            0.8,
            SmartEditAction::Shorten {
                target_duration_us: 1_000_000,
            },
        )];
        let cuts = recommendations_to_cuts(&recs, "m1");
        assert_eq!(cuts.len(), 1);
        // Keeps the first 1s of the span (1.0..2.0), removes the rest (2.0..5.0).
        assert_eq!(cuts[0].start_us, 2_000_000);
        assert_eq!(cuts[0].end_us, 5_000_000);
    }

    #[test]
    fn keep_and_highlight_produce_no_cuts() {
        let recs = [
            recommendation(
                "r1",
                0,
                100,
                SmartEditCategory::WeakSentence,
                0.5,
                SmartEditAction::Keep,
            ),
            recommendation(
                "r2",
                100,
                200,
                SmartEditCategory::BoringSection,
                0.5,
                SmartEditAction::Highlight,
            ),
        ];
        assert!(recommendations_to_cuts(&recs, "m1").is_empty());
    }

    #[test]
    fn a_hand_built_out_of_range_shorten_is_defensively_skipped_not_panicked() {
        // Bypasses parse_and_validate entirely (as a malicious/buggy caller
        // might) — recommendations_to_cuts must not panic or produce a
        // nonsensical inverted Cut.
        let recs = [recommendation(
            "r1",
            0,
            100,
            SmartEditCategory::FalseStart,
            0.5,
            SmartEditAction::Shorten {
                target_duration_us: 1000,
            },
        )];
        assert!(recommendations_to_cuts(&recs, "m1").is_empty());
    }

    #[test]
    fn a_mixed_selection_only_produces_cuts_for_remove_and_shorten() {
        let recs = [
            recommendation(
                "r1",
                0,
                100,
                SmartEditCategory::Repetition,
                0.9,
                SmartEditAction::Remove,
            ),
            recommendation(
                "r2",
                100,
                200,
                SmartEditCategory::WeakSentence,
                0.9,
                SmartEditAction::Keep,
            ),
            recommendation(
                "r3",
                200,
                400,
                SmartEditCategory::FalseStart,
                0.9,
                SmartEditAction::Shorten {
                    target_duration_us: 50,
                },
            ),
            recommendation(
                "r4",
                400,
                500,
                SmartEditCategory::BoringSection,
                0.9,
                SmartEditAction::Highlight,
            ),
        ];
        let cuts = recommendations_to_cuts(&recs, "m1");
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[0].start_us, 0);
        assert_eq!(cuts[0].end_us, 100);
        assert_eq!(cuts[1].start_us, 250);
        assert_eq!(cuts[1].end_us, 400);
    }

    // -- integration: an accepted selection applies through the real timeline engine ----

    #[test]
    fn accepted_recommendations_apply_undo_and_redo_cleanly_through_the_real_timeline() {
        use crate::project::{Clip, ClipSettings, ProjectV1, Track, TrackKind};
        use crate::timeline::command::{History, MAX_HISTORY};
        use crate::timeline::ops::clip_span;
        use crate::timeline::silence;

        let mut project = ProjectV1::new("smart edit integration test");
        project.tracks.push(Track {
            id: "t1".into(),
            kind: TrackKind::Video,
            name: "t1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c1".into()],
        });
        project.clips.push(Clip {
            id: "c1".into(),
            track_id: "t1".into(),
            media_id: Some("m1".into()),
            source_in_us: 0,
            source_out_us: 10_000_000,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });
        let before = serde_json::to_value(&project).unwrap();

        // One REMOVE, one SHORTEN, one KEEP (no-op), one HIGHLIGHT (no-op).
        let recs = [
            recommendation(
                "r1",
                3_000_000,
                4_000_000,
                SmartEditCategory::Repetition,
                0.9,
                SmartEditAction::Remove,
            ),
            recommendation(
                "r2",
                6_000_000,
                8_000_000,
                SmartEditCategory::FalseStart,
                0.8,
                SmartEditAction::Shorten {
                    target_duration_us: 500_000,
                },
            ),
            recommendation(
                "r3",
                8_500_000,
                9_000_000,
                SmartEditCategory::WeakSentence,
                0.4,
                SmartEditAction::Keep,
            ),
            recommendation(
                "r4",
                9_000_000,
                9_500_000,
                SmartEditCategory::BoringSection,
                0.4,
                SmartEditAction::Highlight,
            ),
        ];

        let cuts = recommendations_to_cuts(&recs, "m1");
        // KEEP/HIGHLIGHT produced no cuts; REMOVE + SHORTEN produced two.
        assert_eq!(cuts.len(), 2);

        let edit_command =
            silence::apply_cuts_to_clip(&project, "c1", &cuts).expect("cuts apply to the clip");

        let mut history = History::new(MAX_HISTORY);
        history
            .apply(&mut project, edit_command)
            .expect("batch applies as one undo step");

        // Removed: [3s,4s) and [6.5s, 8s) (Shorten kept 6.0..6.5, cut the
        // rest), leaving three surviving pieces.
        let mut spans: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans.sort();
        assert_eq!(
            spans,
            vec![
                (0, 3_000_000),
                (4_000_000, 6_500_000),
                (8_000_000, 10_000_000)
            ]
        );

        history
            .undo(&mut project)
            .expect("undo restores original state");
        assert_eq!(serde_json::to_value(&project).unwrap(), before);

        history.redo(&mut project).expect("redo reapplies cleanly");
        let mut spans_after_redo: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans_after_redo.sort();
        assert_eq!(
            spans_after_redo,
            vec![
                (0, 3_000_000),
                (4_000_000, 6_500_000),
                (8_000_000, 10_000_000)
            ]
        );
    }
}
