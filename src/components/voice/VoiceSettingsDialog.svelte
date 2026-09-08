<!--
  Voice Settings dialog (`promt.md` §9 "VOICE / TTS CONFIGURATION"):
  Provider / Server API URL / API Key + a real "Test Connection"/"Refresh
  Voices" pair, plus Voice Mapping (Speaker role -> voice). The frontend half
  of Phase S7's `voice::` provider abstraction — read
  `src-tauri/src/commands/voice.rs`'s own module doc comment before touching
  this file: only `custom_api` has a real backend adapter, and there is
  deliberately no `synthesize_speech` command anywhere yet (this dialog
  configures a provider/lists its voices/saves a role mapping; it does not
  generate any audio).

  Placement: mirrors `AiSettingsDialog.svelte`'s own precedent exactly — a
  standalone dialog, mounted once in `App.svelte`, reachable from a real
  "Open Voice Settings…" button on the new Voice card in
  `AutomationSettingsTab.svelte` (Tab 2).

  Pure UI over `stores/voiceSettings.svelte.ts` — read that store's own
  module doc comment for the full reasoning behind the write-only credential
  posture and the "Voice Mapping is saved but not yet consumed by any real
  pipeline" honesty note (stated again below, in-dialog, not just in code).
-->
<script lang="ts">
  import { voiceSettingsStore, VOICE_PROVIDER_KINDS } from "../../stores/voiceSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { VoiceInfo, VoiceProviderKind } from "../../types/bindings";
  import Modal from "../ui/Modal.svelte";
  import Panel from "../ui/Panel.svelte";
  import Select from "../ui/Select.svelte";
  import Badge from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import type { SelectOption } from "../ui/Select.svelte";

  function providerLabel(kind: VoiceProviderKind): string {
    switch (kind) {
      case "custom_api":
        return t("voiceSettings.providerCustomApi");
      case "nts_gen_ai":
        return t("voiceSettings.providerNtsGenAi");
      case "gpt_so_vits":
        return t("voiceSettings.providerGptSoVits");
    }
  }

  function genderLabel(voice: VoiceInfo): string {
    switch (voice.gender) {
      case "male":
        return t("voiceSettings.genderMale");
      case "female":
        return t("voiceSettings.genderFemale");
      case "other":
        return t("voiceSettings.genderOther");
      case null:
        return t("voiceSettings.genderUnknown");
    }
  }

  const providerOptions: SelectOption[] = VOICE_PROVIDER_KINDS.map((kind) => ({
    value: kind,
    label: providerLabel(kind),
  }));

  let selectedVoiceId = $state<string>("");
  const voiceOptions = $derived<SelectOption[]>(
    voiceSettingsStore.voices.map((v) => ({ value: v.voice_id, label: `${v.name} (${genderLabel(v)})` })),
  );

  function addMapping(): void {
    const voice = voiceSettingsStore.voices.find((v) => v.voice_id === selectedVoiceId);
    if (!voice) return;
    voiceSettingsStore.addMapping(voice);
  }
</script>

<Modal
  open={voiceSettingsStore.open}
  title={t("voiceSettings.title")}
  width={640}
  onClose={() => voiceSettingsStore.close()}
>
  <p class="vs-explainer muted-2">{t("voiceSettings.explainer")}</p>

  <Panel title={t("voiceSettings.providerSectionTitle")}>
    <Select
      id="vs-provider"
      label={t("voiceSettings.providerLabel")}
      value={voiceSettingsStore.provider}
      options={providerOptions}
      onchange={(v) => voiceSettingsStore.setProvider(v as VoiceProviderKind)}
    />
    {#if !voiceSettingsStore.needsConnectionDetails}
      <p class="vs-hint muted-2">{t("voiceSettings.stubProviderHint")}</p>
    {:else}
      <div class="vs-row">
        <label class="vs-label" for="vs-base-url">{t("voiceSettings.baseUrlLabel")}</label>
        <input
          id="vs-base-url"
          class="ui-input"
          type="text"
          placeholder={t("voiceSettings.baseUrlPlaceholder")}
          value={voiceSettingsStore.baseUrl}
          onblur={(e) => voiceSettingsStore.setBaseUrl((e.target as HTMLInputElement).value)}
        />
      </div>
    {/if}
  </Panel>

  {#if voiceSettingsStore.needsConnectionDetails}
    <Panel title={t("voiceSettings.credentialsSectionTitle")}>
      <div class="vs-row">
        <Badge variant={voiceSettingsStore.hasKeyConfigured ? "pos" : "neutral"}>
          {voiceSettingsStore.hasKeyConfigured ? t("voiceSettings.keyConfigured") : t("voiceSettings.keyNotConfigured")}
        </Badge>
      </div>
      <div class="vs-row">
        <input
          class="ui-input"
          type="password"
          autocomplete="off"
          placeholder={t("voiceSettings.keyInputPlaceholder")}
          bind:value={voiceSettingsStore.apiKeyDraft}
        />
        <Button
          size="sm"
          disabled={voiceSettingsStore.apiKeyDraft.trim() === "" || voiceSettingsStore.savingKey}
          onclick={() => void voiceSettingsStore.saveApiKey()}
        >
          {voiceSettingsStore.savingKey ? t("voiceSettings.savingKey") : t("voiceSettings.saveKeyButton")}
        </Button>
        {#if voiceSettingsStore.hasKeyConfigured}
          <Button variant="ghost" size="sm" disabled={voiceSettingsStore.savingKey} onclick={() => void voiceSettingsStore.deleteApiKey()}>
            {t("voiceSettings.deleteKeyButton")}
          </Button>
        {/if}
      </div>
      <p class="vs-hint muted-2">{t("voiceSettings.keyNeverRedisplayedNote")}</p>
      {#if voiceSettingsStore.keyActionError}
        <ErrorState message={voiceSettingsStore.keyActionError} />
      {/if}
    </Panel>
  {/if}

  <Panel title={t("voiceSettings.testSectionTitle")}>
    <div class="vs-row">
      <Button disabled={voiceSettingsStore.testing} onclick={() => void voiceSettingsStore.testConnection()}>
        {voiceSettingsStore.testing ? t("voiceSettings.testing") : t("voiceSettings.testButton")}
      </Button>
      <Button variant="ghost" disabled={voiceSettingsStore.loadingVoices} onclick={() => void voiceSettingsStore.refreshVoices()}>
        {voiceSettingsStore.loadingVoices ? t("voiceSettings.loadingVoices") : t("voiceSettings.refreshVoicesButton")}
      </Button>
    </div>
    {#if voiceSettingsStore.testResult}
      {#if voiceSettingsStore.testResult.success}
        <p class="vs-test-ok">{voiceSettingsStore.testResult.message}</p>
      {:else}
        <ErrorState message={voiceSettingsStore.testResult.message} />
      {/if}
    {/if}
    {#if voiceSettingsStore.voicesError}
      <ErrorState message={voiceSettingsStore.voicesError} />
    {:else if voiceSettingsStore.voices.length > 0}
      <ul class="vs-voice-list">
        {#each voiceSettingsStore.voices as voice (voice.voice_id)}
          <li class="vs-voice-item">
            <span class="vs-voice-name">{voice.name}</span>
            <span class="vs-voice-meta muted-2">{genderLabel(voice)}{voice.language ? ` · ${voice.language}` : ""}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </Panel>

  <Panel title={t("voiceSettings.mappingSectionTitle")}>
    <p class="vs-hint muted-2">{t("voiceSettings.mappingNotConsumedNote")}</p>
    {#if voiceSettingsStore.voices.length === 0}
      <EmptyState title={t("voiceSettings.mappingNeedsVoicesTitle")} />
    {:else}
      <div class="vs-row">
        <input
          class="ui-input vs-role-input"
          type="text"
          placeholder={t("voiceSettings.roleNamePlaceholder")}
          bind:value={voiceSettingsStore.newRoleName}
        />
        <Select
          id="vs-mapping-voice"
          value={selectedVoiceId}
          options={voiceOptions}
          placeholder={t("voiceSettings.selectVoicePlaceholder")}
          onchange={(v) => (selectedVoiceId = v)}
        />
        <Button
          size="sm"
          disabled={voiceSettingsStore.newRoleName.trim() === "" || selectedVoiceId === ""}
          onclick={addMapping}
        >
          {t("voiceSettings.addMappingButton")}
        </Button>
      </div>
    {/if}
    {#if voiceSettingsStore.roleMappings.length === 0}
      <EmptyState title={t("voiceSettings.noMappingsYet")} />
    {:else}
      <ul class="vs-mapping-list">
        {#each voiceSettingsStore.roleMappings as mapping (mapping.role)}
          <li class="vs-mapping-item">
            <span class="vs-mapping-role">{mapping.role}</span>
            <span class="vs-mapping-arrow muted-2">→</span>
            <span class="vs-mapping-voice muted-2">{mapping.voiceName}</span>
            <Button variant="ghost" size="sm" onclick={() => voiceSettingsStore.removeMapping(mapping.role)}>
              {t("voiceSettings.removeMappingButton")}
            </Button>
          </li>
        {/each}
      </ul>
    {/if}
  </Panel>

  {#snippet footer()}
    <Button variant="ghost" onclick={() => voiceSettingsStore.close()}>{t("voiceSettings.close")}</Button>
  {/snippet}
</Modal>

<style>
  /* Same "no Design System paragraph-typography primitive" gap
     `AiSettingsDialog.svelte`'s own <style> block already documents — kept
     as a tiny local class, matching that dialog's exact sizing. */
  .vs-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .vs-hint {
    margin: 0;
    font-size: 10.5px;
  }
  .vs-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .vs-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .vs-role-input {
    flex: 1;
    min-width: 0;
  }
  /* Same "no success-state component exists" gap as AiSettingsDialog's own
     .as-test-ok. */
  .vs-test-ok {
    margin: 0;
    padding: 8px 10px;
    font-size: 11.5px;
    color: var(--pos);
    background: var(--pos-bg);
    border: 1px solid var(--pos-border);
    border-radius: var(--radius-sm);
  }
  .vs-voice-list,
  .vs-mapping-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    max-height: 160px;
    overflow-y: auto;
  }
  .vs-voice-item,
  .vs-mapping-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: 11.5px;
    padding: 4px 0;
  }
  .vs-voice-name,
  .vs-mapping-role {
    font-weight: 600;
  }
  .vs-mapping-arrow {
    flex-shrink: 0;
  }
  .vs-mapping-voice {
    flex: 1;
    min-width: 0;
  }
</style>
