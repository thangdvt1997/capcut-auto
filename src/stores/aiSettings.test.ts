// Store test for `stores/aiSettings.svelte.ts` — chosen as the required
// "one real store test with mocked Tauri commands" (IMPLEMENTATION_PLAN.md
// Phase 13) over `timeline.svelte.ts`: this store's most test-worthy logic
// (localStorage persistence round-trip, per-provider defaults, the
// `credentialRef`/`hasKeyConfigured` derivations, and its two IPC-calling
// methods) is entirely independent of any *other* runes-under-Vitest
// question, and its `localStorage`-seeded-at-module-load pattern needs the
// same `vi.resetModules()` + dynamic-`import()` per test regardless of which
// store is picked. See `stores/timeline.svelte.test.ts` (same directory) for
// a second, real-runes store test that *does* exercise `$state`/`$derived`
// directly on the more complex `TimelineStore` — this file's job is the
// mocked-IPC + localStorage half of Phase 13's requirement.
//
// `commands` (from `../types/bindings`) is mocked so no real Tauri backend
// is needed: `setAiApiKey`/`deleteAiApiKey`/`testAiConnection` are the only
// three commands this store ever calls.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AiConnectionTestResult } from "../types/bindings";

const setAiApiKey = vi.fn();
const deleteAiApiKey = vi.fn();
const testAiConnection = vi.fn();

vi.mock("../types/bindings", () => ({
  commands: {
    setAiApiKey: (...args: unknown[]) => setAiApiKey(...args),
    deleteAiApiKey: (...args: unknown[]) => deleteAiApiKey(...args),
    testAiConnection: (...args: unknown[]) => testAiConnection(...args),
  },
}));

const SETTINGS_KEY = "ave:ai:settings";
const KEY_CONFIGURED_KEY = "ave:ai:keyConfigured";

/** Fresh import of the module under test — necessary because
 * `aiSettingsStore`'s initial field values are computed once, at module
 * load, from whatever is in `localStorage` at that moment (see the module's
 * own `initialSettings`/`initialKeyConfigured` top-level consts). Testing
 * different starting localStorage states therefore requires a real fresh
 * module instance per test, not just a fresh class instance. */
async function freshImport() {
  vi.resetModules();
  return import("./aiSettings.svelte");
}

beforeEach(() => {
  localStorage.clear();
  setAiApiKey.mockReset();
  deleteAiApiKey.mockReset();
  testAiConnection.mockReset();
});

afterEach(() => {
  localStorage.clear();
});

describe("aiSettingsStore — initial load", () => {
  it("defaults to open_ai with its own default base URL/model when localStorage is empty", async () => {
    const { aiSettingsStore, defaultBaseUrlFor, defaultModelFor } = await freshImport();
    expect(aiSettingsStore.provider).toBe("open_ai");
    expect(aiSettingsStore.baseUrl).toBe(defaultBaseUrlFor("open_ai"));
    expect(aiSettingsStore.model).toBe(defaultModelFor("open_ai"));
    expect(aiSettingsStore.temperature).toBe(0.7);
    expect(aiSettingsStore.timeoutMs).toBe(30_000);
    expect(aiSettingsStore.hasKeyConfigured).toBe(false);
  });

  it("round-trips a previously persisted settings object", async () => {
    localStorage.setItem(
      SETTINGS_KEY,
      JSON.stringify({
        provider: "anthropic",
        base_url: "https://custom.example/v1",
        model: "claude-3-7-sonnet",
        temperature: 0.2,
        timeout_ms: 15_000,
      }),
    );
    const { aiSettingsStore } = await freshImport();
    expect(aiSettingsStore.provider).toBe("anthropic");
    expect(aiSettingsStore.baseUrl).toBe("https://custom.example/v1");
    expect(aiSettingsStore.model).toBe("claude-3-7-sonnet");
    expect(aiSettingsStore.temperature).toBe(0.2);
    expect(aiSettingsStore.timeoutMs).toBe(15_000);
  });

  it("round-trips the keyConfigured map from localStorage", async () => {
    localStorage.setItem(KEY_CONFIGURED_KEY, JSON.stringify({ "ai-provider:open_ai": true }));
    const { aiSettingsStore } = await freshImport();
    expect(aiSettingsStore.hasKeyConfigured).toBe(true);
  });

  it("falls back to defaults when localStorage holds malformed JSON", async () => {
    localStorage.setItem(SETTINGS_KEY, "{not valid json");
    const { aiSettingsStore, defaultBaseUrlFor } = await freshImport();
    expect(aiSettingsStore.provider).toBe("open_ai");
    expect(aiSettingsStore.baseUrl).toBe(defaultBaseUrlFor("open_ai"));
  });

  it("falls back to defaults when the persisted provider isn't a known AiProviderKind", async () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ provider: "not_a_real_provider" }));
    const { aiSettingsStore } = await freshImport();
    expect(aiSettingsStore.provider).toBe("open_ai");
  });
});

describe("aiSettingsStore — setProvider default-refresh behavior", () => {
  it("refreshes base URL/model to the new provider's defaults when the old values were exactly the old provider's defaults", async () => {
    const { aiSettingsStore, defaultBaseUrlFor, defaultModelFor } = await freshImport();
    aiSettingsStore.setProvider("ollama");
    expect(aiSettingsStore.provider).toBe("ollama");
    expect(aiSettingsStore.baseUrl).toBe(defaultBaseUrlFor("ollama"));
    expect(aiSettingsStore.model).toBe(defaultModelFor("ollama"));
  });

  it("preserves a user-typed base URL/model across a provider switch", async () => {
    const { aiSettingsStore } = await freshImport();
    aiSettingsStore.setBaseUrl("https://mine.example");
    aiSettingsStore.setModel("my-custom-model");
    aiSettingsStore.setProvider("anthropic");
    expect(aiSettingsStore.baseUrl).toBe("https://mine.example");
    expect(aiSettingsStore.model).toBe("my-custom-model");
  });

  it("persists the new provider/base URL/model to localStorage", async () => {
    const { aiSettingsStore } = await freshImport();
    aiSettingsStore.setProvider("gemini");
    const raw = localStorage.getItem(SETTINGS_KEY);
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw!) as { provider: string };
    expect(parsed.provider).toBe("gemini");
  });

  it("is a no-op when setting the provider to its own current value", async () => {
    const { aiSettingsStore } = await freshImport();
    aiSettingsStore.setBaseUrl("https://mine.example");
    aiSettingsStore.setProvider("open_ai"); // already open_ai
    expect(aiSettingsStore.baseUrl).toBe("https://mine.example");
  });
});

describe("aiSettingsStore — credentialRef / hasKeyConfigured derivation", () => {
  it("keys credentialRef off the provider kind", async () => {
    const { aiSettingsStore } = await freshImport();
    expect(aiSettingsStore.credentialRef).toBe("ai-provider:open_ai");
    aiSettingsStore.setProvider("ollama");
    expect(aiSettingsStore.credentialRef).toBe("ai-provider:ollama");
  });

  it("tracks hasKeyConfigured independently per provider", async () => {
    const { aiSettingsStore } = await freshImport();
    setAiApiKey.mockResolvedValue({ status: "ok", data: null });
    aiSettingsStore.apiKeyDraft = "sk-test";
    await aiSettingsStore.saveApiKey();
    expect(aiSettingsStore.hasKeyConfigured).toBe(true);
    aiSettingsStore.setProvider("ollama");
    expect(aiSettingsStore.hasKeyConfigured).toBe(false);
  });
});

describe("aiSettingsStore — saveApiKey / deleteApiKey (mocked commands)", () => {
  it("marks the key configured and clears the draft on a successful save", async () => {
    const { aiSettingsStore } = await freshImport();
    setAiApiKey.mockResolvedValue({ status: "ok", data: null });
    aiSettingsStore.apiKeyDraft = "sk-real-key";
    await aiSettingsStore.saveApiKey();
    expect(setAiApiKey).toHaveBeenCalledWith("ai-provider:open_ai", "sk-real-key");
    expect(aiSettingsStore.hasKeyConfigured).toBe(true);
    expect(aiSettingsStore.apiKeyDraft).toBe("");
    expect(aiSettingsStore.keyActionError).toBeNull();
    // Persisted so a later load remembers it without a live backend check.
    const raw = localStorage.getItem(KEY_CONFIGURED_KEY);
    expect(JSON.parse(raw!)).toEqual({ "ai-provider:open_ai": true });
  });

  it("surfaces the backend error and leaves hasKeyConfigured false on a failed save", async () => {
    const { aiSettingsStore } = await freshImport();
    setAiApiKey.mockResolvedValue({ status: "error", error: { message: "keyring unavailable" } });
    aiSettingsStore.apiKeyDraft = "sk-real-key";
    await aiSettingsStore.saveApiKey();
    expect(aiSettingsStore.hasKeyConfigured).toBe(false);
    expect(aiSettingsStore.keyActionError).toBe("keyring unavailable");
  });

  it("does nothing when the draft is blank", async () => {
    const { aiSettingsStore } = await freshImport();
    aiSettingsStore.apiKeyDraft = "   ";
    await aiSettingsStore.saveApiKey();
    expect(setAiApiKey).not.toHaveBeenCalled();
  });

  it("clears hasKeyConfigured on a successful delete", async () => {
    localStorage.setItem(KEY_CONFIGURED_KEY, JSON.stringify({ "ai-provider:open_ai": true }));
    const { aiSettingsStore } = await freshImport();
    expect(aiSettingsStore.hasKeyConfigured).toBe(true);
    deleteAiApiKey.mockResolvedValue({ status: "ok", data: null });
    await aiSettingsStore.deleteApiKey();
    expect(aiSettingsStore.hasKeyConfigured).toBe(false);
    const raw = localStorage.getItem(KEY_CONFIGURED_KEY);
    expect(JSON.parse(raw!)).toEqual({ "ai-provider:open_ai": false });
  });
});

describe("aiSettingsStore — testConnection (mocked command)", () => {
  it("stores a successful test result", async () => {
    const { aiSettingsStore } = await freshImport();
    const okResult: AiConnectionTestResult = { success: true, message: "Connected" };
    testAiConnection.mockResolvedValue(okResult);
    await aiSettingsStore.testConnection();
    expect(aiSettingsStore.testResult).toEqual(okResult);
    expect(aiSettingsStore.testing).toBe(false);
  });

  it("synthesizes a failed result when the IPC call itself rejects", async () => {
    const { aiSettingsStore } = await freshImport();
    testAiConnection.mockRejectedValue(new Error("IPC channel closed"));
    await aiSettingsStore.testConnection();
    expect(aiSettingsStore.testResult?.success).toBe(false);
    expect(aiSettingsStore.testResult?.message).toContain("IPC channel closed");
    expect(aiSettingsStore.testing).toBe(false);
  });
});
