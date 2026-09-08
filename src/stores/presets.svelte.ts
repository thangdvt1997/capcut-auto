// Svelte 5 runes-based store for the Unified Preset System (`STUDIO_PLAN.md`
// Phase D17, closing the §18 gap that phase's own audit named: `promt.md`
// §18 asks for named, full-pipeline presets — "Spanish Crime Movie,"
// "Spanish Romance," "English Shorts," "TikTok Auto Dub" — each bundling
// "AI config reference / Translation config / Voice config / Subtitle
// config / Video processing / Render config / CapCut config," with the
// explicit rule "KHÔNG lưu plaintext secret vào preset export" (never save a
// plaintext secret into a preset export).
//
// ## What a `Preset` actually bundles, and why not every §18 category
//
// A `Preset` here is a name plus a snapshot of each *real* settings area
// this codebase actually has today, referenced by its real field names —
// never a re-invented shape:
//
// - **AI config** (`aiSettings.svelte.ts`'s `AiSettingsStore`): `provider`,
//   `baseUrl`, `model`, `temperature`, `timeoutMs` — exactly the fields that
//   store itself persists to `localStorage` (`PersistedAiSettings`). The key
//   itself is never included — see "No secrets, ever" below.
// - **Translation config** (`translationReview.svelte.ts`'s
//   `TranslationReviewStore`): `targetLanguage`, `genre`,
//   `translationStyle`, `characterContext`, `preserveNames`,
//   `preserveTerminology`, `profanityHandling`, `sentenceLengthOptimization`,
//   `voiceFriendlyRewrite` — every field that store's own `buildSettings()`
//   sends to `translate_captions`, plus `targetLanguage` itself (not part of
//   `TranslationSettings`, but the other real, adjacent field on that same
//   store).
// - **Voice config** (`voiceSettings.svelte.ts`'s `VoiceSettingsStore`):
//   `provider`, `baseUrl`, `roleMappings` — the same fields that store
//   itself persists. The key itself is never included.
// - **Render config** (`render::presets`, `stores/render.svelte.ts`): a
//   preset here only ever stores `exportPresetId` — the existing
//   `RenderPreset.id` (e.g. `"p1080"`) `render.svelte.ts::selectPreset` and
//   `automation.svelte.ts::createExportPresetId` already reference this same
//   way. There's no need to duplicate `RenderSettings`'s own fields; the id
//   *is* the reference.
// - **CapCut config** (`capcut.svelte.ts`'s `CapCutStore`):
//   `manualDraftRoot` — the one real, machine-wide, persisted CapCut setting
//   that store owns (`setManualDraftRoot`).
//
// **Deliberately NOT included: "Subtitle config" and "Video processing"**
// (`promt.md` §18's own list also names these). Investigated and confirmed
// absent as a *persisted, reusable settings concept*: `BatchPipelineConfig`
// (`StartBatchDialog.svelte`) does carry real remove-silence/caption-
// generation fields, but that dialog builds a fresh one from its own
// in-component `$state` on every open — there is no store anywhere that
// persists "my usual silence-removal threshold" or "my usual caption
// grouping" as a standalone, reusable setting the way `aiSettingsStore`/
// `voiceSettingsStore` persist theirs. Bundling them into a `Preset` here
// would mean inventing a brand-new persisted-settings concept this task's
// own scope doesn't cover, and would violate this store's own "don't invent
// fields that don't correspond to something real" discipline. A future
// "Batch defaults" settings store would be the honest way to close that
// specific gap; this phase does not fake it.
//
// ## No secrets, ever — the structural guarantee, not just a promise
//
// `Preset.aiConfig`/`Preset.voiceConfig` never have a field capable of
// holding a key: `AiSettingsStore`/`VoiceSettingsStore` themselves never
// hold the raw key in memory past a `saveApiKey()` call (both stores' own
// module doc comments: "write-only... never kept in memory after a
// successful save"), and neither store exposes anything resembling a
// `credential`/`apiKey`/`secret` field for `save()` below to even read by
// mistake — the *only* provider-identifying value either store has is
// `credentialRef`/`provider`, an opaque, non-secret string
// (`ai::credentials` module doc comment: "opaque `String` that never
// carries a secret itself"). A preset can honestly say "this preset expects
// an OpenAI key to be configured" (via `aiConfig.provider`) without ever
// containing one. `presets.test.ts` proves this for real: it seeds a fake
// key into `aiSettingsStore`, calls `save()`, and asserts the resulting
// preset's JSON serialization contains no trace of it.
//
// ## Storage: `localStorage`, matching every other settings store here
//
// Same reasoning `aiSettings.svelte.ts`/`voiceSettings.svelte.ts`/
// `capcut.svelte.ts` each already give for their own non-secret settings:
// nothing about a preset needs to exist before the frontend has loaded (the
// one *documented* exception in this codebase, `batch::settings`'s
// `max_concurrent_jobs`, needs backend storage only because
// `spawn_worker_pool` runs before the webview loads at all — nothing here
// runs before the frontend). A preset is purely "apply these settings to my
// current session," so `localStorage` is the honest, minimal-footprint
// choice; no new backend settings-persistence surface was added for it.
//
// ## Export/Import: the one real backend addition, and why
//
// `promt.md` §18's own "cho phép lưu toàn bộ config thành preset" implies
// shareability, and there is no way for the frontend to read/write an
// arbitrary user-chosen file's *contents* without either a backend command
// or the (absent) `@tauri-apps/plugin-fs` dependency — `@tauri-apps/
// plugin-dialog`'s `save()`/`open()` only ever resolve a *path*
// (`TopBar.svelte::saveProjectAsToDisk`'s own established pattern, reused
// here verbatim for picking the path). `commands.exportPresetToFile`/
// `commands.importPresetFromFile` (`commands::presets`, Rust) are the thin,
// schema-free IPC wrapper this genuinely requires — see that module's own
// doc comment. This store owns the actual `Preset` JSON shape and all of
// its validation; the backend never inspects it.

import { save, open } from "@tauri-apps/plugin-dialog";
import { commands } from "../types/bindings";
import type {
  AiProviderKind,
  ProfanityHandling,
  RenderPreset,
  TranslationGenre,
  VoiceProviderKind,
} from "../types/bindings";
import { aiSettingsStore, defaultBaseUrlFor, defaultModelFor } from "./aiSettings.svelte";
import { translationReviewStore } from "./translationReview.svelte";
import { voiceSettingsStore, type VoiceRoleMapping } from "./voiceSettings.svelte";
import { capcutStore } from "./capcut.svelte";
import { renderStore } from "./render.svelte";

export interface PresetAiConfig {
  provider: AiProviderKind;
  baseUrl: string;
  model: string;
  temperature: number;
  timeoutMs: number;
}

export interface PresetTranslationConfig {
  targetLanguage: string;
  genre: TranslationGenre | "";
  translationStyle: string;
  characterContext: string;
  preserveNames: boolean;
  preserveTerminology: boolean;
  profanityHandling: ProfanityHandling | "";
  sentenceLengthOptimization: boolean;
  voiceFriendlyRewrite: boolean;
}

export interface PresetVoiceConfig {
  provider: VoiceProviderKind;
  baseUrl: string;
  roleMappings: VoiceRoleMapping[];
}

export interface PresetCapCutConfig {
  manualDraftRoot: string | null;
}

export interface Preset {
  id: string;
  name: string;
  createdAt: string;
  aiConfig: PresetAiConfig;
  translationConfig: PresetTranslationConfig;
  voiceConfig: PresetVoiceConfig;
  exportPresetId: string | null;
  capcutConfig: PresetCapCutConfig;
}

const STORAGE_KEY = "ave:presets:list";
const AI_PROVIDER_KINDS: readonly AiProviderKind[] = ["open_ai", "ollama", "custom_open_ai_compatible", "anthropic", "gemini"];
const VOICE_PROVIDER_KINDS: readonly VoiceProviderKind[] = ["custom_api", "nts_gen_ai", "gpt_so_vits"];
const TRANSLATION_GENRES: readonly TranslationGenre[] = [
  "drama_romance",
  "fantasy_cultivation",
  "crime_detective",
  "police_bodycam",
  "prison_crime",
  "survival",
  "documentary",
  "custom",
];
const PROFANITY_HANDLINGS: readonly ProfanityHandling[] = ["preserve", "soften", "remove"];

function isAiProviderKind(v: unknown): v is AiProviderKind {
  return typeof v === "string" && (AI_PROVIDER_KINDS as readonly string[]).includes(v);
}
function isVoiceProviderKind(v: unknown): v is VoiceProviderKind {
  return typeof v === "string" && (VOICE_PROVIDER_KINDS as readonly string[]).includes(v);
}
function isTranslationGenreOrBlank(v: unknown): v is TranslationGenre | "" {
  return v === "" || (typeof v === "string" && (TRANSLATION_GENRES as readonly string[]).includes(v));
}
function isProfanityHandlingOrBlank(v: unknown): v is ProfanityHandling | "" {
  return v === "" || (typeof v === "string" && (PROFANITY_HANDLINGS as readonly string[]).includes(v));
}
function isRoleMapping(v: unknown): v is VoiceRoleMapping {
  return (
    !!v &&
    typeof v === "object" &&
    typeof (v as VoiceRoleMapping).role === "string" &&
    typeof (v as VoiceRoleMapping).voiceId === "string" &&
    typeof (v as VoiceRoleMapping).voiceName === "string"
  );
}

/** Defensive re-validation of a `Preset`-shaped value — used both for
 * `localStorage` (which could hold a stale/foreign-build shape, same
 * "fall back rather than throw" posture every other store's own
 * `loadPersisted*` uses) and for an *imported* file (which could be
 * hand-edited or from an incompatible future version). Returns `null`
 * rather than throwing on anything that doesn't check out — never a
 * partially-valid `Preset` silently missing fields. */
export function validatePreset(value: unknown): Preset | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  if (typeof v.id !== "string" || typeof v.name !== "string" || typeof v.createdAt !== "string") return null;

  const ai = v.aiConfig as Record<string, unknown> | undefined;
  if (!ai || !isAiProviderKind(ai.provider) || typeof ai.baseUrl !== "string" || typeof ai.model !== "string") {
    return null;
  }
  const aiConfig: PresetAiConfig = {
    provider: ai.provider,
    baseUrl: ai.baseUrl,
    model: ai.model,
    temperature: typeof ai.temperature === "number" ? ai.temperature : 0.7,
    timeoutMs: typeof ai.timeoutMs === "number" ? ai.timeoutMs : 30_000,
  };

  const tr = v.translationConfig as Record<string, unknown> | undefined;
  if (!tr || typeof tr.targetLanguage !== "string" || !isTranslationGenreOrBlank(tr.genre)) return null;
  const translationConfig: PresetTranslationConfig = {
    targetLanguage: tr.targetLanguage,
    genre: tr.genre,
    translationStyle: typeof tr.translationStyle === "string" ? tr.translationStyle : "",
    characterContext: typeof tr.characterContext === "string" ? tr.characterContext : "",
    preserveNames: tr.preserveNames === true,
    preserveTerminology: tr.preserveTerminology === true,
    profanityHandling: isProfanityHandlingOrBlank(tr.profanityHandling) ? tr.profanityHandling : "",
    sentenceLengthOptimization: tr.sentenceLengthOptimization === true,
    voiceFriendlyRewrite: tr.voiceFriendlyRewrite === true,
  };

  const vo = v.voiceConfig as Record<string, unknown> | undefined;
  if (!vo || !isVoiceProviderKind(vo.provider) || typeof vo.baseUrl !== "string") return null;
  const roleMappings = Array.isArray(vo.roleMappings) ? vo.roleMappings.filter(isRoleMapping) : [];
  const voiceConfig: PresetVoiceConfig = { provider: vo.provider, baseUrl: vo.baseUrl, roleMappings };

  const cc = v.capcutConfig as Record<string, unknown> | undefined;
  const capcutConfig: PresetCapCutConfig = {
    manualDraftRoot: cc && typeof cc.manualDraftRoot === "string" ? cc.manualDraftRoot : null,
  };

  return {
    id: v.id,
    name: v.name,
    createdAt: v.createdAt,
    aiConfig,
    translationConfig,
    voiceConfig,
    exportPresetId: typeof v.exportPresetId === "string" ? v.exportPresetId : null,
    capcutConfig,
  };
}

function loadPersisted(): Preset[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.map(validatePreset).filter((p): p is Preset => p !== null);
  } catch {
    return [];
  }
}

function savePersisted(presets: Preset[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(presets));
  } catch {
    /* storage may be disabled — presets simply won't survive a restart */
  }
}

/** Characters reserved/illegal in a Windows filename — reused for the
 * export dialog's suggested filename, same purpose as `capcut.svelte.ts`'s
 * own `sanitizeDraftName` for a folder name. */
function sanitizeFileName(name: string): string {
  return name.replace(/["*/:<>?|\\]/g, "").trim() || "preset";
}

function newId(): string {
  // `crypto.randomUUID` is available in every Tauri webview target this app
  // ships for (same assumption `stores/media.svelte.ts` already makes for
  // asset ids) — no extra dependency needed for a client-generated id.
  return crypto.randomUUID();
}

const initialPresets = loadPersisted();

class PresetsStore {
  open = $state(false);
  presets = $state<Preset[]>(initialPresets);

  // ---- Save current settings as a new preset ----
  saveNameDraft = $state("");
  saveError = $state<string | null>(null);

  // ---- Apply ----
  applyingId = $state<string | null>(null);

  // ---- Remove (two-step confirm, same arm/cancel/confirm shape
  //      `stores/automation.svelte.ts`'s own delete already uses) ----
  pendingDeleteId = $state<string | null>(null);

  // ---- Export/Import ----
  exportingId = $state<string | null>(null);
  exportError = $state<string | null>(null);
  importing = $state(false);
  importError = $state<string | null>(null);

  // ---- Render presets (for the export-preset picker + name lookup) ----
  renderPresets = $state<RenderPreset[]>([]);
  private renderPresetsLoaded = false;

  canSave = $derived(this.saveNameDraft.trim().length > 0);

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openDialog(): void {
    this.open = true;
    this.saveNameDraft = "";
    this.saveError = null;
    this.pendingDeleteId = null;
    this.exportError = null;
    this.importError = null;
    void this.ensureRenderPresetsLoaded();
  }

  close(): void {
    this.open = false;
    this.pendingDeleteId = null;
  }

  async ensureRenderPresetsLoaded(): Promise<void> {
    if (this.renderPresetsLoaded) return;
    this.renderPresets = await commands.listRenderPresets();
    this.renderPresetsLoaded = true;
  }

  renderPresetName(id: string | null): string | null {
    if (!id) return null;
    return this.renderPresets.find((p) => p.id === id)?.name ?? id;
  }

  // -------------------------------------------------------------------
  // Save (snapshot every real store's current non-secret settings)
  // -------------------------------------------------------------------

  save(name: string): Preset {
    const trimmed = name.trim();
    const preset: Preset = {
      id: newId(),
      name: trimmed || "Untitled Preset",
      createdAt: new Date().toISOString(),
      aiConfig: {
        provider: aiSettingsStore.provider,
        baseUrl: aiSettingsStore.baseUrl,
        model: aiSettingsStore.model,
        temperature: aiSettingsStore.temperature,
        timeoutMs: aiSettingsStore.timeoutMs,
      },
      translationConfig: {
        targetLanguage: translationReviewStore.targetLanguage,
        genre: translationReviewStore.genre,
        translationStyle: translationReviewStore.translationStyle,
        characterContext: translationReviewStore.characterContext,
        preserveNames: translationReviewStore.preserveNames,
        preserveTerminology: translationReviewStore.preserveTerminology,
        profanityHandling: translationReviewStore.profanityHandling,
        sentenceLengthOptimization: translationReviewStore.sentenceLengthOptimization,
        voiceFriendlyRewrite: translationReviewStore.voiceFriendlyRewrite,
      },
      voiceConfig: {
        provider: voiceSettingsStore.provider,
        baseUrl: voiceSettingsStore.baseUrl,
        // Plain array copy (not the live `$state` proxy) — same "snapshot,
        // don't alias" discipline `capcut.svelte.ts`'s own `snap()` helper
        // documents, so a later mutation of `voiceSettingsStore.roleMappings`
        // never silently rewrites an already-saved preset.
        roleMappings: voiceSettingsStore.roleMappings.map((m) => ({ ...m })),
      },
      exportPresetId: renderStore.selectedPresetId,
      capcutConfig: {
        manualDraftRoot: capcutStore.manualDraftRoot,
      },
    };
    this.presets = [...this.presets, preset];
    savePersisted(this.presets);
    return preset;
  }

  submitSave(): void {
    if (!this.canSave) return;
    this.save(this.saveNameDraft);
    this.saveNameDraft = "";
    this.saveError = null;
  }

  // -------------------------------------------------------------------
  // Apply (write each field back into its owning real store, via that
  // store's own existing setter methods)
  // -------------------------------------------------------------------

  async apply(presetId: string): Promise<void> {
    const preset = this.presets.find((p) => p.id === presetId);
    if (!preset || this.applyingId) return;
    this.applyingId = presetId;
    try {
      // AI config — `setProvider` alone can auto-refresh baseUrl/model to
      // the new provider's own defaults (its own doc comment); calling
      // `setBaseUrl`/`setModel` afterward always wins, so the preset's exact
      // saved values land regardless of that side effect.
      aiSettingsStore.setProvider(preset.aiConfig.provider);
      aiSettingsStore.setBaseUrl(preset.aiConfig.baseUrl || defaultBaseUrlFor(preset.aiConfig.provider));
      aiSettingsStore.setModel(preset.aiConfig.model || defaultModelFor(preset.aiConfig.provider));
      aiSettingsStore.setTemperature(preset.aiConfig.temperature);
      aiSettingsStore.setTimeoutMs(preset.aiConfig.timeoutMs);

      // Translation config — `translationReviewStore` has no dedicated
      // setter methods (every field is bound directly from
      // `TranslationReviewDialog.svelte` via `bind:value`/`bind:checked`,
      // its own established mutation convention); direct assignment here
      // matches that store's real, existing convention rather than
      // bypassing one that doesn't exist.
      translationReviewStore.targetLanguage = preset.translationConfig.targetLanguage;
      translationReviewStore.genre = preset.translationConfig.genre;
      translationReviewStore.translationStyle = preset.translationConfig.translationStyle;
      translationReviewStore.characterContext = preset.translationConfig.characterContext;
      translationReviewStore.preserveNames = preset.translationConfig.preserveNames;
      translationReviewStore.preserveTerminology = preset.translationConfig.preserveTerminology;
      translationReviewStore.profanityHandling = preset.translationConfig.profanityHandling;
      translationReviewStore.sentenceLengthOptimization = preset.translationConfig.sentenceLengthOptimization;
      translationReviewStore.voiceFriendlyRewrite = preset.translationConfig.voiceFriendlyRewrite;

      // Voice config
      voiceSettingsStore.setProvider(preset.voiceConfig.provider);
      voiceSettingsStore.setBaseUrl(preset.voiceConfig.baseUrl);
      voiceSettingsStore.setRoleMappings(preset.voiceConfig.roleMappings.map((m) => ({ ...m })));

      // Render/export config — only meaningful once the real preset catalog
      // is loaded (`selectPreset` looks it up by id).
      if (preset.exportPresetId) {
        await renderStore.ensurePresetsLoaded();
        renderStore.selectPreset(preset.exportPresetId);
      }

      // CapCut config
      capcutStore.setManualDraftRoot(preset.capcutConfig.manualDraftRoot);
    } finally {
      this.applyingId = null;
    }
  }

  // -------------------------------------------------------------------
  // Remove (arm/confirm — `stores/automation.svelte.ts`'s own delete)
  // -------------------------------------------------------------------

  armDelete(id: string): void {
    this.pendingDeleteId = id;
  }

  cancelDelete(): void {
    this.pendingDeleteId = null;
  }

  remove(id: string): void {
    this.presets = this.presets.filter((p) => p.id !== id);
    savePersisted(this.presets);
    if (this.pendingDeleteId === id) this.pendingDeleteId = null;
  }

  // -------------------------------------------------------------------
  // Export/Import (`commands::presets`, see module doc comment)
  // -------------------------------------------------------------------

  async exportToFile(id: string): Promise<void> {
    const preset = this.presets.find((p) => p.id === id);
    if (!preset || this.exportingId) return;
    const chosen = await save({
      filters: [{ name: "Preset", extensions: ["json"] }],
      defaultPath: `${sanitizeFileName(preset.name)}.json`,
    });
    if (!chosen) return;
    this.exportingId = id;
    this.exportError = null;
    try {
      const json = JSON.stringify(preset, null, 2);
      const result = await commands.exportPresetToFile(json, chosen);
      if (result.status === "error") {
        this.exportError = result.error.message;
      }
    } catch (err) {
      this.exportError = String(err);
    } finally {
      this.exportingId = null;
    }
  }

  async importFromFile(): Promise<void> {
    if (this.importing) return;
    const chosen = await open({ filters: [{ name: "Preset", extensions: ["json"] }] });
    if (!chosen || typeof chosen !== "string") return;
    this.importing = true;
    this.importError = null;
    try {
      const result = await commands.importPresetFromFile(chosen);
      if (result.status === "error") {
        this.importError = result.error.message;
        return;
      }
      let parsedRaw: unknown;
      try {
        parsedRaw = JSON.parse(result.data);
      } catch {
        this.importError = "The chosen file is not valid JSON.";
        return;
      }
      const validated = validatePreset(parsedRaw);
      if (!validated) {
        this.importError = "The chosen file is not a recognizable preset.";
        return;
      }
      // A fresh id/name-collision is possible (importing the same file
      // twice, or a file exported from another machine) — always mint a new
      // local id so an import never silently overwrites an existing saved
      // preset by accident; the imported name is kept as-is.
      const imported: Preset = { ...validated, id: newId() };
      this.presets = [...this.presets, imported];
      savePersisted(this.presets);
    } catch (err) {
      this.importError = String(err);
    } finally {
      this.importing = false;
    }
  }
}

export const presetsStore = new PresetsStore();
