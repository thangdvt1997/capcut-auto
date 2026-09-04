<!--
  CapCut / Jianying Settings dialog (Phase 9, master prompt §30): shows every
  confirmed installation `detect_capcut_installations` found (product,
  user profile, draft directory, which marker confirmed it), plus
  `detect_capcut_registry_hints`'s best-effort registry results clearly
  labeled as supplementary/lower-confidence, and a manual-override text
  input + native directory picker for when nothing was auto-detected or the
  user keeps drafts somewhere else. Read-only with respect to the
  filesystem — this dialog only detects and lets the user choose a path; it
  never writes anything (the confirmation-before-overwrite requirement lives
  in `CapCutExportDialog.svelte`, the one place this app actually writes a
  draft).

  Placement decision (documented here + `IMPLEMENTATION_PLAN.md`): mirrors
  `ModelManagerDialog.svelte`'s own placement precedent exactly — no master
  prompt §46 Settings surface exists yet to host this as a section, so this
  is a standalone dialog, mounted once in `App.svelte`, reachable from a
  "CapCut…" toolbar button in `TopBar.svelte` placed right next to the
  existing "Models…" button.

  Pure UI over `stores/capcut.svelte.ts`.
-->
<script lang="ts">
  import { capcutStore } from "../../stores/capcut.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { CapCutRegistryHint, DetectedCapCutInstallation } from "../../types/bindings";

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      capcutStore.closeSettings();
    }
  }

  function productLabel(product: DetectedCapCutInstallation["product"] | CapCutRegistryHint["product"]): string {
    return product === "jianying" ? t("capcutSettings.productJianying") : t("capcutSettings.productCapCut");
  }

  let overrideDraft = $state(capcutStore.manualDraftRoot ?? "");

  $effect(() => {
    // Keep the text input in sync when the override changes from outside
    // this component (e.g. "Use this path" on a detected row, or "Clear
    // override") without fighting the user's own typing — only resync when
    // the store's value and the local draft have actually diverged.
    if (capcutStore.manualDraftRoot !== overrideDraft && document.activeElement?.id !== "cc-override-input") {
      overrideDraft = capcutStore.manualDraftRoot ?? "";
    }
  });

  function commitOverrideDraft(): void {
    capcutStore.setManualDraftRoot(overrideDraft);
  }

  function clearOverride(): void {
    overrideDraft = "";
    capcutStore.setManualDraftRoot(null);
  }
</script>

{#if capcutStore.settingsOpen}
  <div class="cs-backdrop" role="presentation" onclick={() => capcutStore.closeSettings()}>
    <div
      class="cs-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("capcutSettings.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="cs-header">
        <span class="cs-title">{t("capcutSettings.title")}</span>
        <button class="btn btn-ghost" onclick={() => capcutStore.closeSettings()} title={t("capcutSettings.close")}>×</button>
      </div>

      <div class="cs-body">
        <p class="cs-explainer muted-2">{t("capcutSettings.explainer")}</p>

        <section class="cs-section">
          <div class="cs-section-header">
            <h3 class="cs-section-title">{t("capcutSettings.detectedSectionTitle")}</h3>
            <button class="btn btn-ghost btn-sm" disabled={capcutStore.detectLoading} onclick={() => void capcutStore.rescan()}>
              {capcutStore.detectLoading ? t("capcutSettings.detecting") : t("capcutSettings.rescanButton")}
            </button>
          </div>

          {#if capcutStore.detectError}
            <div class="cs-error">{t("capcutSettings.detectFailed", { error: capcutStore.detectError })}</div>
          {/if}

          {#if capcutStore.installations.length === 0 && !capcutStore.detectLoading && !capcutStore.detectError}
            <p class="cs-empty muted-2">{t("capcutSettings.noneDetected")}</p>
          {/if}

          <div class="cs-list">
            {#each capcutStore.installations as inst, i (inst.draft_root)}
              <div class="cs-card">
                <div class="cs-card-main">
                  <div class="cs-card-info">
                    <span class="cs-name">{productLabel(inst.product)}</span>
                    <span class="cs-meta muted-2">{t("capcutSettings.userProfileLabel")}: {inst.user_profile}</span>
                    <span class="cs-path muted-2" title={inst.draft_root}>{inst.draft_root}</span>
                    <span class="cs-marker muted-2">
                      {inst.has_root_meta_info ? t("capcutSettings.markerRootMetaInfo") : t("capcutSettings.markerRecycleBin")}
                    </span>
                  </div>
                  <div class="cs-card-actions">
                    {#if capcutStore.manualDraftRoot === inst.draft_root || (!capcutStore.manualDraftRoot && i === 0)}
                      <span class="cs-status cs-status-inuse">{t("capcutSettings.inUseBadge")}</span>
                    {:else}
                      <button
                        class="btn btn-ghost btn-sm"
                        onclick={() => {
                          overrideDraft = inst.draft_root;
                          capcutStore.setManualDraftRoot(inst.draft_root);
                        }}
                      >
                        {t("capcutSettings.useThisPathButton")}
                      </button>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        </section>

        <section class="cs-section">
          <h3 class="cs-section-title">{t("capcutSettings.registrySectionTitle")}</h3>
          <p class="cs-explainer muted-2">{t("capcutSettings.registryExplainer")}</p>
          {#if capcutStore.registryHints.length === 0 && !capcutStore.detectLoading}
            <p class="cs-empty muted-2">{t("capcutSettings.noneInRegistry")}</p>
          {/if}
          <div class="cs-list">
            {#each capcutStore.registryHints as hint (hint.display_name)}
              <div class="cs-card">
                <div class="cs-card-info">
                  <span class="cs-name">{hint.display_name}</span>
                  <span class="cs-meta muted-2">{productLabel(hint.product)}</span>
                  <span class="cs-meta muted-2">
                    {t("capcutSettings.versionLabel")}: {hint.display_version ?? t("capcutSettings.unknownValue")}
                  </span>
                  <span class="cs-meta muted-2">
                    {t("capcutSettings.installLocationLabel")}: {hint.install_location ?? t("capcutSettings.unknownValue")}
                  </span>
                </div>
              </div>
            {/each}
          </div>
        </section>

        <section class="cs-section">
          <h3 class="cs-section-title">{t("capcutSettings.overrideSectionTitle")}</h3>
          <p class="cs-explainer muted-2">{t("capcutSettings.overrideExplainer")}</p>
          <div class="cs-row">
            <input
              id="cc-override-input"
              class="cs-input"
              type="text"
              placeholder={t("capcutSettings.overridePlaceholder")}
              bind:value={overrideDraft}
              onblur={commitOverrideDraft}
              onkeydown={(e) => {
                if (e.key === "Enter") commitOverrideDraft();
              }}
            />
            <button class="btn btn-sm" onclick={() => void capcutStore.browseManualDraftRoot()}>
              {t("capcutSettings.browseButton")}
            </button>
            {#if capcutStore.manualDraftRoot}
              <button class="btn btn-ghost btn-sm" onclick={clearOverride}>{t("capcutSettings.clearButton")}</button>
            {/if}
          </div>
        </section>

        <section class="cs-section">
          <h3 class="cs-section-title">{t("capcutSettings.effectivePathLabel")}</h3>
          {#if capcutStore.effectiveDraftRoot}
            <p class="cs-effective-path">{capcutStore.effectiveDraftRoot}</p>
          {:else}
            <p class="cs-empty muted-2">{t("capcutSettings.effectivePathNone")}</p>
          {/if}
        </section>
      </div>

      <div class="cs-footer">
        <span class="cs-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => capcutStore.closeSettings()}>{t("capcutSettings.close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .cs-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .cs-dialog {
    width: min(640px, 94vw);
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
  .cs-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .cs-title {
    font-size: 13px;
    font-weight: 600;
  }
  .cs-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .cs-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .cs-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .cs-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .cs-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .cs-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .cs-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cs-card {
    padding: 10px 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .cs-card-main {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    min-width: 0;
  }
  .cs-card-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .cs-card-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .cs-name {
    font-size: 12.5px;
    font-weight: 600;
  }
  .cs-meta {
    font-size: 10.5px;
  }
  .cs-path {
    font-size: 10.5px;
    font-family: var(--font-mono, monospace);
    overflow-wrap: anywhere;
  }
  .cs-marker {
    font-size: 10.5px;
    color: var(--pos, #3fb950);
  }
  .cs-status {
    font-size: 10.5px;
    padding: 2px 8px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .cs-status-inuse {
    color: var(--accent);
    background: hsl(213 94% 68% / 0.1);
  }
  .btn-sm {
    height: 24px;
    padding: 0 10px;
    font-size: 11px;
  }
  .cs-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .cs-input {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
  }
  .cs-effective-path {
    margin: 0;
    padding: 8px 10px;
    font-size: 11.5px;
    font-family: var(--font-mono, monospace);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow-wrap: anywhere;
  }
  .cs-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .cs-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .cs-footer-spacer {
    flex: 1;
  }
</style>
