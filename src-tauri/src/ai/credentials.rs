//! Secure API-key storage (master prompt §17: "Store credentials securely
//! using Windows Credential Manager or equivalent secure storage. NEVER
//! store API keys plaintext inside project.json.").
//!
//! `AiState::provider_settings_ref` (`project::types`) is an opaque `String`
//! that never carries a secret itself — it is only ever used as the account
//! name (`credential_ref`) under which this module writes/reads a real
//! secret in a platform-native secure store. Nothing in `project::types` or
//! anywhere else this crate serializes to `project.json` ever holds a raw
//! API key: `CredentialStore::set` is the *only* place a key is written
//! anywhere, and there is deliberately no command anywhere in
//! `commands::ai` that reads a stored key back out to the frontend (the
//! only consumer of `CredentialStore::get` is this crate's own provider
//! adapters, building an `Authorization`/`x-api-key` header server-side).
//!
//! ## Windows implementation
//!
//! [`WindowsCredentialStore`] uses the `keyring` crate (`Cargo.toml`'s
//! `[target.'cfg(windows)'.dependencies]`), which itself wraps the real
//! Win32 Credential Manager API (`CredWriteW`/`CredReadW`/`CredDeleteW`) on
//! Windows — the exact "purpose-built crate that wraps Credential Manager"
//! option this phase's brief calls out as acceptable. It is gated
//! `#[cfg(target_os = "windows")]`, matching the precedent
//! `capcut::detect`'s `registry` submodule already established for
//! Windows-only functionality (that module's doc comment).
//!
//! **Verification limit, stated honestly**: this crate is built and tested
//! from WSL2 Linux (`HANDOFF.md` "Build/test environment"), which cannot
//! compile or exercise Windows-only code at all — `keyring` is a
//! target-specific dependency exactly like `winreg` already is, so it is
//! never even resolved/downloaded for a Linux build. The real Windows
//! Credential Manager round-trip (`WindowsCredentialStore`) is therefore
//! **unverified in this environment** — only [`InMemoryCredentialStore`]
//! (the non-Windows fallback and the type every test below actually
//! exercises) has been run for real.
//!
//! ## Non-Windows fallback
//!
//! [`InMemoryCredentialStore`] is what `default_store()` returns on any
//! non-Windows target, and what every unit test in this module uses
//! regardless of platform. It is **not** secure storage — it is a
//! process-local, non-persistent `HashMap` — but it lets the
//! `CredentialStore` trait's contract (set/get/delete, not-found behavior)
//! be exercised for real in this dev environment, the same "OS-agnostic
//! testable core + thin real-filesystem wrapper" split `capcut::detect`
//! documents for its own Windows-only detection logic.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::error::AiProviderError;

/// Platform-independent secret store, keyed by an opaque `credential_ref`
/// (module doc comment). One credential per ref — setting an existing ref
/// overwrites it.
pub trait CredentialStore: Send + Sync {
    fn set(&self, credential_ref: &str, secret: &str) -> Result<(), AiProviderError>;
    fn get(&self, credential_ref: &str) -> Result<String, AiProviderError>;
    fn delete(&self, credential_ref: &str) -> Result<(), AiProviderError>;
}

/// The Windows Credential Manager "service" every credential in this app is
/// stored under (keyring's `Entry::new(service, username)` first argument),
/// with `credential_ref` as the per-provider-settings username — so
/// switching provider profiles never collides with another app's own
/// credentials in the same store. Only referenced from
/// `WindowsCredentialStore` below, which is itself Windows-only — see that
/// impl block's own `#[cfg(target_os = "windows")]`.
#[cfg(target_os = "windows")]
const SERVICE_NAME: &str = "AI Video Editor";

#[cfg(target_os = "windows")]
pub struct WindowsCredentialStore;

#[cfg(target_os = "windows")]
impl CredentialStore for WindowsCredentialStore {
    fn set(&self, credential_ref: &str, secret: &str) -> Result<(), AiProviderError> {
        let entry = keyring::Entry::new(SERVICE_NAME, credential_ref).map_err(|e| {
            AiProviderError::CredentialStoreFailed {
                details: e.to_string(),
            }
        })?;
        entry
            .set_password(secret)
            .map_err(|e| AiProviderError::CredentialStoreFailed {
                details: e.to_string(),
            })
    }

    fn get(&self, credential_ref: &str) -> Result<String, AiProviderError> {
        let entry = keyring::Entry::new(SERVICE_NAME, credential_ref).map_err(|e| {
            AiProviderError::CredentialStoreFailed {
                details: e.to_string(),
            }
        })?;
        match entry.get_password() {
            Ok(secret) => Ok(secret),
            Err(keyring::Error::NoEntry) => Err(AiProviderError::CredentialNotFound {
                credential_ref: credential_ref.to_string(),
            }),
            Err(e) => Err(AiProviderError::CredentialStoreFailed {
                details: e.to_string(),
            }),
        }
    }

    fn delete(&self, credential_ref: &str) -> Result<(), AiProviderError> {
        let entry = keyring::Entry::new(SERVICE_NAME, credential_ref).map_err(|e| {
            AiProviderError::CredentialStoreFailed {
                details: e.to_string(),
            }
        })?;
        match entry.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AiProviderError::CredentialStoreFailed {
                details: e.to_string(),
            }),
        }
    }
}

/// Non-persistent, process-local fallback (module doc comment). Real
/// runtime behavior on any non-Windows target, and the only implementation
/// this module's own tests exercise.
#[derive(Default)]
pub struct InMemoryCredentialStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl CredentialStore for InMemoryCredentialStore {
    fn set(&self, credential_ref: &str, secret: &str) -> Result<(), AiProviderError> {
        let mut guard = self
            .secrets
            .lock()
            .expect("in-memory credential store mutex poisoned");
        guard.insert(credential_ref.to_string(), secret.to_string());
        Ok(())
    }

    fn get(&self, credential_ref: &str) -> Result<String, AiProviderError> {
        let guard = self
            .secrets
            .lock()
            .expect("in-memory credential store mutex poisoned");
        guard
            .get(credential_ref)
            .cloned()
            .ok_or_else(|| AiProviderError::CredentialNotFound {
                credential_ref: credential_ref.to_string(),
            })
    }

    fn delete(&self, credential_ref: &str) -> Result<(), AiProviderError> {
        let mut guard = self
            .secrets
            .lock()
            .expect("in-memory credential store mutex poisoned");
        guard.remove(credential_ref);
        Ok(())
    }
}

/// Real Windows entry point: the real Credential Manager-backed store.
#[cfg(target_os = "windows")]
pub fn default_store() -> Arc<dyn CredentialStore> {
    Arc::new(WindowsCredentialStore)
}

/// Non-Windows entry point. This crate is built/tested on Linux (module doc
/// comment); the in-memory fallback keeps `commands::ai`'s credential
/// commands callable (and testable) on every host this crate compiles for,
/// same rationale as `capcut::detect::detect_windows_installations`'s own
/// non-Windows stub.
#[cfg(not(target_os = "windows"))]
pub fn default_store() -> Arc<dyn CredentialStore> {
    Arc::new(InMemoryCredentialStore::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_then_get_round_trips_the_secret() {
        let store = InMemoryCredentialStore::default();
        store.set("provider-1", "sk-abc123").unwrap();
        assert_eq!(store.get("provider-1").unwrap(), "sk-abc123");
    }

    #[test]
    fn setting_an_existing_ref_overwrites_it() {
        let store = InMemoryCredentialStore::default();
        store.set("provider-1", "sk-old").unwrap();
        store.set("provider-1", "sk-new").unwrap();
        assert_eq!(store.get("provider-1").unwrap(), "sk-new");
    }

    #[test]
    fn getting_an_unknown_ref_errors_not_found() {
        let store = InMemoryCredentialStore::default();
        assert!(matches!(
            store.get("does-not-exist"),
            Err(AiProviderError::CredentialNotFound { .. })
        ));
    }

    #[test]
    fn delete_removes_the_secret_and_is_idempotent() {
        let store = InMemoryCredentialStore::default();
        store.set("provider-1", "sk-abc123").unwrap();
        store.delete("provider-1").unwrap();
        assert!(matches!(
            store.get("provider-1"),
            Err(AiProviderError::CredentialNotFound { .. })
        ));
        // Deleting again (already gone) must not error.
        store.delete("provider-1").unwrap();
    }

    #[test]
    fn default_store_is_usable_on_this_platform() {
        let store = default_store();
        store.set("smoke-test", "value").unwrap();
        assert_eq!(store.get("smoke-test").unwrap(), "value");
        store.delete("smoke-test").unwrap();
    }
}
