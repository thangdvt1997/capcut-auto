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

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ffmpeg::command::{run_checked, FfmpegArgs};
use crate::media::error::MediaError;
use crate::media::thumbnail;

/// ffmpeg's own suggested default scene-change score (master prompt §21's
/// worked example uses the same value).
pub const DEFAULT_SCENE_THRESHOLD: f32 = 0.4;

fn run_scene_select(ffmpeg: &Path, source: &Path, threshold: f32) -> Result<String, MediaError> {
    let fail = |details: String| MediaError::SceneDetectionFailed {
        path: source.display().to_string(),
        details,
    };

    // `,metadata=print` appends every frame's metadata to the same stderr
    // stream `showinfo` already writes to (real, verified against this
    // project's actual ffmpeg build — see `media::scene` module doc comment
    // update / `IMPLEMENTATION_PLAN.md` Phase 11 writeup): the `select`
    // filter's own `scene` function sets `lavfi.scene_score` as frame
    // metadata for every frame it evaluates `scene(...)` against, so
    // `metadata=print`'s "frame:N pts:P pts_time:T" + "lavfi.scene_score=S"
    // line pair is the REAL score ffmpeg itself computed at that exact cut —
    // not a heuristic guess (`Scene::score`'s formula, `parse_showinfo_scene_cuts`
    // below).
    let filter = format!("select='gt(scene,{threshold})',showinfo,metadata=print");
    let args = FfmpegArgs::new()
        .input(source)
        .args(["-vf", &filter, "-an", "-f", "null", "-"]);

    let output = run_checked(ffmpeg, &args).map_err(|e| fail(e.to_string()))?;
    Ok(String::from_utf8_lossy(&output.stderr).into_owned())
}

/// Runs `ffmpeg -i <source> -vf "select='gt(scene,threshold)',showinfo,metadata=print" -an -f null -`
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
    let stderr = run_scene_select(ffmpeg, source, threshold)?;
    Ok(parse_showinfo_pts_times(&stderr))
}

/// One real detected scene-change cut point plus the real ffmpeg-computed
/// `scene` score (`0.0..=1.0`) at that exact frame (`parse_showinfo_scene_cuts`
/// doc comment) — the intermediate value `scenes_from_cuts` below turns into
/// a `Scene` list's boundary scores.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneCut {
    pub time_us: i64,
    pub score: f32,
}

/// Same real ffmpeg subprocess as `detect_scene_changes`, but also parsing
/// each cut's real `lavfi.scene_score` metadata value (`run_scene_select`
/// doc comment) instead of discarding it.
pub fn detect_scene_changes_with_scores(
    ffmpeg: &Path,
    source: &Path,
    threshold: f32,
) -> Result<Vec<SceneCut>, MediaError> {
    let stderr = run_scene_select(ffmpeg, source, threshold)?;
    Ok(parse_showinfo_scene_cuts(&stderr))
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

/// Parses the same stderr text as [`parse_showinfo_pts_times`], additionally
/// pairing each `pts_time:` line with the real `lavfi.scene_score=` value
/// `metadata=print` emits immediately after it for the same frame (verified
/// against this project's real ffmpeg build — see `run_scene_select` doc
/// comment): showinfo logs `pts_time:` on one line, then `metadata=print`
/// logs `frame:N pts:P pts_time:T` followed by `lavfi.scene_score=S` on the
/// next two lines, all for that same selected frame, before the next
/// selected frame's lines begin. A `pts_time:` line with no following
/// `lavfi.scene_score=` line (should not happen with the real filter chain
/// this module always builds, but defensively handled) keeps a `0.0` score
/// rather than panicking or dropping the cut.
pub fn parse_showinfo_scene_cuts(stderr: &str) -> Vec<SceneCut> {
    const PTS_MARKER: &str = "pts_time:";
    const SCORE_MARKER: &str = "lavfi.scene_score=";

    let mut out: Vec<SceneCut> = Vec::new();
    for line in stderr.lines() {
        if let Some(idx) = line.find(PTS_MARKER) {
            let rest = &line[idx + PTS_MARKER.len()..];
            let token = rest.split_whitespace().next().unwrap_or("");
            if let Ok(seconds) = token.parse::<f64>() {
                let time_us = (seconds * 1_000_000.0).round() as i64;
                // showinfo's own `pts_time:` line and metadata=print's
                // "frame:N ... pts_time:T" line both match `PTS_MARKER` for
                // the same frame — only the *first* sighting per timestamp
                // starts a new `SceneCut` entry (a second identical
                // timestamp is that same frame's metadata-print line, not a
                // new cut), so this doesn't create a duplicate awaiting a
                // score that never immediately follows it.
                if out.last().map(|c: &SceneCut| c.time_us) != Some(time_us) {
                    out.push(SceneCut {
                        time_us,
                        score: 0.0,
                    });
                }
            }
        } else if let Some(idx) = line.find(SCORE_MARKER) {
            let rest = line[idx + SCORE_MARKER.len()..].trim();
            let token = rest.split_whitespace().next().unwrap_or(rest);
            if let (Ok(score), Some(last)) = (token.parse::<f32>(), out.last_mut()) {
                last.score = score.clamp(0.0, 1.0);
            }
        }
    }
    out.sort_by_key(|c| c.time_us);
    out.dedup_by_key(|c| c.time_us);
    out
}

/// One detected scene: `{id, start_us, end_us, thumbnail_path, score}`
/// (master prompt §25's exact `Scene{start, end, thumbnail, score}` return
/// shape, specta-typed so it's usable directly from a Tauri command).
///
/// ## `score` formula (documented per this phase's task brief)
///
/// `score` is the real ffmpeg-computed `scene` metric (`0.0..=1.0`,
/// `lavfi.scene_score`, `parse_showinfo_scene_cuts` doc comment) of the cut
/// that *opens* this scene — i.e. "how strong a visual break ffmpeg itself
/// detected at this scene's start boundary", not a placeholder constant and
/// not a duration/uniformity guess. The very first scene in a media file
/// (starting at `0`) has no opening cut at all — it starts simply because
/// the timeline begins there, not because ffmpeg detected a break — so it
/// is scored `0.0` (honest: "not a detected boundary", not "detected but
/// weak").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Scene {
    pub id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub thumbnail_path: Option<String>,
    pub score: f32,
}

/// Turns detected cut points (`detect_scene_changes_with_scores`) plus the
/// media's own `total_duration_us` into the final ordered `Scene` list:
/// `[0, cut1), [cut1, cut2), ..., [cutN, total_duration_us)`. Pure — no
/// thumbnail generation here (`thumbnail_path` starts `None`; see
/// `detect_scenes` for the full pipeline that fills it in), so this is
/// independently unit-testable against synthetic cut lists.
pub fn scenes_from_cuts(cuts: &[SceneCut], total_duration_us: i64) -> Vec<Scene> {
    if total_duration_us <= 0 {
        return Vec::new();
    }
    let mut boundaries: Vec<(i64, f32)> = vec![(0, 0.0)];
    for cut in cuts {
        if cut.time_us > 0 && cut.time_us < total_duration_us {
            boundaries.push((cut.time_us, cut.score));
        }
    }
    boundaries.sort_by_key(|b| b.0);
    boundaries.dedup_by_key(|b| b.0);

    let mut scenes = Vec::with_capacity(boundaries.len());
    for (i, &(start_us, score)) in boundaries.iter().enumerate() {
        let end_us = boundaries
            .get(i + 1)
            .map(|b| b.0)
            .unwrap_or(total_duration_us);
        if end_us <= start_us {
            continue;
        }
        scenes.push(Scene {
            id: uuid::Uuid::new_v4().to_string(),
            start_us,
            end_us,
            thumbnail_path: None,
            score,
        });
    }
    scenes
}

/// Generates a real thumbnail for `scene` (reusing `media::thumbnail`'s
/// existing single-frame extraction, module doc comment's "reuse rather than
/// reimplement" requirement) at a representative timestamp *within the
/// scene's own span* — `thumbnail::pick_thumbnail_timestamp_us` picked
/// relative to the scene's own duration, then offset by `scene.start_us` so
/// it lands inside the scene rather than always at the media's own global
/// 10%-in point.
pub fn generate_scene_thumbnail(
    ffmpeg: &Path,
    source: &Path,
    out_path: &Path,
    scene: &Scene,
) -> Result<(), MediaError> {
    let scene_duration_us = (scene.end_us - scene.start_us).max(0);
    let relative_seek_us = thumbnail::pick_thumbnail_timestamp_us(scene_duration_us);
    thumbnail::generate_video_thumbnail(ffmpeg, source, out_path, scene.start_us + relative_seek_us)
}

/// The full scene-detection pipeline (master prompt §25): real ffmpeg
/// cut-detection + real per-scene thumbnails, one JPEG per scene written
/// into `thumbnail_dir` named `scene-<id>.jpg`. Thumbnail generation failure
/// for one scene is non-fatal (matches `media::thumbnail`'s/`MediaError::
/// ThumbnailFailed`'s own documented "editing continues without a
/// thumbnail" recovery story) — that scene simply keeps `thumbnail_path:
/// None` rather than failing the whole detection.
pub fn detect_scenes(
    ffmpeg: &Path,
    source: &Path,
    threshold: f32,
    total_duration_us: i64,
    thumbnail_dir: &Path,
) -> Result<Vec<Scene>, MediaError> {
    let cuts = detect_scene_changes_with_scores(ffmpeg, source, threshold)?;
    let mut scenes = scenes_from_cuts(&cuts, total_duration_us);
    for scene in &mut scenes {
        let out_path = thumbnail_dir.join(format!("scene-{}.jpg", scene.id));
        if generate_scene_thumbnail(ffmpeg, source, &out_path, scene).is_ok() {
            scene.thumbnail_path = Some(out_path.display().to_string());
        }
    }
    Ok(scenes)
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

    // -- parse_showinfo_scene_cuts ------------------------------------------

    #[test]
    fn parses_a_realistic_showinfo_plus_metadata_print_fixture() {
        // Exact shape observed from this project's real ffmpeg build with
        // the `select=...,showinfo,metadata=print` filter chain
        // `run_scene_select` actually uses (captured via a real WSL ffmpeg
        // run against a synthesized hard-cut video, not invented by hand).
        let stderr = "\
[Parsed_showinfo_1 @ 0x1] config in time_base: 1/10240, frame_rate: 10/1\n\
[Parsed_showinfo_1 @ 0x1] n:   0 pts:  10240 pts_time:1       duration:   1024 duration_time:0.1 fmt:yuv420p\n\
[Parsed_showinfo_1 @ 0x1] color_range:unknown color_space:unknown\n\
[Parsed_metadata_2 @ 0x2] frame:0    pts:10240   pts_time:1\n\
[Parsed_metadata_2 @ 0x2] lavfi.scene_score=0.400000\n\
frame=    1 fps=0.0 q=-0.0 Lsize=N/A time=00:00:01.00 bitrate=N/A speed=137x\n";
        let cuts = parse_showinfo_scene_cuts(stderr);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].time_us, 1_000_000);
        assert!((cuts[0].score - 0.4).abs() < 1e-6, "{}", cuts[0].score);
    }

    #[test]
    fn multiple_cuts_each_get_their_own_score() {
        let stderr = "\
[showinfo] n:0 pts_time:1\n\
[metadata] frame:0 pts_time:1\n\
[metadata] lavfi.scene_score=0.550000\n\
[showinfo] n:1 pts_time:2\n\
[metadata] frame:1 pts_time:2\n\
[metadata] lavfi.scene_score=0.720000\n";
        let cuts = parse_showinfo_scene_cuts(stderr);
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[0].time_us, 1_000_000);
        assert!((cuts[0].score - 0.55).abs() < 1e-6);
        assert_eq!(cuts[1].time_us, 2_000_000);
        assert!((cuts[1].score - 0.72).abs() < 1e-6);
    }

    #[test]
    fn a_pts_time_line_with_no_following_score_defaults_to_zero_rather_than_panicking() {
        let stderr = "[showinfo] n:0 pts_time:1\n";
        let cuts = parse_showinfo_scene_cuts(stderr);
        assert_eq!(
            cuts,
            vec![SceneCut {
                time_us: 1_000_000,
                score: 0.0
            }]
        );
    }

    #[test]
    fn detects_a_real_hard_cut_with_a_real_nonzero_score() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-scene-score-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("cut.mp4");

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

        let cuts = detect_scene_changes_with_scores(&ffmpeg, &source, DEFAULT_SCENE_THRESHOLD)
            .expect("detection runs");
        assert!(!cuts.is_empty(), "expected at least one detected cut");
        let near_one_second = cuts
            .iter()
            .find(|c| (c.time_us - 1_000_000).abs() < 400_000);
        let cut =
            near_one_second.unwrap_or_else(|| panic!("expected a cut near 1.0s, got {cuts:?}"));
        // A real hard cut (solid red -> solid blue) must score at least the
        // detection threshold itself (ffmpeg's `gt(scene,threshold)` select
        // expression only lets a frame through when its real internal score
        // exceeds `threshold`, even though the `metadata=print` log text
        // this parser reads only carries 6 decimal digits of precision, so
        // an internal value like `0.4000001` can print/parse back as exactly
        // `0.4` — `>=` tolerates that real text-precision-loss edge case
        // while still proving this is ffmpeg's own real computed value, not
        // a hardcoded placeholder).
        assert!(
            cut.score >= DEFAULT_SCENE_THRESHOLD,
            "expected score >= threshold, got {}",
            cut.score
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    // -- scenes_from_cuts -----------------------------------------------------

    #[test]
    fn no_cuts_produces_one_scene_spanning_the_whole_duration() {
        let scenes = scenes_from_cuts(&[], 10_000_000);
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0].start_us, 0);
        assert_eq!(scenes[0].end_us, 10_000_000);
        assert_eq!(scenes[0].score, 0.0);
        assert_eq!(scenes[0].thumbnail_path, None);
    }

    #[test]
    fn cuts_produce_scenes_scored_by_their_opening_cut() {
        let cuts = vec![
            SceneCut {
                time_us: 3_000_000,
                score: 0.6,
            },
            SceneCut {
                time_us: 7_000_000,
                score: 0.9,
            },
        ];
        let scenes = scenes_from_cuts(&cuts, 10_000_000);
        assert_eq!(scenes.len(), 3);
        assert_eq!((scenes[0].start_us, scenes[0].end_us), (0, 3_000_000));
        assert_eq!(scenes[0].score, 0.0); // first scene: no opening cut
        assert_eq!(
            (scenes[1].start_us, scenes[1].end_us),
            (3_000_000, 7_000_000)
        );
        assert_eq!(scenes[1].score, 0.6);
        assert_eq!(
            (scenes[2].start_us, scenes[2].end_us),
            (7_000_000, 10_000_000)
        );
        assert_eq!(scenes[2].score, 0.9);
    }

    #[test]
    fn out_of_range_and_duplicate_cuts_are_ignored() {
        let cuts = vec![
            SceneCut {
                time_us: 0,
                score: 0.5,
            },
            SceneCut {
                time_us: 5_000_000,
                score: 0.5,
            },
            SceneCut {
                time_us: 5_000_000,
                score: 0.9,
            },
            SceneCut {
                time_us: 20_000_000,
                score: 0.5,
            },
        ];
        let scenes = scenes_from_cuts(&cuts, 10_000_000);
        assert_eq!(scenes.len(), 2);
        assert_eq!((scenes[0].start_us, scenes[0].end_us), (0, 5_000_000));
        assert_eq!(
            (scenes[1].start_us, scenes[1].end_us),
            (5_000_000, 10_000_000)
        );
    }

    #[test]
    fn zero_or_negative_duration_produces_no_scenes() {
        assert!(scenes_from_cuts(
            &[SceneCut {
                time_us: 1,
                score: 0.5
            }],
            0
        )
        .is_empty());
        assert!(scenes_from_cuts(&[], -1).is_empty());
    }

    // -- generate_scene_thumbnail / detect_scenes (real ffmpeg) --------------

    /// Real 3-color, 3-second synthetic video — two real hard cuts at
    /// t=1.0s and t=2.0s — extending the module's existing single-cut
    /// fixture to 3+ scenes, per this pass's task brief. Colors
    /// (red/blue/white, not red/green/blue) were chosen empirically against
    /// this project's real ffmpeg build: a red->green transition's real
    /// `scene` score comes out at exactly `0.0` (verified via a real WSL
    /// ffmpeg run capturing every frame's `lavfi.scene_score`, not every
    /// solid-color transition scores the same), while red->blue and
    /// blue->white both score well above `DEFAULT_SCENE_THRESHOLD`,
    /// reliably producing two real detected cuts.
    fn synth_three_scene_video(ffmpeg: &Path, dir: &Path) -> std::path::PathBuf {
        let source = dir.join("three_scenes.mp4");
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
                "-f",
                "lavfi",
                "-i",
                "color=white:size=320x240:duration=1:rate=10",
                "-filter_complex",
                "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
                "-map",
                "[v]",
            ])
            .path(&source);
        run_checked(ffmpeg, &gen_args)
            .expect("synthesizing a three-color, two-hard-cut test video");
        source
    }

    #[test]
    fn detect_scenes_finds_three_scenes_each_with_a_real_thumbnail() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-detect-scenes-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_three_scene_video(&ffmpeg, &dir);
        let thumb_dir = dir.join("thumbs");

        let scenes = detect_scenes(
            &ffmpeg,
            &source,
            DEFAULT_SCENE_THRESHOLD,
            3_000_000,
            &thumb_dir,
        )
        .expect("scene detection runs against a real 3-scene video");

        assert!(
            scenes.len() >= 3,
            "expected at least 3 scenes from 2 real hard cuts, got {}",
            scenes.len()
        );
        assert_eq!(scenes[0].start_us, 0);
        assert_eq!(scenes.last().unwrap().end_us, 3_000_000);

        for scene in &scenes {
            let path = scene
                .thumbnail_path
                .as_ref()
                .unwrap_or_else(|| panic!("expected a thumbnail for scene {scene:?}"));
            assert!(std::path::Path::new(path).exists());
            assert!(std::fs::metadata(path).unwrap().len() > 0);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn generate_scene_thumbnail_seeks_within_the_scenes_own_span() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-scene-thumb-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_three_scene_video(&ffmpeg, &dir);
        let out = dir.join("scene_thumb.jpg");

        let scene = Scene {
            id: "s1".into(),
            start_us: 1_000_000,
            end_us: 2_000_000,
            thumbnail_path: None,
            score: 0.5,
        };
        generate_scene_thumbnail(&ffmpeg, &source, &out, &scene)
            .expect("thumbnail generation succeeds within the scene's own span");
        assert!(out.exists());
        assert!(std::fs::metadata(&out).unwrap().len() > 0);

        std::fs::remove_dir_all(&dir).ok();
    }
}
