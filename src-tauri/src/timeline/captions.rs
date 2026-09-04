//! Caption correction operations (master prompt §28): split, merge, retime
//! (drag-boundaries is the same operation from a UI drag gesture — no
//! separate primitive), find/replace, and bulk style. Every function here is
//! a pure builder — reads `&ProjectV1`, returns a `timeline::command::Command`
//! — exactly the same discipline `timeline::ops`/`silence`/`sync` already
//! follow (see `timeline::ops` module doc comment): it never mutates the
//! project itself, so undo/redo for every caption edit falls out of the
//! existing `InsertCaption`/`RemoveCaption`/`SetCaption` primitives and
//! `TimelineSession`'s bounded `History` with no new machinery.
//!
//! ## Retime and word timing
//!
//! Retiming a caption (dragging its start/end boundary) can optionally
//! rescale its `words`' timestamps proportionally to the new span
//! (`scale_words: true`) or leave them untouched (`scale_words: false`).
//! Both are legitimate, so the caller decides explicitly per call rather
//! than this module picking one silently — see `retime_caption`'s doc
//! comment for the tradeoff.
//!
//! ## Find/replace and word timing
//!
//! Master prompt §28: "Maintain timestamps when possible." A text
//! replacement that doesn't change a caption's word *count* leaves
//! `words` untouched — the existing per-word timestamps are still accurate
//! for the new text. A replacement that *does* change the word count makes
//! the old per-word timing meaningless (which word would timestamp N even
//! refer to?), so that caption's `words` is cleared rather than fabricating
//! plausible-looking-but-wrong per-word timestamps — the same honest
//! failure mode `captions::generate` uses for entries with no word-level
//! timing at all.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::project::{Caption, ProjectV1, Word};

use super::command::{
    BatchCommand, Command, InsertCaptionCommand, RemoveCaptionCommand, SetCaptionCommand,
};
use super::error::TimelineError;

pub(crate) fn find_caption<'a>(
    project: &'a ProjectV1,
    caption_id: &str,
) -> Result<&'a Caption, TimelineError> {
    project
        .captions
        .iter()
        .find(|c| c.id == caption_id)
        .ok_or_else(|| TimelineError::CaptionNotFound {
            caption_id: caption_id.to_string(),
        })
}

// ---------------------------------------------------------------------------
// Split
// ---------------------------------------------------------------------------

/// Where to split a caption: either an absolute timeline instant, or a
/// direct index into its `words` (0 = before the first word). Splitting
/// requires per-word timing (`Caption::words` non-empty) — without it there
/// is no principled way to decide which characters of `text` belong to which
/// half, so this is rejected rather than guessed at.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CaptionSplitPoint {
    TimeUs(i64),
    WordIndex(usize),
}

/// Splits `caption_id` into two captions at `split_point`, each with
/// correctly partitioned `words`/`text`/`start_us`/`end_us` — a `Batch` of
/// `RemoveCaption` (the original) + two `InsertCaption`s (master prompt
/// §28).
pub fn split_caption(
    project: &ProjectV1,
    caption_id: &str,
    split_point: CaptionSplitPoint,
) -> Result<Command, TimelineError> {
    let caption = find_caption(project, caption_id)?;
    if caption.words.is_empty() {
        return Err(TimelineError::InvalidCaptionSplit {
            details: "caption has no per-word timing to split by".to_string(),
        });
    }

    let idx = match split_point {
        CaptionSplitPoint::WordIndex(i) => i,
        CaptionSplitPoint::TimeUs(t) => caption
            .words
            .iter()
            .position(|w| w.start_us >= t)
            .unwrap_or(caption.words.len()),
    };
    if idx == 0 || idx >= caption.words.len() {
        return Err(TimelineError::InvalidCaptionSplit {
            details: format!(
                "split point must leave at least one word on each side (got word index {idx} of {})",
                caption.words.len()
            ),
        });
    }

    let first_words = caption.words[..idx].to_vec();
    let second_words = caption.words[idx..].to_vec();
    let first = caption_from_words(caption, &first_words, caption.start_us);
    let second = caption_from_words(caption, &second_words, second_words[0].start_us);

    Ok(Command::Batch(BatchCommand {
        commands: vec![
            Command::RemoveCaption(RemoveCaptionCommand {
                caption: caption.clone(),
            }),
            Command::InsertCaption(InsertCaptionCommand { caption: first }),
            Command::InsertCaption(InsertCaptionCommand { caption: second }),
        ],
    }))
}

fn caption_from_words(template: &Caption, words: &[Word], start_us: i64) -> Caption {
    let text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let end_us = words.last().map(|w| w.end_us).unwrap_or(start_us);
    Caption {
        id: uuid::Uuid::new_v4().to_string(),
        track_id: template.track_id.clone(),
        start_us,
        end_us,
        text,
        words: words.to_vec(),
        style_id: template.style_id.clone(),
    }
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

/// Combines every caption in `caption_ids` (at least two, all on the same
/// track) into one: concatenated text/words in time order, spanning the
/// earliest start to the latest end — a `Batch` of `RemoveCaption`×N + one
/// `InsertCaption` (master prompt §28).
pub fn merge_captions(
    project: &ProjectV1,
    caption_ids: &[String],
) -> Result<Command, TimelineError> {
    if caption_ids.len() < 2 {
        return Err(TimelineError::InvalidCaptionMerge {
            details: "merge needs at least two captions".to_string(),
        });
    }

    let mut captions: Vec<&Caption> = Vec::with_capacity(caption_ids.len());
    for id in caption_ids {
        captions.push(find_caption(project, id)?);
    }
    let track_id = captions[0].track_id.clone();
    if captions.iter().any(|c| c.track_id != track_id) {
        return Err(TimelineError::InvalidCaptionMerge {
            details: "all captions being merged must be on the same track".to_string(),
        });
    }
    captions.sort_by_key(|c| c.start_us);

    let start_us = captions.iter().map(|c| c.start_us).min().unwrap();
    let end_us = captions.iter().map(|c| c.end_us).max().unwrap();
    let text = captions
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let words: Vec<Word> = captions.iter().flat_map(|c| c.words.clone()).collect();
    let style_id = captions[0].style_id.clone();

    let merged = Caption {
        id: uuid::Uuid::new_v4().to_string(),
        track_id,
        start_us,
        end_us,
        text,
        words,
        style_id,
    };

    let mut commands: Vec<Command> = captions
        .iter()
        .map(|c| {
            Command::RemoveCaption(RemoveCaptionCommand {
                caption: (*c).clone(),
            })
        })
        .collect();
    commands.push(Command::InsertCaption(InsertCaptionCommand {
        caption: merged,
    }));

    Ok(Command::Batch(BatchCommand { commands }))
}

// ---------------------------------------------------------------------------
// Retime / drag boundaries
// ---------------------------------------------------------------------------

/// Adjusts `caption_id`'s `start_us`/`end_us` to the given span — the same
/// operation whether it comes from a precise numeric edit or a UI drag
/// gesture on either boundary. `scale_words`:
/// - `true`: every word's timestamp is linearly rescaled to fit the new
///   span, preserving each word's *relative* position within the caption —
///   the right choice when the drag is meant to genuinely re-time the
///   speech (e.g. correcting a caption that drifted out of sync).
/// - `false`: `words` is left exactly as-is — the right choice when the
///   drag is purely a *display* adjustment (e.g. nudging a caption's box
///   earlier so it doesn't visually overlap the next one) that shouldn't
///   pretend the words were spoken at different times than they were.
///
/// Always a single `SetCaption` — never a `Batch` — since exactly one
/// caption is affected.
pub fn retime_caption(
    project: &ProjectV1,
    caption_id: &str,
    new_start_us: i64,
    new_end_us: i64,
    scale_words: bool,
) -> Result<Command, TimelineError> {
    let caption = find_caption(project, caption_id)?;
    if new_end_us <= new_start_us {
        return Err(TimelineError::InvalidCaptionRetime {
            details: format!("end {new_end_us} must be after start {new_start_us}"),
        });
    }

    let mut new_caption = caption.clone();
    new_caption.start_us = new_start_us;
    new_caption.end_us = new_end_us;

    if scale_words && !caption.words.is_empty() {
        let old_span = (caption.end_us - caption.start_us).max(1) as f64;
        let new_span = (new_end_us - new_start_us) as f64;
        let scale = new_span / old_span;
        new_caption.words = caption
            .words
            .iter()
            .map(|w| {
                let rel_start = (w.start_us - caption.start_us) as f64 * scale;
                let rel_end = (w.end_us - caption.start_us) as f64 * scale;
                Word {
                    text: w.text.clone(),
                    start_us: new_start_us + rel_start.round() as i64,
                    end_us: new_start_us + rel_end.round() as i64,
                    confidence: w.confidence,
                }
            })
            .collect();
    }

    Ok(Command::SetCaption(SetCaptionCommand {
        old: caption.clone(),
        new: new_caption,
    }))
}

// ---------------------------------------------------------------------------
// Find / replace
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Type)]
pub struct FindReplaceOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

/// Replaces every match of `find` with `replace` across every caption's
/// `text` in the project — a `Batch` of `SetCaption` for every caption whose
/// text actually changed (captions with no match produce no command at
/// all). See module doc comment for the word-count/timestamp policy.
pub fn find_replace_captions(
    project: &ProjectV1,
    find: &str,
    replace: &str,
    options: FindReplaceOptions,
) -> Result<Command, TimelineError> {
    if find.is_empty() {
        return Err(TimelineError::InvalidFindReplace {
            details: "search string must not be empty".to_string(),
        });
    }

    let mut commands = Vec::new();
    for caption in &project.captions {
        let Some(new_text) = replace_in_text(&caption.text, find, replace, options) else {
            continue;
        };
        if new_text == caption.text {
            continue;
        }
        let mut new_caption = caption.clone();
        let word_count_changed = word_count(&caption.text) != word_count(&new_text);
        new_caption.text = new_text;
        if word_count_changed {
            // Word count changed: the old per-word timestamps no longer
            // correspond to anything real. Drop them rather than fabricate
            // plausible-looking-but-wrong timing (module doc comment).
            new_caption.words = Vec::new();
        }
        commands.push(Command::SetCaption(SetCaptionCommand {
            old: caption.clone(),
            new: new_caption,
        }));
    }

    Ok(Command::Batch(BatchCommand { commands }))
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replaces every occurrence of `find` in `text` with `replace`, honoring
/// `case_sensitive`/`whole_word`. Returns `None` only when `find` is empty
/// (callers already reject that); otherwise always returns `Some`, even when
/// no replacement happened (caller compares against the original to detect
/// a no-op). No external regex dependency — this codebase has none, and the
/// matching needed here (plain substring + an alphanumeric boundary check)
/// doesn't warrant adding one.
fn replace_in_text(
    text: &str,
    find: &str,
    replace: &str,
    options: FindReplaceOptions,
) -> Option<String> {
    if find.is_empty() {
        return None;
    }
    let haystack: Vec<char> = text.chars().collect();
    let needle: Vec<char> = find.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < haystack.len() {
        let fits = i + needle.len() <= haystack.len();
        let content_matches = fits
            && haystack[i..i + needle.len()]
                .iter()
                .zip(needle.iter())
                .all(|(&a, &b)| {
                    if options.case_sensitive {
                        a == b
                    } else {
                        a.to_lowercase().eq(b.to_lowercase())
                    }
                });
        let boundary_ok = if content_matches && options.whole_word {
            let before_ok = i == 0 || !is_word_char(haystack[i - 1]);
            let after_idx = i + needle.len();
            let after_ok = after_idx >= haystack.len() || !is_word_char(haystack[after_idx]);
            before_ok && after_ok
        } else {
            true
        };

        if content_matches && boundary_ok {
            result.push_str(replace);
            i += needle.len();
        } else {
            result.push(haystack[i]);
            i += 1;
        }
    }
    Some(result)
}

// ---------------------------------------------------------------------------
// Bulk style
// ---------------------------------------------------------------------------

/// Applies `style_id` (`None` clears the style back to project default) to
/// every caption in `caption_ids` at once — a `Batch` of `SetCaption` (only
/// for captions whose `style_id` actually changes; already-matching
/// captions produce no command).
pub fn bulk_set_caption_style(
    project: &ProjectV1,
    caption_ids: &[String],
    style_id: Option<String>,
) -> Result<Command, TimelineError> {
    if caption_ids.is_empty() {
        return Err(TimelineError::CaptionNotFound {
            caption_id: "<none provided>".to_string(),
        });
    }
    let mut commands = Vec::new();
    for id in caption_ids {
        let caption = find_caption(project, id)?;
        if caption.style_id == style_id {
            continue;
        }
        let mut new_caption = caption.clone();
        new_caption.style_id = style_id.clone();
        commands.push(Command::SetCaption(SetCaptionCommand {
            old: caption.clone(),
            new: new_caption,
        }));
    }
    Ok(Command::Batch(BatchCommand { commands }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::session::TimelineSession;

    fn word(text: &str, start_us: i64, end_us: i64) -> Word {
        Word {
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.9,
        }
    }

    fn caption_with_words(id: &str, track_id: &str, words: Vec<Word>) -> Caption {
        let start_us = words.first().map(|w| w.start_us).unwrap_or(0);
        let end_us = words.last().map(|w| w.end_us).unwrap_or(0);
        let text = words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Caption {
            id: id.into(),
            track_id: track_id.into(),
            start_us,
            end_us,
            text,
            words,
            style_id: None,
        }
    }

    fn project_with_captions(captions: Vec<Caption>) -> ProjectV1 {
        let mut p = ProjectV1::new("caption ops test");
        p.captions = captions;
        p
    }

    fn apply(project: &mut ProjectV1, command: Command) {
        command.apply(project).expect("apply should succeed");
    }

    // -- split ---------------------------------------------------------

    #[test]
    fn split_by_word_index_partitions_words_text_and_span() {
        let words = vec![
            word("this", 0, 300_000),
            word("is", 300_000, 500_000),
            word("a", 500_000, 600_000),
            word("test", 600_000, 1_000_000),
        ];
        let mut project = project_with_captions(vec![caption_with_words("c1", "t1", words)]);
        let cmd = split_caption(&project, "c1", CaptionSplitPoint::WordIndex(2)).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.captions.len(), 2);
        let first = &project.captions[0];
        let second = &project.captions[1];
        assert_eq!(first.text, "this is");
        assert_eq!(first.start_us, 0);
        assert_eq!(first.end_us, 500_000);
        assert_eq!(second.text, "a test");
        assert_eq!(second.start_us, 500_000);
        assert_eq!(second.end_us, 1_000_000);
        assert_ne!(first.id, "c1");
        assert_ne!(second.id, "c1");
    }

    #[test]
    fn split_by_time_us_finds_the_word_boundary() {
        let words = vec![
            word("one", 0, 100),
            word("two", 100, 200),
            word("three", 200, 300),
        ];
        let mut project = project_with_captions(vec![caption_with_words("c1", "t1", words)]);
        let cmd = split_caption(&project, "c1", CaptionSplitPoint::TimeUs(150)).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.captions.len(), 2);
        assert_eq!(project.captions[0].text, "one two");
        assert_eq!(project.captions[1].text, "three");
    }

    #[test]
    fn split_without_word_timing_is_rejected() {
        let mut c = caption_with_words("c1", "t1", Vec::new());
        c.text = "no word timing".into();
        c.start_us = 0;
        c.end_us = 1_000_000;
        let project = project_with_captions(vec![c]);
        assert!(matches!(
            split_caption(&project, "c1", CaptionSplitPoint::WordIndex(1)).unwrap_err(),
            TimelineError::InvalidCaptionSplit { .. }
        ));
    }

    #[test]
    fn split_at_edge_index_is_rejected() {
        let words = vec![word("one", 0, 100), word("two", 100, 200)];
        let project = project_with_captions(vec![caption_with_words("c1", "t1", words)]);
        assert!(matches!(
            split_caption(&project, "c1", CaptionSplitPoint::WordIndex(0)).unwrap_err(),
            TimelineError::InvalidCaptionSplit { .. }
        ));
        assert!(matches!(
            split_caption(&project, "c1", CaptionSplitPoint::WordIndex(2)).unwrap_err(),
            TimelineError::InvalidCaptionSplit { .. }
        ));
    }

    #[test]
    fn split_undo_redo_round_trips_through_session() {
        let words = vec![
            word("one", 0, 100),
            word("two", 100, 200),
            word("three", 200, 300),
        ];
        let mut session = TimelineSession::new(project_with_captions(vec![caption_with_words(
            "c1", "t1", words,
        )]));
        let before = serde_json::to_value(&session.project).unwrap();

        let cmd = split_caption(&session.project, "c1", CaptionSplitPoint::WordIndex(1)).unwrap();
        session.apply(cmd).unwrap();
        assert_eq!(session.project.captions.len(), 2);

        session.undo().unwrap();
        assert_eq!(serde_json::to_value(&session.project).unwrap(), before);
        session.redo().unwrap();
        assert_eq!(session.project.captions.len(), 2);
    }

    // -- merge -----------------------------------------------------------

    #[test]
    fn merge_combines_text_words_and_span() {
        let c1 = caption_with_words("c1", "t1", vec![word("hello", 0, 300_000)]);
        let c2 = caption_with_words("c2", "t1", vec![word("world", 400_000, 800_000)]);
        let mut project = project_with_captions(vec![c1, c2]);
        let cmd = merge_captions(&project, &["c1".to_string(), "c2".to_string()]).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.captions.len(), 1);
        let merged = &project.captions[0];
        assert_eq!(merged.text, "hello world");
        assert_eq!(merged.start_us, 0);
        assert_eq!(merged.end_us, 800_000);
        assert_eq!(merged.words.len(), 2);
    }

    #[test]
    fn merge_out_of_order_ids_still_orders_by_start_time() {
        let c1 = caption_with_words("c1", "t1", vec![word("second", 400_000, 800_000)]);
        let c2 = caption_with_words("c2", "t1", vec![word("first", 0, 300_000)]);
        let project = project_with_captions(vec![c1, c2]);
        // Pass ids in reverse chronological order deliberately.
        let cmd = merge_captions(&project, &["c1".to_string(), "c2".to_string()]).unwrap();
        let mut project = project;
        apply(&mut project, cmd);
        assert_eq!(project.captions[0].text, "first second");
    }

    #[test]
    fn merge_rejects_fewer_than_two_captions() {
        let c1 = caption_with_words("c1", "t1", vec![word("hi", 0, 100)]);
        let project = project_with_captions(vec![c1]);
        assert!(matches!(
            merge_captions(&project, &["c1".to_string()]).unwrap_err(),
            TimelineError::InvalidCaptionMerge { .. }
        ));
    }

    #[test]
    fn merge_rejects_captions_on_different_tracks() {
        let c1 = caption_with_words("c1", "t1", vec![word("hi", 0, 100)]);
        let c2 = caption_with_words("c2", "t2", vec![word("there", 100, 200)]);
        let project = project_with_captions(vec![c1, c2]);
        assert!(matches!(
            merge_captions(&project, &["c1".to_string(), "c2".to_string()]).unwrap_err(),
            TimelineError::InvalidCaptionMerge { .. }
        ));
    }

    #[test]
    fn merge_undo_redo_round_trips_through_session() {
        let c1 = caption_with_words("c1", "t1", vec![word("hello", 0, 300_000)]);
        let c2 = caption_with_words("c2", "t1", vec![word("world", 400_000, 800_000)]);
        let mut session = TimelineSession::new(project_with_captions(vec![c1, c2]));
        let before = serde_json::to_value(&session.project).unwrap();

        let cmd = merge_captions(&session.project, &["c1".to_string(), "c2".to_string()]).unwrap();
        session.apply(cmd).unwrap();
        assert_eq!(session.project.captions.len(), 1);

        session.undo().unwrap();
        assert_eq!(serde_json::to_value(&session.project).unwrap(), before);
        session.redo().unwrap();
        assert_eq!(session.project.captions.len(), 1);
    }

    // -- retime ------------------------------------------------------------

    #[test]
    fn retime_with_scaling_rescales_word_timestamps_proportionally() {
        let words = vec![word("a", 0, 500_000), word("b", 500_000, 1_000_000)];
        let mut project = project_with_captions(vec![caption_with_words("c1", "t1", words)]);
        // Double the span: 0..1_000_000 -> 0..2_000_000.
        let cmd = retime_caption(&project, "c1", 0, 2_000_000, true).unwrap();
        apply(&mut project, cmd);

        let c = &project.captions[0];
        assert_eq!(c.start_us, 0);
        assert_eq!(c.end_us, 2_000_000);
        assert_eq!(c.words[0].start_us, 0);
        assert_eq!(c.words[0].end_us, 1_000_000);
        assert_eq!(c.words[1].start_us, 1_000_000);
        assert_eq!(c.words[1].end_us, 2_000_000);
    }

    #[test]
    fn retime_without_scaling_leaves_words_untouched() {
        let words = vec![word("a", 0, 500_000), word("b", 500_000, 1_000_000)];
        let mut project =
            project_with_captions(vec![caption_with_words("c1", "t1", words.clone())]);
        let cmd = retime_caption(&project, "c1", 0, 2_000_000, false).unwrap();
        apply(&mut project, cmd);

        let c = &project.captions[0];
        assert_eq!(c.start_us, 0);
        assert_eq!(c.end_us, 2_000_000);
        assert_eq!(c.words, words);
    }

    #[test]
    fn retime_rejects_end_before_start() {
        let words = vec![word("a", 0, 100)];
        let project = project_with_captions(vec![caption_with_words("c1", "t1", words)]);
        assert!(matches!(
            retime_caption(&project, "c1", 100, 50, true).unwrap_err(),
            TimelineError::InvalidCaptionRetime { .. }
        ));
    }

    #[test]
    fn retime_undo_redo_round_trips_through_session() {
        let words = vec![word("a", 0, 500_000), word("b", 500_000, 1_000_000)];
        let mut session = TimelineSession::new(project_with_captions(vec![caption_with_words(
            "c1", "t1", words,
        )]));
        let before = serde_json::to_value(&session.project).unwrap();

        let cmd = retime_caption(&session.project, "c1", 0, 2_000_000, true).unwrap();
        session.apply(cmd).unwrap();
        assert_eq!(session.project.captions[0].end_us, 2_000_000);

        session.undo().unwrap();
        assert_eq!(serde_json::to_value(&session.project).unwrap(), before);
        session.redo().unwrap();
        assert_eq!(session.project.captions[0].end_us, 2_000_000);
    }

    // -- find/replace --------------------------------------------------

    #[test]
    fn find_replace_same_word_count_keeps_word_timing() {
        let words = vec![word("hello", 0, 300_000), word("world", 300_000, 600_000)];
        let mut project =
            project_with_captions(vec![caption_with_words("c1", "t1", words.clone())]);
        let cmd = find_replace_captions(&project, "world", "there", FindReplaceOptions::default())
            .unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.captions[0].text, "hello there");
        assert_eq!(project.captions[0].words, words); // timing untouched
    }

    #[test]
    fn find_replace_changing_word_count_drops_word_timing() {
        let words = vec![word("hello", 0, 300_000), word("world", 300_000, 600_000)];
        let mut project = project_with_captions(vec![caption_with_words("c1", "t1", words)]);
        let cmd = find_replace_captions(
            &project,
            "world",
            "there world",
            FindReplaceOptions::default(),
        )
        .unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.captions[0].text, "hello there world");
        assert!(project.captions[0].words.is_empty());
    }

    #[test]
    fn find_replace_is_case_insensitive_by_default() {
        let mut c = caption_with_words("c1", "t1", Vec::new());
        c.text = "Hello HELLO hello".into();
        let mut project = project_with_captions(vec![c]);
        let cmd =
            find_replace_captions(&project, "hello", "hi", FindReplaceOptions::default()).unwrap();
        apply(&mut project, cmd);
        assert_eq!(project.captions[0].text, "hi hi hi");
    }

    #[test]
    fn find_replace_case_sensitive_only_matches_exact_case() {
        let mut c = caption_with_words("c1", "t1", Vec::new());
        c.text = "Hello hello".into();
        let mut project = project_with_captions(vec![c]);
        let cmd = find_replace_captions(
            &project,
            "hello",
            "hi",
            FindReplaceOptions {
                case_sensitive: true,
                whole_word: false,
            },
        )
        .unwrap();
        apply(&mut project, cmd);
        assert_eq!(project.captions[0].text, "Hello hi");
    }

    #[test]
    fn find_replace_whole_word_does_not_match_inside_a_larger_word() {
        let mut c = caption_with_words("c1", "t1", Vec::new());
        c.text = "cat catalog cats".into();
        let mut project = project_with_captions(vec![c]);
        let cmd = find_replace_captions(
            &project,
            "cat",
            "dog",
            FindReplaceOptions {
                case_sensitive: false,
                whole_word: true,
            },
        )
        .unwrap();
        apply(&mut project, cmd);
        assert_eq!(project.captions[0].text, "dog catalog cats");
    }

    #[test]
    fn find_replace_with_no_matches_produces_no_commands() {
        let mut c = caption_with_words("c1", "t1", Vec::new());
        c.text = "nothing to see here".into();
        let project = project_with_captions(vec![c]);
        let cmd =
            find_replace_captions(&project, "xyz", "abc", FindReplaceOptions::default()).unwrap();
        match cmd {
            Command::Batch(b) => assert!(b.commands.is_empty()),
            _ => panic!("expected an (empty) batch"),
        }
    }

    #[test]
    fn find_replace_rejects_empty_search_string() {
        let project = project_with_captions(vec![]);
        assert!(matches!(
            find_replace_captions(&project, "", "x", FindReplaceOptions::default()).unwrap_err(),
            TimelineError::InvalidFindReplace { .. }
        ));
    }

    #[test]
    fn find_replace_undo_redo_round_trips_through_session() {
        let mut c = caption_with_words("c1", "t1", Vec::new());
        c.text = "hello world".into();
        let mut session = TimelineSession::new(project_with_captions(vec![c]));
        let before = serde_json::to_value(&session.project).unwrap();

        let cmd = find_replace_captions(
            &session.project,
            "world",
            "there",
            FindReplaceOptions::default(),
        )
        .unwrap();
        session.apply(cmd).unwrap();
        assert_eq!(session.project.captions[0].text, "hello there");

        session.undo().unwrap();
        assert_eq!(serde_json::to_value(&session.project).unwrap(), before);
        session.redo().unwrap();
        assert_eq!(session.project.captions[0].text, "hello there");
    }

    // -- bulk style ----------------------------------------------------

    #[test]
    fn bulk_set_style_applies_to_every_listed_caption() {
        let c1 = caption_with_words("c1", "t1", vec![word("a", 0, 100)]);
        let c2 = caption_with_words("c2", "t1", vec![word("b", 100, 200)]);
        let mut project = project_with_captions(vec![c1, c2]);
        let cmd = bulk_set_caption_style(
            &project,
            &["c1".to_string(), "c2".to_string()],
            Some("template_tiktok".to_string()),
        )
        .unwrap();
        apply(&mut project, cmd);

        assert_eq!(
            project.captions[0].style_id.as_deref(),
            Some("template_tiktok")
        );
        assert_eq!(
            project.captions[1].style_id.as_deref(),
            Some("template_tiktok")
        );
    }

    #[test]
    fn bulk_set_style_skips_captions_already_at_that_style() {
        let mut c1 = caption_with_words("c1", "t1", vec![word("a", 0, 100)]);
        c1.style_id = Some("template_minimal".to_string());
        let project = project_with_captions(vec![c1]);
        let cmd = bulk_set_caption_style(
            &project,
            &["c1".to_string()],
            Some("template_minimal".to_string()),
        )
        .unwrap();
        match cmd {
            Command::Batch(b) => assert!(b.commands.is_empty()),
            _ => panic!("expected an (empty) batch"),
        }
    }

    #[test]
    fn bulk_set_style_rejects_empty_id_list() {
        let project = project_with_captions(vec![]);
        assert!(matches!(
            bulk_set_caption_style(&project, &[], None).unwrap_err(),
            TimelineError::CaptionNotFound { .. }
        ));
    }

    #[test]
    fn bulk_set_style_undo_redo_round_trips_through_session() {
        let c1 = caption_with_words("c1", "t1", vec![word("a", 0, 100)]);
        let mut session = TimelineSession::new(project_with_captions(vec![c1]));
        let before = serde_json::to_value(&session.project).unwrap();

        let cmd = bulk_set_caption_style(
            &session.project,
            &["c1".to_string()],
            Some("template_tiktok".to_string()),
        )
        .unwrap();
        session.apply(cmd).unwrap();
        assert_eq!(
            session.project.captions[0].style_id.as_deref(),
            Some("template_tiktok")
        );

        session.undo().unwrap();
        assert_eq!(serde_json::to_value(&session.project).unwrap(), before);
        session.redo().unwrap();
        assert_eq!(
            session.project.captions[0].style_id.as_deref(),
            Some("template_tiktok")
        );
    }
}
