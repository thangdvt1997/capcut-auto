//! Thumbnail generation for imported video/image media (master prompt §7).
//!
//! Video: extract a single downscaled frame at a representative timestamp.
//! Image: downscale (or copy, if already small) via the same ffmpeg call —
//! reusing one code path instead of adding an `image`-crate dependency just
//! for the image case. Audio never gets a thumbnail; the media library UI
//! shows a generic waveform icon instead (there is nothing to extract a
//! frame from).

use std::path::Path;

use crate::ffmpeg::command::{run_checked, FfmpegArgs};
use crate::media::error::MediaError;

/// Longest edge of a generated thumbnail, in pixels. Small enough to keep
/// the media library grid responsive with hundreds of imported items
/// (master prompt §50), large enough to not look blurry in a library card.
const THUMBNAIL_MAX_EDGE: u32 = 320;

/// Extract a single frame from a video at `seek_us` microseconds in
/// (clamped to `[0, duration_us)` by the caller — see
/// `pick_thumbnail_timestamp_us`), scaled so its longest edge is
/// `THUMBNAIL_MAX_EDGE`, written as a JPEG to `out_path`.
pub fn generate_video_thumbnail(
    ffmpeg: &Path,
    source: &Path,
    out_path: &Path,
    seek_us: i64,
) -> Result<(), MediaError> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MediaError::ThumbnailFailed {
            path: source.display().to_string(),
            details: format!("creating thumbnail dir {}: {e}", parent.display()),
        })?;
    }

    let seek_seconds = seek_us as f64 / 1_000_000.0;
    let scale = format!(
        "scale='min({THUMBNAIL_MAX_EDGE},iw)':'min({THUMBNAIL_MAX_EDGE},ih)':force_original_aspect_ratio=decrease"
    );
    let args = FfmpegArgs::new()
        .args(["-y", "-v", "error", "-ss"])
        .arg(format!("{seek_seconds:.3}"))
        .input(source)
        .args(["-frames:v", "1", "-vf", &scale])
        .path(out_path);

    run_checked(ffmpeg, &args)
        .map(|_| ())
        .map_err(|e| MediaError::ThumbnailFailed {
            path: source.display().to_string(),
            details: e.to_string(),
        })
}

/// Downscale a source image into a thumbnail. Same ffmpeg invocation shape
/// as the video path minus the seek, since ffmpeg reads/writes still image
/// formats directly.
pub fn generate_image_thumbnail(
    ffmpeg: &Path,
    source: &Path,
    out_path: &Path,
) -> Result<(), MediaError> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MediaError::ThumbnailFailed {
            path: source.display().to_string(),
            details: format!("creating thumbnail dir {}: {e}", parent.display()),
        })?;
    }

    let scale = format!(
        "scale='min({THUMBNAIL_MAX_EDGE},iw)':'min({THUMBNAIL_MAX_EDGE},ih)':force_original_aspect_ratio=decrease"
    );
    let args = FfmpegArgs::new()
        .args(["-y", "-v", "error"])
        .input(source)
        .args(["-vf", &scale, "-frames:v", "1"])
        .path(out_path);

    run_checked(ffmpeg, &args)
        .map(|_| ())
        .map_err(|e| MediaError::ThumbnailFailed {
            path: source.display().to_string(),
            details: e.to_string(),
        })
}

/// Pick a representative timestamp for a video thumbnail: 10% into the
/// clip, capped at 5 seconds, so a black title-card intro frame at t=0
/// doesn't become every video's thumbnail, while a very short clip still
/// gets a frame from within its actual duration.
pub fn pick_thumbnail_timestamp_us(duration_us: i64) -> i64 {
    if duration_us <= 0 {
        return 0;
    }
    let ten_percent = duration_us / 10;
    let cap = 5_000_000;
    ten_percent
        .min(cap)
        .max(0)
        .min(duration_us.saturating_sub(1).max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_ten_percent_for_a_short_clip() {
        assert_eq!(pick_thumbnail_timestamp_us(2_000_000), 200_000);
    }

    #[test]
    fn caps_at_five_seconds_for_a_long_clip() {
        assert_eq!(pick_thumbnail_timestamp_us(600_000_000), 5_000_000);
    }

    #[test]
    fn never_exceeds_duration_for_a_very_short_clip() {
        assert_eq!(pick_thumbnail_timestamp_us(500_000), 50_000);
        // The duration clamp itself: an artificially tiny duration where
        // even 10% would land past the last valid instant.
        assert_eq!(pick_thumbnail_timestamp_us(1), 0);
    }

    #[test]
    fn handles_zero_duration() {
        assert_eq!(pick_thumbnail_timestamp_us(0), 0);
    }

    #[test]
    fn generates_a_real_thumbnail_from_a_synthetic_video() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-thumb-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("in.mp4");
        let out = dir.join("thumb.jpg");

        let gen_args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=320x240:rate=10",
            ])
            .path(&source);
        run_checked(&ffmpeg, &gen_args)
            .expect("synthesizing a test clip with ffmpeg's lavfi source");

        generate_video_thumbnail(&ffmpeg, &source, &out, 0).expect("thumbnail generation succeeds");
        assert!(out.exists());
        assert!(std::fs::metadata(&out).unwrap().len() > 0);

        std::fs::remove_dir_all(&dir).ok();
    }
}
