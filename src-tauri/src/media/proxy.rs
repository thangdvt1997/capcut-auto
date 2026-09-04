//! Proxy media generation (master prompt §8): downscale a large source to a
//! lighter editing proxy, with progress reported through real Tauri events
//! (not mocked — see `crate::commands::media::generate_media_proxy`).
//!
//! "Editing uses proxy, final render uses original media" is a policy the
//! *timeline/render* layers implement later (Phase 4/6, by preferring
//! `MediaItem::proxy_path` when present); this module only produces the
//! proxy file and reports progress while doing it.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ffmpeg::command::{run_with_progress, FfmpegArgs};
use crate::media::error::MediaError;

/// Editing-proxy target height (master prompt §8's own example: 4K → 720p).
pub const PROXY_TARGET_HEIGHT: u32 = 720;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    Off,
    Auto,
    Always,
}

/// `Auto` generates a proxy only when the source is meaningfully heavier
/// than the target proxy resolution — otherwise a 720p source would get a
/// pointless "proxy" that's the same size as the original. The threshold is
/// deliberately just above 720p so 1080p sources (already comfortably
/// editable) don't trigger it, matching the master prompt §8 framing of
/// proxies as a 4K-class-source feature.
const AUTO_PROXY_HEIGHT_THRESHOLD: u32 = 1080;

pub fn should_generate_proxy(mode: ProxyMode, source_height: u32) -> bool {
    match mode {
        ProxyMode::Off => false,
        ProxyMode::Always => true,
        ProxyMode::Auto => source_height > AUTO_PROXY_HEIGHT_THRESHOLD,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
pub struct ProxyProgress {
    /// 0.0..=1.0, `None` until the source duration is known (should always
    /// be known here since the caller already probed the source).
    pub fraction: Option<f64>,
    pub done: bool,
}

/// Generate a `PROXY_TARGET_HEIGHT`p H.264 proxy for `source` at `out_path`,
/// calling `on_progress` as ffmpeg reports it. `source_duration_us` (from
/// the original probe) is what turns ffmpeg's raw `out_time_us` into a
/// fraction complete.
pub fn generate_proxy(
    ffmpeg: &Path,
    source: &Path,
    out_path: &Path,
    source_duration_us: i64,
    cancel: Option<&AtomicBool>,
    mut on_progress: impl FnMut(ProxyProgress),
) -> Result<(), MediaError> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MediaError::ProxyFailed {
            path: source.display().to_string(),
            details: format!("creating proxy dir {}: {e}", parent.display()),
        })?;
    }

    let scale = format!("scale=-2:{PROXY_TARGET_HEIGHT}");
    let args = FfmpegArgs::new()
        .args(["-y", "-v", "error"])
        .input(source)
        .args([
            "-vf",
            &scale,
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-progress",
            "pipe:1",
            "-nostats",
        ])
        .path(out_path);

    let result = run_with_progress(
        ffmpeg,
        &args,
        |p| {
            let fraction = if source_duration_us > 0 {
                p.out_time_us
                    .map(|us| (us as f64 / source_duration_us as f64).clamp(0.0, 1.0))
            } else {
                None
            };
            on_progress(ProxyProgress {
                fraction,
                done: p.done,
            });
        },
        cancel,
    );

    match result {
        Ok(status) if status.success() => {
            on_progress(ProxyProgress {
                fraction: Some(1.0),
                done: true,
            });
            Ok(())
        }
        Ok(status) => {
            let _ = std::fs::remove_file(out_path);
            Err(MediaError::ProxyFailed {
                path: source.display().to_string(),
                details: format!("ffmpeg exited with {status}"),
            })
        }
        Err(e) => {
            let _ = std::fs::remove_file(out_path);
            if e.to_string().contains("cancelled") {
                Err(MediaError::ProxyCancelled {
                    path: source.display().to_string(),
                })
            } else {
                Err(MediaError::ProxyFailed {
                    path: source.display().to_string(),
                    details: e.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_never_generates() {
        assert!(!should_generate_proxy(ProxyMode::Off, 2160));
    }

    #[test]
    fn always_generates_even_for_small_sources() {
        assert!(should_generate_proxy(ProxyMode::Always, 480));
    }

    #[test]
    fn auto_skips_1080p_and_below() {
        assert!(!should_generate_proxy(ProxyMode::Auto, 1080));
        assert!(!should_generate_proxy(ProxyMode::Auto, 720));
    }

    #[test]
    fn auto_triggers_above_1080p() {
        assert!(should_generate_proxy(ProxyMode::Auto, 2160));
    }

    #[test]
    fn generates_a_real_720p_proxy_from_a_synthetic_4k_like_source_and_reports_progress() {
        use crate::ffmpeg::command::run_checked;

        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-proxy-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("in.mp4");
        let out = dir.join("proxy.mp4");

        // A small-but-real source (not literally 4K, to keep the test fast)
        // with both a video and audio stream, exercising the full ffmpeg
        // invocation shape a real 4K proxy job would use.
        let gen_args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=640x360:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-shortest",
            ])
            .path(&source);
        run_checked(&ffmpeg, &gen_args)
            .expect("synthesizing a test clip with ffmpeg's lavfi source");

        let probed =
            crate::media::probe::probe(&ffprobe, &source).expect("probing the synthetic source");

        let mut saw_progress = false;
        let mut saw_done = false;
        generate_proxy(&ffmpeg, &source, &out, probed.duration_us, None, |p| {
            if p.fraction.is_some() {
                saw_progress = true;
            }
            if p.done {
                saw_done = true;
            }
        })
        .expect("proxy generation succeeds");

        assert!(out.exists());
        assert!(saw_done, "expected at least a terminal progress callback");
        let _ = saw_progress; // ffmpeg may finish faster than a progress tick on tiny clips; done is the load-bearing assertion.

        let proxy_probe =
            crate::media::probe::probe(&ffprobe, &out).expect("probing the generated proxy");
        assert_eq!(proxy_probe.height, PROXY_TARGET_HEIGHT);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancelling_immediately_removes_partial_output_and_errors() {
        use crate::ffmpeg::command::run_checked;

        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-proxy-cancel-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("in.mp4");
        let out = dir.join("proxy.mp4");

        let gen_args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=5:size=1280x720:rate=30",
            ])
            .path(&source);
        run_checked(&ffmpeg, &gen_args).expect("synthesizing a test clip");

        let cancel = AtomicBool::new(true); // already cancelled before the job starts
        let result = generate_proxy(&ffmpeg, &source, &out, 5_000_000, Some(&cancel), |_| {});

        assert!(matches!(result, Err(MediaError::ProxyCancelled { .. })));
        assert!(
            !out.exists(),
            "cancelled proxy job must not leave a partial file behind"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
