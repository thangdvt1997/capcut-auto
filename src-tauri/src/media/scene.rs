//! Real, non-AI scene-change (hard-cut) detection via ffmpeg's own
//! documented `select='gt(scene,THRESHOLD)'` frame-difference filter (master
//! prompt §21's own worked example: `-vf
//! "select=gt(scene,0.4),showinfo"`) — one of highlight detection's real
//! local signals (`crate::highlights`, Phase 10 follow-up), genuinely
//! achievable without any AI provider call.
//!
//! Matches `media::thumbnail`/`media::proxy`'s existing "shell out to real
//! ffmpeg, parse real output" pattern: arguments go through
//! `ffmpeg::command::FfmpegArgs`'s argument-array builder (never shell
//! string concatenation, per `docs/architecture.md`'s stated convention), and
//! the *detection* itself is real ffmpeg frame-difference scoring, not a
//! heuristic guess — every timestamp this returns is one ffmpeg's own
//! `select` filter actually decided crossed `threshold`.
//!
//! ## How it works
//!
//! ffmpeg's `select` filter evaluates its expression per frame and only
//! passes through frames where it's true; `scene` is a built-in per-frame
//! metric (roughly: how different this frame is from the previous one,
//! `0.0..=1.0`). Chaining `,showinfo` after `select` makes ffmpeg log one
//! line per *selected* frame to stderr, including `pts_time:<seconds>` — so
//! parsing stderr for `pts_time:` after running with `-f null -` (decode
//! only, produce no output file) yields exactly the detected cut
//! timestamps, with zero AI/model involvement.

use std::path::Path;

use crate::ffmpeg::command::{run_checked, FfmpegArgs};
use crate::media::error::MediaError;

/// ffmpeg's own suggested default scene-change score (master prompt §21's
/// worked example uses the same value).
pub const DEFAULT_SCENE_THRESHOLD: f32 = 0.4;

/// Runs `ffmpeg -i <source> -vf "select='gt(scene,threshold)',showinfo" -an -f null -`
/// and parses every `showinfo`-reported `pts_time:` from stderr into a
/// sorted, deduplicated list of microsecond timestamps — the real detected
/// scene-cut points (module doc comment). `-an` skips audio entirely (this
/// is a purely visual signal); no output file is ever written (`-f null -`),
/// so this never touches the filesystem beyond reading `source`.
pub fn detect_scene_changes(
    ffmpeg: &Path,
    source: &Path,
    threshold: f32,
) -> Result<Vec<i64>, MediaError> {
    let fail = |details: String| MediaError::SceneDetectionFailed {
        path: source.display().to_string(),
        details,
    };

    let filter = format!("select='gt(scene,{threshold})',showinfo");
    let args = FfmpegArgs::new()
        .input(source)
        .args(["-vf", &filter, "-an", "-f", "null", "-"]);

    let output = run_checked(ffmpeg, &args).map_err(|e| fail(e.to_string()))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_showinfo_pts_times(&stderr))
}

/// Parses ffmpeg/`showinfo`'s own stderr text, extracting every
/// `pts_time:<seconds>` token into a microsecond `i64`, sorted ascending and
/// deduplicated. Split out from [`detect_scene_changes`] so the parsing
/// logic is unit-testable against a fixed fixture string without spawning
/// ffmpeg — mirrors `ffmpeg::command::parse_progress_block`'s own split
/// between "run the real subprocess" and "parse this exact text shape".
pub fn parse_showinfo_pts_times(stderr: &str) -> Vec<i64> {
    const MARKER: &str = "pts_time:";
    let mut out = Vec::new();
    for line in stderr.lines() {
        let Some(idx) = line.find(MARKER) else {
            continue;
        };
        let rest = &line[idx + MARKER.len()..];
        let token = rest.split_whitespace().next().unwrap_or("");
        if let Ok(seconds) = token.parse::<f64>() {
            out.push((seconds * 1_000_000.0).round() as i64);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_realistic_showinfo_stderr_fixture() {
        // Shape ffmpeg's real showinfo filter actually emits (abbreviated —
        // only the fields this parser cares about need to be realistic).
        let stderr = "\
[Parsed_showinfo_1 @ 0x55a1] config in time_base: 1/10, frame_rate: 10/1\n\
[Parsed_showinfo_1 @ 0x55a1] n:   0 pts:      0 pts_time:0        pos:  1234\n\
[Parsed_showinfo_1 @ 0x55a1] n:   1 pts:     10 pts_time:1.033333 pos:  5678\n\
frame=   20 fps=0.0 q=-0.0 Lsize=N/A time=00:00:02.00 bitrate=N/A speed=123x\n";
        let times = parse_showinfo_pts_times(stderr);
        assert_eq!(times, vec![0, 1_033_333]);
    }

    #[test]
    fn ignores_lines_with_no_pts_time_token() {
        let stderr = "ffmpeg version 6.0\nbuilt with gcc\nStream mapping:\n";
        assert!(parse_showinfo_pts_times(stderr).is_empty());
    }

    #[test]
    fn sorts_and_deduplicates_out_of_order_or_repeated_timestamps() {
        let stderr = "x pts_time:2.0 y\nx pts_time:1.0 y\nx pts_time:1.0 y\n";
        assert_eq!(parse_showinfo_pts_times(stderr), vec![1_000_000, 2_000_000]);
    }

    #[test]
    fn ignores_a_malformed_pts_time_value_without_erroring() {
        let stderr = "x pts_time:not-a-number y\nx pts_time:3.5 y\n";
        assert_eq!(parse_showinfo_pts_times(stderr), vec![3_500_000]);
    }

    /// The genuinely valuable test this pass's brief calls for: a *real*
    /// ffmpeg-synthesized video with one hard visual cut (red for 1s, then
    /// blue for 1s, concatenated — the same `lavfi` synthetic-media-generation
    /// technique `render::job`'s own tests use for a real tiny source), fed
    /// through the real detector, confirming it actually finds the real cut
    /// point — not a mocked ffmpeg call.
    #[test]
    fn detects_a_real_hard_cut_in_a_synthesized_two_color_video() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-scene-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("cut.mp4");

        // Two visually distinct 1-second segments concatenated back to
        // back: a real, deterministic hard cut at t=1.0s.
        let gen_args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=320x240:duration=1:rate=10",
                "-f",
                "lavfi",
                "-i",
                "color=blue:size=320x240:duration=1:rate=10",
                "-filter_complex",
                "[0:v][1:v]concat=n=2:v=1:a=0[v]",
                "-map",
                "[v]",
            ])
            .path(&source);
        run_checked(&ffmpeg, &gen_args).expect("synthesizing a two-color hard-cut test video");

        let cuts = detect_scene_changes(&ffmpeg, &source, DEFAULT_SCENE_THRESHOLD)
            .expect("detection runs");

        assert!(
            !cuts.is_empty(),
            "expected at least one detected scene change in a red->blue hard cut"
        );
        // The cut should land close to the real 1.0s boundary — generous
        // tolerance (encoding/frame-timing rounding), but this is the one
        // assertion that proves the detector found the *real* cut, not just
        // "returned something".
        let near_one_second = cuts.iter().any(|&t| (t - 1_000_000).abs() < 400_000);
        assert!(
            near_one_second,
            "expected a detected cut near 1.0s, got {cuts:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
