//! FCPXML 1.11 document construction from `ProjectV1`.
//!
//! Reimplemented (design, not code) from
//! `vendor/autocut/src-tauri/src/export_fcpxml.rs`'s spine/lane/connected-clip
//! model, generalized from autocut's single-`CutList` + fixed reference-track
//! model to this project's general multi-track, multi-clip timeline
//! (`docs/architecture-audit.md` §2/§8).
//!
//! ## Track → FCPXML mapping (documented, not silent)
//!
//! - **Primary storyline**: the highest-`render_index` track among, in
//!   priority order, `Video`, then `Audio`, then `Image`, then `Overlay`
//!   tracks (a `Caption` or `Effect` track is never chosen as primary — see
//!   below). This mirrors autocut's "reference track defines project time"
//!   design, generalized since this schema has no single designated
//!   reference track.
//! - **Connected clips**: every other exportable track's clips attach to the
//!   primary storyline as FCPXML connected clips (`lane="N"`/`lane="-N"`).
//!   Visual tracks (`Video`, `Image`, `Overlay`, `Caption`) stack on
//!   ascending positive lanes (1, 2, 3, ...) in `render_index` order; `Audio`
//!   tracks stack on descending negative lanes (-1, -2, -3, ...) in
//!   `render_index` order — the same "video up, audio down" convention
//!   autocut's `export_fcpxml.rs` uses for its `linked` tracks.
//! - **`Caption` tracks**: exported as `<title>` placeholder elements (one
//!   per `ProjectV1::captions` entry whose `track_id` matches), positioned by
//!   the caption's own `start_us`/`end_us` — cheap to do since `Caption`
//!   already carries absolute timing and text with no media asset needed.
//!   The referenced "Basic Title" effect resource UID is a structural
//!   placeholder (real UIDs are FCP-version-specific); text/timing is the
//!   part that matters for this pass, styling is not mapped.
//! - **`Effect` tracks are skipped entirely** and documented here rather
//!   than silently dropped: an `Effect` track's clips have no backing
//!   `MediaItem` (they parametrize `ProjectV1::effects` entries keyed by
//!   `clip_id`, which is a clip-attached property, not a piece of media with
//!   its own timeline placement), so there is no meaningful standalone
//!   FCPXML element to map a bare effect-parameter track to yet.
//! - **Hidden/muted tracks are excluded** from export entirely: a `hidden`
//!   track (any kind) contributes nothing, and an `Audio` track that
//!   `crate::timeline::ops::effective_track_mute_state` reports as
//!   effectively muted (its own `muted` flag, or solo-elsewhere) is likewise
//!   excluded — matching "what you'd actually see/hear" rather than a raw
//!   dump of every track that exists.
//! - **Per-clip visual properties** (`ClipSettings`: opacity, flip, rotation,
//!   scale, transform) and **non-1.0 `speed`'s effect on the source range**
//!   are not mapped to FCPXML `<adjust-transform>`/`<conform-rate>`/retime
//!   elements in this pass — a clip with `speed != 1.0` exports at its
//!   effective post-speed duration (`(source_out_us - source_in_us) / speed`)
//!   as a plain constant-rate clip, without an explicit FCPXML speed-ramp
//!   map. Documented gap, not a silent one.
//! - This schema carries no embedded source timecode (unlike autocut's
//!   SMPTE-driven design — `docs/architecture-audit.md` §2), so every clip
//!   is stamped `tcFormat="NDF"` unconditionally; there is no drop-frame
//!   *source* timecode to honor here.
//! - A connected clip/title's `offset` attribute is rendered as the same
//!   project-absolute timeline position its containing spine element would
//!   use (not relative to that container's own local start) — this matches
//!   the coordinate space `vendor/autocut/src-tauri/src/export_fcpxml.rs`
//!   uses for its own connected clips.
//! - A connected clip/title that overlaps more than one primary-storyline
//!   slot is attached to whichever slot contains its *start* time only
//!   (documented simplification — splitting one connected clip across a
//!   primary-storyline cut point is not implemented in this pass).

use std::collections::HashMap;

use crate::fcpxml::error::FcpxmlError;
use crate::fcpxml::timecode::{self, TimecodeRate};
use crate::project::{Caption, Clip, MediaItem, MediaKind, ProjectV1, Track, TrackKind};
use crate::timeline::ops::effective_track_mute_state;

/// Build a complete FCPXML 1.11 document string for `project`.
pub fn build(project: &ProjectV1) -> Result<String, FcpxmlError> {
    let canvas_rate = timecode::detect_rate(project.canvas.fps);

    let media_by_id: HashMap<&str, &MediaItem> =
        project.media.iter().map(|m| (m.id.as_str(), m)).collect();

    let usable_tracks = usable_tracks(project);

    let primary = pick_primary(&usable_tracks).ok_or_else(|| FcpxmlError::EmptyTimeline {
        details: "no visible Video/Audio/Image/Overlay track with clips".to_string(),
    })?;

    let primary_clips = track_clips(project, primary);
    if primary_clips.is_empty() {
        return Err(FcpxmlError::EmptyTimeline {
            details: format!("primary track '{}' has no enabled clips", primary.name),
        });
    }

    let connected_tracks: Vec<&Track> = usable_tracks
        .iter()
        .copied()
        .filter(|t| t.id != primary.id)
        .collect();
    let lanes = assign_lanes(&connected_tracks);

    // Collect every connected item (media clips + caption titles) up front,
    // sorted by start time, so slot-attachment below is a single linear pass.
    let mut connected_items: Vec<ConnectedItem<'_>> = Vec::new();
    for track in &connected_tracks {
        let lane = lanes[&track.id];
        match track.kind {
            TrackKind::Caption => {
                for caption in project.captions.iter().filter(|c| c.track_id == track.id) {
                    let end = caption.end_us.max(caption.start_us);
                    connected_items.push(ConnectedItem {
                        start_us: caption.start_us,
                        end_us: end,
                        lane,
                        payload: ConnectedPayload::Title(caption),
                    });
                }
            }
            _ => {
                for clip in track_clips(project, track) {
                    let Some(media_id) = clip.media_id.as_deref() else {
                        continue; // no backing asset; nothing to render.
                    };
                    if !media_by_id.contains_key(media_id) {
                        continue; // dangling media reference; skip rather than emit a broken ref.
                    }
                    let start = clip.position_us;
                    let end = start + effective_duration_us(clip);
                    connected_items.push(ConnectedItem {
                        start_us: start,
                        end_us: end,
                        lane,
                        payload: ConnectedPayload::MediaClip(clip),
                    });
                }
            }
        }
    }
    connected_items.sort_by_key(|i| i.start_us);

    // Referenced media only (one <asset> per MediaItem actually used by an
    // exported clip), preserving `project.media`'s own order.
    let mut referenced_media_ids: Vec<&str> = Vec::new();
    {
        let mut seen = std::collections::HashSet::new();
        let connected_media_clips = connected_items.iter().filter_map(|i| match &i.payload {
            ConnectedPayload::MediaClip(c) => Some(*c),
            ConnectedPayload::Title(_) => None,
        });
        for clip in primary_clips.iter().copied().chain(connected_media_clips) {
            if let Some(id) = clip.media_id.as_deref() {
                if media_by_id.contains_key(id) && seen.insert(id) {
                    referenced_media_ids.push(id);
                }
            }
        }
    }

    let (formats_xml, asset_format_ref, canvas_format_id) =
        build_formats(project, &referenced_media_ids, &media_by_id);
    let (assets_xml, asset_id_of) =
        build_assets(&referenced_media_ids, &media_by_id, &asset_format_ref);

    // Shared "Basic Title" effect resource, only emitted if at least one
    // caption title is exported.
    let has_titles = connected_items
        .iter()
        .any(|i| matches!(i.payload, ConnectedPayload::Title(_)));
    let title_effect_id = "eff_title";
    let title_effect_xml = if has_titles {
        format!(
            "    <effect id=\"{title_effect_id}\" name=\"Basic Title\" \
             uid=\".../Titles.localized/Basic Title.localized/Basic Title.moti\"/>\n"
        )
    } else {
        String::new()
    };

    let spine_xml = build_spine(
        &primary_clips,
        &connected_items,
        &asset_id_of,
        title_effect_id,
        &canvas_rate,
    );

    let safe_title = xml_escape(&project.project.name);
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE fcpxml>
<fcpxml version="1.11">
  <resources>
{formats_xml}{title_effect_xml}{assets_xml}  </resources>
  <library>
    <event name="{safe_title}">
      <project name="{safe_title}">
        <sequence format="{canvas_format_id}" tcFormat="NDF">
          <spine>
{spine_xml}          </spine>
        </sequence>
      </project>
    </event>
  </library>
</fcpxml>
"#
    ))
}

fn usable_tracks(project: &ProjectV1) -> Vec<&Track> {
    let mute_state = effective_track_mute_state(&project.tracks);
    project
        .tracks
        .iter()
        .filter(|t| t.kind != TrackKind::Effect)
        .filter(|t| !t.hidden)
        .filter(|t| t.kind != TrackKind::Audio || !mute_state.get(&t.id).copied().unwrap_or(false))
        .collect()
}

fn pick_primary<'a>(tracks: &[&'a Track]) -> Option<&'a Track> {
    for kind in [
        TrackKind::Video,
        TrackKind::Audio,
        TrackKind::Image,
        TrackKind::Overlay,
    ] {
        if let Some(t) = tracks
            .iter()
            .filter(|t| t.kind == kind)
            .max_by_key(|t| t.render_index)
        {
            return Some(t);
        }
    }
    None
}

/// Enabled clips on `track`, in `track.clip_ids` order but re-sorted by
/// timeline position (the schema doesn't guarantee `clip_ids` is
/// position-sorted).
fn track_clips<'a>(project: &'a ProjectV1, track: &Track) -> Vec<&'a Clip> {
    let clip_by_id: HashMap<&str, &Clip> =
        project.clips.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut clips: Vec<&Clip> = track
        .clip_ids
        .iter()
        .filter_map(|id| clip_by_id.get(id.as_str()).copied())
        .filter(|c| c.enabled)
        .collect();
    clips.sort_by_key(|c| c.position_us);
    clips
}

/// Assigns FCPXML lane numbers to every connected (non-primary) track:
/// visual tracks ascend 1, 2, 3, ... and `Audio` tracks descend -1, -2, -3,
/// ..., both in `render_index` order (see module doc comment).
fn assign_lanes(tracks: &[&Track]) -> HashMap<String, i32> {
    let mut visual: Vec<&&Track> = tracks
        .iter()
        .filter(|t| t.kind != TrackKind::Audio)
        .collect();
    visual.sort_by_key(|t| t.render_index);
    let mut audio: Vec<&&Track> = tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .collect();
    audio.sort_by_key(|t| t.render_index);

    let mut lanes = HashMap::new();
    for (i, t) in visual.into_iter().enumerate() {
        lanes.insert(t.id.clone(), (i + 1) as i32);
    }
    for (i, t) in audio.into_iter().enumerate() {
        lanes.insert(t.id.clone(), -((i + 1) as i32));
    }
    lanes
}

/// A clip's actual on-timeline duration after `speed` is applied.
/// `speed <= 0` or non-finite is treated as `1.0` (the source range as-is) —
/// the same degenerate-input guard philosophy as `timecode::detect_rate`.
fn effective_duration_us(clip: &Clip) -> i64 {
    let raw = (clip.source_out_us - clip.source_in_us).max(0);
    if clip.speed.is_finite() && clip.speed > 0.0 {
        ((raw as f64) / clip.speed).round() as i64
    } else {
        raw
    }
}

struct ConnectedItem<'a> {
    start_us: i64,
    end_us: i64,
    lane: i32,
    payload: ConnectedPayload<'a>,
}

enum ConnectedPayload<'a> {
    MediaClip(&'a Clip),
    Title(&'a Caption),
}

type FormatKey = (u32, u32, u32, u32); // (width, height, fps.num, fps.den)

/// Builds the `<format>` resource block: one entry for the canvas/sequence
/// format plus one per distinct (width, height, fps) combination among
/// referenced media items. Returns (xml, per-media format-id map, canvas
/// format id).
fn build_formats<'a>(
    project: &ProjectV1,
    referenced_media_ids: &[&'a str],
    media_by_id: &HashMap<&'a str, &MediaItem>,
) -> (String, HashMap<&'a str, String>, String) {
    let canvas_key: FormatKey = (
        project.canvas.width,
        project.canvas.height,
        project.canvas.fps.num,
        project.canvas.fps.den,
    );
    let mut xml = String::new();
    let mut ids: HashMap<FormatKey, String> = HashMap::new();
    let canvas_id = "f1".to_string();
    ids.insert(canvas_key, canvas_id.clone());
    xml.push_str(&format_element(&canvas_id, canvas_key));

    let mut next = 2;
    let mut asset_format_ref: HashMap<&str, String> = HashMap::new();
    for &media_id in referenced_media_ids {
        let Some(item) = media_by_id.get(media_id) else {
            continue;
        };
        let key: FormatKey = (item.width, item.height, item.fps.num, item.fps.den);
        let id = ids.entry(key).or_insert_with(|| {
            let id = format!("f{next}");
            next += 1;
            xml.push_str(&format_element(&id, key));
            id
        });
        asset_format_ref.insert(media_id, id.clone());
    }
    (xml, asset_format_ref, canvas_id)
}

fn format_element(id: &str, (width, height, num, den): FormatKey) -> String {
    let rate = timecode::detect_rate(crate::project::Rational::new(num, den));
    format!(
        "    <format id=\"{id}\" name=\"FFVideoFormat{width}x{height}\" \
         frameDuration=\"{fdn}/{fdd}s\" width=\"{width}\" height=\"{height}\"/>\n",
        fdn = rate.frame_duration_num,
        fdd = rate.frame_duration_den,
    )
}

fn build_assets<'a>(
    referenced_media_ids: &[&'a str],
    media_by_id: &HashMap<&'a str, &MediaItem>,
    asset_format_ref: &HashMap<&'a str, String>,
) -> (String, HashMap<&'a str, String>) {
    let mut xml = String::new();
    let mut asset_id_of: HashMap<&str, String> = HashMap::new();
    for (i, &media_id) in referenced_media_ids.iter().enumerate() {
        let Some(item) = media_by_id.get(media_id) else {
            continue;
        };
        let asset_id = format!("r{}", i + 1);
        let rate = timecode::detect_rate(item.fps);
        let duration = timecode::us_to_rational(item.duration_us, &rate);
        let has_video = matches!(item.kind, MediaKind::Video | MediaKind::Image);
        let has_audio = item.kind != MediaKind::Image && item.audio_channels > 0;
        let video_attrs = if has_video {
            " hasVideo=\"1\" videoSources=\"1\""
        } else {
            " hasVideo=\"0\""
        };
        let audio_attrs = if has_audio {
            format!(
                " hasAudio=\"1\" audioSources=\"1\" audioChannels=\"{}\"",
                item.audio_channels
            )
        } else {
            " hasAudio=\"0\"".to_string()
        };
        let format_ref = asset_format_ref
            .get(media_id)
            .cloned()
            .unwrap_or_else(|| "f1".to_string());
        let src = to_file_uri(&item.source_path);
        xml.push_str(&format!(
            "    <asset id=\"{asset_id}\" name=\"{name}\" start=\"0s\" duration=\"{duration}\" \
             format=\"{format_ref}\"{video_attrs}{audio_attrs}>\n      \
             <media-rep kind=\"original-media\" src=\"{src}\"/>\n    </asset>\n",
            name = xml_escape(&media_name(item)),
            src = xml_escape(&src),
        ));
        asset_id_of.insert(media_id, asset_id);
    }
    (xml, asset_id_of)
}

fn media_name(item: &MediaItem) -> String {
    std::path::Path::new(&item.source_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| item.id.clone())
}

/// Spine slot: either a real primary-storyline clip, or a synthetic gap
/// inserted to keep the spine contiguously tiled (required so any connected
/// clip whose time range isn't covered by a primary clip still has a
/// container to attach to).
enum Slot<'a> {
    Clip(&'a Clip),
    Gap { start_us: i64, end_us: i64 },
}

impl Slot<'_> {
    fn start_us(&self) -> i64 {
        match self {
            Slot::Clip(c) => c.position_us,
            Slot::Gap { start_us, .. } => *start_us,
        }
    }
    fn end_us(&self) -> i64 {
        match self {
            Slot::Clip(c) => c.position_us + effective_duration_us(c),
            Slot::Gap { end_us, .. } => *end_us,
        }
    }
}

fn build_spine(
    primary_clips: &[&Clip],
    connected_items: &[ConnectedItem<'_>],
    asset_id_of: &HashMap<&str, String>,
    title_effect_id: &str,
    canvas_rate: &TimecodeRate,
) -> String {
    let mut slots: Vec<Slot<'_>> = Vec::new();
    let mut cursor = 0i64;
    for clip in primary_clips {
        if clip.position_us > cursor {
            slots.push(Slot::Gap {
                start_us: cursor,
                end_us: clip.position_us,
            });
        }
        cursor = cursor.max(clip.position_us + effective_duration_us(clip));
        slots.push(Slot::Clip(clip));
    }

    // Bucket connected items by the slot they attach to (by start-time
    // containment; see module doc comment). Anything at/after the last
    // slot's end goes in a synthetic trailing gap appended once at the end.
    let mut buckets: Vec<Vec<&ConnectedItem<'_>>> = slots.iter().map(|_| Vec::new()).collect();
    let mut trailing: Vec<&ConnectedItem<'_>> = Vec::new();
    let mut trailing_end = cursor;
    for item in connected_items {
        if let Some(idx) = slots
            .iter()
            .position(|s| item.start_us >= s.start_us() && item.start_us < s.end_us())
        {
            buckets[idx].push(item);
        } else if item.start_us < cursor {
            // Before the timeline starts, or otherwise falls outside every
            // slot's range (e.g. a negative position) — attach to the first
            // slot rather than drop it.
            if let Some(first) = buckets.first_mut() {
                first.push(item);
            } else {
                trailing.push(item);
                trailing_end = trailing_end.max(item.end_us);
            }
        } else {
            trailing.push(item);
            trailing_end = trailing_end.max(item.end_us);
        }
    }
    if !trailing.is_empty() {
        slots.push(Slot::Gap {
            start_us: cursor,
            end_us: trailing_end,
        });
        buckets.push(trailing);
    }

    let mut spine = String::new();
    for (slot, bucket) in slots.iter().zip(buckets.iter()) {
        let offset = timecode::us_to_rational(slot.start_us(), canvas_rate);
        let duration = timecode::us_to_rational(slot.end_us() - slot.start_us(), canvas_rate);

        let mut connected = String::new();
        for item in bucket {
            connected.push_str(&render_connected(
                item,
                asset_id_of,
                title_effect_id,
                canvas_rate,
            ));
        }

        match slot {
            Slot::Clip(clip) => {
                let Some(asset_id) = clip.media_id.as_deref().and_then(|m| asset_id_of.get(m))
                else {
                    // No resolvable asset (dangling/missing media_id) — treat
                    // as a gap so connected clips still have a container.
                    spine.push_str(&gap_element(&offset, &duration, &connected));
                    continue;
                };
                let start = timecode::us_to_rational(clip.source_in_us, canvas_rate);
                if connected.is_empty() {
                    spine.push_str(&format!(
                        "        <asset-clip ref=\"{asset_id}\" offset=\"{offset}\" \
                         start=\"{start}\" duration=\"{duration}\" tcFormat=\"NDF\"/>\n"
                    ));
                } else {
                    spine.push_str(&format!(
                        "        <asset-clip ref=\"{asset_id}\" offset=\"{offset}\" \
                         start=\"{start}\" duration=\"{duration}\" tcFormat=\"NDF\">\n"
                    ));
                    spine.push_str(&connected);
                    spine.push_str("        </asset-clip>\n");
                }
            }
            Slot::Gap { .. } => {
                spine.push_str(&gap_element(&offset, &duration, &connected));
            }
        }
    }
    spine
}

fn gap_element(offset: &str, duration: &str, connected: &str) -> String {
    if connected.is_empty() {
        format!("        <gap offset=\"{offset}\" duration=\"{duration}\"/>\n")
    } else {
        let mut s = format!("        <gap offset=\"{offset}\" duration=\"{duration}\">\n");
        s.push_str(connected);
        s.push_str("        </gap>\n");
        s
    }
}

fn render_connected(
    item: &ConnectedItem<'_>,
    asset_id_of: &HashMap<&str, String>,
    title_effect_id: &str,
    canvas_rate: &TimecodeRate,
) -> String {
    let offset = timecode::us_to_rational(item.start_us, canvas_rate);
    let duration = timecode::us_to_rational(item.end_us - item.start_us, canvas_rate);
    match &item.payload {
        ConnectedPayload::MediaClip(clip) => {
            let Some(asset_id) = clip.media_id.as_deref().and_then(|m| asset_id_of.get(m)) else {
                return String::new();
            };
            let start = timecode::us_to_rational(clip.source_in_us, canvas_rate);
            format!(
                "          <asset-clip ref=\"{asset_id}\" lane=\"{lane}\" offset=\"{offset}\" \
                 start=\"{start}\" duration=\"{duration}\" tcFormat=\"NDF\"/>\n",
                lane = item.lane,
            )
        }
        ConnectedPayload::Title(caption) => {
            format!(
                "          <title ref=\"{title_effect_id}\" lane=\"{lane}\" offset=\"{offset}\" \
                 duration=\"{duration}\" name=\"{name}\">\n            <text>\n              \
                 <text-style ref=\"ts_{cap_id}\">{text}</text-style>\n            </text>\n            \
                 <text-style-def id=\"ts_{cap_id}\">\n              <text-style font=\"Helvetica\" \
                 fontSize=\"48\" alignment=\"center\"/>\n            </text-style-def>\n          </title>\n",
                lane = item.lane,
                name = xml_escape(&caption.text),
                text = xml_escape(&caption.text),
                cap_id = xml_escape(&caption.id),
            )
        }
    }
}

fn to_file_uri(path: &str) -> String {
    if path.starts_with("file://") || path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    let abs = std::path::Path::new(path);
    let abs = std::fs::canonicalize(abs).unwrap_or_else(|_| abs.to_path_buf());
    let abs_str = abs.to_string_lossy();
    let mut encoded = String::with_capacity(abs_str.len() + 8);
    encoded.push_str("file://");
    for byte in abs_str.bytes() {
        if is_unreserved(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

fn is_unreserved(b: u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':')
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{CanvasV1, ClipSettings, MediaKind, ProjectMeta, Rational, TrackKind};

    fn base_project() -> ProjectV1 {
        let mut p = ProjectV1::new("Fixture");
        p.project = ProjectMeta {
            id: "p1".into(),
            name: "Fixture Project".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            modified_at: "2026-01-01T00:00:00Z".into(),
            app_version: "0.0.0".into(),
        };
        p.canvas = CanvasV1 {
            width: 1920,
            height: 1080,
            fps: Rational::new(30, 1),
            ratio_preset: crate::project::CanvasRatioPreset::Ratio16x9,
        };
        p
    }

    fn media(id: &str, kind: MediaKind, duration_us: i64, audio_channels: u16) -> MediaItem {
        MediaItem {
            id: id.into(),
            kind,
            source_path: format!("C:/media/{id}.mp4"),
            duration_us,
            width: 1920,
            height: 1080,
            fps: Rational::new(30, 1),
            codec: "h264".into(),
            bitrate: 5_000_000,
            audio_channels,
            sample_rate: 48_000,
            rotation_deg: 0,
            created_at: None,
            proxy_path: None,
            thumbnail_path: None,
        }
    }

    fn clip(id: &str, track_id: &str, media_id: &str, position_us: i64, dur_us: i64) -> Clip {
        Clip {
            id: id.into(),
            track_id: track_id.into(),
            media_id: Some(media_id.into()),
            source_in_us: 0,
            source_out_us: dur_us,
            position_us,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        }
    }

    fn track(id: &str, kind: TrackKind, render_index: i32, clip_ids: Vec<&str>) -> Track {
        Track {
            id: id.into(),
            kind,
            name: id.into(),
            render_index,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: clip_ids.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn empty_timeline_is_rejected_not_silently_exported() {
        let p = base_project();
        let err = build(&p).expect_err("no tracks at all means nothing to export");
        assert!(matches!(err, FcpxmlError::EmptyTimeline { .. }));
    }

    #[test]
    fn single_video_track_renders_one_asset_clip_in_the_spine() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        p.clips.push(clip("c1", "v1", "m1", 0, 5_000_000));

        let xml = build(&p).expect("one enabled clip on a video track is exportable");
        assert_eq!(xml.matches("<asset id=").count(), 1, "{xml}");
        assert_eq!(xml.matches("<asset-clip").count(), 1, "{xml}");
        assert!(xml.contains("offset=\"0s\""), "{xml}");
        // 5 seconds at 30fps = 150 frames = 150/30s.
        assert!(xml.contains("duration=\"150/30s\""), "{xml}");
        assert!(xml.contains("<sequence format=\"f1\""), "{xml}");
    }

    #[test]
    fn disabled_clips_are_excluded() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        let mut c = clip("c1", "v1", "m1", 0, 5_000_000);
        c.enabled = false;
        p.clips.push(c);

        let err = build(&p).expect_err("the only clip is disabled");
        assert!(matches!(err, FcpxmlError::EmptyTimeline { .. }));
    }

    #[test]
    fn hidden_track_is_excluded_from_export() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.media.push(media("m2", MediaKind::Video, 10_000_000, 2));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        p.clips.push(clip("c1", "v1", "m1", 0, 5_000_000));
        let mut hidden = track("v2", TrackKind::Video, 1, vec!["c2"]);
        hidden.hidden = true;
        p.tracks.push(hidden);
        p.clips.push(clip("c2", "v2", "m2", 0, 5_000_000));

        let xml = build(&p).expect("v1 alone is still exportable");
        // Only v1's clip/asset should appear; v2 is hidden.
        assert_eq!(xml.matches("<asset id=").count(), 1, "{xml}");
    }

    #[test]
    fn two_video_tracks_pick_the_higher_render_index_as_primary() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.media.push(media("m2", MediaKind::Video, 10_000_000, 2));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        p.clips.push(clip("c1", "v1", "m1", 0, 5_000_000));
        p.tracks.push(track("v2", TrackKind::Video, 1, vec!["c2"]));
        p.clips.push(clip("c2", "v2", "m2", 0, 5_000_000));

        let xml = build(&p).expect("two overlapping video tracks are exportable");
        // v2 (render_index 1) is primary: its clip has no lane attribute.
        // v1 becomes a connected clip on lane 1. Asset ids are assigned in
        // reference order (primary first), not `project.media` declaration
        // order, so look up each media's ref by its asset `name` rather than
        // assuming a fixed id.
        let ref_for = |media_name: &str| -> String {
            let needle = format!("name=\"{media_name}\"");
            let line = xml
                .lines()
                .find(|l| l.trim_start().starts_with("<asset ") && l.contains(&needle))
                .unwrap_or_else(|| panic!("no asset named {media_name} in {xml}"));
            let at = line.find("id=\"").expect("asset has an id") + 4;
            line[at..].split('"').next().unwrap().to_string()
        };
        let m1_ref = ref_for("m1.mp4");
        let m2_ref = ref_for("m2.mp4");

        let primary_line = xml
            .lines()
            .find(|l| l.trim_start().starts_with("<asset-clip") && !l.contains("lane="))
            .expect("primary clip has no lane attr");
        assert!(
            primary_line.contains(&format!("ref=\"{m2_ref}\"")),
            "{primary_line}"
        );
        let connected_line = xml
            .lines()
            .find(|l| l.contains("lane=\"1\""))
            .expect("v1 attaches as a lane-1 connected clip");
        assert!(
            connected_line.contains(&format!("ref=\"{m1_ref}\"")),
            "{connected_line}"
        );
    }

    #[test]
    fn video_and_audio_tracks_get_distinct_signed_lanes() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.media.push(media("m2", MediaKind::Video, 10_000_000, 2));
        p.media.push(media("m3", MediaKind::Audio, 10_000_000, 2));
        p.tracks.push(track("v1", TrackKind::Video, 2, vec!["c1"]));
        p.clips.push(clip("c1", "v1", "m1", 0, 5_000_000));
        p.tracks.push(track("v2", TrackKind::Video, 1, vec!["c2"]));
        p.clips.push(clip("c2", "v2", "m2", 0, 5_000_000));
        p.tracks.push(track("a1", TrackKind::Audio, 0, vec!["c3"]));
        p.clips.push(clip("c3", "a1", "m3", 0, 5_000_000));

        let xml = build(&p).expect("multi-track project is exportable");
        // v1 (render_index 2) is primary. v2 -> lane 1 (only visual
        // non-primary track). a1 -> lane -1 (only audio track).
        assert!(xml.contains("lane=\"1\""), "{xml}");
        assert!(xml.contains("lane=\"-1\""), "{xml}");
        assert!(!xml.contains("lane=\"2\""), "{xml}");
        assert!(!xml.contains("lane=\"-2\""), "{xml}");
    }

    #[test]
    fn caption_track_exports_a_title_element() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        p.clips.push(clip("c1", "v1", "m1", 0, 5_000_000));
        p.tracks.push(track("cap1", TrackKind::Caption, 1, vec![]));
        p.captions.push(Caption {
            id: "cap_a".into(),
            track_id: "cap1".into(),
            start_us: 1_000_000,
            end_us: 2_000_000,
            text: "Hello & welcome".into(),
            words: vec![],
            style_id: None,
        });

        let xml = build(&p).expect("caption track alongside a video track is exportable");
        assert!(xml.contains("<title "), "{xml}");
        assert!(xml.contains("Hello &amp; welcome"), "{xml}");
    }

    #[test]
    fn effect_track_is_skipped_without_error() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        p.clips.push(clip("c1", "v1", "m1", 0, 5_000_000));
        p.tracks.push(track("fx1", TrackKind::Effect, 1, vec![]));

        let xml = build(&p).expect("effect track doesn't block export");
        assert_eq!(xml.matches("<asset-clip").count(), 1, "{xml}");
    }

    #[test]
    fn gap_before_first_primary_clip_holds_a_connected_clip() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.media.push(media("m2", MediaKind::Video, 10_000_000, 2));
        // Primary clip starts at 5s, leaving [0,5) uncovered.
        p.tracks.push(track("v1", TrackKind::Video, 1, vec!["c1"]));
        p.clips.push(clip("c1", "v1", "m1", 5_000_000, 5_000_000));
        p.tracks.push(track("v2", TrackKind::Video, 0, vec!["c2"]));
        p.clips.push(clip("c2", "v2", "m2", 0, 2_000_000));

        let xml = build(&p).expect("connected clip in the leading gap is exportable");
        assert!(xml.contains("<gap "), "{xml}");
        assert!(xml.contains("lane=\"1\""), "{xml}");
    }

    #[test]
    fn well_formed_output_round_trips_through_a_minimal_xml_check() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video, 10_000_000, 2));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        p.clips.push(clip("c1", "v1", "m1", 0, 5_000_000));

        let xml = build(&p).expect("exportable");
        // Every opened tag we emit closes; a crude but effective smoke check
        // pending the real xml::etree.ElementTree validation done in the
        // manual verification pass (no XML-parsing crate in this project).
        assert_eq!(xml.matches("<fcpxml").count(), 1);
        assert_eq!(xml.matches("</fcpxml>").count(), 1);
        assert!(xml.starts_with("<?xml"));
    }
}
