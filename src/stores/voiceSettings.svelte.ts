// Svelte 5 runes-based store for the Voice Settings dialog (`promt.md` §9
// "VOICE / TTS CONFIGURATION"): Provider / Server API URL / API Key + a real
// "Test Connection"/"Refresh Voices" pair, plus Voice Mapping (Speaker
// role -> voice). Mirrors `stores/aiSettings.svelte.ts`'s own shape closely
// (same localStorage-persistence-for-non-secrets, credential-store-for-the-
// key, write-only-key posture) — read that file's own module doc comment
// for the full reasoning behind each of these choices, not repeated here.
//
// ## Backend reality this store must stay honest about
//
// Only `custom_api` (`VoiceProviderKind`) has a real backend adapter
// (`voice::custom_api::CustomApiVoiceProvider`) — `nts_gen_ai`/`gpt_so_vits`
// are honest stubs that immediately fail every call with a clear
// `NotImplemented`-shaped error (`voice::stub` module doc comment). This
// store does not hide that: `testConnection`/`refreshVoices` against a stub
// provider will show that stub's own real, honest failure message, not a
// fabricated success.
//
// ## Voice Mapping — saved, not yet consumed by anything real
//
// `promt.md` §9's "Male/Female/Narrator" voice-role concept has no
// synthesis pipeline anywhere in this codebase yet (`commands::voice`
// module doc comment: no `synthesize_speech` command exists) — so this
// store's `roleMappings` is a real, persisted user preference with nothing
// downstream that reads it yet. Stated plainly in the dialog itself, not
// hidden — matching this project's "state a gap plainly, never fake the
// missing half" discipline (same posture `voice::stub`'s own immediate,
// honest `NotImplemented` established for the backend side of this same
// feature).

import { commands } from "../types/bindings";
import type {
  VoiceConnectionTestResult,
  VoiceInfo,
  VoiceProviderKind,
  VoiceProviderSettings,
} from "../types/bindings";

export const VOICE_PROVIDER_KINDS: readonly VoiceProviderKind[] = ["custom_api", "nts_gen_ai", "gpt_so_vits"];

/** Only `custom_api` makes a real network call (module doc comment) — the
 * other two are honest stubs with no base URL/credential of their own. */
export function providerNeedsConnectionDetails(provider: VoiceProviderKind): boolean {
  return provider === "custom_api";
}

function credentialRefFor(provider: VoiceProviderKind): string | null {
  return providerNeedsConnectionDetails(provider) ? `voice-provider:${provider}` : null;
}

// ---------------------------------------------------------------------------
// localStorage persistence (non-secret settings + the voice-role mapping —
// same "no separate Save button, persist per-field" precedent
// `aiSettings.svelte.ts` already establishes; never the API key itself)
// ---------------------------------------------------------------------------

const SETTINGS_STORAGE_KEY = "ave:voice:settings";
const KEY_CONFIGURED_STORAGE_KEY = "ave:voice:keyConfigured";
const MAPPING_STORAGE_KEY = "ave:voice:roleMappings";

export interface VoiceRoleMapping {
  /** Free-text role name (`promt.md`'s own worked example: "Narrator",
   * "Speaker A", "Speaker B" — not a closed enum, since a real project may
   * have any number of named speakers). */
  role: string;
  voiceId: string;
  voiceName: string;
}

interface PersistedVoiceSettings {
  provider: VoiceProviderKind;
  base_url: string;
}

function isProviderKind(value: unknown): value is VoiceProviderKind {
  return typeof value === "string" && (VOICE_PROVIDER_KINDS as readonly string[]).includes(value);
}

function loadPersistedSettings(): PersistedVoiceSettings | null {
  try {
    const raw = localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<PersistedVoiceSettings> | null;
    if (!parsed || !isProviderKind(parsed.provider)) return null;
    return {
      provider: parsed.provider,
      base_url: typeof parsed.base_url === "string" ? parsed.base_url : "",
    };
  } catch {
    return null;
  }
}

function savePersistedSettings(settings: PersistedVoiceSettings): void {
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

function loadMappings(): VoiceRoleMapping[] {
  try {
    const raw = localStorage.getItem(MAPPING_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (m): m is VoiceRoleMapping =>
        !!m && typeof m.role === "string" && typeof m.voiceId === "string" && typeof m.voiceName === "string",
    );
  } catch {
    return [];
  }
}

function saveMappings(mappings: VoiceRoleMapping[]): void {
  try {
    localStorage.setItem(MAPPING_STORAGE_KEY, JSON.stringify(mappings));
  } catch {
    /* storage may be disabled — mappings simply won't survive a restart */
  }
}

const initialSettings = loadPersistedSettings();
const initialKeyConfigured = loadKeyConfigured();
const initialMappings = loadMappings();

class VoiceSettingsStore {
  open = $state(false);

  provider = $state<VoiceProviderKind>(initialSettings?.provider ?? "custom_api");
  baseUrl = $state<string>(initialSettings?.base_url ?? "");

  /** Best-effort local record of which `credential_ref`s this store has
   * successfully saved a key for — see `aiSettings.svelte.ts`'s own module
   * doc comment for why this (not a backend read) is the only honest source
   * of truth available (no `get_voice_api_key`-shaped command exists, or
   * should exist, anywhere). */
  keyConfigured = $state<Record<string, boolean>>(initialKeyConfigured);

  apiKeyDraft = $state("");
  savingKey = $state(false);
  keyActionError = $state<string | null>(null);

  testing = $state(false);
  testResult = $state<VoiceConnectionTestResult | null>(null);

  loadingVoices = $state(false);
  voices = $state<VoiceInfo[]>([]);
  voicesError = $state<string | null>(null);

  roleMappings = $state<VoiceRoleMapping[]>(initialMappings);
  newRoleName = $state("");

  needsConnectionDetails = $derived(providerNeedsConnectionDetails(this.provider));
  credentialRef = $derived(credentialRefFor(this.provider));
  hasKeyConfigured = $derived(this.credentialRef !== null && (this.keyConfigured[this.credentialRef] ?? false));

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
    savePersistedSettings({ provider: this.provider, base_url: this.baseUrl });
  }

  setProvider(next: VoiceProviderKind): void {
    if (next === this.provider) return;
    this.provider = next;
    this.testResult = null;
    this.voicesError = null;
    this.persist();
  }

  setBaseUrl(value: string): void {
    this.baseUrl = value;
    this.persist();
  }

  /** Plain-object snapshot handed to `testVoiceConnection`/`listVoices` —
   * safe to pass over IPC (not a `$state` proxy). */
  settingsSnapshot(): VoiceProviderSettings {
    return {
      provider: this.provider,
      base_url: this.baseUrl,
      credential_ref: this.credentialRef,
    };
  }

  // -------------------------------------------------------------------
  // Credential storage (write-only — see `aiSettings.svelte.ts`'s own
  // module doc comment for why: no read-back command exists or should)
  // -------------------------------------------------------------------

  async saveApiKey(): Promise<void> {
    const key = this.apiKeyDraft.trim();
    const ref = this.credentialRef;
    if (!key || !ref || this.savingKey) return;
    this.savingKey = true;
    this.keyActionError = null;
    try {
      const result = await commands.setAiApiKey(ref, key);
      if (result.status === "ok") {
        this.keyConfigured = { ...this.keyConfigured, [ref]: true };
        saveKeyConfigured(this.keyConfigured);
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
    const ref = this.credentialRef;
    if (!ref || this.savingKey) return;
    this.savingKey = true;
    this.keyActionError = null;
    try {
      const result = await commands.deleteAiApiKey(ref);
      if (result.status === "ok") {
        this.keyConfigured = { ...this.keyConfigured, [ref]: false };
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
  // Connection test + voice listing (`promt.md` §9)
  // -------------------------------------------------------------------

  async testConnection(): Promise<void> {
    if (this.testing) return;
    this.testing = true;
    this.testResult = null;
    try {
      // `test_voice_connection` never throws by design (folds every failure
      // into `{success: false, message}`, including a stub provider's own
      // honest NotImplemented), but the IPC call itself could still reject.
      this.testResult = await commands.testVoiceConnection(this.settingsSnapshot());
    } catch (err) {
      this.testResult = { success: false, message: String(err) };
    } finally {
      this.testing = false;
    }
  }

  async refreshVoices(): Promise<void> {
    if (this.loadingVoices) return;
    this.loadingVoices = true;
    this.voicesError = null;
    try {
      const result = await commands.listVoices(this.settingsSnapshot());
      if (result.status === "ok") {
        this.voices = result.data;
      } else {
        this.voices = [];
        this.voicesError = result.error.message;
      }
    } catch (err) {
      this.voices = [];
      this.voicesError = String(err);
    } finally {
      this.loadingVoices = false;
    }
  }

  // -------------------------------------------------------------------
  // Voice Mapping (`promt.md` §9 — saved, not yet consumed by any real
  // pipeline; see module doc comment)
  // -------------------------------------------------------------------

  addMapping(voice: VoiceInfo): void {
    const role = this.newRoleName.trim();
    if (!role) return;
    this.roleMappings = [
      ...this.roleMappings.filter((m) => m.role !== role),
      { role, voiceId: voice.voice_id, voiceName: voice.name },
    ];
    saveMappings(this.roleMappings);
    this.newRoleName = "";
  }

  removeMapping(role: string): void {
    this.roleMappings = this.roleMappings.filter((m) => m.role !== role);
    saveMappings(this.roleMappings);
  }

  /** Bulk replace — added for `stores/presets.svelte.ts::apply()` (Phase
   * D17), which restores a saved preset's *entire* role-mapping list in one
   * shot rather than one `addMapping` call per role. Persists exactly like
   * `addMapping`/`removeMapping` already do, so a preset applied this
   * session survives a restart the same way a manually-added mapping does. */
  setRoleMappings(mappings: VoiceRoleMapping[]): void {
    this.roleMappings = mappings;
    saveMappings(this.roleMappings);
  }
}

export const voiceSettingsStore = new VoiceSettingsStore();
