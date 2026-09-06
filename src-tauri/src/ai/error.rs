//! `AiProviderError`/`EditPlanError` — this subsystem's slice of the
//! standardized error model (master prompt §56, `docs/project-format.md`
//! "Error model"), following the same `{code, message, details,
//! recoverable, suggested_action}` pattern as `transcription::error`'s own
//! two-enums-per-subsystem split.
//!
//! Two enums, not one, for the same reason `transcription::error` keeps
//! `TranscriptionError`/`ModelError` separate: `AiProviderError` covers
//! talking to an LLM backend (network, auth, malformed HTTP response) —
//! nothing here knows what an `EditPlan` is. `EditPlanError` covers the
//! completely separate concern of validating whatever text a provider
//! handed back against the strict `EditPlan` schema (`ai::edit_plan`) — a
//! provider call can succeed perfectly and still return text that fails
//! `EditPlanError` validation (the model hallucinated invalid JSON), and a
//! validation failure has nothing to do with whether the network call
//! itself worked.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum AiProviderError {
    #[error("{provider} request failed: {details}")]
    RequestFailed { provider: String, details: String },

    #[error("{provider} returned HTTP {status}: {body}")]
    HttpError {
        provider: String,
        status: u16,
        body: String,
    },

    #[error("could not parse {provider} response: {details}")]
    InvalidResponse { provider: String, details: String },

    #[error("{provider} requires an API key but none is configured")]
    MissingApiKey { provider: String },

    #[error("no stored credential for ref {credential_ref}")]
    CredentialNotFound { credential_ref: String },

    #[error("failed to access secure credential storage: {details}")]
    CredentialStoreFailed { details: String },
}

impl From<&AiProviderError> for AppErrorPayload {
    fn from(err: &AiProviderError) -> Self {
        let message = err.to_string();
        match err {
            AiProviderError::RequestFailed { details, .. } => {
                AppErrorPayload::new("AI_PROVIDER_REQUEST_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Check the provider's base URL and your network connection, then retry.",
                    )
            }
            AiProviderError::HttpError { status, body, .. } => {
                AppErrorPayload::new("AI_PROVIDER_HTTP_ERROR", message)
                    .with_details(format!("status={status}: {body}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Check the API key, model name, and account quota/billing, then retry.",
                    )
            }
            AiProviderError::InvalidResponse { details, .. } => {
                AppErrorPayload::new("AI_PROVIDER_INVALID_RESPONSE", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The provider returned an unexpected response shape; verify the base URL points at a compatible API.",
                    )
            }
            AiProviderError::MissingApiKey { .. } => {
                AppErrorPayload::new("AI_PROVIDER_MISSING_API_KEY", message)
                    .recoverable(true)
                    .with_suggestion("Enter and save an API key for this provider in AI Settings.")
            }
            AiProviderError::CredentialNotFound { credential_ref } => {
                AppErrorPayload::new("AI_CREDENTIAL_NOT_FOUND", message)
                    .with_details(credential_ref.clone())
                    .recoverable(true)
                    .with_suggestion("Save an API key for this provider before using it.")
            }
            AiProviderError::CredentialStoreFailed { details } => {
                AppErrorPayload::new("AI_CREDENTIAL_STORE_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion(
                        "Windows Credential Manager could not be accessed; check Windows account/policy restrictions.",
                    )
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum EditPlanError {
    #[error("could not parse AI output as JSON: {details}")]
    MalformedJson { details: String },

    #[error("unsupported EditPlan schema version: {version}")]
    UnsupportedVersion { version: u32 },

    #[error("operation {index} is invalid: {details}")]
    InvalidOperation { index: usize, details: String },
}

impl From<&EditPlanError> for AppErrorPayload {
    fn from(err: &EditPlanError) -> Self {
        let message = err.to_string();
        match err {
            EditPlanError::MalformedJson { details } => {
                AppErrorPayload::new("EDIT_PLAN_MALFORMED_JSON", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The AI response was not valid EditPlan JSON; ask it to try again.",
                    )
            }
            EditPlanError::UnsupportedVersion { version } => {
                AppErrorPayload::new("EDIT_PLAN_UNSUPPORTED_VERSION", message)
                    .with_details(version.to_string())
                    .recoverable(false)
                    .with_suggestion("This app only understands EditPlan version 1.")
            }
            EditPlanError::InvalidOperation { index, details } => {
                AppErrorPayload::new("EDIT_PLAN_INVALID_OPERATION", message)
                    .with_details(format!("operation[{index}]: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Reject this EditPlan and ask the AI to produce a corrected one.",
                    )
            }
        }
    }
}

/// `ai::smart_edit`'s validation-stage error — same two-stage split as
/// `EditPlanError` above (module doc comment): a provider call can succeed
/// and still return text that fails `SmartEditError` validation.
#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum SmartEditError {
    #[error("could not parse AI output as JSON: {details}")]
    MalformedJson { details: String },

    #[error("unsupported SmartEdit schema version: {version}")]
    UnsupportedVersion { version: u32 },

    #[error("recommendation {index} is invalid: {details}")]
    InvalidRecommendation { index: usize, details: String },
}

impl From<&SmartEditError> for AppErrorPayload {
    fn from(err: &SmartEditError) -> Self {
        let message = err.to_string();
        match err {
            SmartEditError::MalformedJson { details } => {
                AppErrorPayload::new("SMART_EDIT_MALFORMED_JSON", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The AI response was not valid Smart Edit JSON; ask it to try again.",
                    )
            }
            SmartEditError::UnsupportedVersion { version } => {
                AppErrorPayload::new("SMART_EDIT_UNSUPPORTED_VERSION", message)
                    .with_details(version.to_string())
                    .recoverable(false)
                    .with_suggestion("This app only understands Smart Edit schema version 1.")
            }
            SmartEditError::InvalidRecommendation { index, details } => {
                AppErrorPayload::new("SMART_EDIT_INVALID_RECOMMENDATION", message)
                    .with_details(format!("recommendations[{index}]: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Reject this recommendation set and ask the AI to produce a corrected one.",
                    )
            }
        }
    }
}

/// `ai::media_tags`'s validation-stage error (master prompt §35's "Optional
/// AI-generated tags" enhancement) — same two-stage split as
/// `EditPlanError`/`SmartEditError` above: a provider call can succeed and
/// still return text that fails `MediaTagError` validation.
#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum MediaTagError {
    #[error("could not parse AI output as a JSON string array: {details}")]
    MalformedJson { details: String },

    #[error("AI suggested {count} tags, which exceeds the maximum of {max}")]
    TooManyTags { count: usize, max: usize },

    #[error("tag {index} is invalid: {details}")]
    InvalidTag { index: usize, details: String },
}

impl From<&MediaTagError> for AppErrorPayload {
    fn from(err: &MediaTagError) -> Self {
        let message = err.to_string();
        match err {
            MediaTagError::MalformedJson { details } => AppErrorPayload::new(
                "MEDIA_TAG_MALFORMED_JSON",
                message,
            )
            .with_details(details.clone())
            .recoverable(true)
            .with_suggestion(
                "The AI response was not a valid JSON array of tag strings; ask it to try again.",
            ),
            MediaTagError::TooManyTags { count, max } => {
                AppErrorPayload::new("MEDIA_TAG_TOO_MANY_TAGS", message)
                    .with_details(format!("count={count}, max={max}"))
                    .recoverable(true)
                    .with_suggestion("Ask the AI to suggest fewer, more focused tags.")
            }
            MediaTagError::InvalidTag { index, details } => {
                AppErrorPayload::new("MEDIA_TAG_INVALID_TAG", message)
                    .with_details(format!("tags[{index}]: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Reject these suggested tags and ask the AI to produce corrected ones.",
                    )
            }
        }
    }
}

/// `ai::auto_template`'s validation-stage error — same two-stage split as
/// `EditPlanError`/`SmartEditError`/`MediaTagError` above: a provider call
/// can succeed and still return text that fails `AutoTemplateError`
/// validation. `UnknownTemplateId` is this module's own extra case (see
/// `ai::auto_template` module doc comment, "Why `parse_and_validate` needs a
/// real catalog, not just a schema") — a `template_id` the caller's real
/// catalog (built-ins + custom templates) doesn't actually contain.
#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum AutoTemplateError {
    #[error("could not parse AI output as JSON: {details}")]
    MalformedJson { details: String },

    #[error("unsupported Auto Template schema version: {version}")]
    UnsupportedVersion { version: u32 },

    #[error("confidence must be within 0.0..=1.0, got {confidence}")]
    InvalidConfidence { confidence: f32 },

    #[error("unknown template id: {template_id}")]
    UnknownTemplateId { template_id: String },
}

impl From<&AutoTemplateError> for AppErrorPayload {
    fn from(err: &AutoTemplateError) -> Self {
        let message = err.to_string();
        match err {
            AutoTemplateError::MalformedJson { details } => {
                AppErrorPayload::new("AUTO_TEMPLATE_MALFORMED_JSON", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The AI response was not valid Auto Template JSON; ask it to try again.",
                    )
            }
            AutoTemplateError::UnsupportedVersion { version } => {
                AppErrorPayload::new("AUTO_TEMPLATE_UNSUPPORTED_VERSION", message)
                    .with_details(version.to_string())
                    .recoverable(false)
                    .with_suggestion("This app only understands Auto Template schema version 1.")
            }
            AutoTemplateError::InvalidConfidence { confidence } => {
                AppErrorPayload::new("AUTO_TEMPLATE_INVALID_CONFIDENCE", message)
                    .with_details(confidence.to_string())
                    .recoverable(true)
                    .with_suggestion(
                        "Reject this recommendation and ask the AI to produce a corrected one.",
                    )
            }
            AutoTemplateError::UnknownTemplateId { template_id } => {
                AppErrorPayload::new("AUTO_TEMPLATE_UNKNOWN_TEMPLATE_ID", message)
                    .with_details(template_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The AI recommended a template id that doesn't exist in this install's catalog; ask it to try again.",
                    )
            }
        }
    }
}

/// `ai::template_generator`'s validation-stage error (upgrade spec §8's AI
/// Template Generator) — same two-stage split as `EditPlanError`/
/// `SmartEditError`/`MediaTagError` above: a provider call can succeed and
/// still return text that fails `TemplateGeneratorError` validation.
/// `UnknownCaptionStyle`/`UnknownExportPreset`/`UnknownAsset` deliberately
/// mirror `templates::TemplateError`'s own variants of the same name (same
/// underlying catalog checks, reused via `templates::validate_asset_references`
/// and direct catalog lookups — see `ai::template_generator::parse_and_validate`)
/// rather than nesting `TemplateError` itself here, which would collide with
/// this enum's own `#[serde(tag = "variant")]` internal tagging.
#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum TemplateGeneratorError {
    #[error("could not parse AI output as JSON: {details}")]
    MalformedJson { details: String },

    #[error("unsupported GeneratedTemplateSpec schema version: {version}")]
    UnsupportedVersion { version: u32 },

    #[error("field {field} is invalid: {details}")]
    InvalidField { field: String, details: String },

    #[error("unknown caption style id: {style_id}")]
    UnknownCaptionStyle { style_id: String },

    #[error("unknown export preset id: {preset_id}")]
    UnknownExportPreset { preset_id: String },

    #[error("unknown asset id: {asset_id}")]
    UnknownAsset { asset_id: String },
}

impl From<&TemplateGeneratorError> for AppErrorPayload {
    fn from(err: &TemplateGeneratorError) -> Self {
        let message = err.to_string();
        match err {
            TemplateGeneratorError::MalformedJson { details } => AppErrorPayload::new(
                "TEMPLATE_GENERATOR_MALFORMED_JSON",
                message,
            )
            .with_details(details.clone())
            .recoverable(true)
            .with_suggestion(
                "The AI response was not valid GeneratedTemplateSpec JSON; ask it to try again.",
            ),
            TemplateGeneratorError::UnsupportedVersion { version } => {
                AppErrorPayload::new("TEMPLATE_GENERATOR_UNSUPPORTED_VERSION", message)
                    .with_details(version.to_string())
                    .recoverable(false)
                    .with_suggestion("This app only understands GeneratedTemplateSpec version 1.")
            }
            TemplateGeneratorError::InvalidField { field, details } => {
                AppErrorPayload::new("TEMPLATE_GENERATOR_INVALID_FIELD", message)
                    .with_details(format!("{field}: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Reject this generated template and ask the AI to produce a corrected one.",
                    )
            }
            TemplateGeneratorError::UnknownCaptionStyle { style_id } => {
                AppErrorPayload::new("TEMPLATE_GENERATOR_UNKNOWN_CAPTION_STYLE", message)
                    .with_details(style_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Choose a caption style id that exists in the built-in catalog.",
                    )
            }
            TemplateGeneratorError::UnknownExportPreset { preset_id } => {
                AppErrorPayload::new("TEMPLATE_GENERATOR_UNKNOWN_EXPORT_PRESET", message)
                    .with_details(preset_id.clone())
                    .recoverable(true)
                    .with_suggestion("Choose one of render::presets::all_presets()'s ids.")
            }
            TemplateGeneratorError::UnknownAsset { asset_id } => {
                AppErrorPayload::new("TEMPLATE_GENERATOR_UNKNOWN_ASSET", message)
                    .with_details(asset_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Add this asset to the Asset Library first, or ask the AI to omit it.",
                    )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ai_provider_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(AiProviderError, &str)> = vec![
            (
                AiProviderError::RequestFailed {
                    provider: "openai".into(),
                    details: "d".into(),
                },
                "AI_PROVIDER_REQUEST_FAILED",
            ),
            (
                AiProviderError::HttpError {
                    provider: "openai".into(),
                    status: 401,
                    body: "b".into(),
                },
                "AI_PROVIDER_HTTP_ERROR",
            ),
            (
                AiProviderError::InvalidResponse {
                    provider: "openai".into(),
                    details: "d".into(),
                },
                "AI_PROVIDER_INVALID_RESPONSE",
            ),
            (
                AiProviderError::MissingApiKey {
                    provider: "openai".into(),
                },
                "AI_PROVIDER_MISSING_API_KEY",
            ),
            (
                AiProviderError::CredentialNotFound {
                    credential_ref: "r1".into(),
                },
                "AI_CREDENTIAL_NOT_FOUND",
            ),
            (
                AiProviderError::CredentialStoreFailed {
                    details: "d".into(),
                },
                "AI_CREDENTIAL_STORE_FAILED",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn every_edit_plan_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(EditPlanError, &str)> = vec![
            (
                EditPlanError::MalformedJson {
                    details: "d".into(),
                },
                "EDIT_PLAN_MALFORMED_JSON",
            ),
            (
                EditPlanError::UnsupportedVersion { version: 2 },
                "EDIT_PLAN_UNSUPPORTED_VERSION",
            ),
            (
                EditPlanError::InvalidOperation {
                    index: 0,
                    details: "d".into(),
                },
                "EDIT_PLAN_INVALID_OPERATION",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn every_smart_edit_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(SmartEditError, &str)> = vec![
            (
                SmartEditError::MalformedJson {
                    details: "d".into(),
                },
                "SMART_EDIT_MALFORMED_JSON",
            ),
            (
                SmartEditError::UnsupportedVersion { version: 2 },
                "SMART_EDIT_UNSUPPORTED_VERSION",
            ),
            (
                SmartEditError::InvalidRecommendation {
                    index: 0,
                    details: "d".into(),
                },
                "SMART_EDIT_INVALID_RECOMMENDATION",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn every_media_tag_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(MediaTagError, &str)> = vec![
            (
                MediaTagError::MalformedJson {
                    details: "d".into(),
                },
                "MEDIA_TAG_MALFORMED_JSON",
            ),
            (
                MediaTagError::TooManyTags { count: 20, max: 12 },
                "MEDIA_TAG_TOO_MANY_TAGS",
            ),
            (
                MediaTagError::InvalidTag {
                    index: 0,
                    details: "d".into(),
                },
                "MEDIA_TAG_INVALID_TAG",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn every_auto_template_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(AutoTemplateError, &str)> = vec![
            (
                AutoTemplateError::MalformedJson {
                    details: "d".into(),
                },
                "AUTO_TEMPLATE_MALFORMED_JSON",
            ),
            (
                AutoTemplateError::UnsupportedVersion { version: 2 },
                "AUTO_TEMPLATE_UNSUPPORTED_VERSION",
            ),
            (
                AutoTemplateError::InvalidConfidence { confidence: 1.5 },
                "AUTO_TEMPLATE_INVALID_CONFIDENCE",
            ),
            (
                AutoTemplateError::UnknownTemplateId {
                    template_id: "x".into(),
                },
                "AUTO_TEMPLATE_UNKNOWN_TEMPLATE_ID",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn every_template_generator_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(TemplateGeneratorError, &str)> = vec![
            (
                TemplateGeneratorError::MalformedJson {
                    details: "d".into(),
                },
                "TEMPLATE_GENERATOR_MALFORMED_JSON",
            ),
            (
                TemplateGeneratorError::UnsupportedVersion { version: 2 },
                "TEMPLATE_GENERATOR_UNSUPPORTED_VERSION",
            ),
            (
                TemplateGeneratorError::InvalidField {
                    field: "f".into(),
                    details: "d".into(),
                },
                "TEMPLATE_GENERATOR_INVALID_FIELD",
            ),
            (
                TemplateGeneratorError::UnknownCaptionStyle {
                    style_id: "x".into(),
                },
                "TEMPLATE_GENERATOR_UNKNOWN_CAPTION_STYLE",
            ),
            (
                TemplateGeneratorError::UnknownExportPreset {
                    preset_id: "x".into(),
                },
                "TEMPLATE_GENERATOR_UNKNOWN_EXPORT_PRESET",
            ),
            (
                TemplateGeneratorError::UnknownAsset {
                    asset_id: "x".into(),
                },
                "TEMPLATE_GENERATOR_UNKNOWN_ASSET",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
