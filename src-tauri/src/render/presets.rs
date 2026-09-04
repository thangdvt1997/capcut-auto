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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub fps: Rational,
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
        if self.width == 0 || self.height == 0 {
            return Err(RenderError::InvalidSettings {
                details: "width/height must be positive".into(),
            });
        }
        if self.width % 2 != 0 || self.height % 2 != 0 {
            return Err(RenderError::InvalidSettings {
                details: "width/height must be even (required by H.264/H.265/VP9)".into(),
            });
        }
        if self.fps.num == 0 || self.fps.den == 0 {
            return Err(RenderError::InvalidSettings {
                details: "fps must be a positive rational".into(),
            });
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
    width: u32,
    height: u32,
    fps: Rational,
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

/// The exact preset list from master prompt §32. Resolutions/bitrates below
/// are deliberately documented, sensible defaults, not guesses:
/// - CRF values follow libx264/265's own documented quality bands (18 =
///   visually lossless .. 28 = noticeably lossy but small).
/// - YouTube's two presets use YouTube's own published recommended upload
///   bitrates for standard frame rates (1080p30 ~8 Mbps H.264, 4K30
///   ~35-45 Mbps H.264 — we use H.265 at a somewhat lower bitrate for the
///   4K preset since H.265 reaches comparable quality at roughly 60-70% of
///   H.264's bitrate).
pub fn all_presets() -> Vec<RenderPreset> {
    let fps30 = Rational::new(30, 1);
    vec![
        RenderPreset {
            id: "fast_preview",
            name: "Fast Preview",
            description: "Low-resolution, fast encode for a quick check — not for final delivery.",
            settings: settings(SettingsSpec {
                width: 854,
                height: 480,
                fps: fps30,
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
                width: 1920,
                height: 1080,
                fps: fps30,
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
                width: 2560,
                height: 1440,
                fps: fps30,
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
                width: 3840,
                height: 2160,
                fps: fps30,
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
                width: 1080,
                height: 1920,
                fps: fps30,
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
                width: 1920,
                height: 1080,
                fps: fps30,
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
                width: 3840,
                height: 2160,
                fps: fps30,
                container: Container::Mp4,
                video_codec: VideoCodec::H265,
                x264_preset: "medium",
                crf: None,
                video_bitrate_kbps: Some(25_000),
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
        assert_eq!((p.settings.width, p.settings.height), (1080, 1920));
    }

    #[test]
    fn youtube_4k_preset_is_3840x2160_h265() {
        let p = find_preset("youtube_4k").unwrap();
        assert_eq!((p.settings.width, p.settings.height), (3840, 2160));
        assert_eq!(p.settings.video_codec, VideoCodec::H265);
    }

    #[test]
    fn fast_preview_uses_the_fastest_x264_preset_and_lowest_resolution() {
        let p = find_preset("fast_preview").unwrap();
        assert_eq!(p.settings.x264_preset, "ultrafast");
        assert!(p.settings.height <= 480);
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
        s.width = 1921;
        assert!(s.validate().is_err());
    }

    #[test]
    fn settings_require_either_crf_or_bitrate() {
        let mut s = find_preset("p1080").unwrap().settings;
        s.crf = None;
        s.video_bitrate_kbps = None;
        assert!(s.validate().is_err());
    }
}
