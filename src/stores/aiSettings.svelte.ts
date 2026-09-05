// Svelte 5 runes-based store for the Phase 10 AI Settings dialog (master
// prompt §17): "Provider / Base URL / API Key / Model / Temperature /
// Timeout" + "Implement connection test." — the frontend half of
// `src-tauri/src/commands/ai.rs`'s deliberately backend-persistence-free
// `AiProviderSettings` (that module's own doc comment: "the frontend owns
// storing `AiProviderSettings` and passes it into each call").
//
// Mirrors `stores/capcut.svelte.ts` / `stores/modelManager.svelte.ts`'s own
// "detect/configure + localStorage persistence + a destructive-ish write
// action" shape: every non-secret field persists to `localStorage`
// immediately on change (same "no separate big Save button" precedent
// `capcutStore.setManualDraftRoot` already establishes — there is nothing to
// lose by persisting per-field, unlike the API key itself, which is the one
// piece of state that genuinely needs an explicit, deliberate action before
// it leaves this store).
//
// ## Credential ref generation (task brief asks this to be called out)
//
// `credential_ref` is this store's own choice of caller-chosen string key
// (`ai::commands::ai` module doc comment: "your call how to generate/manage
// it, just keep it stable across sessions"). This store keys it off the
// *provider kind* (`ai-provider:{provider}`) rather than one single global
// ref: switching providers is common (comparing OpenAI vs. a local Ollama,
// say), and a single shared ref would mean every provider switch either
// clobbers the previous provider's stored key or silently reuses a key that
// was never valid for the newly selected provider's API. Keying by provider
// kind means each of the five `AiProviderKind` variants remembers its own
// key independently and deterministically (no randomness needed for
// stability — the same provider kind always resolves to the same ref).
//
// ## Why `keyConfigured` (not the key itself) lives in localStorage
//
// There is deliberately no `get_ai_api_key` command anywhere in this
// codebase (`commands::ai` module doc comment) — the only way a stored key
// is ever read back is server-side, inside `test_ai_connection`, never to
// the frontend. So this store cannot ask the backend "is a key configured
// for this ref?" on load; instead it remembers, client-side, whether *this
// store* has successfully called `set_ai_api_key`/`delete_ai_api_key` for a
// given ref. This is a best-effort local flag, not an authoritative read of
// Windows Credential Manager: if `localStorage` is cleared independently of
// the Credential Manager entry (e.g. a different app profile, manual
// clearing), this flag can read "not configured" while a real credential
// still exists server-side — harmless (an unnecessary "Save Key" re-prompt
// at worst), and unavoidable without a backend read path this phase
// deliberately does not add.

import { commands } from "../types/bindings";
import type { AiConnectionTestResult, AiProviderKind, AiProviderSettings } from "../types/bindings";

export const AI_PROVIDER_KINDS: readonly AiProviderKind[] = [
  "open_ai",
  "ollama",
  "custom_open_ai_compatible",
  "anthropic",
  "gemini",
];

export type AiKeyRequirement = "required" | "recommended" | "optional";

/** Per-provider sensible default base URL (task brief: "sensible per-provider
 * defaults"). Blank for the two providers that have no single sensible
 * default at all — a custom endpoint is by definition user-specific, and a
 * real Anthropic/Gemini base URL is provided here but still editable (a
 * corporate proxy/gateway may front either). */
export function defaultBaseUrlFor(provider: AiProviderKind): string {
  switch (provider) {
    case "open_ai":
      return "https://api.openai.com/v1";
    case "ollama":
      return "http://localhost:11434/v1";
    case "custom_open_ai_compatible":
      return "";
    case "anthropic":
      return "https://api.anthropic.com";
    case "gemini":
      return "https://generativelanguage.googleapis.com";
  }
}

/** Per-provider suggested default model name — a UI nicety only (never sent
 * anywhere unless the user keeps it), so a fresh dialog isn't left with an
 * empty, guaranteed-to-fail model field. */
export function defaultModelFor(provider: AiProviderKind): string {
  switch (provider) {
    case "open_ai":
      return "gpt-4o-mini";
    case "ollama":
      return "llama3.1";
    case "custom_open_ai_compatible":
      return "";
    case "anthropic":
      return "claude-3-5-sonnet-latest";
    case "gemini":
      return "gemini-1.5-flash";
  }
}

/** Whether a key is *normally* needed for this provider kind — a labeling
 * hint only, distinct from what the backend actually enforces
 * (`commands::ai::build_provider` hard-requires a key only for `Anthropic`/
 * `Gemini`; `OpenAi`/`CustomOpenAiCompatible`/`Ollama` accept `None`, since a
 * local or self-hosted endpoint may need no auth at all). */
export function keyRequirementFor(provider: AiProviderKind): AiKeyRequirement {
  switch (provider) {
    case "anthropic":
    case "gemini":
      return "required";
    case "ollama":
      return "optional";
    case "open_ai":
    case "custom_open_ai_compatible":
      return "recommended";
  }
}

function credentialRefFor(provider: AiProviderKind): string {
  return `ai-provider:${provider}`;
}

// ---------------------------------------------------------------------------
// localStorage persistence (non-secret settings only — see module doc
// comment for why the secret itself never lives here)
// ---------------------------------------------------------------------------

const SETTINGS_STORAGE_KEY = "ave:ai:settings";
const KEY_CONFIGURED_STORAGE_KEY = "ave:ai:keyConfigured";

interface PersistedAiSettings {
  provider: AiProviderKind;
  base_url: string;
  model: string;
  temperature: number;
  timeout_ms: number;
}

function isProviderKind(value: unknown): value is AiProviderKind {
  return typeof value === "string" && (AI_PROVIDER_KINDS as readonly string[]).includes(value);
}

function loadPersistedSettings(): PersistedAiSettings | null {
  try {
    const raw = localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<PersistedAiSettings> | null;
    if (!parsed || !isProviderKind(parsed.provider)) return null;
    const provider = parsed.provider;
    return {
      provider,
      base_url: typeof parsed.base_url === "string" ? parsed.base_url : defaultBaseUrlFor(provider),
      model: typeof parsed.model === "string" ? parsed.model : defaultModelFor(provider),
      temperature: typeof parsed.temperature === "number" ? parsed.temperature : 0.7,
      timeout_ms: typeof parsed.timeout_ms === "number" ? parsed.timeout_ms : 30_000,
    };
  } catch {
    // localStorage may be unavailable, or hold a malformed value from an
    // older/foreign build — fall back to fresh defaults rather than throw.
    return null;
  }
}

function savePersistedSettings(settings: PersistedAiSettings): void {
  try {
    localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  } catch {
    /* storage may be disabled — settings simply won't survive a restart */
  }
}

function loadKeyConfigured(): Record<string, boolean> {
  try {
    const raw = localStorage.getItem(KEY_CONFIGURED_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    return parsed && typeof parsed === "object" ? (parsed as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

function saveKeyConfigured(map: Record<string, boolean>): void {
  try {
    localStorage.setItem(KEY_CONFIGURED_STORAGE_KEY, JSON.stringify(map));
  } catch {
    /* storage may be disabled — this flag simply won't survive a restart */
  }
}

const initialSettings = loadPersistedSettings();
const initialKeyConfigured = loadKeyConfigured();

class AiSettingsStore {
  open = $state(false);

  provider = $state<AiProviderKind>(initialSettings?.provider ?? "open_ai");
  baseUrl = $state<string>(initialSettings?.base_url ?? defaultBaseUrlFor("open_ai"));
  model = $state<string>(initialSettings?.model ?? defaultModelFor("open_ai"));
  temperature = $state<number>(initialSettings?.temperature ?? 0.7);
  timeoutMs = $state<number>(initialSettings?.timeout_ms ?? 30_000);

  /** Best-effort local record of which `credential_ref`s this store has
   * successfully saved a key for — see module doc comment. */
  keyConfigured = $state<Record<string, boolean>>(initialKeyConfigured);

  /** Transient only — never persisted, never re-read after a successful
   * `saveApiKey()` (which clears it immediately). */
  apiKeyDraft = $state("");
  savingKey = $state(false);
  keyActionError = $state<string | null>(null);

  testing = $state(false);
  testResult = $state<AiConnectionTestResult | null>(null);

  credentialRef = $derived(credentialRefFor(this.provider));
  hasKeyConfigured = $derived(this.keyConfigured[this.credentialRef] ?? false);
  keyRequirement = $derived(keyRequirementFor(this.provider));

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openDialog(): void {
    this.open = true;
    this.apiKeyDraft = "";
    this.keyActionError = null;
    this.testResult = null;
  }

  close(): void {
    this.open = false;
  }

  private persist(): void {
    savePersistedSettings({
      provider: this.provider,
      base_url: this.baseUrl,
      model: this.model,
      temperature: this.temperature,
      timeout_ms: this.timeoutMs,
    });
  }

  /** Changing provider also refreshes base URL/model to the new provider's
   * defaults, but only when the current value is empty or was exactly the
   * *previous* provider's own default — a value the user has actually typed
   * themselves is never silently overwritten. */
  setProvider(next: AiProviderKind): void {
    if (next === this.provider) return;
    const prevDefaultBase = defaultBaseUrlFor(this.provider);
    const prevDefaultModel = defaultModelFor(this.provider);
    this.provider = next;
    if (this.baseUrl.trim() === "" || this.baseUrl === prevDefaultBase) {
      this.baseUrl = defaultBaseUrlFor(next);
    }
    if (this.model.trim() === "" || this.model === prevDefaultModel) {
      this.model = defaultModelFor(next);
    }
    this.testResult = null;
    this.persist();
  }

  setBaseUrl(value: string): void {
    this.baseUrl = value;
    this.persist();
  }

  setModel(value: string): void {
    this.model = value;
    this.persist();
  }

  setTemperature(value: number): void {
    this.temperature = value;
    this.persist();
  }

  setTimeoutMs(value: number): void {
    this.timeoutMs = value;
    this.persist();
  }

  /** Plain-object snapshot handed to every `AiProviderSettings`-taking
   * command — safe to pass over IPC (not a `$state` proxy). */
  settingsSnapshot(): AiProviderSettings {
    return {
      provider: this.provider,
      base_url: this.baseUrl,
      model: this.model,
      temperature: this.temperature,
      timeout_ms: this.timeoutMs,
      credential_ref: this.credentialRef,
    };
  }

  // -------------------------------------------------------------------
  // Credential storage (write-only from this store's perspective too — see
  // module doc comment: no `get_ai_api_key` exists anywhere in this app)
  // -------------------------------------------------------------------

  async saveApiKey(): Promise<void> {
    const key = this.apiKeyDraft.trim();
    if (!key || this.savingKey) return;
    this.savingKey = true;
    this.keyActionError = null;
    try {
      const result = await commands.setAiApiKey(this.credentialRef, key);
      if (result.status === "ok") {
        this.keyConfigured = { ...this.keyConfigured, [this.credentialRef]: true };
        saveKeyConfigured(this.keyConfigured);
        // Never kept in memory after a successful save — the whole point of
        // "write-only" (module doc comment).
        this.apiKeyDraft = "";
      } else {
        this.keyActionError = result.error.message;
      }
    } catch (err) {
      this.keyActionError = String(err);
    } finally {
      this.savingKey = false;
    }
  }

  async deleteApiKey(): Promise<void> {
    if (this.savingKey) return;
    this.savingKey = true;
    this.keyActionError = null;
    try {
      const result = await commands.deleteAiApiKey(this.credentialRef);
      if (result.status === "ok") {
        this.keyConfigured = { ...this.keyConfigured, [this.credentialRef]: false };
        saveKeyConfigured(this.keyConfigured);
      } else {
        this.keyActionError = result.error.message;
      }
    } catch (err) {
      this.keyActionError = String(err);
    } finally {
      this.savingKey = false;
    }
  }

  // -------------------------------------------------------------------
  // Connection test (master prompt §17: "Implement connection test.")
  // -------------------------------------------------------------------

  async testConnection(): Promise<void> {
    if (this.testing) return;
    this.testing = true;
    this.testResult = null;
    try {
      // `test_ai_connection` never throws by design (folds every failure
      // into `{success: false, message}`), but the IPC call itself could
      // still reject — surfaced as a synthetic failed result rather than an
      // unhandled rejection.
      this.testResult = await commands.testAiConnection(this.settingsSnapshot());
    } catch (err) {
      this.testResult = { success: false, message: String(err) };
    } finally {
      this.testing = false;
    }
  }
}

export const aiSettingsStore = new AiSettingsStore();

/**
 * Convenience entry point for other stores (the NL command box, and a future
 * Smart Edit UI) to read the currently configured provider settings without
 * importing this store's full dialog-shaped class — same "expose a plain
 * function, not the whole class" precedent `stores/modelManager.svelte.ts`'s
 * `openModelManager()` already establishes.
 */
export function currentAiProviderSettings(): AiProviderSettings {
  return aiSettingsStore.settingsSnapshot();
}
