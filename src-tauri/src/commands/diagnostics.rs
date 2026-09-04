use serde::Serialize;
use specta::Type;

/// Static, build-time facts about this build. No telemetry, no network
/// calls, nothing environment-dependent beyond `std::env::consts`. Exists in
/// Phase 2 to exercise the Rust -> specta -> TypeScript pipeline end-to-end
/// with a genuinely working command; the full Settings > About / "Copy
/// System Information" panel (master prompt §78) is Phase 12 scope.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ShellInfo {
    pub app_version: String,
    pub tauri_version: String,
    pub os: String,
    pub arch: String,
}

#[tauri::command]
#[specta::specta]
pub fn get_shell_info() -> ShellInfo {
    ShellInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}
