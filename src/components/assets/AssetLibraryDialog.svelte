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

  **Phase D7c Design System retrofit (`STUDIO_PLAN.md`):** the hand-rolled
  backdrop/dialog shell is now `Modal.svelte` (Phase D1), the two flex-row
  asset lists are now `DataTable.svelte` (Name/Kind/Path/Actions columns,
  real client-side sort added on Name/Kind — a free, honest addition the
  retrofit enables, not a behavior change), the kind pill is `Badge.svelte`,
  the kind/name inputs are `Select.svelte`/`Input.svelte`, every button is
  `Button.svelte`, and the three empty/loading messages are
  `EmptyState.svelte`. Every real behavior is unchanged: the exact same
  `assetsStore` state/methods drive every conditional, disabled state, and
  click handler as before (`armRemove`/`cancelRemove`/`confirmRemove`,
  `pickFile`/`submitAdd`, the `canSubmitAdd` gate).
-->
<script lang="ts">
  import { assetsStore, ASSET_KINDS, CONSUMED_ASSET_KINDS } from "../../stores/assets.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { Asset, AssetKind } from "../../types/bindings";
  import Modal from "../ui/Modal.svelte";
  import DataTable from "../ui/DataTable.svelte";
  import Badge from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import Select from "../ui/Select.svelte";
  import type { SelectOption } from "../ui/Select.svelte";
  import Input from "../ui/Input.svelte";
  import EmptyState from "../ui/EmptyState.svelte";

  const consumedKinds = ASSET_KINDS.filter((k) => CONSUMED_ASSET_KINDS.has(k));
  const structuralKinds = ASSET_KINDS.filter((k) => !CONSUMED_ASSET_KINDS.has(k));

  function assetsFor(kinds: AssetKind[]): Asset[] {
    return assetsStore.assets.filter((a) => kinds.includes(a.kind));
  }

  function assetKey(asset: Asset): string {
    return asset.id;
  }

  let kindOptions = $derived<SelectOption[]>(ASSET_KINDS.map((k) => ({ value: k, label: t(`assetLibrary.kind.${k}`) })));
</script>

{#snippet nameCell(asset: Asset)}
  <span class="al-name">{asset.name}</span>
{/snippet}
{#snippet kindCell(asset: Asset)}
  <Badge>{t(`assetLibrary.kind.${asset.kind}`)}</Badge>
{/snippet}
{#snippet pathCell(asset: Asset)}
  <span class="al-path mono muted-2" title={asset.file_path}>{asset.file_path}</span>
{/snippet}
{#snippet actionsCell(asset: Asset)}
  <div class="al-actions">
    {#if assetsStore.pendingRemoveId === asset.id}
      <Button
        variant="danger"
        size="sm"
        disabled={assetsStore.removingId === asset.id}
        onclick={() => void assetsStore.confirmRemove(asset.id)}
      >
        {assetsStore.removingId === asset.id ? t("assetLibrary.removing") : t("assetLibrary.removeConfirmButton")}
      </Button>
      <Button variant="ghost" size="sm" onclick={() => assetsStore.cancelRemove()}>
        {t("assetLibrary.removeCancelButton")}
      </Button>
    {:else}
      <Button variant="ghost" size="sm" onclick={() => assetsStore.armRemove(asset.id)}>
        {t("assetLibrary.removeButton")}
      </Button>
    {/if}
  </div>
{/snippet}

<Modal open={assetsStore.open} title={t("assetLibrary.title")} onClose={() => assetsStore.close()} width={680}>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => assetsStore.close()}>{t("assetLibrary.closeButton")}</Button>
  {/snippet}

  <p class="al-explainer muted-2">{t("assetLibrary.explainer")}</p>

  {#if assetsStore.loadError}
    <div class="al-error">{t("assetLibrary.loadFailed", { error: assetsStore.loadError })}</div>
  {/if}

  <div class="al-add-form">
    <span class="al-section-title">{t("assetLibrary.addSectionTitle")}</span>
    <div class="al-add-row">
      <div class="al-field-narrow">
        <Select
          value={assetsStore.addKind}
          options={kindOptions}
          onchange={(v) => (assetsStore.addKind = v as AssetKind)}
        />
      </div>
      <div class="al-field-grow">
        <Input bind:value={assetsStore.addName} placeholder={t("assetLibrary.namePlaceholder")} />
      </div>
      <Button variant="ghost" size="sm" onclick={() => void assetsStore.pickFile()}>
        {t("assetLibrary.chooseFileButton")}
      </Button>
      <Button size="sm" disabled={!assetsStore.canSubmitAdd} onclick={() => void assetsStore.submitAdd()}>
        {assetsStore.adding ? t("assetLibrary.adding") : t("assetLibrary.addButton")}
      </Button>
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
    <EmptyState title={t("assetLibrary.loading")} />
  {/if}

  <span class="al-section-title">{t("assetLibrary.consumedSectionTitle")}</span>
  <p class="al-note muted-2">{t("assetLibrary.consumedSectionNote")}</p>
  {#if assetsFor(consumedKinds).length === 0}
    <EmptyState title={t("assetLibrary.noneRegistered")} />
  {:else}
    <DataTable
      columns={[
        { key: "name", label: t("assetLibrary.colName"), sortable: true, accessor: (a) => a.name, cell: nameCell },
        {
          key: "kind",
          label: t("assetLibrary.colKind"),
          sortable: true,
          accessor: (a) => t(`assetLibrary.kind.${a.kind}`),
          cell: kindCell,
        },
        { key: "path", label: t("assetLibrary.colPath"), cell: pathCell },
        { key: "actions", label: t("assetLibrary.colActions"), cell: actionsCell },
      ]}
      rows={assetsFor(consumedKinds)}
      rowKey={assetKey}
    />
  {/if}

  <span class="al-section-title">{t("assetLibrary.structuralSectionTitle")}</span>
  <p class="al-note muted-2">{t("assetLibrary.structuralSectionNote")}</p>
  {#if assetsFor(structuralKinds).length === 0}
    <EmptyState title={t("assetLibrary.noneRegistered")} />
  {:else}
    <DataTable
      columns={[
        { key: "name", label: t("assetLibrary.colName"), sortable: true, accessor: (a) => a.name, cell: nameCell },
        {
          key: "kind",
          label: t("assetLibrary.colKind"),
          sortable: true,
          accessor: (a) => t(`assetLibrary.kind.${a.kind}`),
          cell: kindCell,
        },
        { key: "path", label: t("assetLibrary.colPath"), cell: pathCell },
        { key: "actions", label: t("assetLibrary.colActions"), cell: actionsCell },
      ]}
      rows={assetsFor(structuralKinds)}
      rowKey={assetKey}
    />
  {/if}
</Modal>

<style>
  /* Design System retrofit (Phase D7c, `STUDIO_PLAN.md`): the dialog shell
     (`.al-backdrop`/`.al-dialog`/`.al-header`/`.al-title`/`.al-footer`/
     `.al-footer-spacer`), the two flex-row asset lists (`.al-list`/`.al-row`/
     `.al-row-info`/`.al-badge`), the hand-rolled `<select>`/`<input>`
     (`.al-input*`), and every button's sizing/danger override (`.btn-sm`/
     `.btn-danger`) are ALL gone — `Modal`/`DataTable`/`Badge`/`Select`/
     `Input`/`Button`/`EmptyState` (Design System) now own that chrome. Only
     the handful of layout rules with no Design System equivalent remain:
     the explainer/section-title/note text sizing, the add-form's bordered
     box, the narrow/growing flex sizing for the inline kind-select/name-
     input row (matching the original `.al-input-narrow`/`.al-input`'s
     140px-fixed/flex:1 split), the picked-path/name/path text truncation,
     and the inline error banners. */
  .al-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .al-section-title {
    margin-top: var(--space-1);
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
    gap: var(--space-2);
    padding: var(--space-3);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .al-add-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .al-field-grow {
    flex: 1;
    min-width: 120px;
  }
  .al-field-narrow {
    flex: 0 0 auto;
    width: 140px;
    min-width: 0;
  }
  .al-picked-path {
    font-size: 10.5px;
    word-break: break-all;
  }
  .al-name {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
  .al-path {
    display: block;
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .al-actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .al-error {
    padding: var(--space-2) var(--space-3);
    font-size: 10.5px;
    color: var(--neg);
    background: var(--neg-bg);
    border: 1px solid var(--neg-border);
    border-radius: var(--radius-sm);
  }
</style>
