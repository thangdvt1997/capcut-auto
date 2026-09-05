//! Turns a free-text command (e.g. `"Remove all silence longer than
//! 800ms."`) plus the current project's transcript into a well-formed prompt
//! for the *existing* `AIProvider` → `EditPlan` pipeline (master prompt
//! §20's "Natural language → AI Provider → EditPlan → Schema validation →
//! Preview → Apply" architecture). This module is ONLY the
//! prompt-construction layer: the `EditPlan` schema, its strict validation,
//! and the closed `EditOperation` enum that makes arbitrary AI output
//! structurally incapable of executing anything already exist in
//! `ai::edit_plan` and are reused unchanged here (see
//! `commands::ai::generate_edit_plan_from_nl_command`, the only thing that
//! actually calls `AIProvider::complete` and then
//! `edit_plan::parse_and_validate` — this module never talks to a provider
//! itself, so it stays a pure, easily unit-tested string builder).
//!
//! ## Scope honesty (master prompt §20's own example command list)
//!
//! §20 lists nine example commands. This pass makes the pipeline work
//! end-to-end only for the subset `EditPlan`'s current two-operation schema
//! can actually express — that was the task brief, not an oversight, so it's
//! worth spelling out plainly which examples land where:
//!
//! - **Work end-to-end today** (pure removal/timing edits — exactly what
//!   `EditOperation::Remove` represents, and the transcript grounding this
//!   module builds into the prompt gives the model real timestamps to anchor
//!   them to): *"Remove all silence longer than 800ms."*, *"Remove filler
//!   words."*, *"Remove the intro."*
//! - **Parse and validate, but don't yet apply to the timeline** (produces
//!   `EditOperation::Zoom` entries — these round-trip through
//!   `ai::edit_plan::parse_and_validate` cleanly, but
//!   `commands::ai::apply_edit_plan_to_clip`/`_to_track` silently skip
//!   `Zoom`, per that module's own doc comment: no keyframe-authoring UI
//!   exists yet to make "zoom" meaningful): *"Zoom in when the speaker says
//!   something important."*
//! - **Cannot be expressed by this schema at all yet**: *"Turn this into a
//!   60 second TikTok."*, *"Add captions."*, *"Make this video faster."*,
//!   *"Create 3 shorts."* — these need operation kinds (retime/speed change,
//!   caption generation, export reframe/aspect change, multi-clip
//!   generation) `EditOperation` doesn't have. Inventing new variants to
//!   cover them unilaterally is explicitly out of scope for this pass
//!   (`IMPLEMENTATION_PLAN.md` Phase 10 brief) — asking the model one of
//!   these today either yields an empty `EditPlan` (no operations match) or
//!   a plan that fails `edit_plan::parse_and_validate`, never anything that
//!   silently does the wrong thing. *"Find the 5 best highlights"* is
//!   covered by a wholly separate feature, real highlight detection
//!   (`crate::highlights`) — not this pipeline at all.
//!
//! None of the "cannot express yet" cases are a safety gap: an
//! out-of-schema response can only ever fail validation or produce an empty
//! plan, never anything that executes as code — the same structural
//! guarantee `ai::edit_plan`'s own module doc comment describes.

use crate::ai::provider::AiRequest;
use crate::project::TranscriptEntry;

/// A constructed two-part prompt, ready to become a real [`AiRequest`] once
/// a caller supplies the provider-call knobs (`temperature`/`timeout_ms`/
/// `max_tokens`) this module has no opinion about — those live in
/// `commands::ai::AiProviderSettings`, not here.
#[derive(Debug, Clone, PartialEq)]
pub struct EditPlanPrompt {
    pub system_prompt: String,
    pub user_prompt: String,
}

impl EditPlanPrompt {
    /// Combines this prompt with the provider-call parameters into a
    /// ready-to-send [`AiRequest`].
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

/// The exact `EditPlan` JSON schema (`ai::edit_plan::EditPlan`/
/// `EditOperation`), spelled out for the model rather than hoping it infers
/// the shape — this pass's brief: "include the schema/format instructions
/// explicitly in the prompt, the same way you'd brief a real production
/// system". Kept as one constant so the system prompt and this module's own
/// content test can't silently drift apart.
const EDIT_PLAN_SCHEMA_INSTRUCTIONS: &str = r#"Respond with ONLY a single JSON object (no markdown code fences, no commentary before or after) matching exactly this schema:

{
  "version": 1,
  "operations": [
    {"type": "remove", "start_us": <integer microseconds, >= 0>, "end_us": <integer microseconds, must be > start_us>, "reason": "<short human-readable reason>", "confidence": <float 0.0-1.0, or null>},
    {"type": "zoom", "start_us": <integer microseconds>, "end_us": <integer microseconds, must be > start_us>, "scale": <positive float, e.g. 1.2>, "reason": "<short human-readable reason>"}
  ]
}

Rules:
- "version" must always be exactly 1.
- Every timestamp is in MICROSECONDS (1 second = 1,000,000 microseconds), matching the transcript timestamps given below.
- "operations" may be an empty array if the command does not require any edit.
- Use "remove" for any instruction that deletes/cuts a time range (silence, filler words, an intro, a specific passage).
- Use "zoom" only for an explicit zoom/emphasis instruction.
- Do not invent an operation "type" other than "remove" or "zoom" — no other operation kind exists in this schema.
- Do not include any field not listed above."#;

/// Builds the full grounding prompt for `nl_command` (master prompt §20's AI
/// command box): a system prompt carrying the exact `EditPlan` schema
/// (above), plus a user prompt carrying the transcript (with timestamps) and
/// the media's total duration — the minimum context an LLM needs to ground
/// real, time-ranged `EditOperation`s, without shipping the entire project
/// JSON (this pass's brief explicitly does not ask for that).
pub fn build_edit_plan_prompt(
    nl_command: &str,
    transcript: &[TranscriptEntry],
    total_duration_us: i64,
) -> EditPlanPrompt {
    let system_prompt = format!(
        "You are a precise video-editing assistant embedded in a desktop video editor. \
         Your only job is to translate a user's natural-language editing instruction into a \
         structured EditPlan describing which time ranges to remove or zoom. You never explain \
         yourself, never write prose, and never produce anything other than the JSON object \
         described below.\n\n{EDIT_PLAN_SCHEMA_INSTRUCTIONS}"
    );

    let mut user_prompt = String::new();
    user_prompt.push_str(&format!("User command: \"{nl_command}\"\n\n"));
    user_prompt.push_str(&format!(
        "Total media duration: {total_duration_us} microseconds ({:.2} seconds).\n\n",
        total_duration_us as f64 / 1_000_000.0
    ));
    if transcript.is_empty() {
        user_prompt.push_str("Transcript: (none available for this media).\n");
    } else {
        user_prompt.push_str("Transcript (start_us, end_us, filler-word flag, text):\n");
        for entry in transcript {
            user_prompt.push_str(&format!(
                "- [{}..{}] filler={}: {}\n",
                entry.start_us, entry.end_us, entry.is_filler, entry.text
            ));
        }
    }
    user_prompt.push_str(
        "\nProduce the EditPlan JSON for the user command above, grounded in these timestamps.",
    );

    EditPlanPrompt {
        system_prompt,
        user_prompt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, start_us: i64, end_us: i64, text: &str, is_filler: bool) -> TranscriptEntry {
        TranscriptEntry {
            id: id.to_string(),
            media_id: "m1".to_string(),
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.95,
            words: vec![],
            is_filler,
        }
    }

    #[test]
    fn the_system_prompt_carries_the_exact_edit_plan_schema() {
        let prompt = build_edit_plan_prompt("Remove filler words.", &[], 10_000_000);
        assert!(prompt.system_prompt.contains("\"version\": 1"));
        assert!(prompt.system_prompt.contains("\"type\": \"remove\""));
        assert!(prompt.system_prompt.contains("\"type\": \"zoom\""));
        assert!(prompt.system_prompt.contains("start_us"));
        assert!(prompt.system_prompt.contains("end_us"));
    }

    #[test]
    fn the_user_prompt_contains_the_verbatim_nl_command() {
        let prompt = build_edit_plan_prompt("Remove the intro.", &[], 5_000_000);
        assert!(prompt.user_prompt.contains("Remove the intro."));
    }

    #[test]
    fn the_user_prompt_grounds_every_transcript_entry_with_its_timestamps() {
        let transcript = vec![
            entry("t1", 0, 2_000_000, "um so today", true),
            entry(
                "t2",
                2_000_000,
                6_000_000,
                "we're going to talk about cutting silence",
                false,
            ),
        ];
        let prompt = build_edit_plan_prompt("Remove filler words.", &transcript, 6_000_000);
        assert!(prompt.user_prompt.contains("um so today"));
        assert!(prompt
            .user_prompt
            .contains("we're going to talk about cutting silence"));
        assert!(prompt.user_prompt.contains("[0..2000000]"));
        assert!(prompt.user_prompt.contains("[2000000..6000000]"));
        assert!(prompt.user_prompt.contains("filler=true"));
        assert!(prompt.user_prompt.contains("filler=false"));
    }

    #[test]
    fn the_user_prompt_states_the_total_duration_in_microseconds() {
        let prompt = build_edit_plan_prompt("Remove filler words.", &[], 12_500_000);
        assert!(prompt.user_prompt.contains("12500000 microseconds"));
        assert!(prompt.user_prompt.contains("12.50 seconds"));
    }

    #[test]
    fn an_empty_transcript_still_produces_a_usable_prompt() {
        let prompt = build_edit_plan_prompt("Remove the intro.", &[], 1_000_000);
        assert!(prompt.user_prompt.contains("none available"));
        assert!(!prompt.system_prompt.is_empty());
    }

    #[test]
    fn into_request_carries_the_provider_call_parameters_through() {
        let prompt = build_edit_plan_prompt("Remove filler words.", &[], 1_000_000);
        let request = prompt.into_request(0.3, 20_000, Some(1024));
        assert_eq!(request.temperature, 0.3);
        assert_eq!(request.timeout_ms, 20_000);
        assert_eq!(request.max_tokens, Some(1024));
        assert!(request.system_prompt.is_some());
    }
}
