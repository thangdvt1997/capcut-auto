//! AI Auto Template (upgrade spec §7, `UPGRADE_PLAN.md` Phase U2): given one
//! video's real signals — duration/aspect (`media::probe::ProbedMedia`,
//! Phase 3), speech (a caller-supplied transcript, same "caller passes the
//! transcript in directly" convention `commands::highlights`/`commands::shorts`
//! already use), scenes (`media::scene::detect_scene_changes`, Phase 10/11),
//! and important segments (`crate::highlights`, Phase 10) — asks the
//! configured `AIProvider` to recommend one template from the real catalog
//! (`templates::all_templates()` + `templates::io::list_custom_templates`),
//! with a reason and a confidence.
//!
//! Same overall shape as `ai::smart_edit`/`ai::edit_plan`/`ai::media_tags`:
//! [`build_auto_template_prompt`] is a real, pure, testable prompt-building
//! function; a caller (`commands::auto_template::suggest_template_for_media`)
//! sends the resulting `AiRequest` through `AIProvider::complete`;
//! [`parse_and_validate`] is the *only* way that raw response text becomes an
//! [`AiTemplateRecommendation`] this app will show a user — never a
//! partially-populated result, and never auto-applied (upgrade spec §7's own
//! "Accept / Change Template / Customize / Run" flow is a later, explicit
//! step this module has no part in).
//!
//! ## Why `parse_and_validate` needs a real catalog, not just a schema
//!
//! Every other closed-schema AI validator in this crate
//! (`edit_plan::parse_and_validate`, `smart_edit::parse_and_validate`,
//! `media_tags::parse_and_validate`) is a pure function of its `raw` string
//! alone — every field it checks is bounded by the schema itself (an enum
//! variant, a numeric range). A `template_id` is different: the schema alone
//! can't tell a real id from a hallucinated one, because the set of valid ids
//! is a *runtime* catalog (built-ins plus whatever custom templates this
//! install currently has saved to disk), not a fixed enum. So
//! [`parse_and_validate`] additionally takes `known_templates: &[Template]` —
//! the caller (which has an `AppHandle` and can read
//! `templates::io::list_custom_templates`) is responsible for loading that
//! catalog fresh and passing it in. The function itself stays pure and
//! directly unit-testable against a synthetic in-test catalog fixture; only
//! its *caller* touches the filesystem.
//!
//! `template_name` on the returned [`AiTemplateRecommendation`] is always the
//! resolved catalog entry's own real name, never whatever string (if any) the
//! model put in its response — this guarantees `template_id`/`template_name`
//! can never disagree with each other in a result this app hands to a user.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::highlights::Highlight;
use crate::media::probe::ProbedMedia;
use crate::project::{CanvasRatioPreset, TranscriptEntry};
use crate::shorts::settings::ShortsAspect;
use crate::templates::Template;

use super::error::AutoTemplateError;
use super::provider::AiRequest;

/// The only schema version this module understands today — same
/// "exact match, no migration logic" convention as
/// `ai::edit_plan::CURRENT_VERSION`/`ai::smart_edit::CURRENT_VERSION`.
pub const CURRENT_VERSION: u32 = 1;

/// How many of the caller's real highlights get folded into the prompt —
/// generous enough to give the model a genuine sense of "where the good
/// parts are" without dumping an unbounded list into the prompt for a
/// long-form source.
const MAX_HIGHLIGHTS_IN_PROMPT: usize = 5;

/// A soft cap on how much raw transcript text gets excerpted into the
/// prompt (module doc comment: this is a *summary/excerpt*, not the full
/// transcript verbatim — an hour-long transcript would otherwise dominate
/// the prompt's token budget for no proportional benefit to a template
/// *recommendation*, which cares about genre/tone more than exact wording).
const TRANSCRIPT_EXCERPT_CHAR_BUDGET: usize = 2_000;

/// One AI-recommended template (upgrade spec §7's own worked example:
/// "Detected: Football Highlight. Recommended: Football Short V3."). Closed,
/// strictly typed, specta-typed for eventual frontend consumption — a
/// *proposal* the user Accepts/Changes/Customizes (module doc comment),
/// never applied by this module itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AiTemplateRecommendation {
    /// Always a real id from the catalog passed to [`parse_and_validate`] —
    /// never an unvalidated string straight from the model.
    pub template_id: String,
    /// Always the resolved catalog entry's own real name (module doc
    /// comment) — never re-derived from the model's own response text.
    pub template_name: String,
    pub reason: String,
    pub confidence: f32,
    /// Upgrade spec §7's own example output includes an aspect
    /// ("Output: 9:16") that may differ from the recommended template's own
    /// default `canvas` (e.g. the Football Highlight built-in defaults to
    /// 16:9 but is "also usable at 9:16", per that template's own doc
    /// comment) — `None` when the model has no opinion beyond the
    /// template's own default canvas. Reuses `shorts::settings::ShortsAspect`
    /// (the one real aspect-ratio enum this codebase already has for
    /// exactly this "which of a small closed set of aspects" question)
    /// rather than inventing a second one.
    pub suggested_aspect: Option<ShortsAspect>,
}

/// The wire-format shape an `AIProvider`'s raw response text must parse
/// as. `template_name` is deliberately NOT part of this wire struct (module
/// doc comment) — only `template_id` is trusted from the model; the real
/// name is filled in from the resolved catalog entry in
/// [`parse_and_validate`].
#[derive(Debug, Clone, Deserialize)]
struct AutoTemplateResponseWire {
    version: u32,
    template_id: String,
    reason: String,
    confidence: f32,
    #[serde(default)]
    suggested_aspect: Option<ShortsAspect>,
}

/// Parses `raw` (whatever text an `AIProvider::complete` returned) as JSON
/// and validates it into a real, catalog-backed [`AiTemplateRecommendation`],
/// or a specific `AutoTemplateError` and *nothing* else — mirroring
/// `ai::smart_edit::parse_and_validate`'s "never a partially-populated or
/// partially-validated result" discipline exactly.
///
/// Validation, in order:
/// 1. `raw` must parse as valid JSON matching [`AutoTemplateResponseWire`]'s
///    shape at all (`AutoTemplateError::MalformedJson`) — this also rejects a
///    `suggested_aspect` naming anything outside `ShortsAspect`'s closed
///    enum, since serde fails the whole document rather than partially
///    accepting one.
/// 2. `version` must equal [`CURRENT_VERSION`] exactly
///    (`AutoTemplateError::UnsupportedVersion`).
/// 3. `confidence` must be within `0.0..=1.0`
///    (`AutoTemplateError::InvalidConfidence`).
/// 4. `template_id` must name a real entry in `known_templates` (module doc
///    comment — `AutoTemplateError::UnknownTemplateId`), which the caller is
///    responsible for loading fresh from
///    `templates::all_templates()` + `templates::io::list_custom_templates`
///    before calling this.
pub fn parse_and_validate(
    raw: &str,
    known_templates: &[Template],
) -> Result<AiTemplateRecommendation, AutoTemplateError> {
    let parsed: AutoTemplateResponseWire =
        serde_json::from_str(raw).map_err(|e| AutoTemplateError::MalformedJson {
            details: e.to_string(),
        })?;

    if parsed.version != CURRENT_VERSION {
        return Err(AutoTemplateError::UnsupportedVersion {
            version: parsed.version,
        });
    }

    if !(0.0..=1.0).contains(&parsed.confidence) {
        return Err(AutoTemplateError::InvalidConfidence {
            confidence: parsed.confidence,
        });
    }

    let matched = known_templates
        .iter()
        .find(|t| t.id == parsed.template_id)
        .ok_or_else(|| AutoTemplateError::UnknownTemplateId {
            template_id: parsed.template_id.clone(),
        })?;

    Ok(AiTemplateRecommendation {
        template_id: matched.id.clone(),
        template_name: matched.name.clone(),
        reason: parsed.reason,
        confidence: parsed.confidence,
        suggested_aspect: parsed.suggested_aspect,
    })
}

// ---------------------------------------------------------------------------
// Prompt construction
// ---------------------------------------------------------------------------

fn ratio_preset_label(preset: CanvasRatioPreset) -> &'static str {
    match preset {
        CanvasRatioPreset::Ratio16x9 => "16:9",
        CanvasRatioPreset::Ratio9x16 => "9:16",
        CanvasRatioPreset::Ratio1x1 => "1:1",
        CanvasRatioPreset::Ratio4x5 => "4:5",
        CanvasRatioPreset::Custom => "custom",
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

/// A human-readable aspect label for the *source media*'s own probed
/// dimensions (unlike [`ratio_preset_label`], which is for a `Template`'s
/// already-named `CanvasRatioPreset`) — real media doesn't come with a named
/// preset attached, just pixels.
fn media_aspect_label(width: u32, height: u32) -> String {
    if width == 0 || height == 0 {
        return "unknown (no video stream)".to_string();
    }
    let divisor = gcd(width, height);
    format!(
        "{}x{} ({}:{})",
        width,
        height,
        width / divisor,
        height / divisor
    )
}

fn format_probed_media(media: &ProbedMedia) -> String {
    format!(
        "- Duration: {:.1}s\n- Resolution/aspect: {}\n- Frame rate: {}/{} fps\n- Has audio: {}\n- Has video: {}\n- Codec: {}\n",
        media.duration_us as f64 / 1_000_000.0,
        media_aspect_label(media.width, media.height),
        media.fps.num,
        media.fps.den,
        media.has_audio,
        media.has_video,
        media.codec,
    )
}

/// A compact excerpt (module doc comment: not the full transcript) of
/// `entries`' spoken text, plus the real total spoken duration — the two
/// transcript-derived facts a template recommendation actually benefits from
/// knowing.
fn format_transcript_summary(entries: &[TranscriptEntry]) -> String {
    if entries.is_empty() {
        return "(no transcript provided)\n".to_string();
    }

    let total_spoken_us: i64 = entries.iter().map(|e| (e.end_us - e.start_us).max(0)).sum();

    let mut excerpt = String::new();
    for entry in entries {
        if excerpt.len() >= TRANSCRIPT_EXCERPT_CHAR_BUDGET {
            excerpt.push_str(" ...");
            break;
        }
        if !excerpt.is_empty() {
            excerpt.push(' ');
        }
        excerpt.push_str(entry.text.trim());
    }

    format!(
        "- Entries: {}\n- Total spoken duration: {:.1}s\n- Excerpt: \"{}\"\n",
        entries.len(),
        total_spoken_us as f64 / 1_000_000.0,
        excerpt
    )
}

fn format_scene_summary(scene_cuts: &[i64]) -> String {
    if scene_cuts.is_empty() {
        return "- Scene cuts detected: 0 (no hard cuts found)\n".to_string();
    }
    format!(
        "- Scene cuts detected: {} (first cut at {:.1}s, last at {:.1}s)\n",
        scene_cuts.len(),
        scene_cuts[0] as f64 / 1_000_000.0,
        scene_cuts[scene_cuts.len() - 1] as f64 / 1_000_000.0
    )
}

fn format_highlights(highlights: &[Highlight]) -> String {
    if highlights.is_empty() {
        return "(no important segments detected)\n".to_string();
    }
    let mut out = String::new();
    for h in highlights.iter().take(MAX_HIGHLIGHTS_IN_PROMPT) {
        out.push_str(&format!(
            "- [{:.1}s -> {:.1}s] score={:.1} \"{}\" ({})\n",
            h.start_us as f64 / 1_000_000.0,
            h.end_us as f64 / 1_000_000.0,
            h.score,
            h.title,
            h.reason
        ));
    }
    out
}

fn format_catalog(catalog: &[Template]) -> String {
    let mut out = String::new();
    for t in catalog {
        out.push_str(&format!(
            "- id=\"{}\" name=\"{}\" canvas={}x{} ({}) — {}\n",
            t.id,
            t.name,
            t.canvas.width,
            t.canvas.height,
            ratio_preset_label(t.canvas.ratio_preset),
            t.description
        ));
    }
    out
}

/// Pure, testable string-building: given a video's real signals plus the
/// real template catalog, builds the user-prompt text an `AIProvider` should
/// receive to recommend exactly one template. Includes explicit schema/
/// format instructions (same discipline `ai::smart_edit::build_smart_edit_prompt`
/// already established — never hoping the model infers the shape).
pub fn build_auto_template_prompt(
    media: &ProbedMedia,
    transcript: &[TranscriptEntry],
    scene_cuts: &[i64],
    highlights: &[Highlight],
    catalog: &[Template],
) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are an expert video editor recommending an editing template (AUTO TEMPLATE mode).\n\
         Analyze the real signals below and recommend exactly one template from the catalog that \
         best fits this video's content.\n\n",
    );

    prompt.push_str("Video signals:\n");
    prompt.push_str(&format_probed_media(media));
    prompt.push('\n');

    prompt.push_str("Transcript / speech:\n");
    prompt.push_str(&format_transcript_summary(transcript));
    prompt.push('\n');

    prompt.push_str("Scene changes:\n");
    prompt.push_str(&format_scene_summary(scene_cuts));
    prompt.push('\n');

    prompt.push_str("Important segments (highlights):\n");
    prompt.push_str(&format_highlights(highlights));
    prompt.push('\n');

    prompt.push_str("Available templates (choose \"template_id\" from exactly these ids):\n");
    prompt.push_str(&format_catalog(catalog));

    prompt.push_str(&format!(
        "\nRespond with a single JSON object and nothing else, exactly matching this schema:\n\
         {{\n\
         \x20\x20\"version\": {CURRENT_VERSION},\n\
         \x20\x20\"template_id\": \"one of the ids listed above, exactly as written\",\n\
         \x20\x20\"reason\": \"a short human-readable reason grounded in the signals above\",\n\
         \x20\x20\"confidence\": 0.0,\n\
         \x20\x20\"suggested_aspect\": \"vertical9x16\" | \"square1x1\" | \"portrait4x5\" | null\n\
         }}\n\
         `confidence` must be within 0.0..=1.0. `template_id` must be exactly one of the ids \
         listed above — do not invent a new one. `suggested_aspect` is optional: only include it \
         if you recommend an aspect different from the chosen template's own default canvas, \
         otherwise use null."
    ));

    prompt
}

/// Builds the full `AiRequest` for an Auto Template recommendation call:
/// this module's own system prompt plus [`build_auto_template_prompt`]'s
/// user prompt, with caller-supplied `temperature`/`timeout_ms` (the same
/// per-call knobs every other `AIProvider` caller in this crate threads
/// through).
pub fn build_auto_template_request(
    media: &ProbedMedia,
    transcript: &[TranscriptEntry],
    scene_cuts: &[i64],
    highlights: &[Highlight],
    catalog: &[Template],
    temperature: f32,
    timeout_ms: u64,
) -> AiRequest {
    AiRequest {
        system_prompt: Some(
            "You are an Auto Template assistant for a video editor. You only ever respond with \
             the exact JSON schema you are given — never prose, never markdown code fences, \
             never any other text."
                .to_string(),
        ),
        user_prompt: build_auto_template_prompt(media, transcript, scene_cuts, highlights, catalog),
        temperature,
        timeout_ms,
        max_tokens: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Rational;
    use crate::templates::all_templates;

    fn probed(width: u32, height: u32, duration_us: i64) -> ProbedMedia {
        ProbedMedia {
            duration_us,
            width,
            height,
            fps: Rational::new(30, 1),
            codec: "h264".to_string(),
            bitrate: 5_000_000,
            audio_channels: 2,
            sample_rate: 48_000,
            rotation_deg: 0,
            created_at: None,
            has_video: true,
            has_audio: true,
        }
    }

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

    fn highlight(id: &str, title: &str, start_us: i64, end_us: i64, score: f32) -> Highlight {
        Highlight {
            id: id.to_string(),
            start_us,
            end_us,
            score,
            title: title.to_string(),
            reason: "a real detected reason".to_string(),
        }
    }

    // -- prompt construction --------------------------------------------------

    #[test]
    fn prompt_contains_the_real_media_signals() {
        let media = probed(1080, 1920, 45_000_000);
        let prompt = build_auto_template_prompt(&media, &[], &[], &[], &all_templates());
        assert!(prompt.contains("45.0s"), "{prompt}");
        assert!(prompt.contains("1080x1920"), "{prompt}");
        assert!(prompt.contains("9:16"), "{prompt}");
    }

    #[test]
    fn prompt_contains_the_transcript_excerpt_and_entry_count() {
        let media = probed(1920, 1080, 60_000_000);
        let entries = [
            entry("e1", "welcome to the match analysis", 0, 2_000_000),
            entry("e2", "what a goal that was", 2_000_000, 4_000_000),
        ];
        let prompt = build_auto_template_prompt(&media, &entries, &[], &[], &all_templates());
        assert!(prompt.contains("welcome to the match analysis"), "{prompt}");
        assert!(prompt.contains("what a goal that was"), "{prompt}");
        assert!(prompt.contains("Entries: 2"), "{prompt}");
    }

    #[test]
    fn prompt_contains_the_scene_cut_count() {
        let media = probed(1920, 1080, 60_000_000);
        let scene_cuts = [1_000_000_i64, 5_000_000, 12_000_000];
        let prompt = build_auto_template_prompt(&media, &[], &scene_cuts, &[], &all_templates());
        assert!(prompt.contains("Scene cuts detected: 3"), "{prompt}");
    }

    #[test]
    fn prompt_contains_the_top_highlight_titles_and_scores() {
        let media = probed(1920, 1080, 60_000_000);
        let highlights = [
            highlight("h1", "The winning goal", 80_000_000, 95_000_000, 92.0),
            highlight("h2", "Post-match interview", 200_000_000, 230_000_000, 61.0),
        ];
        let prompt = build_auto_template_prompt(&media, &[], &[], &highlights, &all_templates());
        assert!(prompt.contains("The winning goal"), "{prompt}");
        assert!(prompt.contains("Post-match interview"), "{prompt}");
        assert!(prompt.contains("score=92.0"), "{prompt}");
    }

    #[test]
    fn prompt_contains_the_full_real_template_catalog() {
        let media = probed(1920, 1080, 60_000_000);
        let catalog = all_templates();
        let prompt = build_auto_template_prompt(&media, &[], &[], &[], &catalog);
        for t in &catalog {
            assert!(
                prompt.contains(&format!("id=\"{}\"", t.id)),
                "expected prompt to list catalog entry {:?}\n{prompt}",
                t.id
            );
            assert!(prompt.contains(&t.name), "{prompt}");
        }
    }

    #[test]
    fn prompt_on_empty_signals_still_builds_a_schema_prompt() {
        let media = probed(0, 0, 0);
        let prompt = build_auto_template_prompt(&media, &[], &[], &[], &[]);
        assert!(prompt.contains("no transcript provided"), "{prompt}");
        assert!(prompt.contains("no important segments"), "{prompt}");
        assert!(prompt.contains("\"version\""), "{prompt}");
    }

    #[test]
    fn build_auto_template_request_threads_temperature_and_timeout() {
        let media = probed(1920, 1080, 10_000_000);
        let request =
            build_auto_template_request(&media, &[], &[], &[], &all_templates(), 0.4, 9_999);
        assert_eq!(request.temperature, 0.4);
        assert_eq!(request.timeout_ms, 9_999);
        assert!(request.system_prompt.is_some());
        assert!(request.user_prompt.contains("tmpl_tiktok"));
    }

    // -- parse_and_validate: happy path ---------------------------------------

    fn wire_json(template_id: &str, confidence: f32, aspect: Option<&str>) -> String {
        let aspect_json = match aspect {
            Some(a) => format!("\"{a}\""),
            None => "null".to_string(),
        };
        format!(
            r#"{{"version": 1, "template_id": "{template_id}", "reason": "fits the fast-paced content", "confidence": {confidence}, "suggested_aspect": {aspect_json}}}"#
        )
    }

    #[test]
    fn a_valid_response_resolves_to_the_real_catalog_entrys_own_name() {
        let raw = wire_json("tmpl_tiktok", 0.85, None);
        let rec = parse_and_validate(&raw, &all_templates()).expect("valid response parses");
        assert_eq!(rec.template_id, "tmpl_tiktok");
        assert_eq!(rec.template_name, "TikTok");
        assert_eq!(rec.reason, "fits the fast-paced content");
        assert_eq!(rec.confidence, 0.85);
        assert!(rec.suggested_aspect.is_none());
    }

    #[test]
    fn a_valid_response_with_a_suggested_aspect_round_trips() {
        let raw = wire_json("tmpl_football_highlight", 0.7, Some("vertical9x16"));
        let rec = parse_and_validate(&raw, &all_templates()).expect("valid response parses");
        assert_eq!(rec.template_id, "tmpl_football_highlight");
        assert_eq!(rec.suggested_aspect, Some(ShortsAspect::Vertical9x16));
    }

    #[test]
    fn template_name_always_comes_from_the_catalog_never_the_raw_response() {
        // The wire schema has no template_name field at all — this proves it
        // structurally, not just by omission in a hand-written fixture.
        let raw = wire_json("tmpl_news", 0.5, None);
        assert!(!raw.contains("template_name"));
        let rec = parse_and_validate(&raw, &all_templates()).unwrap();
        assert_eq!(rec.template_name, "News");
    }

    #[test]
    fn resolves_against_a_synthetic_in_test_catalog_fixture() {
        // Confirms `known_templates` is genuinely just data, not tied to
        // `all_templates()` specifically — a caller's real custom-template
        // catalog works identically.
        let mut custom = all_templates().remove(2); // tiktok
        custom.id = "custom_my_template".to_string();
        custom.is_built_in = false;
        custom.name = "My Custom Template".to_string();
        let catalog = vec![custom];

        let raw = wire_json("custom_my_template", 0.9, None);
        let rec = parse_and_validate(&raw, &catalog).expect("resolves against custom catalog");
        assert_eq!(rec.template_id, "custom_my_template");
        assert_eq!(rec.template_name, "My Custom Template");
    }

    // -- parse_and_validate: rejection cases ----------------------------------

    #[test]
    fn malformed_json_is_rejected() {
        let err = parse_and_validate("not json at all", &all_templates()).unwrap_err();
        assert!(matches!(err, AutoTemplateError::MalformedJson { .. }));
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let raw = r#"{"version": 2, "template_id": "tmpl_tiktok", "reason": "x", "confidence": 0.5, "suggested_aspect": null}"#;
        assert!(matches!(
            parse_and_validate(raw, &all_templates()).unwrap_err(),
            AutoTemplateError::UnsupportedVersion { version: 2 }
        ));
    }

    #[test]
    fn an_unknown_template_id_is_rejected() {
        let raw = wire_json("tmpl_does_not_exist", 0.5, None);
        assert!(matches!(
            parse_and_validate(&raw, &all_templates()).unwrap_err(),
            AutoTemplateError::UnknownTemplateId { template_id } if template_id == "tmpl_does_not_exist"
        ));
    }

    #[test]
    fn an_unknown_template_id_against_an_empty_catalog_is_rejected() {
        let raw = wire_json("tmpl_tiktok", 0.5, None);
        assert!(matches!(
            parse_and_validate(&raw, &[]).unwrap_err(),
            AutoTemplateError::UnknownTemplateId { .. }
        ));
    }

    #[test]
    fn out_of_range_confidence_is_rejected() {
        let raw = wire_json("tmpl_tiktok", 1.5, None);
        assert!(matches!(
            parse_and_validate(&raw, &all_templates()).unwrap_err(),
            AutoTemplateError::InvalidConfidence { .. }
        ));
    }

    #[test]
    fn negative_confidence_is_rejected() {
        let raw = wire_json("tmpl_tiktok", -0.1, None);
        assert!(matches!(
            parse_and_validate(&raw, &all_templates()).unwrap_err(),
            AutoTemplateError::InvalidConfidence { .. }
        ));
    }

    #[test]
    fn an_unknown_aspect_tag_fails_to_deserialize_as_malformed_json() {
        let raw = r#"{"version": 1, "template_id": "tmpl_tiktok", "reason": "x", "confidence": 0.5, "suggested_aspect": "widescreen_ultrawide"}"#;
        assert!(matches!(
            parse_and_validate(raw, &all_templates()).unwrap_err(),
            AutoTemplateError::MalformedJson { .. }
        ));
    }

    #[test]
    fn a_missing_suggested_aspect_field_defaults_to_none() {
        let raw =
            r#"{"version": 1, "template_id": "tmpl_tiktok", "reason": "x", "confidence": 0.5}"#;
        let rec = parse_and_validate(raw, &all_templates()).expect("missing field defaults");
        assert!(rec.suggested_aspect.is_none());
    }
}
