//! B-roll Tauri command surface (master prompt §34, `IMPLEMENTATION_PLAN.md`
//! Phase 11). Thin per master prompt §66 — all real logic lives in
//! `crate::broll`.
//!
//! Three commands, matching `crate::broll`'s own three-way split:
//! [`search_local_broll`] is the real, no-AI-needed local keyword search on
//! its own (useful even without any AI provider configured);
//! [`suggest_broll_from_transcript`] is the AI-dependent half alone (a
//! caller that already knows exactly what local media it wants can skip
//! straight to a search instead); [`suggest_and_search_broll`] is the full
//! combined pipeline master prompt §34 describes end-to-end.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::broll::combine::BRollSuggestionWithCandidates;
use crate::broll::provider::{
    BRollCandidate, BRollProvider, BRollQuery, LocalLibraryBRollProvider,
};
use crate::broll::suggest::{self, BRollSuggestion};
use crate::commands::ai::{self, AiProviderSettings};
use crate::db::MediaLibrary;
use crate::error::AppErrorPayload;
use crate::project::{MediaKind, TranscriptEntry};

/// Default cap on how many local candidates a single search/suggestion pass
/// returns — generous enough to be useful without needing its own pagination
/// yet, matching `commands::highlights::DEFAULT_MAX_HIGHLIGHTS`'s reasoning.
const DEFAULT_CANDIDATE_LIMIT: u32 = 10;

// ---------------------------------------------------------------------------
// Local search (no AI needed)
// ---------------------------------------------------------------------------

/// Direct keyword search against the existing local media library
/// (`broll::provider::LocalLibraryBRollProvider`) — no AI provider involved
/// at all. Useful on its own (a user typing a keyword directly into a B-roll
/// panel) and as the second half of [`suggest_and_search_broll`] below.
#[tauri::command]
#[specta::specta]
pub fn search_local_broll(
    library: State<'_, MediaLibrary>,
    keyword: String,
    kind: Option<MediaKind>,
    limit: Option<u32>,
) -> Result<Vec<BRollCandidate>, AppErrorPayload> {
    let provider = LocalLibraryBRollProvider::new(&library);
    let query = BRollQuery {
        keyword,
        kind,
        limit: limit.unwrap_or(DEFAULT_CANDIDATE_LIMIT),
    };
    provider
        .search(&query)
        .map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// AI-dependent suggestion
// ---------------------------------------------------------------------------

/// **Suggest**: builds a B-roll prompt from `entries` (the caller-supplied
/// transcript, same "caller passes the transcript in directly" convention
/// `commands::ai::analyze_smart_edit` already uses) and `total_duration_us`,
/// calls the configured provider, and validates the response into a strict
/// `Vec<BRollSuggestion>` — or a clear error, never a partially populated
/// result (`broll::suggest` module doc comment). This is a *proposal*; it
/// does not search the local library itself — see
/// [`suggest_and_search_broll`] for the combined flow.
#[tauri::command]
#[specta::specta]
pub fn suggest_broll_from_transcript(
    settings: AiProviderSettings,
    entries: Vec<TranscriptEntry>,
    total_duration_us: i64,
) -> Result<Vec<BRollSuggestion>, AppErrorPayload> {
    let api_key = ai::resolve_api_key(&settings).map_err(|e| AppErrorPayload::from(&e))?;
    let provider = ai::build_provider(&settings, api_key).map_err(|e| AppErrorPayload::from(&e))?;
    let request = suggest::build_broll_request(
        &entries,
        total_duration_us,
        settings.temperature,
        settings.timeout_ms,
    );
    let response = provider
        .complete(&request)
        .map_err(|e| AppErrorPayload::from(&e))?;
    suggest::parse_and_validate(&response.text, total_duration_us)
        .map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// Combined suggest-then-search pipeline
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct BRollSuggestionWithCandidatesPayload {
    pub suggestion: BRollSuggestion,
    pub candidates: Vec<BRollCandidate>,
}

impl From<BRollSuggestionWithCandidates> for BRollSuggestionWithCandidatesPayload {
    fn from(value: BRollSuggestionWithCandidates) -> Self {
        Self {
            suggestion: value.suggestion,
            candidates: value.candidates,
        }
    }
}

/// **Suggest, then search**: the full master prompt §34 pipeline end to end
/// — [`suggest_broll_from_transcript`]'s AI call/validation, then, for every
/// validated suggestion, a real local-library search
/// (`broll::combine::suggest_and_search`) for its `keyword`. Each result
/// pairs a suggestion with whatever real local candidates were found for
/// it — possibly none; an honest empty list is expected and is not an error
/// (`broll::combine` module doc comment).
#[tauri::command]
#[specta::specta]
pub fn suggest_and_search_broll(
    library: State<'_, MediaLibrary>,
    settings: AiProviderSettings,
    entries: Vec<TranscriptEntry>,
    total_duration_us: i64,
    candidates_per_suggestion: Option<u32>,
) -> Result<Vec<BRollSuggestionWithCandidatesPayload>, AppErrorPayload> {
    let suggestions = suggest_broll_from_transcript(settings, entries, total_duration_us)?;
    let provider = LocalLibraryBRollProvider::new(&library);
    let paired = crate::broll::combine::suggest_and_search(
        &provider,
        suggestions,
        candidates_per_suggestion.unwrap_or(DEFAULT_CANDIDATE_LIMIT),
    );
    Ok(paired.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_http::spawn_one_shot;
    use crate::commands::ai::AiProviderKind;
    use crate::db::{self, MediaLibraryEntry};
    use std::sync::Mutex;

    fn settings(base_url: String) -> AiProviderSettings {
        AiProviderSettings {
            provider: AiProviderKind::OpenAi,
            base_url,
            model: "test-model".to_string(),
            temperature: 0.2,
            timeout_ms: 5_000,
            credential_ref: None,
        }
    }

    fn transcript_entry(id: &str, text: &str, start_us: i64, end_us: i64) -> TranscriptEntry {
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

    fn chat_completion_body(content: &str) -> String {
        serde_json::json!({
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }]
        })
        .to_string()
    }

    fn library_with(entries: &[MediaLibraryEntry]) -> MediaLibrary {
        let conn = db::open_in_memory().unwrap();
        for entry in entries {
            db::upsert_media(&conn, entry).unwrap();
        }
        MediaLibrary(Mutex::new(conn))
    }

    fn sample_entry(id: &str, filename: &str, tags: Vec<&str>) -> MediaLibraryEntry {
        MediaLibraryEntry {
            id: id.to_string(),
            filename: filename.to_string(),
            path: format!("/media/{filename}"),
            kind: MediaKind::Video,
            duration_us: 5_000_000,
            width: 1920,
            height: 1080,
            tags: tags.into_iter().map(String::from).collect(),
            created_at: None,
            imported_at: "2026-09-04T00:00:00Z".to_string(),
            thumbnail_path: None,
            proxy_path: None,
        }
    }

    // `search_local_broll`/`suggest_and_search_broll` take `State<'_,
    // MediaLibrary>`, which needs a real running Tauri app to construct —
    // like `commands::timeline`'s `State<'_, TimelineState>`-taking commands,
    // this codebase doesn't unit-test the thin `#[tauri::command]` wrapper
    // itself at that layer. Their real logic (`LocalLibraryBRollProvider::
    // search`, `broll::combine::suggest_and_search`) is already fully
    // covered in `broll::provider`/`broll::combine`'s own tests; the
    // "combined pipeline" tests below exercise that same real logic wired to
    // a real in-memory `MediaLibrary`, exactly what `suggest_and_search_broll`
    // does internally minus the `State` extraction Tauri itself performs
    // (mirroring `commands::highlights`'s own `run_detection` direct-call
    // test pattern).

    #[test]
    fn suggest_broll_from_transcript_round_trips_a_well_formed_mock_response() {
        let broll_json = r#"{"version":1,"suggestions":[{"id":"b1","insertion_time_us":32500000,"duration_us":3000000,"keyword":"bitcoin price chart","reason":"speaker mentions a new high"}]}"#;
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(broll_json));

        let entries = vec![transcript_entry(
            "e1",
            "Bitcoin reached a new high today",
            30_000_000,
            34_000_000,
        )];
        let suggestions = suggest_broll_from_transcript(settings(base_url), entries, 60_000_000)
            .expect("well-formed response should parse and validate");

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].keyword, "bitcoin price chart");
        assert_eq!(suggestions[0].insertion_time_us, 32_500_000);
        assert_eq!(suggestions[0].duration_us, 3_000_000);
    }

    #[test]
    fn suggest_broll_from_transcript_surfaces_a_clear_error_for_a_malformed_mock_response() {
        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body("not json at all"));

        let err =
            suggest_broll_from_transcript(settings(base_url), vec![], 10_000_000).unwrap_err();
        assert_eq!(err.code, "BROLL_SUGGEST_MALFORMED_JSON");
    }

    #[test]
    fn suggest_broll_from_transcript_surfaces_a_clear_error_for_an_out_of_range_suggestion() {
        // Well-formed JSON, but the insertion time falls outside the given
        // total_duration_us — the provider call itself succeeds; validation
        // is what must reject this.
        let broll_json = r#"{"version":1,"suggestions":[{"id":"b1","insertion_time_us":9000000,"duration_us":5000000,"keyword":"k","reason":"r"}]}"#;
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(broll_json));

        let err =
            suggest_broll_from_transcript(settings(base_url), vec![], 10_000_000).unwrap_err();
        assert_eq!(err.code, "BROLL_SUGGEST_INVALID_SUGGESTION");
    }

    #[test]
    fn suggest_broll_from_transcript_surfaces_a_clear_error_when_unreachable() {
        let dead_url = crate::ai::test_http::spawn_connection_refused();
        let err =
            suggest_broll_from_transcript(settings(dead_url), vec![], 10_000_000).unwrap_err();
        assert_eq!(err.code, "AI_PROVIDER_REQUEST_FAILED");
    }

    // -- the combined pipeline, exercised via the pure `broll::combine` layer
    // wired to a real in-memory `MediaLibrary` (the same thing
    // `suggest_and_search_broll` does internally, minus the `State<'_, _>`
    // extraction Tauri itself is responsible for) --------------------------

    #[test]
    fn combined_pipeline_pairs_a_suggestion_with_a_real_local_match() {
        let library = library_with(&[sample_entry(
            "m1",
            "bitcoin_chart.mp4",
            vec!["bitcoin", "finance"],
        )]);
        let broll_json = r#"{"version":1,"suggestions":[{"id":"b1","insertion_time_us":1000000,"duration_us":2000000,"keyword":"bitcoin","reason":"r"}]}"#;
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(broll_json));

        let suggestions =
            suggest_broll_from_transcript(settings(base_url), vec![], 10_000_000).unwrap();
        let provider = LocalLibraryBRollProvider::new(&library);
        let paired = crate::broll::combine::suggest_and_search(&provider, suggestions, 10);

        assert_eq!(paired.len(), 1);
        assert_eq!(paired[0].candidates.len(), 1);
        assert_eq!(paired[0].candidates[0].media_id, "m1");
    }

    #[test]
    fn combined_pipeline_honestly_reports_no_local_candidates_found() {
        let library = library_with(&[sample_entry("m1", "cooking.mp4", vec!["food"])]);
        let broll_json = r#"{"version":1,"suggestions":[{"id":"b1","insertion_time_us":1000000,"duration_us":2000000,"keyword":"spaceship","reason":"r"}]}"#;
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(broll_json));

        let suggestions =
            suggest_broll_from_transcript(settings(base_url), vec![], 10_000_000).unwrap();
        let provider = LocalLibraryBRollProvider::new(&library);
        let paired = crate::broll::combine::suggest_and_search(&provider, suggestions, 10);

        assert_eq!(paired.len(), 1);
        assert!(paired[0].candidates.is_empty());
    }
}
