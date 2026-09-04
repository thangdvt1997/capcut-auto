//! `Track`/`TrackType` — port of `track.py`.
//!
//! `SegmentSlot` (this module) is the Rust stand-in for `track.py`'s
//! `Track[Seg_type]` generic parameter — since a `Vec` needs one concrete
//! element type, each track holds a `Vec<SegmentSlot>` and `add_segment`
//! below checks the enum variant matches the track's declared `TrackType`
//! (mirroring `Track.add_segment`'s `isinstance(segment,
//! self.accept_segment_type)` check).

use serde_json::{json, Value};

use crate::capcut::error::CapCutError;
use crate::capcut::segment::{
    AudioSegment, EffectSegment, FilterSegment, StickerSegment, TextSegment, VideoSegment,
};
use crate::capcut::timerange::Timerange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackType {
    Video,
    Audio,
    Effect,
    Filter,
    Sticker,
    Text,
}

impl TrackType {
    /// `render_index` default per `Track_meta` in `track.py`.
    pub fn default_render_index(self) -> i32 {
        match self {
            TrackType::Video | TrackType::Audio => 0,
            TrackType::Effect => 10_000,
            TrackType::Filter => 11_000,
            TrackType::Sticker => 14_000,
            TrackType::Text => 15_000,
        }
    }

    fn wire_value(self) -> &'static str {
        match self {
            TrackType::Video => "video",
            TrackType::Audio => "audio",
            TrackType::Effect => "effect",
            TrackType::Filter => "filter",
            TrackType::Sticker => "sticker",
            TrackType::Text => "text",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SegmentSlot {
    Video(VideoSegment),
    Audio(AudioSegment),
    Text(TextSegment),
    Sticker(StickerSegment),
    Effect(EffectSegment),
    Filter(FilterSegment),
}

impl SegmentSlot {
    fn track_type(&self) -> TrackType {
        match self {
            SegmentSlot::Video(_) => TrackType::Video,
            SegmentSlot::Audio(_) => TrackType::Audio,
            SegmentSlot::Text(_) => TrackType::Text,
            SegmentSlot::Sticker(_) => TrackType::Sticker,
            SegmentSlot::Effect(_) => TrackType::Effect,
            SegmentSlot::Filter(_) => TrackType::Filter,
        }
    }

    pub fn target_timerange(&self) -> Timerange {
        match self {
            SegmentSlot::Video(s) => s.target_timerange(),
            SegmentSlot::Audio(s) => s.target_timerange(),
            SegmentSlot::Text(s) => s.target_timerange(),
            SegmentSlot::Sticker(s) => s.target_timerange(),
            SegmentSlot::Effect(s) => s.target_timerange,
            SegmentSlot::Filter(s) => s.target_timerange,
        }
    }

    /// The generated `segment_id` `capcut::adapter`'s `add_animation`/
    /// `add_keyframe`/`add_mask` use as their handle back into an
    /// already-inserted segment.
    pub fn segment_id(&self) -> &str {
        match self {
            SegmentSlot::Video(s) => &s.visual.media.base.segment_id,
            SegmentSlot::Audio(s) => &s.media.base.segment_id,
            SegmentSlot::Text(s) => &s.visual.media.base.segment_id,
            SegmentSlot::Sticker(s) => &s.visual.media.base.segment_id,
            SegmentSlot::Effect(s) => &s.segment_id,
            SegmentSlot::Filter(s) => &s.segment_id,
        }
    }

    fn export_json(&self) -> Value {
        match self {
            SegmentSlot::Video(s) => s.export_json(),
            SegmentSlot::Audio(s) => s.export_json(),
            SegmentSlot::Text(s) => s.export_json(),
            SegmentSlot::Sticker(s) => s.export_json(),
            SegmentSlot::Effect(s) => s.export_json(),
            SegmentSlot::Filter(s) => s.export_json(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub track_id: String,
    pub track_type: TrackType,
    pub name: String,
    pub render_index: i32,
    pub mute: bool,
    pub segments: Vec<SegmentSlot>,
}

impl Track {
    pub fn new(
        track_id: impl Into<String>,
        track_type: TrackType,
        name: impl Into<String>,
        render_index: i32,
        mute: bool,
    ) -> Self {
        Self {
            track_id: track_id.into(),
            track_type,
            name: name.into(),
            render_index,
            mute,
            segments: Vec::new(),
        }
    }

    pub fn end_time_us(&self) -> i64 {
        self.segments
            .iter()
            .map(|s| s.target_timerange().end())
            .max()
            .unwrap_or(0)
    }

    pub fn find_segment_mut(&mut self, segment_id: &str) -> Option<&mut SegmentSlot> {
        self.segments
            .iter_mut()
            .find(|s| s.segment_id() == segment_id)
    }

    /// Matches `Track.add_segment` in `track.py`: rejects a segment whose
    /// kind doesn't match the track's declared type, and rejects a segment
    /// that overlaps an existing one already on this track.
    pub fn add_segment(&mut self, segment: SegmentSlot) -> Result<(), CapCutError> {
        if segment.track_type() != self.track_type {
            return Err(CapCutError::SegmentTrackTypeMismatch {
                track_name: self.name.clone(),
            });
        }
        let new_range = segment.target_timerange();
        for existing in &self.segments {
            if existing.target_timerange().overlaps(&new_range) {
                return Err(CapCutError::SegmentOverlap {
                    track_name: self.name.clone(),
                    start_us: new_range.start,
                    end_us: new_range.end(),
                });
            }
        }
        self.segments.push(segment);
        Ok(())
    }

    /// Matches `Track.export_json` in `track.py`: writes `render_index` onto
    /// every exported segment.
    pub fn export_json(&self) -> Value {
        let segments: Vec<Value> = self
            .segments
            .iter()
            .map(|s| {
                let mut v = s.export_json();
                if let Value::Object(map) = &mut v {
                    map.insert("render_index".into(), json!(self.render_index));
                }
                v
            })
            .collect();
        json!({
            "attribute": if self.mute { 1 } else { 0 },
            "flag": 0,
            "id": self.track_id,
            "is_default_name": self.name.is_empty(),
            "name": self.name,
            "segments": segments,
            "type": self.track_type.wire_value(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capcut::caption_style::TextStyle;
    use crate::capcut::clip_settings::CapCutClipSettings;
    use crate::capcut::segment::TextSegment;

    fn text_segment(start: i64, dur: i64) -> SegmentSlot {
        let style = TextStyle {
            size: 10.0,
            bold: false,
            italic: false,
            color: (1.0, 1.0, 1.0),
            alpha: 1.0,
            align: 1,
        };
        SegmentSlot::Text(TextSegment::new(
            "x",
            Timerange::new(start, dur),
            style,
            CapCutClipSettings::default(),
        ))
    }

    #[test]
    fn rejects_segment_of_wrong_track_type() {
        let mut track = Track::new("t1", TrackType::Video, "V1", 0, false);
        let err = track.add_segment(text_segment(0, 1_000_000)).unwrap_err();
        assert!(matches!(err, CapCutError::SegmentTrackTypeMismatch { .. }));
    }

    #[test]
    fn rejects_overlapping_segments_on_the_same_track() {
        let mut track = Track::new("t1", TrackType::Text, "Text", 15_000, false);
        track
            .add_segment(text_segment(0, 1_000_000))
            .expect("first segment ok");
        let err = track
            .add_segment(text_segment(500_000, 1_000_000))
            .unwrap_err();
        assert!(matches!(err, CapCutError::SegmentOverlap { .. }));
    }

    #[test]
    fn accepts_non_overlapping_segments_and_writes_render_index() {
        let mut track = Track::new("t1", TrackType::Text, "Text", 15_000, false);
        track.add_segment(text_segment(0, 1_000_000)).expect("ok");
        track
            .add_segment(text_segment(1_000_000, 1_000_000))
            .expect("ok, touching but not overlapping");
        let v = track.export_json();
        assert_eq!(v["segments"].as_array().unwrap().len(), 2);
        assert_eq!(v["segments"][0]["render_index"], json!(15_000));
    }
}
