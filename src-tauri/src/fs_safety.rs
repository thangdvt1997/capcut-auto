//! Generic defense-in-depth check for a single path *component* — an id
//! used to build one path segment joined onto an app-controlled base
//! directory, never a full path — per master prompt §53 "prevent path
//! traversal".
//!
//! Every legitimate id this crate joins onto a base directory this way (a
//! media library item id, a custom template id, a Whisper model id) is, by
//! construction, either a UUID this backend generated itself
//! (`uuid::Uuid::new_v4()`, e.g. `commands::media::import_one`'s `id`) or a
//! fixed catalog string (`transcription::models::ModelId`'s closed enum) —
//! never free text a caller chooses. But a Tauri command's string
//! parameters are the real trust boundary between the (less-trusted)
//! webview frontend and this (fully-trusted) Rust backend, and a `Template`
//! read back from a user-chosen import file carries whatever `id` string
//! that file's author put in it. A frontend bug, a compromised dependency,
//! or a crafted/corrupted imported template file could still hand a command
//! a value like `"../../../../Users/victim/secret"`. This validates
//! defensively at the boundary rather than relying only on "every current
//! caller happens to pass something safe" — see
//! `commands::media::generate_media_proxy`/`generate_thumbnail_strip` and
//! `templates::io::template_file_path` for the real call sites this
//! guards.

/// `true` if `component` is safe to `Path::join` onto a trusted base
/// directory as a single segment (a bare filename or a bare one-level
/// subdirectory name) — i.e. it cannot escape that directory. Rejects: an
/// empty string, `.`/`..` themselves, anything containing a path separator
/// (`/` or `\` — checked for both regardless of host OS, since a
/// Windows-shaped `..\..\` string must be rejected even when this code
/// happens to run on Linux, and vice versa), and an embedded NUL byte (not
/// a valid path component on any platform this app targets).
pub fn is_safe_path_component(component: &str) -> bool {
    !component.is_empty()
        && component != "."
        && component != ".."
        && !component.contains('/')
        && !component.contains('\\')
        && !component.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_real_uuid() {
        assert!(is_safe_path_component(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
    }

    #[test]
    fn accepts_a_custom_template_id() {
        assert!(is_safe_path_component(
            "custom_550e8400-e29b-41d4-a716-446655440000"
        ));
    }

    #[test]
    fn rejects_an_empty_string() {
        assert!(!is_safe_path_component(""));
    }

    #[test]
    fn rejects_dot_and_dotdot() {
        assert!(!is_safe_path_component("."));
        assert!(!is_safe_path_component(".."));
    }

    #[test]
    fn rejects_a_unix_style_traversal() {
        assert!(!is_safe_path_component("../../../etc/passwd"));
        assert!(!is_safe_path_component("foo/../bar"));
        assert!(!is_safe_path_component("foo/bar"));
    }

    #[test]
    fn rejects_a_windows_style_traversal_even_when_checked_on_linux() {
        // This crate's real dev/test environment is WSL2/Linux
        // (`HANDOFF.md`), but the shipped app is Windows-only — a
        // backslash-separated traversal string must be rejected regardless
        // of which OS is running this exact check.
        assert!(!is_safe_path_component("..\\..\\Users\\victim\\secret"));
        assert!(!is_safe_path_component("foo\\bar"));
    }

    #[test]
    fn rejects_an_absolute_looking_path() {
        assert!(!is_safe_path_component("/etc/passwd"));
        assert!(!is_safe_path_component("C:\\Windows\\System32"));
    }

    #[test]
    fn rejects_an_embedded_null_byte() {
        assert!(!is_safe_path_component("a\0b"));
    }
}
