//! AI Template Generator (upgrade spec §8): given a natural-language prompt
//! (§8's worked example — a Vietnamese description of a 30-45s 9:16 football
//! TikTok template with large subtitles, a top-right logo, a fast 2s intro,
//! and quick transitions), ask the configured AI provider to produce a
//! structured, schema-validated `Template` — never a raw/uncontrolled value,
//! same "AI proposes -> schema validation -> preview -> save" discipline as
//! `ai::edit_plan`/`ai::smart_edit`/`ai::nl_command`.
//!
//! ## Option A vs. Option B (documented per this pass's own brief)
//!
//! This module implements **Option A**, the safer of the two shapes this
//! pass's brief lays out: the AI does not invent a brand-new nested
//! `CaptionStyle`/`RenderSettings`-level object from scratch. It picks
//! **existing, real catalog entries by id** — one of `captions::styles::
//! all_caption_templates()`'s 6 built-in caption styles, one of
//! `render::presets::all_presets()`'s real export presets, and (optionally)
//! an existing `Asset` this user has already registered in their Asset
//! Library — plus a closed set of **scalar/enum choices** this codebase
//! already has real, working, already-scalar/enum types for:
//! `zoom::ZoomIntensity`, `vad::CutParams` (three `i64` microsecond fields),
//! `templates::TransitionSettings` (a closed `TransitionType` enum + one
//! `i64` duration), a closed `GeneratedCanvasAspect` enum (below — a strict
//! *subset* of `project::CanvasRatioPreset` that excludes `Custom`, so the
//! model can never hand back an arbitrary, unvalidated width/height pair),
//! `templates::{AssetReference, WatermarkReference, BackgroundMusicReference}`,
//! and `templates::SportsOverlaySettings`. [`GeneratedTemplateSpec`] bundles
//! exactly these fields; [`parse_and_validate`] is the only way raw AI text
//! becomes a real, fully-formed `Template` — it resolves every referenced id
//! against the real catalogs server-side, deterministically, so there is no
//! AI-authored nested object (a font size, a safe margin, a render bitrate)
//! that could ever reach the render pipeline unvalidated.
//!
//! Option B (letting the AI also invent brand-new nested values — a custom
//! caption style not matching one of the 6 built-ins, say) was considered
//! and rejected for this pass: it would need its own tight per-field bounds
//! validation duplicating `captions::styles`/`render::presets`' own
//! judgment calls (safe margins, contrast, bitrate/CRF sanity) for no clear
//! benefit over "pick the closest existing built-in, then let the user
//! tweak it after Preview" — matching how `templates::save_as_template_from_project`
//! itself already works for a human building a template by hand. If a
//! future pass wants Option B, it should extend this schema additively
//! (e.g. an optional caption-style *override* subset), not replace it.
//!
//! ## Where this fits in the pipeline (upgrade spec §8's own diagram)
//!
//! *Natural language prompt -> [`build_generate_template_prompt`] -> AI
//! provider -> [`parse_and_validate`] -> a real, ready-to-preview `Template`
//! (`commands::ai::generate_template_from_prompt`) -> Preview (frontend, a
//! later pass) -> Save Template.* This module only covers "Generate ->
//! Validate"; it never saves anything to disk itself — see
//! `commands::ai::generate_template_from_prompt`'s own doc comment for why
//! the existing `commands::templates::save_as_template` command is *not* a
//! clean fit for the final Save step (it demands a real `ProjectV1` to read
//! `canvas` from, which a from-scratch AI-generated template has none of),
//! and what the honest gap is instead.
//!
//! ## Security note (master prompt §53, same discipline as every AI pipeline
//! in this crate)
//!
//! [`GeneratedTemplateSpec`]'s fields are all either closed enums, `i64`/`f64`
//! scalars (every one range-validated below), or a plain `String` id that
//! [`parse_and_validate`] resolves against a real, caller-supplied catalog
//! before it is ever trusted — an unknown id is rejected, never silently
//! accepted as an opaque reference. There is no field here an AI response
//! could use to smuggle a path, a shell fragment, or an unbounded numeric
//! value into the render pipeline.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ai::provider::AiRequest;
use crate::ai::smart_edit::SmartEditCategory;
use crate::assets::Asset;
use crate::captions::styles;
use crate::project::{CanvasRatioPreset, CanvasV1, Rational};
use crate::render::{self, RenderPreset};
use crate::templates::{
    self, AiPromptConfig, AssetReference, BackgroundMusicReference, SportsOverlaySettings,
    Template, TransitionSettings, TransitionType, WatermarkReference,
};
use crate::vad::CutParams;
use crate::zoom::ZoomIntensity;

use super::error::TemplateGeneratorError;

/// The only schema version this module understands today — same "exact
/// match, no migration logic" convention as `ai::edit_plan::CURRENT_VERSION`/
/// `ai::smart_edit::CURRENT_VERSION`.
pub const CURRENT_VERSION: u32 = 1;

/// Every numeric `_us` field below (padding/merge/transition duration/
/// ducking attack-release) is bounded to this many microseconds (5 seconds)
/// — generous relative to every real built-in template's own values
/// (`templates::mod`'s largest is News' 800ms `merge_gap_us`), but still a
/// real, finite bound rather than trusting an AI-authored `i64` unbounded.
const MAX_DURATION_US: i64 = 5_000_000;

/// Linear-gain bound for `background_music.volume` (upgrade spec §3's own
/// example uses `0.2`) — generous enough to allow a deliberate boost above
/// unity gain, never unbounded.
const MAX_VOLUME: f64 = 4.0;

/// Bound for `system_prompt_prefix`'s length — a real string this module
/// stores and a later caller prepends ahead of another AI prompt
/// (`AiPromptConfig`'s own doc comment), never executed as code, but still
/// capped so an AI response can't hand back an unbounded blob.
const MAX_SYSTEM_PROMPT_PREFIX_CHARS: usize = 2_000;

/// A strict *subset* of `project::CanvasRatioPreset` — every named preset
/// except `Custom` (module doc comment: a `Custom` canvas needs an
/// AI-invented width/height pair this pass deliberately never trusts).
/// `#[serde(rename = ...)]` per variant mirrors `CanvasRatioPreset`'s own
/// wire format exactly, so the model is asked for the same familiar
/// "16:9"/"9:16"/"1:1"/"4:5" strings this app already uses elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum GeneratedCanvasAspect {
    #[serde(rename = "16:9")]
    Ratio16x9,
    #[serde(rename = "9:16")]
    Ratio9x16,
    #[serde(rename = "1:1")]
    Ratio1x1,
    #[serde(rename = "4:5")]
    Ratio4x5,
}

impl GeneratedCanvasAspect {
    /// A concrete, deterministic canvas for this aspect — the same
    /// 1080-scaled convention `shorts::settings::ShortsAspect::canvas_dimensions`
    /// and `render::presets`' named presets already use, at a fixed 30fps
    /// (`templates::canvas_9x16`'s own convention), never a value the AI
    /// response itself supplies.
    pub fn to_canvas(self) -> CanvasV1 {
        let (width, height, ratio_preset) = match self {
            GeneratedCanvasAspect::Ratio16x9 => (1920, 1080, CanvasRatioPreset::Ratio16x9),
            GeneratedCanvasAspect::Ratio9x16 => (1080, 1920, CanvasRatioPreset::Ratio9x16),
            GeneratedCanvasAspect::Ratio1x1 => (1080, 1080, CanvasRatioPreset::Ratio1x1),
            GeneratedCanvasAspect::Ratio4x5 => (1080, 1350, CanvasRatioPreset::Ratio4x5),
        };
        CanvasV1 {
            width,
            height,
            fps: Rational::new(30, 1),
            ratio_preset,
        }
    }
}

/// The closed intermediate schema an `AIProvider` must answer with (module
/// doc comment, "Option A"). Every field is either a closed enum, a
/// range-validated scalar, or a `String` id resolved against a real catalog
/// by [`parse_and_validate`] — never a free-form nested object.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GeneratedTemplateSpec {
    pub version: u32,
    pub name: String,
    pub description: String,
    pub canvas_aspect: GeneratedCanvasAspect,
    /// Must be one of `captions::styles::all_caption_templates()`'s 6 stable
    /// built-in ids (e.g. `"template_tiktok"`) — never a caller-invented
    /// style.
    pub caption_style_id: String,
    pub zoom_intensity: ZoomIntensity,
    pub silence_settings: CutParams,
    pub transition_settings: TransitionSettings,
    /// Must be one of `render::presets::all_presets()`'s stable ids (e.g.
    /// `"tiktok_1080x1920"`).
    pub export_preset_id: String,
    pub emphasized_categories: Vec<SmartEditCategory>,
    pub system_prompt_prefix: Option<String>,
    pub sports_overlay: Option<SportsOverlaySettings>,
    pub intro: Option<AssetReference>,
    pub outro: Option<AssetReference>,
    pub watermark: Option<WatermarkReference>,
    pub background_music: Option<BackgroundMusicReference>,
}

/// A constructed two-part prompt, ready to become a real `AiRequest` once a
/// caller supplies the provider-call knobs (`ai::nl_command::EditPlanPrompt`'s
/// own precedent for this exact shape).
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateTemplatePrompt {
    pub system_prompt: String,
    pub user_prompt: String,
}

impl GenerateTemplatePrompt {
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

/// The exact `GeneratedTemplateSpec` JSON schema, spelled out for the model —
/// same "include the schema/format instructions explicitly in the prompt"
/// discipline `ai::nl_command::EDIT_PLAN_SCHEMA_INSTRUCTIONS` already uses.
/// Kept as one constant so the system prompt and this module's own content
/// test can't silently drift apart.
const TEMPLATE_SPEC_SCHEMA_INSTRUCTIONS: &str = r#"Respond with ONLY a single JSON object (no markdown code fences, no commentary before or after) matching exactly this schema:

{
  "version": 1,
  "name": "<short template name>",
  "description": "<one or two sentence description>",
  "canvas_aspect": "16:9" | "9:16" | "1:1" | "4:5",
  "caption_style_id": "<one of the existing caption style ids listed below>",
  "zoom_intensity": "off" | "low" | "medium" | "high",
  "silence_settings": {
    "padding_before_us": <integer microseconds, 0 to 5000000>,
    "padding_after_us": <integer microseconds, 0 to 5000000>,
    "merge_gap_us": <integer microseconds, 0 to 5000000>
  },
  "transition_settings": {
    "transition_type": "cut" | "cross_fade",
    "duration_us": <integer microseconds; must be exactly 0 when transition_type is "cut", otherwise 1 to 5000000>
  },
  "export_preset_id": "<one of the existing export preset ids listed below>",
  "emphasized_categories": [zero or more of: "repetition", "false_start", "off_topic", "weak_sentence", "long_pause", "filler_word", "unnecessary_intro", "duplicate_idea", "boring_section"],
  "system_prompt_prefix": "<short optional string, or null>",
  "sports_overlay": null OR {
    "score_overlay_suggested": <true or false>,
    "music_role": "standard" | "music" | "voice",
    "music_ducking": {
      "duck_level": <float 0.0 to 1.0, linear gain>,
      "attack_us": <integer microseconds, 0 to 5000000>,
      "release_us": <integer microseconds, 0 to 5000000>
    }
  },
  "intro": null OR {"asset_id": "<an existing asset id listed below>"},
  "outro": null OR {"asset_id": "<an existing asset id listed below>"},
  "watermark": null OR {"asset_id": "<an existing asset id listed below>", "position": "top_left" | "top_right" | "bottom_left" | "bottom_right" | "center"},
  "background_music": null OR {"asset_id": "<an existing asset id listed below>", "volume": <float 0.0 to 4.0, linear gain, NOT a percentage or decibels>}
}

Rules:
- "version" must always be exactly 1.
- Only reference an id (caption style / export preset / asset) from the real lists given below — never invent one.
- Only use "intro"/"outro"/"watermark"/"background_music" if a suitable real asset is listed below; otherwise use null for each.
- Do not include any field not listed above."#;

fn format_catalog_lines<'a, T>(items: &'a [T], line: impl Fn(&'a T) -> String) -> String {
    if items.is_empty() {
        return "(none)".to_string();
    }
    items.iter().map(line).collect::<Vec<_>>().join("\n")
}

/// Builds the full grounding prompt for the AI Template Generator (upgrade
/// spec §8): a system prompt carrying the exact `GeneratedTemplateSpec`
/// schema (above), plus a user prompt carrying the user's real
/// natural-language request and the real, current catalogs of caption
/// styles / export presets / registered assets — so the model can reference
/// real ids instead of guessing at ones that don't exist (this pass's own
/// brief, point 2).
pub fn build_generate_template_prompt(
    nl_prompt: &str,
    caption_styles: &[crate::project::CaptionStyle],
    export_presets: &[RenderPreset],
    assets: &[Asset],
) -> GenerateTemplatePrompt {
    let system_prompt = format!(
        "You are a precise video-editing assistant embedded in a desktop video editor. \
         Your only job is to translate a user's natural-language description of a video \
         template into a structured GeneratedTemplateSpec. You never explain yourself, never \
         write prose, and never produce anything other than the JSON object described below.\n\n\
         {TEMPLATE_SPEC_SCHEMA_INSTRUCTIONS}"
    );

    let mut user_prompt = String::new();
    user_prompt.push_str(&format!("User's template request: \"{nl_prompt}\"\n\n"));

    user_prompt.push_str("Existing caption styles (id: name — description):\n");
    user_prompt.push_str(&format_catalog_lines(caption_styles, |s| {
        format!("- {}: {}", s.id, s.name)
    }));
    user_prompt.push_str("\n\n");

    user_prompt.push_str("Existing export presets (id: name — description):\n");
    user_prompt.push_str(&format_catalog_lines(export_presets, |p| {
        format!("- {}: {} — {}", p.id, p.name, p.description)
    }));
    user_prompt.push_str("\n\n");

    user_prompt.push_str("Existing registered assets (id: kind name):\n");
    user_prompt.push_str(&format_catalog_lines(assets, |a| {
        format!("- {}: {:?} {}", a.id, a.kind, a.name)
    }));
    user_prompt.push_str(
        "\n\nProduce the GeneratedTemplateSpec JSON for the user's template request above.",
    );

    GenerateTemplatePrompt {
        system_prompt,
        user_prompt,
    }
}

fn invalid(field: &str, details: impl Into<String>) -> TemplateGeneratorError {
    TemplateGeneratorError::InvalidField {
        field: field.to_string(),
        details: details.into(),
    }
}

fn validate_duration_field(field: &str, value_us: i64) -> Result<(), TemplateGeneratorError> {
    if !(0..=MAX_DURATION_US).contains(&value_us) {
        return Err(invalid(
            field,
            format!("must be within 0..={MAX_DURATION_US}, got {value_us}"),
        ));
    }
    Ok(())
}

fn validate_spec(spec: &GeneratedTemplateSpec) -> Result<(), TemplateGeneratorError> {
    if spec.name.trim().is_empty() {
        return Err(invalid("name", "must not be empty"));
    }
    if spec.description.trim().is_empty() {
        return Err(invalid("description", "must not be empty"));
    }

    validate_duration_field(
        "silence_settings.padding_before_us",
        spec.silence_settings.padding_before_us,
    )?;
    validate_duration_field(
        "silence_settings.padding_after_us",
        spec.silence_settings.padding_after_us,
    )?;
    validate_duration_field(
        "silence_settings.merge_gap_us",
        spec.silence_settings.merge_gap_us,
    )?;

    match spec.transition_settings.transition_type {
        TransitionType::Cut => {
            if spec.transition_settings.duration_us != 0 {
                return Err(invalid(
                    "transition_settings.duration_us",
                    format!(
                        "must be exactly 0 when transition_type is \"cut\", got {}",
                        spec.transition_settings.duration_us
                    ),
                ));
            }
        }
        TransitionType::CrossFade => {
            let duration_us = spec.transition_settings.duration_us;
            if !(1..=MAX_DURATION_US).contains(&duration_us) {
                return Err(invalid(
                    "transition_settings.duration_us",
                    format!(
                        "must be within 1..={MAX_DURATION_US} when transition_type is \"cross_fade\", got {duration_us}"
                    ),
                ));
            }
        }
    }

    if let Some(prefix) = &spec.system_prompt_prefix {
        if prefix.chars().count() > MAX_SYSTEM_PROMPT_PREFIX_CHARS {
            return Err(invalid(
                "system_prompt_prefix",
                format!(
                    "must be at most {MAX_SYSTEM_PROMPT_PREFIX_CHARS} characters, got {}",
                    prefix.chars().count()
                ),
            ));
        }
    }

    if let Some(overlay) = &spec.sports_overlay {
        if !(0.0..=1.0).contains(&overlay.music_ducking.duck_level) {
            return Err(invalid(
                "sports_overlay.music_ducking.duck_level",
                format!(
                    "must be within 0.0..=1.0, got {}",
                    overlay.music_ducking.duck_level
                ),
            ));
        }
        validate_duration_field(
            "sports_overlay.music_ducking.attack_us",
            overlay.music_ducking.attack_us,
        )?;
        validate_duration_field(
            "sports_overlay.music_ducking.release_us",
            overlay.music_ducking.release_us,
        )?;
    }

    if let Some(music) = &spec.background_music {
        if !music.volume.is_finite() || !(0.0..=MAX_VOLUME).contains(&music.volume) {
            return Err(invalid(
                "background_music.volume",
                format!(
                    "must be a finite number within 0.0..={MAX_VOLUME}, got {}",
                    music.volume
                ),
            ));
        }
    }

    Ok(())
}

/// Parses `raw` (whatever text an `AIProvider::complete` returned) as JSON,
/// validates it against the strict `GeneratedTemplateSpec` schema, resolves
/// every referenced id against the real catalogs, and returns a real,
/// ready-to-preview `Template` — `is_built_in: false`, a fresh
/// `custom_<uuid>` id, `version: 1` — or a specific `TemplateGeneratorError`
/// and *nothing* else (never a partially-populated `Template`), mirroring
/// `ai::edit_plan::parse_and_validate`/`ai::smart_edit::parse_and_validate`
/// exactly.
///
/// `known_asset_ids` is the caller's current Asset Library snapshot (the
/// same convention `templates::save_as_template_from_project`'s own
/// `known_asset_ids` parameter already uses) — checked via the *exact same*
/// `templates::validate_asset_references` function that command already
/// calls, not a reinvented parallel check.
///
/// Validation, in order:
/// 1. `raw` must parse as valid JSON matching `GeneratedTemplateSpec`'s shape
///    at all (`TemplateGeneratorError::MalformedJson`).
/// 2. `version` must equal [`CURRENT_VERSION`] exactly
///    (`TemplateGeneratorError::UnsupportedVersion`).
/// 3. `name`/`description` must not be empty; every duration field
///    (`silence_settings.*`, `transition_settings.duration_us`,
///    `sports_overlay.music_ducking.*`) must be within `0..=5_000_000`
///    microseconds (a `cross_fade` transition's duration must additionally
///    be `> 0`, a `cut`'s must be exactly `0`); `sports_overlay.music_ducking.duck_level`
///    must be within `0.0..=1.0`; `background_music.volume` must be a finite
///    number within `0.0..=4.0`; `system_prompt_prefix`, if present, must be
///    at most 2,000 characters (`TemplateGeneratorError::InvalidField`).
/// 4. `caption_style_id` must match a real `captions::styles::all_caption_templates()`
///    entry (`TemplateGeneratorError::UnknownCaptionStyle`).
/// 5. `export_preset_id` must match a real `render::presets::all_presets()`
///    entry (`TemplateGeneratorError::UnknownExportPreset`).
/// 6. Every `intro`/`outro`/`watermark`/`background_music` asset id, if
///    present, must exist in `known_asset_ids`
///    (`TemplateGeneratorError::UnknownAsset`).
pub fn parse_and_validate(
    raw: &str,
    known_asset_ids: &HashSet<String>,
) -> Result<Template, TemplateGeneratorError> {
    let spec: GeneratedTemplateSpec =
        serde_json::from_str(raw).map_err(|e| TemplateGeneratorError::MalformedJson {
            details: e.to_string(),
        })?;

    if spec.version != CURRENT_VERSION {
        return Err(TemplateGeneratorError::UnsupportedVersion {
            version: spec.version,
        });
    }

    validate_spec(&spec)?;

    let caption_style = styles::all_caption_templates()
        .into_iter()
        .find(|s| s.id == spec.caption_style_id)
        .ok_or_else(|| TemplateGeneratorError::UnknownCaptionStyle {
            style_id: spec.caption_style_id.clone(),
        })?;

    render::find_preset(&spec.export_preset_id).map_err(|_| {
        TemplateGeneratorError::UnknownExportPreset {
            preset_id: spec.export_preset_id.clone(),
        }
    })?;

    templates::validate_asset_references(
        spec.intro.as_ref(),
        spec.outro.as_ref(),
        spec.watermark.as_ref(),
        spec.background_music.as_ref(),
        known_asset_ids,
    )
    .map_err(|e| match e {
        templates::TemplateError::UnknownAsset { asset_id } => {
            TemplateGeneratorError::UnknownAsset { asset_id }
        }
        other => invalid("asset_reference", other.to_string()),
    })?;

    Ok(Template {
        id: format!("custom_{}", uuid::Uuid::new_v4()),
        name: spec.name,
        description: spec.description,
        is_built_in: false,
        canvas: spec.canvas_aspect.to_canvas(),
        caption_style,
        zoom_intensity: spec.zoom_intensity,
        silence_settings: spec.silence_settings,
        transition_settings: spec.transition_settings,
        export_preset_id: spec.export_preset_id,
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: spec.emphasized_categories,
            system_prompt_prefix: spec.system_prompt_prefix,
        },
        sports_overlay: spec.sports_overlay,
        intro: spec.intro,
        outro: spec.outro,
        watermark: spec.watermark,
        background_music: spec.background_music,
        version: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{AudioRole, CaptionStyle, DuckingSettings};

    fn caption_style_fixture(id: &str) -> CaptionStyle {
        styles::all_caption_templates()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap()
    }

    fn valid_spec_json() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "name": "Football TikTok",
            "description": "Fast-paced 9:16 football highlight template for TikTok.",
            "canvas_aspect": "9:16",
            "caption_style_id": "template_tiktok",
            "zoom_intensity": "high",
            "silence_settings": {
                "padding_before_us": 50_000,
                "padding_after_us": 50_000,
                "merge_gap_us": 100_000
            },
            "transition_settings": {
                "transition_type": "cross_fade",
                "duration_us": 100_000
            },
            "export_preset_id": "tiktok_1080x1920",
            "emphasized_categories": ["boring_section", "long_pause"],
            "system_prompt_prefix": "Emphasize goals and highlight player names.",
            "sports_overlay": {
                "score_overlay_suggested": true,
                "music_role": "music",
                "music_ducking": {
                    "duck_level": 0.25,
                    "attack_us": 150_000,
                    "release_us": 400_000
                }
            },
            "intro": null,
            "outro": null,
            "watermark": null,
            "background_music": null
        })
    }

    // -- build_generate_template_prompt --------------------------------------

    #[test]
    fn the_system_prompt_carries_the_exact_generated_template_spec_schema() {
        let prompt = build_generate_template_prompt("A football TikTok template.", &[], &[], &[]);
        assert!(prompt.system_prompt.contains("\"version\": 1"));
        assert!(prompt.system_prompt.contains("canvas_aspect"));
        assert!(prompt.system_prompt.contains("caption_style_id"));
        assert!(prompt.system_prompt.contains("export_preset_id"));
        assert!(prompt.system_prompt.contains("silence_settings"));
        assert!(prompt.system_prompt.contains("transition_settings"));
        assert!(prompt.system_prompt.contains("sports_overlay"));
        assert!(prompt.system_prompt.contains("background_music"));
    }

    #[test]
    fn the_user_prompt_contains_the_verbatim_nl_prompt() {
        let prompt = build_generate_template_prompt(
            "Video bóng đá TikTok 30-45s, 9:16, subtitle lớn.",
            &[],
            &[],
            &[],
        );
        assert!(prompt
            .user_prompt
            .contains("Video bóng đá TikTok 30-45s, 9:16, subtitle lớn."));
    }

    #[test]
    fn the_user_prompt_lists_every_real_caption_style_and_export_preset_id() {
        let caption_styles = styles::all_caption_templates();
        let export_presets = render::all_presets();
        let prompt =
            build_generate_template_prompt("A template.", &caption_styles, &export_presets, &[]);
        for style in &caption_styles {
            assert!(
                prompt.user_prompt.contains(&style.id),
                "missing caption style id {}",
                style.id
            );
        }
        for preset in &export_presets {
            assert!(
                prompt.user_prompt.contains(preset.id),
                "missing export preset id {}",
                preset.id
            );
        }
    }

    #[test]
    fn the_user_prompt_lists_real_registered_asset_ids() {
        let assets = vec![Asset {
            id: "asset_logo_1".to_string(),
            kind: crate::assets::AssetKind::Logo,
            name: "My Logo".to_string(),
            file_path: "/tmp/logo.png".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            tags: vec![],
        }];
        let prompt = build_generate_template_prompt("A template.", &[], &[], &assets);
        assert!(prompt.user_prompt.contains("asset_logo_1"));
        assert!(prompt.user_prompt.contains("My Logo"));
    }

    #[test]
    fn an_empty_catalog_still_produces_a_usable_prompt() {
        let prompt = build_generate_template_prompt("A template.", &[], &[], &[]);
        assert!(prompt.user_prompt.contains("(none)"));
        assert!(!prompt.system_prompt.is_empty());
    }

    #[test]
    fn into_request_carries_the_provider_call_parameters_through() {
        let prompt = build_generate_template_prompt("A template.", &[], &[], &[]);
        let request = prompt.into_request(0.3, 20_000, Some(2048));
        assert_eq!(request.temperature, 0.3);
        assert_eq!(request.timeout_ms, 20_000);
        assert_eq!(request.max_tokens, Some(2048));
        assert!(request.system_prompt.is_some());
    }

    // -- parse_and_validate: success ------------------------------------------

    #[test]
    fn a_well_formed_spec_produces_a_real_valid_template() {
        let raw = valid_spec_json().to_string();
        let template = parse_and_validate(&raw, &HashSet::new()).expect("should validate");

        assert!(!template.is_built_in);
        assert!(template.id.starts_with("custom_"));
        assert_eq!(template.name, "Football TikTok");
        assert_eq!(template.version, 1);
        assert_eq!(template.canvas.ratio_preset, CanvasRatioPreset::Ratio9x16);
        assert_eq!(
            (template.canvas.width, template.canvas.height),
            (1080, 1920)
        );
        assert_eq!(template.caption_style.id, "template_tiktok");
        assert_eq!(template.zoom_intensity, ZoomIntensity::High);
        assert_eq!(template.export_preset_id, "tiktok_1080x1920");
        assert_eq!(
            template.transition_settings.transition_type,
            TransitionType::CrossFade
        );
        assert_eq!(template.transition_settings.duration_us, 100_000);
        assert_eq!(
            template.ai_prompt_config.emphasized_categories,
            vec![
                SmartEditCategory::BoringSection,
                SmartEditCategory::LongPause
            ]
        );
        assert!(template.sports_overlay.is_some());
        assert!(template.intro.is_none());
        assert!(template.watermark.is_none());
    }

    #[test]
    fn a_spec_referencing_real_registered_assets_validates_and_carries_them_through() {
        let mut spec = valid_spec_json();
        spec["intro"] = serde_json::json!({"asset_id": "asset_intro_1"});
        spec["watermark"] =
            serde_json::json!({"asset_id": "asset_logo_1", "position": "bottom_right"});
        spec["background_music"] = serde_json::json!({"asset_id": "asset_music_1", "volume": 0.2});

        let known: HashSet<String> = ["asset_intro_1", "asset_logo_1", "asset_music_1"]
            .into_iter()
            .map(String::from)
            .collect();

        let template = parse_and_validate(&spec.to_string(), &known).expect("should validate");
        assert_eq!(template.intro.unwrap().asset_id, "asset_intro_1");
        assert_eq!(
            template.watermark.as_ref().unwrap().position,
            crate::templates::WatermarkPosition::BottomRight
        );
        assert_eq!(template.background_music.unwrap().volume, 0.2);
    }

    #[test]
    fn a_cut_transition_with_zero_duration_validates() {
        let mut spec = valid_spec_json();
        spec["transition_settings"] =
            serde_json::json!({"transition_type": "cut", "duration_us": 0});
        let template =
            parse_and_validate(&spec.to_string(), &HashSet::new()).expect("should validate");
        assert_eq!(
            template.transition_settings.transition_type,
            TransitionType::Cut
        );
    }

    // -- parse_and_validate: rejection cases ----------------------------------

    #[test]
    fn malformed_json_is_rejected() {
        let err = parse_and_validate("not json at all", &HashSet::new()).unwrap_err();
        assert!(matches!(err, TemplateGeneratorError::MalformedJson { .. }));
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let mut spec = valid_spec_json();
        spec["version"] = serde_json::json!(2);
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::UnsupportedVersion { version: 2 }
        ));
    }

    #[test]
    fn an_empty_name_is_rejected() {
        let mut spec = valid_spec_json();
        spec["name"] = serde_json::json!("   ");
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. } if field == "name"
        ));
    }

    #[test]
    fn an_unknown_caption_style_id_is_rejected() {
        let mut spec = valid_spec_json();
        spec["caption_style_id"] = serde_json::json!("does_not_exist");
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::UnknownCaptionStyle { style_id } if style_id == "does_not_exist"
        ));
    }

    #[test]
    fn an_unknown_export_preset_id_is_rejected() {
        let mut spec = valid_spec_json();
        spec["export_preset_id"] = serde_json::json!("does_not_exist");
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::UnknownExportPreset { preset_id } if preset_id == "does_not_exist"
        ));
    }

    #[test]
    fn an_unknown_asset_id_is_rejected() {
        let mut spec = valid_spec_json();
        spec["intro"] = serde_json::json!({"asset_id": "does_not_exist"});
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::UnknownAsset { asset_id } if asset_id == "does_not_exist"
        ));
    }

    #[test]
    fn a_negative_padding_is_rejected() {
        let mut spec = valid_spec_json();
        spec["silence_settings"]["padding_before_us"] = serde_json::json!(-1);
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. }
                if field == "silence_settings.padding_before_us"
        ));
    }

    #[test]
    fn a_padding_over_the_max_bound_is_rejected() {
        let mut spec = valid_spec_json();
        spec["silence_settings"]["merge_gap_us"] = serde_json::json!(MAX_DURATION_US + 1);
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. }
                if field == "silence_settings.merge_gap_us"
        ));
    }

    #[test]
    fn a_cut_transition_with_a_nonzero_duration_is_rejected() {
        let mut spec = valid_spec_json();
        spec["transition_settings"] =
            serde_json::json!({"transition_type": "cut", "duration_us": 5000});
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. }
                if field == "transition_settings.duration_us"
        ));
    }

    #[test]
    fn a_cross_fade_transition_with_zero_duration_is_rejected() {
        let mut spec = valid_spec_json();
        spec["transition_settings"] =
            serde_json::json!({"transition_type": "cross_fade", "duration_us": 0});
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. }
                if field == "transition_settings.duration_us"
        ));
    }

    #[test]
    fn an_out_of_range_duck_level_is_rejected() {
        let mut spec = valid_spec_json();
        spec["sports_overlay"]["music_ducking"]["duck_level"] = serde_json::json!(1.5);
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. }
                if field == "sports_overlay.music_ducking.duck_level"
        ));
    }

    #[test]
    fn a_negative_background_music_volume_is_rejected() {
        let mut spec = valid_spec_json();
        spec["background_music"] = serde_json::json!({"asset_id": "a1", "volume": -0.5});
        let known: HashSet<String> = ["a1"].into_iter().map(String::from).collect();
        let err = parse_and_validate(&spec.to_string(), &known).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. }
                if field == "background_music.volume"
        ));
    }

    #[test]
    fn a_background_music_volume_over_the_max_bound_is_rejected() {
        let mut spec = valid_spec_json();
        spec["background_music"] = serde_json::json!({"asset_id": "a1", "volume": 100.0});
        let known: HashSet<String> = ["a1"].into_iter().map(String::from).collect();
        let err = parse_and_validate(&spec.to_string(), &known).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. }
                if field == "background_music.volume"
        ));
    }

    #[test]
    fn an_overly_long_system_prompt_prefix_is_rejected() {
        let mut spec = valid_spec_json();
        spec["system_prompt_prefix"] =
            serde_json::json!("x".repeat(MAX_SYSTEM_PROMPT_PREFIX_CHARS + 1));
        let err = parse_and_validate(&spec.to_string(), &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateGeneratorError::InvalidField { field, .. }
                if field == "system_prompt_prefix"
        ));
    }

    #[test]
    fn an_unknown_operation_shaped_field_still_fails_to_deserialize_not_execute() {
        // Same master prompt §53 threat model `ai::edit_plan` documents: an
        // unrecognized enum tag can only ever fail to parse, never be
        // interpreted as something to execute.
        let raw = r#"{"version": 1, "name": "x", "description": "x", "canvas_aspect": "16:9",
            "caption_style_id": "template_news", "zoom_intensity": "off",
            "silence_settings": {"padding_before_us": 0, "padding_after_us": 0, "merge_gap_us": 0},
            "transition_settings": {"transition_type": "rm -rf /", "duration_us": 0},
            "export_preset_id": "p1080", "emphasized_categories": [],
            "system_prompt_prefix": null, "sports_overlay": null, "intro": null, "outro": null,
            "watermark": null, "background_music": null}"#;
        let err = parse_and_validate(raw, &HashSet::new()).unwrap_err();
        assert!(matches!(err, TemplateGeneratorError::MalformedJson { .. }));
    }

    /// A real, unused fixture-construction helper kept alongside the others
    /// above for parity with this crate's other AI modules' test style
    /// (`caption_style_fixture` documents that caption styles here always
    /// come from the real catalog, never hand-built) — exercised directly to
    /// keep clippy's dead-code lint quiet while still documenting intent.
    #[test]
    fn caption_style_fixture_helper_resolves_a_real_built_in_style() {
        let style = caption_style_fixture("template_karaoke");
        assert_eq!(style.id, "template_karaoke");
    }

    #[test]
    fn generated_canvas_aspect_maps_to_deterministic_dimensions() {
        assert_eq!(GeneratedCanvasAspect::Ratio16x9.to_canvas().width, 1920);
        assert_eq!(GeneratedCanvasAspect::Ratio1x1.to_canvas().height, 1080);
        assert_eq!(GeneratedCanvasAspect::Ratio4x5.to_canvas().height, 1350);
    }

    #[test]
    fn sports_overlay_survives_round_trip_with_real_audio_role_and_ducking_values() {
        let raw = valid_spec_json().to_string();
        let template = parse_and_validate(&raw, &HashSet::new()).expect("should validate");
        let overlay = template.sports_overlay.expect("sports overlay present");
        assert_eq!(overlay.music_role, AudioRole::Music);
        assert_eq!(
            overlay.music_ducking,
            DuckingSettings {
                duck_level: 0.25,
                attack_us: 150_000,
                release_us: 400_000,
            }
        );
    }
}
