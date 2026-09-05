//! Import-time helpers: supported-format classification (master prompt §7)
//! and folder scanning. Kept separate from `probe.rs` because file-extension
//! classification, not ffprobe output, is what decides `MediaKind` — ffprobe
//! reports a spurious `video` stream for an MP3/FLAC's embedded cover art,
//! which would otherwise misclassify an audio file as video.

use std::path::{Path, PathBuf};

use crate::project::MediaKind;

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v"];
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "aac", "m4a", "flac"];
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// Classify a path by extension per the master prompt §7 format list.
/// Case-insensitive (`.MP4`, `.Mp4`, `.mp4` are all `Video`).
pub fn classify_extension(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaKind::Video)
    } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaKind::Audio)
    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaKind::Image)
    } else {
        None
    }
}

pub fn is_supported(path: &Path) -> bool {
    classify_extension(path).is_some()
}

/// Recursively walk `folder`, returning every file whose extension is one of
/// the master-prompt §7 supported formats, depth-first, skipping symlinked
/// directories (avoids infinite loops on a cyclic symlink) and any directory
/// starting with `.` (editor/VCS metadata, e.g. `.git`, hidden folders).
///
/// Errors reading an individual subdirectory (permissions, race with a
/// concurrent delete) are swallowed and that subtree is simply skipped
/// rather than failing the whole import — one unreadable folder shouldn't
/// block importing everything else found.
pub fn scan_folder(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scan_into(folder, &mut out);
    out.sort();
    out
}

fn scan_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            scan_into(&path, out);
        } else if file_type.is_file() && is_supported(&path) {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn classifies_every_master_prompt_extension() {
        for ext in VIDEO_EXTENSIONS {
            assert_eq!(
                classify_extension(Path::new(&format!("clip.{ext}"))),
                Some(MediaKind::Video),
                "{ext}"
            );
        }
        for ext in AUDIO_EXTENSIONS {
            assert_eq!(
                classify_extension(Path::new(&format!("clip.{ext}"))),
                Some(MediaKind::Audio),
                "{ext}"
            );
        }
        for ext in IMAGE_EXTENSIONS {
            assert_eq!(
                classify_extension(Path::new(&format!("clip.{ext}"))),
                Some(MediaKind::Image),
                "{ext}"
            );
        }
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(
            classify_extension(Path::new("A.MP4")),
            Some(MediaKind::Video)
        );
        assert_eq!(
            classify_extension(Path::new("A.Mp3")),
            Some(MediaKind::Audio)
        );
    }

    #[test]
    fn rejects_unknown_extensions() {
        assert_eq!(classify_extension(Path::new("readme.txt")), None);
        assert_eq!(classify_extension(Path::new("no_extension")), None);
    }

    #[test]
    fn handles_unicode_and_vietnamese_filenames() {
        // master prompt §88's own example filename.
        let path = Path::new("C:/Video tiếng Việt/phỏng vấn 01.mp4");
        assert_eq!(classify_extension(path), Some(MediaKind::Video));
    }

    #[test]
    fn handles_paths_with_spaces() {
        let path = Path::new("D:/My Videos/Test Video.mp4");
        assert_eq!(classify_extension(path), Some(MediaKind::Video));
    }

    // -- §88 Windows path edge cases: Japanese/emoji, long paths, UNC-shaped
    //    strings, on top of the spaces/Vietnamese cases already above -----

    #[test]
    fn handles_other_non_ascii_unicode_japanese_and_emoji() {
        let path = Path::new("C:/動画プロジェクト/映画 🎬 最終版.mp4");
        assert_eq!(classify_extension(path), Some(MediaKind::Video));
    }

    #[test]
    fn handles_a_very_long_path() {
        // Windows' classic MAX_PATH is 260 characters — this is testing
        // this function's own string/extension-parsing logic, not
        // Windows' actual enforcement of that limit (which WSL2's
        // filesystem doesn't reproduce; needs separate real-Windows
        // verification, including whether a `\\?\`-prefixed path is
        // required there).
        let long_dir =
            "a-long-directory-name-repeated-to-approach-the-260-character-limit/".repeat(6);
        let path_string = format!("C:/{long_dir}video.mp4");
        assert!(path_string.len() > 260, "{}", path_string.len());
        assert_eq!(
            classify_extension(Path::new(&path_string)),
            Some(MediaKind::Video)
        );
    }

    #[test]
    fn handles_a_unc_shaped_path_string() {
        // UNC paths (`\\server\share\...`) are a Windows-specific string
        // convention; real UNC network access can only be verified on real
        // Windows. This proves `classify_extension` (forward-slash-
        // agnostic — it only ever looks at `Path::extension()`) doesn't
        // mis-parse or choke on a UNC-shaped string, which is the thing
        // actually testable here.
        let path = Path::new(r"\\server\share\Video Projects\clip.mp4");
        assert_eq!(classify_extension(path), Some(MediaKind::Video));
    }

    #[test]
    fn scan_folder_finds_supported_files_recursively_and_skips_hidden_and_unsupported() {
        let dir = std::env::temp_dir().join(format!("ave-import-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        fs::write(dir.join("a.mp4"), b"x").unwrap();
        fs::write(dir.join("readme.txt"), b"x").unwrap();
        fs::write(dir.join("sub").join("b.wav"), b"x").unwrap();
        fs::write(dir.join(".hidden").join("c.mp4"), b"x").unwrap();

        let found = scan_folder(&dir);
        fs::remove_dir_all(&dir).ok();

        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|p| p.ends_with("a.mp4")));
        assert!(found.iter().any(|p| p.ends_with("b.wav")));
    }

    #[test]
    fn scan_folder_finds_a_real_file_with_vietnamese_and_other_unicode_names_on_disk() {
        // A real filesystem round trip (not just string-level
        // `classify_extension`) with the exact kind of filenames master
        // prompt §88 calls out: a real Vietnamese filename and other
        // non-ASCII Unicode (Japanese + emoji), plus spaces, all as real
        // files this function actually walks and finds.
        let dir =
            std::env::temp_dir().join(format!("ave-import-unicode-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Việt Nam - Xin chào.mp4"), b"x").unwrap();
        fs::write(dir.join("動画プロジェクト 🎬.mp4"), b"x").unwrap();
        fs::write(dir.join("My Video File.mp4"), b"x").unwrap();

        let found = scan_folder(&dir);
        fs::remove_dir_all(&dir).ok();

        assert_eq!(found.len(), 3);
        assert!(found
            .iter()
            .any(|p| p.file_name().unwrap().to_string_lossy() == "Việt Nam - Xin chào.mp4"));
        assert!(found
            .iter()
            .any(|p| p.file_name().unwrap().to_string_lossy() == "動画プロジェクト 🎬.mp4"));
    }
}
