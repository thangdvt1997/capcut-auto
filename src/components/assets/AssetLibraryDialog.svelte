<!--
  Asset Library management dialog (upgrade spec §17): a small, user-curated
  catalog of reusable external files — intro/outro clips, logo/watermark
  images, background music, etc. — each referenced by a stable id instead of
  a hardcoded path. Pure UI over `stores/assets.svelte.ts`: list (split into
  "used by a Template field today" vs. "registered, not consumed by any
  feature yet" — see `assets::mod`'s own module doc comment for exactly
  which kinds are which), an Add flow (kind + name + a real native file
  picker, mirroring `MediaLibrary.svelte`'s own `@tauri-apps/plugin-dialog`
  usage — never a raw path text field), and Remove (two-step confirm, the
  same arm/cancel/confirm shape `TemplatesPanel.svelte`'s own
  custom-template delete already uses).

  Placement: a standalone dialog reachable from `TopBar.svelte`'s "Assets…"
  button — same "no master prompt §46 Settings surface exists yet"
  rationale every other standalone TopBar dialog in this codebase already
  documents (see `ModelManagerDialog.svelte`'s own doc comment). Asset
  Library is an app-level catalog, not scoped to whatever project happens to
  be open, so it lives here rather than inside a project-scoped panel.
  Mounted once in `App.svelte`, alongside those other dialogs.

  `assetsStore` (this dialog's own backing store) is also the exact list
  `TemplatesPanel.svelte`'s intro/outro/watermark/background-music pickers
  read from — registering an asset here makes it immediately selectable
  there, no separate fetch needed.
-->
<script lang="ts">
  import { assetsStore, ASSET_KINDS, CONSUMED_ASSET_KINDS } from "../../stores/assets.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { Asset, AssetKind } from "../../types/bindings";

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      assetsStore.close();
    }
  }

  const consumedKinds = ASSET_KINDS.filter((k) => CONSUMED_ASSET_KINDS.has(k));
  const structuralKinds = ASSET_KINDS.filter((k) => !CONSUMED_ASSET_KINDS.has(k));

  function assetsFor(kinds: AssetKind[]): Asset[] {
    return assetsStore.assets.filter((a) => kinds.includes(a.kind));
  }
</script>

{#snippet assetRow(asset: Asset)}
  <div class="al-row">
    <div class="al-row-info">
      <span class="al-name">{asset.name}</span>
      <span class="al-badge">{t(`assetLibrary.kind.${asset.kind}`)}</span>
      <span class="al-path mono muted-2" title={asset.file_path}>{asset.file_path}</span>
    </div>
    {#if assetsStore.pendingRemoveId === asset.id}
      <button
        class="btn btn-danger btn-sm"
        disabled={assetsStore.removingId === asset.id}
        onclick={() => void assetsStore.confirmRemove(asset.id)}
      >
        {assetsStore.removingId === asset.id ? t("assetLibrary.removing") : t("assetLibrary.removeConfirmButton")}
      </button>
      <button class="btn btn-ghost btn-sm" onclick={() => assetsStore.cancelRemove()}>
        {t("assetLibrary.removeCancelButton")}
      </button>
    {:else}
      <button class="btn btn-ghost btn-sm" onclick={() => assetsStore.armRemove(asset.id)}>
        {t("assetLibrary.removeButton")}
      </button>
    {/if}
  </div>
{/snippet}

{#if assetsStore.open}
  <div class="al-backdrop" role="presentation" onclick={() => assetsStore.close()}>
    <div
      class="al-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("assetLibrary.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="al-header">
        <span class="al-title">{t("assetLibrary.title")}</span>
        <button class="btn btn-ghost" onclick={() => assetsStore.close()} title={t("assetLibrary.close")}>×</button>
      </div>

      <div class="al-body">
        <p class="al-explainer muted-2">{t("assetLibrary.explainer")}</p>

        {#if assetsStore.loadError}
          <div class="al-error">{t("assetLibrary.loadFailed", { error: assetsStore.loadError })}</div>
        {/if}

        <div class="al-add-form">
          <span class="al-section-title">{t("assetLibrary.addSectionTitle")}</span>
          <div class="al-add-row">
            <select class="al-input al-input-narrow" bind:value={assetsStore.addKind}>
              {#each ASSET_KINDS as k (k)}
                <option value={k}>{t(`assetLibrary.kind.${k}`)}</option>
              {/each}
            </select>
            <input
              class="al-input"
              type="text"
              placeholder={t("assetLibrary.namePlaceholder")}
              bind:value={assetsStore.addName}
            />
            <button class="btn btn-ghost btn-sm" onclick={() => void assetsStore.pickFile()}>
              {t("assetLibrary.chooseFileButton")}
            </button>
            <button class="btn btn-sm" disabled={!assetsStore.canSubmitAdd} onclick={() => void assetsStore.submitAdd()}>
              {assetsStore.adding ? t("assetLibrary.adding") : t("assetLibrary.addButton")}
            </button>
          </div>
          {#if assetsStore.addFilePath}
            <span class="al-picked-path mono muted-2" title={assetsStore.addFilePath}>{assetsStore.addFilePath}</span>
          {/if}
          {#if !CONSUMED_ASSET_KINDS.has(assetsStore.addKind)}
            <span class="al-note muted-2">{t("assetLibrary.structuralKindNote")}</span>
          {/if}
          {#if assetsStore.addError}
            <div class="al-error">{assetsStore.addError}</div>
          {/if}
        </div>

        {#if assetsStore.removeError}
          <div class="al-error">{assetsStore.removeError}</div>
        {/if}

        {#if assetsStore.loading && assetsStore.assets.length === 0}
          <p class="al-empty muted-2">{t("assetLibrary.loading")}</p>
        {/if}

        <span class="al-section-title">{t("assetLibrary.consumedSectionTitle")}</span>
        <p class="al-note muted-2">{t("assetLibrary.consumedSectionNote")}</p>
        {#if assetsFor(consumedKinds).length === 0}
          <p class="al-empty muted-2">{t("assetLibrary.noneRegistered")}</p>
        {:else}
          <div class="al-list">
            {#each assetsFor(consumedKinds) as asset (asset.id)}
              {@render assetRow(asset)}
            {/each}
          </div>
        {/if}

        <span class="al-section-title">{t("assetLibrary.structuralSectionTitle")}</span>
        <p class="al-note muted-2">{t("assetLibrary.structuralSectionNote")}</p>
        {#if assetsFor(structuralKinds).length === 0}
          <p class="al-empty muted-2">{t("assetLibrary.noneRegistered")}</p>
        {:else}
          <div class="al-list">
            {#each assetsFor(structuralKinds) as asset (asset.id)}
              {@render assetRow(asset)}
            {/each}
          </div>
        {/if}
      </div>

      <div class="al-footer">
        <span class="al-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => assetsStore.close()}>{t("assetLibrary.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .al-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .al-dialog {
    width: min(680px, 94vw);
    max-height: 88vh;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px hsl(0 0% 0% / 0.5);
    overflow: hidden;
  }
  .al-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .al-title {
    font-size: 13px;
    font-weight: 600;
  }
  .al-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .al-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .al-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .al-section-title {
    margin-top: 6px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .al-note {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .al-add-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .al-add-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .al-input {
    height: 26px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
    flex: 1;
    min-width: 120px;
  }
  .al-input-narrow {
    flex: 0 0 auto;
    width: 140px;
    min-width: 0;
  }
  .al-picked-path {
    font-size: 10.5px;
    word-break: break-all;
  }
  .al-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .al-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .al-row-info {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex-wrap: wrap;
  }
  .al-name {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
  .al-badge {
    font-size: 9.5px;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--muted);
    white-space: nowrap;
  }
  .al-path {
    font-size: 10px;
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .btn-sm {
    height: 24px;
    padding: 0 8px;
    font-size: 10.5px;
  }
  .btn-danger {
    background: hsl(0 84% 65% / 0.12);
    border: 1px solid hsl(0 84% 65% / 0.4);
    color: var(--neg);
  }
  .al-error {
    padding: 6px 10px;
    font-size: 10.5px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .al-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .al-footer-spacer {
    flex: 1;
  }
</style>
