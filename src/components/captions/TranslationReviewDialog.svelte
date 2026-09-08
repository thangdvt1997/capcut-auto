<!--
  Translation Review dialog (`STUDIO_PLAN.md` Phase D12): the real frontend
  half of Phase S6's `ai::translate_captions` — "propose, never auto-apply,
  review each line, accept individually or all at once, apply for real."
  Mirrors this codebase's own established AI review-dialog shape
  (`stores/smartEdit.svelte.ts`'s Analyze -> per-row review -> Apply, and
  `VoiceSettingsDialog.svelte`'s Design-System-based dialog structure) rather
  than inventing a new pattern.

  Pure UI over `stores/translationReview.svelte.ts` — read that store's own
  module doc comment for the accept/reject model and exactly what happens on
  Apply (a real `apply_caption_translations` call, Phase D12's new backend
  command — the only place a translation proposal ever becomes a real
  project mutation, matching `ai::translate`'s own "propose, never
  auto-apply" module doc comment).

  Placement: mounted once in `WorkspaceSimple.svelte` (Tab 1's Simple mode),
  opened from `ScriptEditor.svelte`'s "Translate…" button — the exact
  "translating/translateError/hasTranslated" wiring point
  `WorkspaceSimple.svelte` already tracked for `PipelineStepper.svelte`
  before this pass, now backed by this real store instead of an inline
  propose-only call with no review UI behind it.
-->
<script lang="ts">
  import { translationReviewStore, TRANSLATION_GENRES, PROFANITY_HANDLINGS } from "../../stores/translationReview.svelte";
  import { captionsStore } from "../../stores/captions.svelte";
  import { formatTimecode } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";
  import type { Caption, ProfanityHandling, TranslationGenre } from "../../types/bindings";
  import Modal from "../ui/Modal.svelte";
  import Panel from "../ui/Panel.svelte";
  import Card from "../ui/Card.svelte";
  import Select from "../ui/Select.svelte";
  import Input from "../ui/Input.svelte";
  import Checkbox from "../ui/Checkbox.svelte";
  import Badge from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import SuccessBanner from "../ui/SuccessBanner.svelte";
  import DataTable from "../ui/DataTable.svelte";
  import type { SelectOption } from "../ui/Select.svelte";

  function genreLabel(genre: TranslationGenre): string {
    switch (genre) {
      case "drama_romance":
        return t("translationReview.genreDramaRomance");
      case "fantasy_cultivation":
        return t("translationReview.genreFantasyCultivation");
      case "crime_detective":
        return t("translationReview.genreCrimeDetective");
      case "police_bodycam":
        return t("translationReview.genrePoliceBodycam");
      case "prison_crime":
        return t("translationReview.genrePrisonCrime");
      case "survival":
        return t("translationReview.genreSurvival");
      case "documentary":
        return t("translationReview.genreDocumentary");
      case "custom":
        return t("translationReview.genreCustom");
    }
  }

  function profanityLabel(handling: ProfanityHandling): string {
    switch (handling) {
      case "preserve":
        return t("translationReview.profanityPreserve");
      case "soften":
        return t("translationReview.profanitySoften");
      case "remove":
        return t("translationReview.profanityRemove");
    }
  }

  const genreOptions: SelectOption[] = TRANSLATION_GENRES.map((g) => ({ value: g, label: genreLabel(g) }));
  const profanityOptions: SelectOption[] = PROFANITY_HANDLINGS.map((p) => ({ value: p, label: profanityLabel(p) }));

  // Rows: every real caption the last successful `translate()` produced a
  // proposal for (full-coverage — `ai::translate::parse_and_validate`), in
  // time order. A caption added/removed after that call simply has no
  // proposal and is correctly absent here.
  let reviewRows = $derived(
    [...captionsStore.captions]
      .filter((c) => c.id in translationReviewStore.proposals)
      .sort((a, b) => a.start_us - b.start_us),
  );
</script>

{#snippet indexCell(row: Caption)}
  <span class="mono muted-2">{reviewRows.indexOf(row) + 1}</span>
{/snippet}
{#snippet timeCell(row: Caption)}
  <span class="mono">{formatTimecode(row.start_us)} – {formatTimecode(row.end_us)}</span>
{/snippet}
{#snippet originalCell(row: Caption)}
  <span class="trd-cell-text" title={row.text}>{row.text}</span>
{/snippet}
{#snippet translationCell(row: Caption)}
  <span class="trd-cell-text" title={translationReviewStore.proposals[row.id]}>{translationReviewStore.proposals[row.id]}</span>
{/snippet}
{#snippet acceptCell(row: Caption)}
  <Checkbox
    checked={translationReviewStore.isAccepted(row.id)}
    onchange={(checked) => translationReviewStore.setAccepted(row.id, checked)}
  />
{/snippet}

<Modal
  open={translationReviewStore.open}
  title={t("translationReview.title")}
  width={880}
  onClose={() => translationReviewStore.close()}
>
  <p class="trd-explainer muted-2">{t("translationReview.explainer")}</p>

  <Panel title={t("translationReview.settingsSectionTitle")}>
    <div class="trd-row">
      <Input
        label={t("translationReview.sourceLanguageLabel")}
        placeholder={t("translationReview.sourceLanguagePlaceholder")}
        bind:value={translationReviewStore.sourceLanguage}
      />
      <Input
        label={t("translationReview.targetLanguageLabel")}
        placeholder={t("translationReview.targetLanguagePlaceholder")}
        bind:value={translationReviewStore.targetLanguage}
      />
    </div>
    <div class="trd-row">
      <Select
        label={t("translationReview.genreLabel")}
        placeholder={t("translationReview.genrePlaceholder")}
        value={translationReviewStore.genre}
        options={genreOptions}
        onchange={(v) => (translationReviewStore.genre = v as TranslationGenre)}
      />
      <Select
        label={t("translationReview.profanityHandlingLabel")}
        placeholder={t("translationReview.profanityPlaceholder")}
        value={translationReviewStore.profanityHandling}
        options={profanityOptions}
        onchange={(v) => (translationReviewStore.profanityHandling = v as ProfanityHandling)}
      />
    </div>
    <Input
      label={t("translationReview.translationStyleLabel")}
      placeholder={t("translationReview.translationStylePlaceholder")}
      bind:value={translationReviewStore.translationStyle}
    />
    <div class="ui-field">
      <label class="ui-label" for="trd-character-context">{t("translationReview.characterContextLabel")}</label>
      <textarea
        id="trd-character-context"
        class="ui-input trd-textarea"
        placeholder={t("translationReview.characterContextPlaceholder")}
        bind:value={translationReviewStore.characterContext}
      ></textarea>
    </div>
    <Checkbox bind:checked={translationReviewStore.preserveNames}>{t("translationReview.preserveNamesLabel")}</Checkbox>
    <Checkbox bind:checked={translationReviewStore.preserveTerminology}>
      {t("translationReview.preserveTerminologyLabel")}
    </Checkbox>
    <Checkbox bind:checked={translationReviewStore.sentenceLengthOptimization}>
      {t("translationReview.sentenceLengthLabel")}
    </Checkbox>
    <Checkbox bind:checked={translationReviewStore.voiceFriendlyRewrite}>
      {t("translationReview.voiceFriendlyLabel")}
    </Checkbox>

    {#if captionsStore.captions.length === 0}
      <p class="trd-hint muted-2">{t("translationReview.noCaptionsHint")}</p>
    {:else if !translationReviewStore.aiConfigured}
      <p class="trd-hint muted-2">{t("translationReview.aiNotConfiguredHint")}</p>
    {/if}
    {#if translationReviewStore.translateError}
      <ErrorState message={translationReviewStore.translateError} />
    {/if}

    <div class="trd-row">
      <Button disabled={!translationReviewStore.canTranslate} onclick={() => void translationReviewStore.translate()}>
        {#if translationReviewStore.translating}
          {t("translationReview.generatingButton")}
        {:else if translationReviewStore.hasProposals}
          {t("translationReview.regenerateButton")}
        {:else}
          {t("translationReview.generateButton")}
        {/if}
      </Button>
    </div>
  </Panel>

  <Panel title={t("translationReview.reviewSectionTitle")}>
    {#if !translationReviewStore.hasProposals}
      <EmptyState
        title={t("translationReview.reviewEmptyTitle")}
        description={t("translationReview.reviewEmptyDesc")}
      />
    {:else}
      <div class="trd-row trd-review-toolbar">
        <Badge variant={translationReviewStore.acceptedCount > 0 ? "accent" : "neutral"}>
          {t("translationReview.acceptedCountLabel", {
            accepted: translationReviewStore.acceptedCount,
            total: translationReviewStore.proposedIds.length,
          })}
        </Badge>
        <span class="trd-spacer"></span>
        <Button size="sm" variant="ghost" onclick={() => translationReviewStore.acceptAll()}>
          {t("translationReview.acceptAllButton")}
        </Button>
        <Button size="sm" variant="ghost" onclick={() => translationReviewStore.rejectAll()}>
          {t("translationReview.rejectAllButton")}
        </Button>
      </div>

      <Card>
        <DataTable
          columns={[
            { key: "idx", label: t("translationReview.colIndex"), cell: indexCell },
            { key: "time", label: t("translationReview.colTime"), cell: timeCell },
            { key: "original", label: t("translationReview.colOriginal"), cell: originalCell },
            { key: "translation", label: t("translationReview.colTranslation"), cell: translationCell },
            { key: "accept", label: t("translationReview.colAccept"), align: "center", cell: acceptCell },
          ]}
          rows={reviewRows}
          rowKey={(c) => c.id}
        />
      </Card>

      {#if translationReviewStore.applyError}
        <ErrorState message={translationReviewStore.applyError} />
      {/if}
      {#if translationReviewStore.appliedThisSession}
        <SuccessBanner
          message={t("translationReview.appliedSuccessMessage", { count: translationReviewStore.lastAppliedCount })}
        />
      {/if}
    {/if}
  </Panel>

  {#snippet footer()}
    <Button variant="ghost" disabled={!translationReviewStore.hasProposals} onclick={() => translationReviewStore.discard()}>
      {t("translationReview.discardButton")}
    </Button>
    <span class="trd-spacer"></span>
    <Button
      variant="primary"
      disabled={!translationReviewStore.canApply}
      onclick={() => void translationReviewStore.apply()}
    >
      {translationReviewStore.applying ? t("translationReview.applyingButton") : t("translationReview.applyButton")}
    </Button>
    <Button variant="ghost" onclick={() => translationReviewStore.close()}>{t("translationReview.closeButton")}</Button>
  {/snippet}
</Modal>

<style>
  .trd-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .trd-hint {
    margin: 0;
    font-size: 10.5px;
  }
  .trd-row {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
    min-width: 0;
  }
  .trd-row > :global(.ui-field) {
    flex: 1;
    min-width: 0;
  }
  .trd-review-toolbar {
    align-items: center;
  }
  .trd-spacer {
    flex: 1;
  }
  .trd-textarea {
    height: auto;
    min-height: 56px;
    padding: 6px var(--space-2);
    resize: vertical;
    font: inherit;
  }
  .trd-cell-text {
    display: block;
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
