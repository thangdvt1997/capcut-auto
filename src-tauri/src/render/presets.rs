//! Export presets and the fully-custom `RenderSettings` they seed (master
//! prompt §32). Presets are *starting points*: `RenderSettings` itself is a
//! flat, independently-overridable struct — the command layer applies a
//! preset's `RenderSettings` first, then overwrites individual fields from
//! whatever the user changed, so nothing here forces an all-or-nothing
//! choice between "use a preset" and "customize".

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::project::Rational;

use super::error::RenderError;
use super::hwaccel::EncoderBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    Mp4,
    WebM,
}

impl Container {
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::WebM => "webm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
    H265,
    Vp9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AudioCodec {
    Aac,
    Opus,
    Vorbis,
}

/// Full, independently-overridable render configuration. A preset
/// (`RenderPreset::settings`) is just one concrete value of this struct.
///
/// **`width`/`height`/`fps` are `Option` (STUDIO_PLAN.md Phase S3)**: every
/// preset before this phase had a fixed value for all three; the new
/// `"original"` preset (pass-through source resolution/fps/aspect) needs to
/// express "no fixed value requested" instead, so all three became optional
/// rather than special-casing a magic id string somewhere. `Some(_)` means
/// exactly what it always meant (an explicit, validated target); `None`
/// means "resolve from the source at render time" — see this struct's own
/// `validate()` (a `None` dimension/fps has nothing to validate, so it's
/// always accepted) and `render::presets::all_presets`'s `"original"` entry
/// for the full design writeup. Importantly, this is honest about — not a
/// new behavior invented for — this codebase's own pre-existing
/// architecture: `render::plan::build_ffmpeg_plan` already reads the real
/// output canvas size/frame rate from `RenderGraph::canvas`
/// (`ProjectV1::canvas`), never from `RenderSettings::width/height/fps`
/// directly (those exist here only for validation/documentation — every
/// built-in `Template`'s own `canvas` already matches its referenced
/// preset's `width`/`height` by construction, see `templates::mod`'s
/// `canvas_16x9`/`canvas_9x16` helpers). So "pass-through" for the
/// `"original"` preset is real and threaded through at the one place that
/// currently hardcodes a fixed canvas independent of the real source:
/// `batch::pipeline::run_pipeline`, which now builds the project's canvas
/// from the real probed source dimensions/fps instead of a template's fixed
/// canvas whenever the resolved preset requests pass-through (`width.is_none()`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct RenderSettings {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<Rational>,
    pub container: Container,
    pub video_codec: VideoCodec,
    /// `libx264`/`libx265`'s `-preset` speed/efficiency knob (`ultrafast`..
    /// `veryslow`); ignored for `Vp9` and for hardware encoder backends
    /// (each of which has its own, differently-named speed knob — out of
    /// scope to expose individually here, see `plan::video_encoder_args`
    /// doc comment). Owned `String` (not `&'static str`) so a user override
    /// value (`commands::render::RenderSettingsInput::x264_preset`) doesn't
    /// need a `'static` lifetime hack to plug in here.
    pub x264_preset: String,
    /// `None` means "use `video_bitrate_kbps` instead" (bitrate-controlled
    /// encode); hardware encoder backends always use bitrate mode (see
    /// `plan.rs`), since NVENC/QSV/AMF do not share libx264/265's CRF scale.
    pub crf: Option<u8>,
    pub video_bitrate_kbps: Option<u32>,
    pub audio_codec: AudioCodec,
    pub audio_bitrate_kbps: u32,
    /// `None` = auto-detect the best available hardware encoder at render
    /// time (falling back to software); `Some(Software)` forces libx264/265;
    /// `Some(other)` forces that specific hardware backend if a working
    /// encoder was actually detected for it (see `hwaccel::detect_encoders`),
    /// erroring rather than silently downgrading if it wasn't.
    pub hardware_encoder: Option<EncoderBackend>,
}

impl RenderSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        // `None` means "resolve from the source at render time" (module doc
        // comment) — nothing to validate about a dimension/fps that isn't
        // fixed here at all; only a `Some(_)` value needs to be sane.
        if let Some(width) = self.width {
            if width == 0 {
                return Err(RenderError::InvalidSettings {
                    details: "width must be positive".into(),
                });
            }
            if width % 2 != 0 {
                return Err(RenderError::InvalidSettings {
                    details: "width must be even (required by H.264/H.265/VP9)".into(),
                });
            }
        }
        if let Some(height) = self.height {
            if height == 0 {
                return Err(RenderError::InvalidSettings {
                    details: "height must be positive".into(),
                });
            }
            if height % 2 != 0 {
                return Err(RenderError::InvalidSettings {
                    details: "height must be even (required by H.264/H.265/VP9)".into(),
                });
            }
        }
        if let Some(fps) = self.fps {
            if fps.num == 0 || fps.den == 0 {
                return Err(RenderError::InvalidSettings {
                    details: "fps must be a positive rational".into(),
                });
            }
        }
        if self.crf.is_none() && self.video_bitrate_kbps.is_none() {
            return Err(RenderError::InvalidSettings {
                details: "either crf or video_bitrate_kbps must be set".into(),
            });
        }
        if self.audio_bitrate_kbps == 0 {
            return Err(RenderError::InvalidSettings {
                details: "audio_bitrate_kbps must be positive".into(),
            });
        }
        match (self.container, self.video_codec) {
            (Container::Mp4, VideoCodec::H264 | VideoCodec::H265) => {}
            (Container::WebM, VideoCodec::Vp9) => {}
            (container, codec) => {
                return Err(RenderError::InvalidSettings {
                    details: format!(
                        "{container:?} + {codec:?} is not a supported container/codec pairing (MP4 requires H.264/H.265, WebM requires VP9)"
                    ),
                })
            }
        }
        match (self.container, self.audio_codec) {
            (Container::Mp4, AudioCodec::Aac) => {}
            (Container::WebM, AudioCodec::Opus | AudioCodec::Vorbis) => {}
            (container, codec) => {
                return Err(RenderError::InvalidSettings {
                    details: format!(
                        "{container:?} + {codec:?} is not a supported container/audio-codec pairing (MP4 uses AAC, WebM uses Opus/Vorbis)"
                    ),
                })
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RenderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub settings: RenderSettings,
}

/// Positional args for `settings()` below, grouped into one struct purely to
/// keep that function under clippy's `too_many_arguments` threshold — this
/// is an internal, file-private helper for `all_presets()`'s literal preset
/// list, not a public API, so named-field construction at each call site
/// (rather than 10 positional arguments) is also just more readable.
struct SettingsSpec {
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<Rational>,
    container: Container,
    video_codec: VideoCodec,
    x264_preset: &'static str,
    crf: Option<u8>,
    video_bitrate_kbps: Option<u32>,
    audio_codec: AudioCodec,
    audio_bitrate_kbps: u32,
}

fn settings(spec: SettingsSpec) -> RenderSettings {
    RenderSettings {
        width: spec.width,
        height: spec.height,
        fps: spec.fps,
        container: spec.container,
        video_codec: spec.video_codec,
        x264_preset: spec.x264_preset.to_string(),
        crf: spec.crf,
        video_bitrate_kbps: spec.video_bitrate_kbps,
        audio_codec: spec.audio_codec,
        audio_bitrate_kbps: spec.audio_bitrate_kbps,
        hardware_encoder: None,
    }
}

/// The exact preset list from master prompt §32, plus three STUDIO_PLAN.md
/// Phase S3 additions (`youtube_shorts_1080x1920`, `facebook_reel_1080x1920`,
/// `original`). Resolutions/bitrates below are deliberately documented,
/// sensible defaults, not guesses:
/// - CRF values follow libx264/265's own documented quality bands (18 =
///   visually lossless .. 28 = noticeably lossy but small).
/// - YouTube's presets use YouTube's own published recommended upload
///   bitrates for standard frame rates (1080p30 ~8 Mbps H.264, 4K30
///   ~35-45 Mbps H.264 — we use H.265 at a somewhat lower bitrate for the
///   4K preset since H.265 reaches comparable quality at roughly 60-70% of
///   H.264's bitrate). YouTube's published table is keyed by *pixel count*,
///   not by orientation — a 1080x1920 vertical frame has exactly the same
///   pixel count as a 1920x1080 frame, so `youtube_shorts_1080x1920` reuses
///   `youtube_1080p`'s own 8 Mbps figure directly, not a guess.
/// - Facebook publishes no equivalent recommended-bitrate table, so
///   `facebook_reel_1080x1920` stays CRF-based (quality-targeted) like
///   `tiktok_1080x1920`, rather than inventing a bitrate number with no
///   published source to check it against.
pub fn all_presets() -> Vec<RenderPreset> {
    let fps30 = Rational::new(30, 1);
    vec![
        RenderPreset {
            id: "fast_preview",
            name: "Fast Preview",
            description: "Low-resolution, fast encode for a quick check — not for final delivery.",
            settings: settings(SettingsSpec {
                width: Some(854),
                height: Some(480),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                x264_preset: "ultrafast",
                crf: Some(30),
                video_bitrate_kbps: None,
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 96,
            }),
        },
        RenderPreset {
            id: "p1080",
            name: "1080p",
            description: "1920x1080, H.264, balanced quality/size.",
            settings: settings(SettingsSpec {
                width: Some(1920),
                height: Some(1080),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                x264_preset: "medium",
                crf: Some(20),
                video_bitrate_kbps: None,
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 192,
            }),
        },
        RenderPreset {
            id: "p1440",
            name: "1440p",
            description: "2560x1440, H.264, balanced quality/size.",
            settings: settings(SettingsSpec {
                width: Some(2560),
                height: Some(1440),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                x264_preset: "medium",
                crf: Some(20),
                video_bitrate_kbps: None,
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 192,
            }),
        },
        RenderPreset {
            id: "p4k",
            name: "4K",
            description: "3840x2160, H.265 for a smaller file at comparable quality.",
            settings: settings(SettingsSpec {
                width: Some(3840),
                height: Some(2160),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H265,
                x264_preset: "medium",
                crf: Some(22),
                video_bitrate_kbps: None,
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 192,
            }),
        },
        RenderPreset {
            id: "tiktok_1080x1920",
            name: "TikTok 1080x1920",
            description: "1080x1920 vertical, H.264, for TikTok/Reels/Shorts.",
            settings: settings(SettingsSpec {
                width: Some(1080),
                height: Some(1920),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                x264_preset: "medium",
                crf: Some(20),
                video_bitrate_kbps: None,
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 128,
            }),
        },
        RenderPreset {
            id: "youtube_1080p",
            name: "YouTube 1080p",
            description: "1920x1080, H.264 at YouTube's recommended 1080p30 upload bitrate.",
            settings: settings(SettingsSpec {
                width: Some(1920),
                height: Some(1080),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                x264_preset: "medium",
                crf: None,
                video_bitrate_kbps: Some(8_000),
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 192,
            }),
        },
        RenderPreset {
            id: "youtube_4k",
            name: "YouTube 4K",
            description: "3840x2160, H.265 at a bitrate comparable to YouTube's recommended 4K30 H.264 upload bitrate.",
            settings: settings(SettingsSpec {
                width: Some(3840),
                height: Some(2160),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H265,
                x264_preset: "medium",
                crf: None,
                video_bitrate_kbps: Some(25_000),
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 192,
            }),
        },
        RenderPreset {
            id: "youtube_shorts_1080x1920",
            name: "YouTube Shorts",
            description: "1080x1920 vertical, H.264 at YouTube's recommended 1080p30 upload \
                bitrate (same pixel count as 16:9 1080p, so the same published bitrate figure \
                applies) with higher-bitrate AAC audio than the TikTok preset — a real, \
                dedicated preset distinct from TikTok's, closing the gap `tmpl_youtube_shorts` \
                used to paper over by reusing `tiktok_1080x1920` directly.",
            settings: settings(SettingsSpec {
                width: Some(1080),
                height: Some(1920),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                x264_preset: "medium",
                crf: None,
                video_bitrate_kbps: Some(8_000),
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 192,
            }),
        },
        RenderPreset {
            id: "facebook_reel_1080x1920",
            name: "Facebook Reel",
            description: "1080x1920 vertical, H.264. Facebook Reels' documented spec is 1080x1920 \
                9:16, but Facebook (unlike YouTube) publishes no recommended-bitrate table, so \
                this preset stays CRF-based (quality-targeted) like the TikTok preset rather than \
                guessing a bitrate number with nothing published to check it against.",
            settings: settings(SettingsSpec {
                width: Some(1080),
                height: Some(1920),
                fps: Some(fps30),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                x264_preset: "medium",
                crf: Some(20),
                video_bitrate_kbps: None,
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 128,
            }),
        },
        RenderPreset {
            id: "original",
            name: "Original",
            description: "Pass-through: no fixed width/height/fps requested — resolves to \
                whatever the real source's own dimensions/frame rate are at render time (see \
                `RenderSettings`'s own doc comment for exactly how/where), never a hardcoded \
                value. CRF-based (a fixed bitrate cannot be sanely chosen without knowing the \
                resolution up front) at a near-lossless quality band, since the entire point of \
                this preset is to preserve what the source already had rather than re-target it.",
            settings: settings(SettingsSpec {
                width: None,
                height: None,
                fps: None,
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                x264_preset: "slow",
                crf: Some(18),
                video_bitrate_kbps: None,
                audio_codec: AudioCodec::Aac,
                audio_bitrate_kbps: 192,
            }),
        },
    ]
}

pub fn find_preset(preset_id: &str) -> Result<RenderPreset, RenderError> {
    all_presets()
        .into_iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| RenderError::UnknownPreset {
            preset_id: preset_id.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_preset_id_is_present_exactly_once() {
        let ids: Vec<&str> = all_presets().iter().map(|p| p.id).collect();
        for expected in [
            "fast_preview",
            "p1080",
            "p1440",
            "p4k",
            "tiktok_1080x1920",
            "youtube_1080p",
            "youtube_4k",
            "youtube_shorts_1080x1920",
            "facebook_reel_1080x1920",
            "original",
        ] {
            assert_eq!(
                ids.iter().filter(|id| **id == expected).count(),
                1,
                "expected exactly one {expected} preset"
            );
        }
    }

    #[test]
    fn tiktok_preset_is_vertical_1080x1920() {
        let p = find_preset("tiktok_1080x1920").unwrap();
        assert_eq!(
            (p.settings.width, p.settings.height),
            (Some(1080), Some(1920))
        );
    }

    #[test]
    fn youtube_4k_preset_is_3840x2160_h265() {
        let p = find_preset("youtube_4k").unwrap();
        assert_eq!(
            (p.settings.width, p.settings.height),
            (Some(3840), Some(2160))
        );
        assert_eq!(p.settings.video_codec, VideoCodec::H265);
    }

    #[test]
    fn fast_preview_uses_the_fastest_x264_preset_and_lowest_resolution() {
        let p = find_preset("fast_preview").unwrap();
        assert_eq!(p.settings.x264_preset, "ultrafast");
        assert!(p.settings.height.unwrap() <= 480);
    }

    #[test]
    fn every_preset_produces_valid_settings() {
        for p in all_presets() {
            p.settings.validate().unwrap_or_else(|e| {
                panic!("preset {} produced invalid settings: {e:?}", p.id);
            });
        }
    }

    #[test]
    fn unknown_preset_id_errors() {
        let err = find_preset("does_not_exist").unwrap_err();
        assert!(matches!(err, RenderError::UnknownPreset { .. }));
    }

    #[test]
    fn settings_reject_odd_dimensions() {
        let mut s = find_preset("p1080").unwrap().settings;
        s.width = Some(1921);
        assert!(s.validate().is_err());
    }

    #[test]
    fn settings_require_either_crf_or_bitrate() {
        let mut s = find_preset("p1080").unwrap().settings;
        s.crf = None;
        s.video_bitrate_kbps = None;
        assert!(s.validate().is_err());
    }

    // -- STUDIO_PLAN.md Phase S3: youtube_shorts / facebook_reel / original --

    #[test]
    fn youtube_shorts_preset_is_vertical_1080x1920_and_distinct_from_tiktoks_own_preset() {
        let shorts = find_preset("youtube_shorts_1080x1920").unwrap();
        let tiktok = find_preset("tiktok_1080x1920").unwrap();
        assert_eq!(
            (shorts.settings.width, shorts.settings.height),
            (Some(1080), Some(1920))
        );
        // Same resolution as TikTok's own preset (both are 1080x1920 9:16),
        // but a genuinely different preset — bitrate-controlled (YouTube's
        // own published figure) with higher-quality audio, rather than the
        // exact same `RenderSettings` value under two ids.
        assert_ne!(shorts.settings, tiktok.settings);
        assert_eq!(shorts.settings.video_bitrate_kbps, Some(8_000));
        assert!(shorts.settings.audio_bitrate_kbps > tiktok.settings.audio_bitrate_kbps);
    }

    #[test]
    fn facebook_reel_preset_is_vertical_1080x1920() {
        let p = find_preset("facebook_reel_1080x1920").unwrap();
        assert_eq!(
            (p.settings.width, p.settings.height),
            (Some(1080), Some(1920))
        );
        assert_eq!(p.settings.video_codec, VideoCodec::H264);
    }

    #[test]
    fn original_preset_has_no_fixed_dimensions_or_fps_and_still_validates() {
        let p = find_preset("original").unwrap();
        assert_eq!(p.settings.width, None);
        assert_eq!(p.settings.height, None);
        assert_eq!(p.settings.fps, None);
        assert!(p.settings.validate().is_ok());
    }

    #[test]
    fn settings_validate_accepts_a_none_dimension_but_still_rejects_a_bad_some_dimension() {
        let mut s = find_preset("original").unwrap().settings;
        assert!(s.validate().is_ok(), "all-None dims/fps must validate");

        s.width = Some(0);
        assert!(
            s.validate().is_err(),
            "Some(0) width must still be rejected"
        );

        s.width = Some(1921);
        assert!(
            s.validate().is_err(),
            "Some(odd) width must still be rejected"
        );

        s.width = None;
        s.height = Some(1);
        assert!(
            s.validate().is_err(),
            "Some(odd) height must still be rejected"
        );

        s.height = None;
        s.fps = Some(Rational::new(0, 1));
        assert!(
            s.validate().is_err(),
            "Some(zero) fps must still be rejected"
        );
    }
}
