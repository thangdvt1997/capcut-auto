//! ffprobe wrapper: `ProbedMedia` — reimplemented from
//! `vendor/autocut/src-tauri/src/probe.rs`'s design (direct
//! `ffprobe -show_streams -show_format -print_format json` parsing, chosen
//! over capcut-mate's `pymediainfo` dependency, `docs/architecture-audit.md`
//! §4). Field names/shape mirror the `media[]` item schema in
//! `docs/project-format.md` so `commands::media` can build a full
//! `project::types::MediaItem` by adding only `id`/`source_path`/
//! `proxy_path`/`thumbnail_path`.
//!
//! Timebase rewrite (per the Phase 3 task brief and audit §1/§5): autocut's
//! `probe.rs` keeps `duration`/`fps` as `f64` seconds throughout. Every
//! duration here is converted to `i64` microseconds at the parse boundary,
//! and `fps` is kept as an exact `Rational` (never resolved to a lossy
//! `f64`) — both match this project's `ProjectV1` timebase decision, not
//! autocut's.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ffmpeg::command::{run_checked, FfmpegArgs};
use crate::media::error::MediaError;
use crate::project::Rational;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProbedMedia {
    pub duration_us: i64,
    /// Zero for audio-only media.
    pub width: u32,
    pub height: u32,
    pub fps: Rational,
    pub codec: String,
    /// Bits per second; 0 when ffprobe reports none.
    pub bitrate: i64,
    /// 0 when the file has no audio stream.
    pub audio_channels: u16,
    /// 0 when the file has no audio stream.
    pub sample_rate: u32,
    /// Normalized to one of 0/90/180/270 (see `normalize_rotation`).
    pub rotation_deg: i32,
    /// RFC3339, straight from ffprobe's `creation_time` tag when present.
    pub created_at: Option<String>,
    pub has_video: bool,
    pub has_audio: bool,
}

/// Assumed when the container declares no usable frame rate (audio-only
/// files, or a video stream ffprobe can't resolve a rate for). Matches
/// autocut's own default (`probe.rs::DEFAULT_FPS`) and this project's
/// `CanvasV1`/`Rational` default philosophy.
const DEFAULT_FPS: Rational = Rational::new(30, 1);

pub fn probe(ffprobe: &Path, media: &Path) -> Result<ProbedMedia, MediaError> {
    let args = FfmpegArgs::new()
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .path(media);

    let out = run_checked(ffprobe, &args).map_err(|e| MediaError::ProbeFailed {
        path: media.display().to_string(),
        details: e.to_string(),
    })?;

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| MediaError::ProbeFailed {
            path: media.display().to_string(),
            details: format!("parsing ffprobe json: {e}"),
        })?;

    parse_probe_json(&json).map_err(|details| MediaError::ProbeFailed {
        path: media.display().to_string(),
        details,
    })
}

/// Turn ffprobe's JSON into a `ProbedMedia`. Split out from the subprocess
/// call so the parsing — where all the container-dependent guesswork lives —
/// can be tested against fixtures without spawning a real ffprobe.
fn parse_probe_json(json: &serde_json::Value) -> Result<ProbedMedia, String> {
    let streams = json
        .get("streams")
        .and_then(|s| s.as_array())
        .ok_or_else(|| "no streams in ffprobe output".to_string())?;

    let of_type = |kind: &'static str| {
        streams
            .iter()
            .find(move |s| s.get("codec_type").and_then(|c| c.as_str()) == Some(kind))
    };
    let video_stream = of_type("video");
    let audio_stream = of_type("audio");
    let has_audio = audio_stream.is_some();
    if video_stream.is_none() && !has_audio {
        return Err("no video or audio stream".to_string());
    }

    let as_u32 = |v: Option<&serde_json::Value>| v.and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let width = as_u32(video_stream.and_then(|s| s.get("width")));
    let height = as_u32(video_stream.and_then(|s| s.get("height")));

    let fps = match video_stream {
        Some(s) => resolve_fps(
            s.get("avg_frame_rate").and_then(|v| v.as_str()),
            s.get("r_frame_rate").and_then(|v| v.as_str()),
        ),
        None => DEFAULT_FPS,
    };

    let seconds_str = |v: Option<&serde_json::Value>| {
        v.and_then(|d| d.as_str())
            .and_then(|d| d.parse::<f64>().ok())
    };
    let duration_seconds = seconds_str(json.get("format").and_then(|f| f.get("duration")))
        .or_else(|| seconds_str(video_stream.and_then(|s| s.get("duration"))))
        .or_else(|| seconds_str(audio_stream.and_then(|s| s.get("duration"))))
        .unwrap_or(0.0);
    let duration_us = seconds_to_us(duration_seconds);

    let bitrate = json
        .get("format")
        .and_then(|f| f.get("bit_rate"))
        .and_then(|b| b.as_str())
        .and_then(|b| b.parse::<i64>().ok())
        .or_else(|| {
            video_stream
                .and_then(|s| s.get("bit_rate"))
                .and_then(|b| b.as_str())
                .and_then(|b| b.parse::<i64>().ok())
        })
        .unwrap_or(0);

    let audio_channels = audio_stream
        .and_then(|s| s.get("channels"))
        .and_then(|c| c.as_u64())
        .unwrap_or(0) as u16;
    let sample_rate = audio_stream
        .and_then(|s| s.get("sample_rate"))
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let codec = video_stream
        .or(audio_stream)
        .and_then(|s| s.get("codec_name"))
        .and_then(|c| c.as_str())
        .unwrap_or("unknown")
        .to_string();

    let rotation_deg = video_stream.map(rotation_of_stream).unwrap_or(0);

    let created_at = video_stream
        .and_then(find_creation_time)
        .or_else(|| audio_stream.and_then(find_creation_time))
        .or_else(|| {
            json.get("format")
                .and_then(|f| f.get("tags"))
                .and_then(|t| t.get("creation_time"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        });

    Ok(ProbedMedia {
        duration_us,
        width,
        height,
        fps,
        codec,
        bitrate,
        audio_channels,
        sample_rate,
        rotation_deg,
        created_at,
        has_video: video_stream.is_some(),
        has_audio,
    })
}

fn seconds_to_us(seconds: f64) -> i64 {
    (seconds * 1_000_000.0).round() as i64
}

/// Pick the first frame rate that is actually a frame rate, keeping the
/// exact rational rather than resolving to `f64` (per this project's
/// timebase decision — `docs/project-format.md`).
///
/// `avg_frame_rate` is preferred, but ffprobe reports it as `0/1` for
/// streams it can't average — a well-formed rational that happens to be
/// zero. Rejecting only parse failures accepts that and falls through to
/// `r_frame_rate`.
fn resolve_fps(avg: Option<&str>, r: Option<&str>) -> Rational {
    [avg, r]
        .into_iter()
        .flatten()
        .filter_map(parse_rational_pair)
        .find(|(num, den)| *den > 0 && *num > 0)
        .map(|(num, den)| Rational::new(num, den))
        .unwrap_or(DEFAULT_FPS)
}

fn parse_rational_pair(s: &str) -> Option<(u32, u32)> {
    let (n, d) = s.split_once('/')?;
    let n: u32 = n.parse().ok()?;
    let d: u32 = d.parse().ok()?;
    Some((n, d))
}

fn find_creation_time(stream: &serde_json::Value) -> Option<String> {
    stream
        .get("tags")
        .and_then(|t| t.get("creation_time"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

/// Rotation can live in the stream's `tags.rotate` (older convention) or in
/// `side_data_list[].rotation` (a `Display Matrix` side-data entry, the
/// convention modern ffmpeg/most phone footage uses instead). Normalizes to
/// one of `{0, 90, 180, 270}` since that's the only set CapCut/FCPXML
/// adapters will ever need to reason about downstream.
fn rotation_of_stream(stream: &serde_json::Value) -> i32 {
    let from_tag = stream
        .get("tags")
        .and_then(|t| t.get("rotate"))
        .and_then(|r| r.as_str())
        .and_then(|r| r.parse::<i32>().ok());

    let from_side_data = stream.get("side_data_list").and_then(|list| {
        list.as_array()?.iter().find_map(|entry| {
            entry
                .get("rotation")
                .and_then(|r| r.as_f64())
                .map(|r| r.round() as i32)
        })
    });

    normalize_rotation(from_tag.or(from_side_data).unwrap_or(0))
}

fn normalize_rotation(deg: i32) -> i32 {
    let normalized = deg.rem_euclid(360);
    match normalized {
        0..=44 | 316..=359 => 0,
        45..=134 => 90,
        135..=224 => 180,
        225..=315 => 270,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(raw: &str) -> serde_json::Value {
        serde_json::from_str(raw).expect("fixture is valid json")
    }

    #[test]
    fn seconds_convert_exactly_to_microseconds() {
        assert_eq!(seconds_to_us(12.5), 12_500_000);
        assert_eq!(seconds_to_us(0.0), 0);
        assert_eq!(seconds_to_us(61.25), 61_250_000);
    }

    #[test]
    fn reads_a_video_with_sound() {
        let info = parse_probe_json(&json(
            r#"{
                "streams": [
                    {"codec_type": "video", "width": 1920, "height": 1080, "codec_name": "h264",
                     "avg_frame_rate": "30000/1001", "r_frame_rate": "30000/1001", "bit_rate": "5000000"},
                    {"codec_type": "audio", "channels": 2, "sample_rate": "48000"}
                ],
                "format": {"duration": "12.5", "bit_rate": "5200000"}
            }"#,
        ))
        .expect("a normal video probes cleanly");

        assert!(info.has_video && info.has_audio);
        assert_eq!((info.width, info.height), (1920, 1080));
        assert_eq!(info.fps, Rational::new(30000, 1001));
        assert_eq!(info.duration_us, 12_500_000);
        assert_eq!(info.codec, "h264");
        assert_eq!(info.bitrate, 5_200_000);
        assert_eq!(info.audio_channels, 2);
        assert_eq!(info.sample_rate, 48_000);
        assert_eq!(info.rotation_deg, 0);
    }

    #[test]
    fn reads_an_audio_only_file() {
        let info = parse_probe_json(&json(
            r#"{
                "streams": [{"codec_type": "audio", "codec_name": "flac", "channels": 2, "sample_rate": "44100"}],
                "format": {"duration": "61.25"}
            }"#,
        ))
        .expect("an audio-only file is a valid track");

        assert!(!info.has_video);
        assert!(info.has_audio);
        assert_eq!((info.width, info.height), (0, 0));
        assert_eq!(info.duration_us, 61_250_000);
        assert_eq!(info.fps, DEFAULT_FPS);
    }

    #[test]
    fn rejects_a_file_with_no_streams_at_all() {
        let err = parse_probe_json(&json(r#"{"streams": [], "format": {"duration": "3.0"}}"#))
            .expect_err("a file with neither picture nor sound is not media");
        assert!(err.contains("no video or audio"), "{err}");
    }

    #[test]
    fn resolve_fps_falls_back_when_the_average_rate_is_zero() {
        // ffprobe reports `0/1` (not `0/0`) for streams whose average it
        // can't compute — a well-formed rational zero, so only rejecting
        // parse failures correctly falls through to r_frame_rate.
        assert_eq!(resolve_fps(Some("0/1"), Some("25/1")), Rational::new(25, 1));
    }

    #[test]
    fn resolve_fps_falls_back_to_default_when_nothing_is_usable() {
        assert_eq!(resolve_fps(Some("0/0"), Some("0/1")), DEFAULT_FPS);
    }

    #[test]
    fn rotation_from_legacy_tag_is_normalized() {
        let stream = json(r#"{"tags": {"rotate": "90"}}"#);
        assert_eq!(rotation_of_stream(&stream), 90);
    }

    #[test]
    fn rotation_from_side_data_display_matrix_is_normalized() {
        // Modern ffmpeg reports this as a negative float rotation.
        let stream = json(
            r#"{"side_data_list": [{"side_data_type": "Display Matrix", "rotation": -90.0}]}"#,
        );
        assert_eq!(rotation_of_stream(&stream), 270);
    }

    #[test]
    fn creation_time_falls_back_to_the_container_tag() {
        let info = parse_probe_json(&json(
            r#"{
                "streams": [{"codec_type": "audio"}],
                "format": {"duration": "5", "tags": {"creation_time": "2024-03-01T10:00:00.000000Z"}}
            }"#,
        ))
        .expect("probes cleanly");
        assert_eq!(
            info.created_at.as_deref(),
            Some("2024-03-01T10:00:00.000000Z")
        );
    }
}
