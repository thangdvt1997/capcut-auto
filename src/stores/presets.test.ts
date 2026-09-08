// Store test for `stores/presets.svelte.ts` (`STUDIO_PLAN.md` Phase D17).
// Every dependency store is mocked (same "mocked Tauri commands, real store
// logic" split `stores/aiSettings.test.ts` already establishes) so this file
// tests `PresetsStore`'s own `save`/`apply`/`remove`/`validatePreset`/
// export-import logic in isolation, without pulling in the real
// `aiSettingsStore`/`voiceSettingsStore`/`translationReviewStore`/
// `capcutStore`/`renderStore` module graph (`translationReviewStore` in
// particular transitively imports `captionsStore`/`timeline.svelte`, which
// this test has no need to exercise for real).
//
// The single most important test in this file is "no secret ever leaks into
// a saved preset" — proven for real, not just asserted, by seeding a fake
// API key into the mocked `aiSettingsStore`, calling `save()`, and asserting
// the resulting preset's JSON serialization contains no trace of it. This is
// a structural guarantee (`Preset`'s own TypeScript shape has no field that
// could hold a key — see `presets.svelte.ts`'s own module doc comment), but
// this test proves the *runtime* behavior matches the shape's promise, in
// case a future edit ever tried to smuggle a secret field in.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const listRenderPresets = vi.fn();
const exportPresetToFile = vi.fn();
const importPresetFromFile = vi.fn();

vi.mock("../types/bindings", () => ({
  commands: {
    listRenderPresets: (...args: unknown[]) => listRenderPresets(...args),
    exportPresetToFile: (...args: unknown[]) => exportPresetToFile(...args),
    importPresetFromFile: (...args: unknown[]) => importPresetFromFile(...args),
  },
}));

const saveDialog = vi.fn();
const openDialog = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => saveDialog(...args),
  open: (...args: unknown[]) => openDialog(...args),
}));

// ---- Fake, minimal stand-ins for every real settings store this store
//      reads from/writes back into. Each mirrors only the fields/methods
//      `presets.svelte.ts` actually touches — see that file for the real
//      shapes this intentionally matches. ----

const aiSettingsStore = {
  provider: "open_ai" as string,
  baseUrl: "https://api.openai.com/v1",
  model: "gpt-4o-mini",
  temperature: 0.7,
  timeoutMs: 30_000,
  // Deliberately NOT present: any `apiKey`/`credential`/`secret` field — the
  // real store never keeps one in memory past a save either (see
  // `aiSettings.svelte.ts`'s own module doc comment), so a fake key set via
  // `apiKeyDraft` below is the closest honest stand-in for "a secret this
  // store once briefly held."
  apiKeyDraft: "",
  setProvider: vi.fn(function (this: typeof aiSettingsStore, next: string) {
    this.provider = next;
  }),
  setBaseUrl: vi.fn(function (this: typeof aiSettingsStore, v: string) {
    this.baseUrl = v;
  }),
  setModel: vi.fn(function (this: typeof aiSettingsStore, v: string) {
    this.model = v;
  }),
  setTemperature: vi.fn(function (this: typeof aiSettingsStore, v: number) {
    this.temperature = v;
  }),
  setTimeoutMs: vi.fn(function (this: typeof aiSettingsStore, v: number) {
    this.timeoutMs = v;
  }),
};
vi.mock("./aiSettings.svelte", () => ({
  aiSettingsStore,
  defaultBaseUrlFor: () => "https://api.openai.com/v1",
  defaultModelFor: () => "gpt-4o-mini",
}));

const translationReviewStore = {
  targetLanguage: "vi",
  genre: "crime_detective" as string,
  translationStyle: "",
  characterContext: "",
  preserveNames: false,
  preserveTerminology: false,
  profanityHandling: "" as string,
  sentenceLengthOptimization: false,
  voiceFriendlyRewrite: false,
};
vi.mock("./translationReview.svelte", () => ({ translationReviewStore }));

const voiceSettingsStore = {
  provider: "custom_api" as string,
  baseUrl: "https://voice.example",
  roleMappings: [{ role: "Narrator", voiceId: "v1", voiceName: "Voice One" }],
  setProvider: vi.fn(function (this: typeof voiceSettingsStore, next: string) {
    this.provider = next;
  }),
  setBaseUrl: vi.fn(function (this: typeof voiceSettingsStore, v: string) {
    this.baseUrl = v;
  }),
  setRoleMappings: vi.fn(function (
    this: typeof voiceSettingsStore,
    mappings: typeof voiceSettingsStore.roleMappings,
  ) {
    this.roleMappings = mappings;
  }),
};
vi.mock("./voiceSettings.svelte", () => ({ voiceSettingsStore }));

const capcutStore = {
  manualDraftRoot: null as string | null,
  setManualDraftRoot: vi.fn(function (this: typeof capcutStore, v: string | null) {
    this.manualDraftRoot = v;
  }),
};
vi.mock("./capcut.svelte", () => ({ capcutStore }));

const renderStore = {
  selectedPresetId: "p1080" as string | null,
  presets: [{ id: "p1080", name: "1080p" }],
  ensurePresetsLoaded: vi.fn(async () => {}),
  selectPreset: vi.fn(function (this: typeof renderStore, id: string) {
    this.selectedPresetId = id;
  }),
};
vi.mock("./render.svelte", () => ({ renderStore }));

const STORAGE_KEY = "ave:presets:list";

async function freshImport() {
  vi.resetModules();
  return import("./presets.svelte");
}

beforeEach(() => {
  localStorage.clear();
  listRenderPresets.mockReset().mockResolvedValue([{ id: "p1080", name: "1080p" }]);
  exportPresetToFile.mockReset();
  importPresetFromFile.mockReset();
  saveDialog.mockReset();
  openDialog.mockReset();

  aiSettingsStore.provider = "open_ai";
  aiSettingsStore.baseUrl = "https://api.openai.com/v1";
  aiSettingsStore.model = "gpt-4o-mini";
  aiSettingsStore.temperature = 0.7;
  aiSettingsStore.timeoutMs = 30_000;
  aiSettingsStore.apiKeyDraft = "";

  translationReviewStore.targetLanguage = "vi";
  translationReviewStore.genre = "crime_detective";
  translationReviewStore.profanityHandling = "";

  voiceSettingsStore.provider = "custom_api";
  voiceSettingsStore.baseUrl = "https://voice.example";
  voiceSettingsStore.roleMappings = [{ role: "Narrator", voiceId: "v1", voiceName: "Voice One" }];

  capcutStore.manualDraftRoot = null;
  renderStore.selectedPresetId = "p1080";
});

afterEach(() => {
  localStorage.clear();
});

describe("presetsStore — save() never serializes a secret", () => {
  it("contains no trace of a fake API key seeded into aiSettingsStore", async () => {
    const FAKE_SECRET = "sk-super-secret-test-key-should-never-appear-anywhere";
    aiSettingsStore.apiKeyDraft = FAKE_SECRET;

    const { presetsStore } = await freshImport();
    const preset = presetsStore.save("Spanish Crime Movie");

    const json = JSON.stringify(preset);
    expect(json).not.toContain(FAKE_SECRET);
    expect(json).not.toContain("sk-super-secret");

    // Structural proof, not just a substring check: `aiConfig` only ever has
    // the exact five real, non-secret fields — no `apiKey`/`credential`/
    // `secret`-shaped key anywhere on it.
    expect(Object.keys(preset.aiConfig).sort()).toEqual(
      ["baseUrl", "model", "provider", "temperature", "timeoutMs"].sort(),
    );

    // And the persisted localStorage copy is equally clean.
    const raw = localStorage.getItem(STORAGE_KEY);
    expect(raw).not.toContain(FAKE_SECRET);
  });

  it("never includes a credential/apiKey/secret-shaped field key anywhere in the preset, even recursively", async () => {
    const { presetsStore } = await freshImport();
    const preset = presetsStore.save("Clean Preset");
    // Checks *keys* only (not values — a preset's own free-text fields, like
    // its name, are user content and may legitimately contain any word) by
    // walking every object's own key set recursively, matching this test's
    // real goal: no field capable of holding a secret exists on the shape,
    // regardless of what value ends up in an unrelated free-text field.
    const suspiciousKeyPattern = /apikey|api_key|credential|secret|password|token/i;
    function assertNoSuspiciousKeys(value: unknown): void {
      if (Array.isArray(value)) {
        value.forEach(assertNoSuspiciousKeys);
        return;
      }
      if (value && typeof value === "object") {
        for (const [key, v] of Object.entries(value)) {
          expect(key).not.toMatch(suspiciousKeyPattern);
          assertNoSuspiciousKeys(v);
        }
      }
    }
    assertNoSuspiciousKeys(preset);
  });
});

describe("presetsStore — save() snapshots the real, current settings", () => {
  it("captures the exact current AI/translation/voice/export/capcut field values", async () => {
    aiSettingsStore.provider = "anthropic";
    aiSettingsStore.model = "claude-3-7-sonnet";
    translationReviewStore.targetLanguage = "es";
    translationReviewStore.genre = "drama_romance";
    voiceSettingsStore.provider = "custom_api";
    capcutStore.manualDraftRoot = "D:\\CapCut\\Drafts";
    renderStore.selectedPresetId = "p1080";

    const { presetsStore } = await freshImport();
    const preset = presetsStore.save("Spanish Romance");

    expect(preset.name).toBe("Spanish Romance");
    expect(preset.aiConfig).toEqual({
      provider: "anthropic",
      baseUrl: "https://api.openai.com/v1",
      model: "claude-3-7-sonnet",
      temperature: 0.7,
      timeoutMs: 30_000,
    });
    expect(preset.translationConfig.targetLanguage).toBe("es");
    expect(preset.translationConfig.genre).toBe("drama_romance");
    expect(preset.voiceConfig.provider).toBe("custom_api");
    expect(preset.voiceConfig.roleMappings).toEqual([{ role: "Narrator", voiceId: "v1", voiceName: "Voice One" }]);
    expect(preset.exportPresetId).toBe("p1080");
    expect(preset.capcutConfig.manualDraftRoot).toBe("D:\\CapCut\\Drafts");
  });

  it("persists the saved preset to localStorage", async () => {
    const { presetsStore } = await freshImport();
    presetsStore.save("Persisted Preset");
    const raw = localStorage.getItem(STORAGE_KEY);
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw!) as Array<{ name: string }>;
    expect(parsed).toHaveLength(1);
    expect(parsed[0]?.name).toBe("Persisted Preset");
  });

  it("snapshots roleMappings as a plain array copy, not a live alias", async () => {
    const { presetsStore } = await freshImport();
    const preset = presetsStore.save("Snapshot Test");
    voiceSettingsStore.roleMappings.push({ role: "New Role", voiceId: "v2", voiceName: "Voice Two" });
    expect(preset.voiceConfig.roleMappings).toHaveLength(1);
  });
});

describe("presetsStore — apply() writes back through each store's own setters", () => {
  it("restores every bundled field via the real setter methods", async () => {
    const { presetsStore } = await freshImport();
    const preset = presetsStore.save("Restore Target");
    preset.aiConfig.provider = "gemini";
    preset.aiConfig.model = "gemini-1.5-flash";
    preset.translationConfig.targetLanguage = "fr";
    preset.voiceConfig.roleMappings = [{ role: "Speaker A", voiceId: "va", voiceName: "A" }];
    preset.exportPresetId = "p1080";
    preset.capcutConfig.manualDraftRoot = "E:\\Drafts";
    presetsStore.presets = [preset];

    await presetsStore.apply(preset.id);

    expect(aiSettingsStore.setProvider).toHaveBeenCalledWith("gemini");
    expect(aiSettingsStore.setModel).toHaveBeenCalledWith("gemini-1.5-flash");
    expect(translationReviewStore.targetLanguage).toBe("fr");
    expect(voiceSettingsStore.setRoleMappings).toHaveBeenCalledWith([
      { role: "Speaker A", voiceId: "va", voiceName: "A" },
    ]);
    expect(renderStore.selectPreset).toHaveBeenCalledWith("p1080");
    expect(capcutStore.setManualDraftRoot).toHaveBeenCalledWith("E:\\Drafts");
  });

  it("does nothing for an unknown preset id", async () => {
    const { presetsStore } = await freshImport();
    await presetsStore.apply("does-not-exist");
    expect(aiSettingsStore.setProvider).not.toHaveBeenCalled();
  });
});

describe("presetsStore — remove()", () => {
  it("deletes the preset and persists the shrunk list", async () => {
    const { presetsStore } = await freshImport();
    const preset = presetsStore.save("To Delete");
    expect(presetsStore.presets).toHaveLength(1);
    presetsStore.remove(preset.id);
    expect(presetsStore.presets).toHaveLength(0);
    const raw = localStorage.getItem(STORAGE_KEY);
    expect(JSON.parse(raw!)).toEqual([]);
  });
});

describe("presetsStore — validatePreset()", () => {
  it("accepts a well-formed preset object", async () => {
    const { presetsStore, validatePreset } = await freshImport();
    const preset = presetsStore.save("Valid");
    expect(validatePreset(JSON.parse(JSON.stringify(preset)))).not.toBeNull();
  });

  it("rejects malformed/foreign JSON rather than throwing", async () => {
    const { validatePreset } = await freshImport();
    expect(validatePreset(null)).toBeNull();
    expect(validatePreset("just a string")).toBeNull();
    expect(validatePreset({ id: "x" })).toBeNull();
    expect(validatePreset({ id: "x", name: "y", createdAt: "z", aiConfig: { provider: "not_a_real_provider" } })).toBeNull();
  });

  it("falls back to safe defaults for missing optional sub-fields", async () => {
    const { validatePreset } = await freshImport();
    const minimal = {
      id: "abc",
      name: "Minimal",
      createdAt: "2026-01-01T00:00:00.000Z",
      aiConfig: { provider: "open_ai", baseUrl: "https://x", model: "m" },
      translationConfig: { targetLanguage: "vi", genre: "" },
      voiceConfig: { provider: "custom_api", baseUrl: "" },
    };
    const validated = validatePreset(minimal);
    expect(validated).not.toBeNull();
    expect(validated!.voiceConfig.roleMappings).toEqual([]);
    expect(validated!.exportPresetId).toBeNull();
    expect(validated!.capcutConfig.manualDraftRoot).toBeNull();
  });
});

describe("presetsStore — export/import (mocked commands + dialog)", () => {
  it("exportToFile does nothing when the user cancels the save dialog", async () => {
    const { presetsStore } = await freshImport();
    const preset = presetsStore.save("Export Me");
    saveDialog.mockResolvedValue(null);
    await presetsStore.exportToFile(preset.id);
    expect(exportPresetToFile).not.toHaveBeenCalled();
  });

  it("exportToFile sends the exact preset JSON to the chosen path", async () => {
    const { presetsStore } = await freshImport();
    const preset = presetsStore.save("Export Me");
    saveDialog.mockResolvedValue("D:\\out\\export-me.json");
    exportPresetToFile.mockResolvedValue({ status: "ok", data: null });
    await presetsStore.exportToFile(preset.id);
    expect(exportPresetToFile).toHaveBeenCalledWith(JSON.stringify(preset, null, 2), "D:\\out\\export-me.json");
  });

  it("importFromFile adds a validated preset with a freshly-minted id", async () => {
    const { presetsStore } = await freshImport();
    const fakeExported = presetsStore.save("Original");
    presetsStore.remove(fakeExported.id); // don't collide with the "imported" copy below
    openDialog.mockResolvedValue("D:\\in\\preset.json");
    importPresetFromFile.mockResolvedValue({ status: "ok", data: JSON.stringify(fakeExported) });
    await presetsStore.importFromFile();
    expect(presetsStore.presets).toHaveLength(1);
    expect(presetsStore.presets[0]?.id).not.toBe(fakeExported.id);
    expect(presetsStore.presets[0]?.name).toBe("Original");
  });

  it("importFromFile surfaces an error for non-JSON file content", async () => {
    const { presetsStore } = await freshImport();
    openDialog.mockResolvedValue("D:\\in\\bad.json");
    importPresetFromFile.mockResolvedValue({ status: "ok", data: "not valid json{{{" });
    await presetsStore.importFromFile();
    expect(presetsStore.importError).not.toBeNull();
    expect(presetsStore.presets).toHaveLength(0);
  });

  it("importFromFile surfaces an error for valid JSON that isn't a recognizable preset", async () => {
    const { presetsStore } = await freshImport();
    openDialog.mockResolvedValue("D:\\in\\notpreset.json");
    importPresetFromFile.mockResolvedValue({ status: "ok", data: JSON.stringify({ foo: "bar" }) });
    await presetsStore.importFromFile();
    expect(presetsStore.importError).not.toBeNull();
    expect(presetsStore.presets).toHaveLength(0);
  });
});
