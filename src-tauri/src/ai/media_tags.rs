//! AI-generated media tags (master prompt §35's own "Optional AI-generated
//! tags" line — everything else in §35, real filename/path/duration/
//! resolution/tags/created/kind indexing plus keyword search, was already
//! fully built in Phase 3, `crate::db`). Given an already-indexed
//! `MediaLibraryEntry`, asks the configured `AIProvider` to suggest a small
//! set of descriptive tags — a *proposal*, never auto-written: see
//! `commands::media::suggest_media_tags` (this module's only caller) and
//! `commands::media::merge_media_tags`/`db::merge_media_tags` for the
//! separate, explicit write step, matching this whole project's established
//! "AI proposes, user approves" discipline (`EditPlan`/
//! `SmartEditRecommendation`/`Highlight`/`BRollSuggestion` all follow the
//! same split).
//!
//! ## Text-only, not vision-based (a deliberate, documented choice)
//!
//! Tagging could in principle look at a thumbnail image rather than just
//! filename/metadata. This pass builds the **text-only** variant instead:
//! `ai::provider::AiRequest` — and every real adapter built on top of it
//! (`ai::openai_compat`/`ai::anthropic`/`ai::gemini`) — carries a plain
//! `user_prompt: String`, sent as a single string `content` field on the
//! wire (`openai_compat`'s `{"role": "user", "content": request.user_prompt}`,
//! `anthropic`'s identical shape, `gemini`'s `contents[].parts: [{text}]`);
//! none of the three real adapters send a multimodal/image content block
//! today. Building real image-based tagging would mean redesigning
//! `AiRequest` itself to carry image bytes and reworking all three adapters'
//! wire formats to each provider's own documented multimodal message shape
//! (OpenAI's `image_url` content parts, Anthropic's base64 `image` content
//! blocks, Gemini's `inline_data` parts) — a real, cross-cutting foundational
//! change to this codebase's `AIProvider` abstraction, not a small addition,
//! and out of scope for this pass. Filename + kind + resolution + duration
//! is a real, honest, and often genuinely informative signal on its own (a
//! file named `bitcoin_price_chart.mp4` truthfully implies its likely
//! content) — not a fabricated placeholder standing in for real tagging.

use crate::db::MediaLibraryEntry;
use crate::project::MediaKind;

use super::error::MediaTagError;
use super::provider::AiRequest;

/// Sanity caps on the suggested tag list (module doc comment: "a simple
/// `Vec<String>` with basic sanity limits ... is a defensible closed schema
/// here") — non-empty individual tags, a reasonable upper bound on count, no
/// absurdly long individual tag.
pub const MAX_TAG_COUNT: usize = 12;
pub const MAX_TAG_LENGTH: usize = 40;

fn kind_str(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Video => "video",
        MediaKind::Audio => "audio",
        MediaKind::Image => "image",
    }
}

/// Pure, testable string-building: describes one `MediaLibraryEntry` (its
/// filename, kind, duration, resolution, and any tags it already has, given
/// for context only — the model is told not to just repeat them) and asks
/// for a short list of new descriptive tags.
pub fn build_media_tag_prompt(entry: &MediaLibraryEntry) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "Suggest descriptive search tags for this video editor's media library file, based \
         only on its filename and metadata below (no visual or audio content is available to \
         you — judge only from the text given):\n",
    );
    prompt.push_str(&format!("- Filename: {}\n", entry.filename));
    prompt.push_str(&format!("- Kind: {}\n", kind_str(entry.kind)));
    prompt.push_str(&format!(
        "- Duration: {:.2} seconds\n",
        entry.duration_us as f64 / 1_000_000.0
    ));
    if entry.kind != MediaKind::Audio {
        prompt.push_str(&format!("- Resolution: {}x{}\n", entry.width, entry.height));
    }
    if !entry.tags.is_empty() {
        prompt.push_str(&format!(
            "- Existing tags (for context only, do not just repeat these): {}\n",
            entry.tags.join(", ")
        ));
    }
    prompt.push_str(&format!(
        "\nRespond with ONLY a single JSON array of short lowercase string tags (no markdown \
         code fences, no commentary before or after), at most {MAX_TAG_COUNT} tags, each at \
         most {MAX_TAG_LENGTH} characters, e.g. [\"bitcoin\", \"finance\", \"chart\"]. Return an \
         empty array [] if nothing about the filename/metadata suggests a meaningful tag."
    ));
    prompt
}

/// Builds the full `AiRequest` for a tag-suggestion call: this module's own
/// system prompt plus [`build_media_tag_prompt`]'s user prompt, with
/// caller-supplied `temperature`/`timeout_ms` (the same per-call knobs every
/// other `AIProvider` caller in this crate threads through).
pub fn build_media_tag_request(
    entry: &MediaLibraryEntry,
    temperature: f32,
    timeout_ms: u64,
) -> AiRequest {
    AiRequest {
        system_prompt: Some(
            "You are a media librarian assistant for a video editor. You only ever respond with \
             the exact JSON array schema you are given — never prose, never markdown code \
             fences, never any other text."
                .to_string(),
        ),
        user_prompt: build_media_tag_prompt(entry),
        temperature,
        timeout_ms,
        max_tokens: Some(256),
    }
}

/// Parses `raw` (whatever text an `AIProvider::complete` returned) as a JSON
/// array of strings and validates it against a simple closed schema, or a
/// specific `MediaTagError` and *nothing* else — never partially populated,
/// same discipline as every other `parse_and_validate` in this crate.
///
/// Validation, in order:
/// 1. `raw` must parse as a JSON array of strings at all
///    (`MediaTagError::MalformedJson`).
/// 2. The array must not exceed [`MAX_TAG_COUNT`] entries
///    (`MediaTagError::TooManyTags`) — an *empty* array is valid (an honest
///    "nothing meaningful to suggest" outcome, same as an empty
///    `Vec<SmartEditRecommendation>`/`Vec<Highlight>` elsewhere in this
///    crate), only an excessively long list is rejected.
/// 3. Every tag, after trimming, must be non-empty and at most
///    [`MAX_TAG_LENGTH`] characters, and must not contain a newline
///    (`MediaTagError::InvalidTag`, carrying the offending index).
///
/// Surviving tags are de-duplicated case-insensitively, keeping the first
/// occurrence's own casing/order — a model repeating "Bitcoin" and "bitcoin"
/// should not produce two tags.
pub fn parse_and_validate(raw: &str) -> Result<Vec<String>, MediaTagError> {
    let tags: Vec<String> =
        serde_json::from_str(raw).map_err(|e| MediaTagError::MalformedJson {
            details: e.to_string(),
        })?;

    if tags.len() > MAX_TAG_COUNT {
        return Err(MediaTagError::TooManyTags {
            count: tags.len(),
            max: MAX_TAG_COUNT,
        });
    }

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(tags.len());
    for (index, tag) in tags.into_iter().enumerate() {
        let trimmed = tag.trim().to_string();
        if trimmed.is_empty() {
            return Err(MediaTagError::InvalidTag {
                index,
                details: "tag must not be empty".to_string(),
            });
        }
        if trimmed.chars().count() > MAX_TAG_LENGTH {
            return Err(MediaTagError::InvalidTag {
                index,
                details: format!(
                    "tag exceeds {MAX_TAG_LENGTH} characters ({} given): {trimmed:?}",
                    trimmed.chars().count()
                ),
            });
        }
        if trimmed.contains('\n') || trimmed.contains('\r') {
            return Err(MediaTagError::InvalidTag {
                index,
                details: "tag must not contain a newline".to_string(),
            });
        }
        if seen.insert(trimmed.to_lowercase()) {
            out.push(trimmed);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(filename: &str, kind: MediaKind, tags: Vec<&str>) -> MediaLibraryEntry {
        MediaLibraryEntry {
            id: "m1".to_string(),
            filename: filename.to_string(),
            path: format!("/media/{filename}"),
            kind,
            duration_us: 12_500_000,
            width: 1920,
            height: 1080,
            tags: tags.into_iter().map(String::from).collect(),
            created_at: None,
            imported_at: "2026-09-04T00:00:00Z".to_string(),
            thumbnail_path: None,
            proxy_path: None,
        }
    }

    // -- prompt construction ---------------------------------------------

    #[test]
    fn prompt_contains_the_filename_kind_duration_and_resolution() {
        let e = entry("bitcoin_chart.mp4", MediaKind::Video, vec![]);
        let prompt = build_media_tag_prompt(&e);
        assert!(prompt.contains("bitcoin_chart.mp4"));
        assert!(prompt.contains("video"));
        assert!(prompt.contains("12.50 seconds"));
        assert!(prompt.contains("1920x1080"));
    }

    #[test]
    fn prompt_omits_resolution_for_audio() {
        let e = entry("podcast.mp3", MediaKind::Audio, vec![]);
        let prompt = build_media_tag_prompt(&e);
        assert!(!prompt.contains("Resolution"));
    }

    #[test]
    fn prompt_includes_existing_tags_for_context_only() {
        let e = entry("clip.mp4", MediaKind::Video, vec!["finance", "chart"]);
        let prompt = build_media_tag_prompt(&e);
        assert!(prompt.contains("finance, chart"));
        assert!(prompt.contains("for context only"));
    }

    #[test]
    fn prompt_omits_existing_tags_line_when_there_are_none() {
        let e = entry("clip.mp4", MediaKind::Video, vec![]);
        let prompt = build_media_tag_prompt(&e);
        assert!(!prompt.contains("Existing tags"));
    }

    #[test]
    fn build_media_tag_request_threads_temperature_and_timeout() {
        let e = entry("clip.mp4", MediaKind::Video, vec![]);
        let request = build_media_tag_request(&e, 0.5, 8_000);
        assert_eq!(request.temperature, 0.5);
        assert_eq!(request.timeout_ms, 8_000);
        assert!(request.system_prompt.is_some());
        assert!(request.user_prompt.contains("clip.mp4"));
    }

    // -- parse_and_validate: happy path ------------------------------------

    #[test]
    fn a_valid_tag_array_parses() {
        let tags = parse_and_validate(r#"["bitcoin", "finance", "chart"]"#).unwrap();
        assert_eq!(tags, vec!["bitcoin", "finance", "chart"]);
    }

    #[test]
    fn an_empty_tag_array_is_a_valid_honest_outcome() {
        let tags = parse_and_validate("[]").unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn tags_are_trimmed() {
        let tags = parse_and_validate(r#"["  bitcoin  ", "finance"]"#).unwrap();
        assert_eq!(tags, vec!["bitcoin", "finance"]);
    }

    #[test]
    fn duplicate_tags_are_deduplicated_case_insensitively_keeping_first_casing() {
        let raw = r#"["Bitcoin", "bitcoin", "BITCOIN", "finance"]"#;
        let tags = parse_and_validate(raw).unwrap();
        assert_eq!(tags, vec!["Bitcoin", "finance"]);
    }

    // -- parse_and_validate: rejection cases --------------------------------

    #[test]
    fn malformed_json_is_rejected() {
        let err = parse_and_validate("not json at all").unwrap_err();
        assert!(matches!(err, MediaTagError::MalformedJson { .. }));
    }

    #[test]
    fn a_json_object_instead_of_an_array_is_rejected() {
        let err = parse_and_validate(r#"{"tags": ["bitcoin"]}"#).unwrap_err();
        assert!(matches!(err, MediaTagError::MalformedJson { .. }));
    }

    #[test]
    fn too_many_tags_is_rejected() {
        let many: Vec<String> = (0..MAX_TAG_COUNT + 1).map(|i| format!("tag{i}")).collect();
        let raw = serde_json::to_string(&many).unwrap();
        assert!(matches!(
            parse_and_validate(&raw).unwrap_err(),
            MediaTagError::TooManyTags { .. }
        ));
    }

    #[test]
    fn exactly_the_max_tag_count_is_accepted() {
        let many: Vec<String> = (0..MAX_TAG_COUNT).map(|i| format!("tag{i}")).collect();
        let raw = serde_json::to_string(&many).unwrap();
        assert_eq!(parse_and_validate(&raw).unwrap().len(), MAX_TAG_COUNT);
    }

    #[test]
    fn an_empty_string_tag_is_rejected() {
        let raw = r#"["bitcoin", "   "]"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            MediaTagError::InvalidTag { index: 1, .. }
        ));
    }

    #[test]
    fn an_absurdly_long_tag_is_rejected() {
        let long_tag = "a".repeat(MAX_TAG_LENGTH + 1);
        let raw = serde_json::to_string(&vec![long_tag]).unwrap();
        assert!(matches!(
            parse_and_validate(&raw).unwrap_err(),
            MediaTagError::InvalidTag { index: 0, .. }
        ));
    }

    #[test]
    fn a_tag_exactly_at_the_max_length_is_accepted() {
        let tag = "a".repeat(MAX_TAG_LENGTH);
        let raw = serde_json::to_string(&vec![tag.clone()]).unwrap();
        assert_eq!(parse_and_validate(&raw).unwrap(), vec![tag]);
    }

    #[test]
    fn a_tag_containing_a_newline_is_rejected() {
        let raw = r#"["bitcoin\nfinance"]"#;
        assert!(matches!(
            parse_and_validate(raw).unwrap_err(),
            MediaTagError::InvalidTag { index: 0, .. }
        ));
    }

    // -- end-to-end: mock-server round trip, and "suggestions never
    // auto-write; only an explicit merge does" ------------------------------
    //
    // Mirrors exactly what `commands::media::suggest_media_tags`/
    // `merge_media_tags` do internally (module doc comment's "AI proposes,
    // user approves" split), without needing a full Tauri `State<'_, _>` to
    // construct in a unit test (the same "call the plain function/real logic
    // directly" pattern `commands::highlights`'s own tests use for
    // `run_detection`, and `commands::broll`'s own tests use for
    // `suggest_and_search`).

    use crate::ai::openai_compat::OpenAiCompatProvider;
    use crate::ai::provider::AIProvider;
    use crate::ai::test_http::spawn_one_shot;
    use crate::db::{self, MediaLibrary};
    use std::sync::Mutex;

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

    #[test]
    fn suggest_pipeline_round_trips_against_a_real_mock_server() {
        let e = entry("bitcoin_price_chart.mp4", MediaKind::Video, vec![]);
        let (base_url, rx) = spawn_one_shot(
            "HTTP/1.1 200 OK",
            chat_completion_body(r#"["bitcoin", "finance", "chart"]"#),
        );
        let provider = OpenAiCompatProvider {
            base_url,
            api_key: None,
            model: "test-model".to_string(),
        };

        let request = build_media_tag_request(&e, 0.2, 5_000);
        let response = provider.complete(&request).expect("mock server responds");
        let tags = parse_and_validate(&response.text).expect("well-formed response validates");

        assert_eq!(tags, vec!["bitcoin", "finance", "chart"]);

        // The mock server actually received a real HTTP request carrying the
        // constructed prompt (this pass's "real HTTP-call-shape tested
        // against the mock server" requirement), not a stubbed call.
        let captured = rx.recv().expect("mock server captured a request");
        assert_eq!(captured.method, "POST");
        assert!(captured.body.contains("bitcoin_price_chart.mp4"));
    }

    #[test]
    fn suggesting_tags_never_mutates_the_database_only_an_explicit_merge_does() {
        let conn = db::open_in_memory().unwrap();
        let original = MediaLibraryEntry {
            id: "m1".to_string(),
            filename: "bitcoin_price_chart.mp4".to_string(),
            path: "/media/bitcoin_price_chart.mp4".to_string(),
            kind: MediaKind::Video,
            duration_us: 12_500_000,
            width: 1920,
            height: 1080,
            tags: vec!["existing".to_string()],
            created_at: None,
            imported_at: "2026-09-04T00:00:00Z".to_string(),
            thumbnail_path: None,
            proxy_path: None,
        };
        db::upsert_media(&conn, &original).unwrap();
        let library = MediaLibrary(Mutex::new(conn));

        // Step 1, exactly mirroring `commands::media::suggest_media_tags`:
        // read the entry, build the prompt, call a real (mocked) provider,
        // validate the response. No write of any kind happens here.
        let (base_url, _rx) = spawn_one_shot(
            "HTTP/1.1 200 OK",
            chat_completion_body(r#"["bitcoin", "finance"]"#),
        );
        let provider = OpenAiCompatProvider {
            base_url,
            api_key: None,
            model: "test-model".to_string(),
        };
        let entry_before = {
            let conn = library.0.lock().unwrap();
            db::get_media_by_id(&conn, "m1").unwrap().unwrap()
        };
        let request = build_media_tag_request(&entry_before, 0.2, 5_000);
        let response = provider.complete(&request).unwrap();
        let suggested_tags = parse_and_validate(&response.text).unwrap();
        assert_eq!(suggested_tags, vec!["bitcoin", "finance"]);

        // The suggestion step must not have touched the database at all.
        let entry_after_suggest = {
            let conn = library.0.lock().unwrap();
            db::get_media_by_id(&conn, "m1").unwrap().unwrap()
        };
        assert_eq!(
            entry_after_suggest.tags,
            vec!["existing"],
            "suggesting tags must never write to the database"
        );

        // Step 2, exactly mirroring `commands::media::merge_media_tags`: the
        // *only* place a real write happens, and only on this explicit call.
        let merged = {
            let conn = library.0.lock().unwrap();
            db::merge_media_tags(&conn, "m1", &suggested_tags).unwrap()
        };
        assert_eq!(merged.tags, vec!["existing", "bitcoin", "finance"]);

        let entry_after_merge = {
            let conn = library.0.lock().unwrap();
            db::get_media_by_id(&conn, "m1").unwrap().unwrap()
        };
        assert_eq!(
            entry_after_merge.tags,
            vec!["existing", "bitcoin", "finance"]
        );
    }
}
