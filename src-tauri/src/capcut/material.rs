//! `VideoMaterial`/`AudioMaterial` — port of `local_materials.py`.
//!
//! Scope reduction: the Python original probes the real media file with
//! `pymediainfo`/`imageio` to discover duration/width/height when they
//! aren't supplied. This crate already has that information on hand (every
//! caller here builds a `VideoMaterial`/`AudioMaterial` from a resolved
//! `crate::project::MediaItem`, which Phase 3's media-probe pipeline already
//! populated) — so these constructors take duration/width/height as plain
//! arguments rather than re-deriving them from the file a second time.
//! `CropSettings` is ported as a fixed default (full-frame, uncropped) only —
//! this project's `Clip`/`ClipSettings` schema has no per-clip crop-region
//! concept yet, so there is nothing to map a non-default crop from.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropSettings {
    pub upper_left_x: f64,
    pub upper_left_y: f64,
    pub upper_right_x: f64,
    pub upper_right_y: f64,
    pub lower_left_x: f64,
    pub lower_left_y: f64,
    pub lower_right_x: f64,
    pub lower_right_y: f64,
}

impl Default for CropSettings {
    /// Uncropped (matches `local_materials.py`'s own default arguments).
    fn default() -> Self {
        Self {
            upper_left_x: 0.0,
            upper_left_y: 0.0,
            upper_right_x: 1.0,
            upper_right_y: 0.0,
            lower_left_x: 0.0,
            lower_left_y: 1.0,
            lower_right_x: 1.0,
            lower_right_y: 1.0,
        }
    }
}

impl CropSettings {
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "upper_left_x": self.upper_left_x, "upper_left_y": self.upper_left_y,
            "upper_right_x": self.upper_right_x, "upper_right_y": self.upper_right_y,
            "lower_left_x": self.lower_left_x, "lower_left_y": self.lower_left_y,
            "lower_right_x": self.lower_right_x, "lower_right_y": self.lower_right_y,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoMaterialKind {
    Video,
    Photo,
}

impl VideoMaterialKind {
    fn wire_value(self) -> &'static str {
        match self {
            VideoMaterialKind::Video => "video",
            VideoMaterialKind::Photo => "photo",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoMaterial {
    pub material_id: String,
    pub material_name: String,
    pub path: String,
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    pub crop_settings: CropSettings,
    pub kind: VideoMaterialKind,
}

impl VideoMaterial {
    pub fn new(
        path: impl Into<String>,
        material_name: impl Into<String>,
        duration_us: i64,
        width: u32,
        height: u32,
        kind: VideoMaterialKind,
    ) -> Self {
        Self {
            material_id: Uuid::new_v4().simple().to_string(),
            material_name: material_name.into(),
            path: path.into(),
            duration_us,
            width,
            height,
            crop_settings: CropSettings::default(),
            kind,
        }
    }

    /// Matches `VideoMaterial.export_json` in `local_materials.py`.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "audio_fade": null,
            "category_id": "",
            "category_name": "local",
            "check_flag": 63487,
            "crop": self.crop_settings.export_json(),
            "crop_ratio": "free",
            "crop_scale": 1.0,
            "duration": self.duration_us,
            "height": self.height,
            "id": self.material_id,
            "local_material_id": "",
            "material_id": self.material_id,
            "material_name": self.material_name,
            "media_path": "",
            "path": self.path,
            "type": self.kind.wire_value(),
            "width": self.width,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioMaterial {
    pub material_id: String,
    pub material_name: String,
    pub path: String,
    pub duration_us: i64,
}

impl AudioMaterial {
    pub fn new(
        path: impl Into<String>,
        material_name: impl Into<String>,
        duration_us: i64,
    ) -> Self {
        Self {
            material_id: Uuid::new_v4().simple().to_string(),
            material_name: material_name.into(),
            path: path.into(),
            duration_us,
        }
    }

    /// Matches `AudioMaterial.export_json` in `local_materials.py`.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "app_id": 0,
            "category_id": "",
            "category_name": "local",
            "check_flag": 3,
            "copyright_limit_type": "none",
            "duration": self.duration_us,
            "effect_id": "",
            "formula_id": "",
            "id": self.material_id,
            "local_material_id": self.material_id,
            "music_id": self.material_id,
            "name": self.material_name,
            "path": self.path,
            "source_platform": 0,
            "type": "extract_music",
            "wave_points": [],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_material_export_json_has_expected_shape() {
        let m = VideoMaterial::new(
            "C:/media/a.mp4",
            "a.mp4",
            5_000_000,
            1920,
            1080,
            VideoMaterialKind::Video,
        );
        let v = m.export_json();
        assert_eq!(v["type"], serde_json::json!("video"));
        assert_eq!(v["width"], serde_json::json!(1920));
        assert_eq!(v["duration"], serde_json::json!(5_000_000));
        assert_eq!(v["id"], v["material_id"]);
    }

    #[test]
    fn audio_material_export_json_has_expected_shape() {
        let m = AudioMaterial::new("C:/media/a.mp3", "a.mp3", 3_000_000);
        let v = m.export_json();
        assert_eq!(v["type"], serde_json::json!("extract_music"));
        assert_eq!(v["duration"], serde_json::json!(3_000_000));
    }

    #[test]
    fn crop_settings_default_is_uncropped() {
        let c = CropSettings::default();
        assert_eq!(c.upper_left_x, 0.0);
        assert_eq!(c.lower_right_x, 1.0);
        assert_eq!(c.lower_right_y, 1.0);
    }
}
