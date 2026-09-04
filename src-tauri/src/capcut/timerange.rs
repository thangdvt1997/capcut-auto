//! `Timerange` — a direct port of `vendor/capcut-mate/src/pyJianYingDraft/time_util.py`'s
//! `Timerange` class: an i64-microsecond `{start, duration}` pair.
//!
//! **JSON shape, read carefully**: `time_util.py`'s `export_json` returns
//! `Dict[str, int]` — i.e. the JSON keys `"start"`/`"duration"` are ordinary
//! *string* keys (as every JSON object key is), but the *values* are real
//! JSON integers, not stringified numbers. `import_json` even calls
//! `int(json_obj["start"])`, which only makes sense if the source is already
//! numeric (`int()` on a non-numeric string would still work, but the
//! round-trip test below pins the actual emitted shape so this doesn't
//! regress). Do not emit `{"start": "123", "duration": "456"}`.

use serde::Serialize;

/// A time range: `start` (absolute microseconds) plus `duration`
/// (microseconds). NOT `{start, end}` — `end` is a derived accessor, never
/// stored, matching the Python original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Timerange {
    pub start: i64,
    pub duration: i64,
}

impl Timerange {
    pub const fn new(start: i64, duration: i64) -> Self {
        Self { start, duration }
    }

    pub const fn end(&self) -> i64 {
        self.start + self.duration
    }

    pub fn overlaps(&self, other: &Timerange) -> bool {
        !(self.end() <= other.start || other.end() <= self.start)
    }

    /// `{"start": <int>, "duration": <int>}` — real JSON integers, string
    /// keys. See module doc comment.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({ "start": self.start, "duration": self.duration })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_is_start_plus_duration() {
        let t = Timerange::new(1_000_000, 500_000);
        assert_eq!(t.end(), 1_500_000);
    }

    #[test]
    fn overlaps_detects_touching_but_not_adjacent_ranges() {
        let a = Timerange::new(0, 1_000_000);
        let b = Timerange::new(500_000, 1_000_000);
        let c = Timerange::new(1_000_000, 1_000_000); // exactly adjacent, not overlapping
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn export_json_uses_real_integers_not_stringified_numbers() {
        let t = Timerange::new(123, 456);
        let v = t.export_json();
        assert_eq!(v["start"], serde_json::json!(123));
        assert_eq!(v["duration"], serde_json::json!(456));
        // Explicitly not strings.
        assert!(v["start"].is_i64() || v["start"].is_u64());
        assert!(!v["start"].is_string());
    }
}
