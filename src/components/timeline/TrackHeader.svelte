<!--
  Track header (master-prompt §10): name, kind, and lock/hide/mute/solo
  toggles wired directly to the store, which calls the real
  `set_track_locked/hidden/muted/solo` commands. Lock applies to every
  track kind; hide applies to every non-audio kind (video/image/overlay/
  caption/effect — audio has no visual layer to hide); mute+solo apply to
  audio tracks only, matching master-prompt §10's exact wording ("hide
  video track", "mute/solo audio track").
-->
<script lang="ts">
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { Track } from "../../types/bindings";

  let { track }: { track: Track } = $props();

  let effectivelyMuted = $derived(timeline.effectiveMute[track.id] ?? false);
  let showHide = $derived(track.kind !== "audio");
  let showMuteSolo = $derived(track.kind === "audio");

  function kindIcon(kind: Track["kind"]): string {
    switch (kind) {
      case "video":
        return "🎬";
      case "audio":
        return "🎵";
      case "caption":
        return "💬";
      case "image":
        return "🖼";
      case "overlay":
        return "🗗";
      case "effect":
        return "✨";
      default:
        return "▦";
    }
  }
</script>

<div class="tl-track-header" class:locked={track.locked}>
  <span class="tl-th-icon" aria-hidden="true">{kindIcon(track.kind)}</span>
  <span class="tl-th-name" title={track.name}>{track.name}</span>
  <span class="tl-th-controls">
    <button
      class="tl-th-btn"
      class:active={track.locked}
      title={track.locked ? t("timelinePanel.unlockTrack") : t("timelinePanel.lockTrack")}
      onclick={() => void timeline.setTrackLocked(track.id, !track.locked)}
    >
      {track.locked ? "🔒" : "🔓"}
    </button>

    {#if showHide}
      <button
        class="tl-th-btn"
        class:active={track.hidden}
        title={track.hidden ? t("timelinePanel.showTrack") : t("timelinePanel.hideTrack")}
        onclick={() => void timeline.setTrackHidden(track.id, !track.hidden)}
      >
        {track.hidden ? "🚫" : "👁"}
      </button>
    {/if}

    {#if showMuteSolo}
      <button
        class="tl-th-btn"
        class:active={track.muted || effectivelyMuted}
        title={track.muted
          ? t("timelinePanel.unmuteTrack")
          : effectivelyMuted
            ? t("timelinePanel.mutedBySolo")
            : t("timelinePanel.muteTrack")}
        onclick={() => void timeline.setTrackMuted(track.id, !track.muted)}
      >
        {track.muted || effectivelyMuted ? "🔇" : "🔈"}
      </button>
      <button
        class="tl-th-btn tl-th-solo"
        class:active={track.solo}
        title={track.solo ? t("timelinePanel.unsoloTrack") : t("timelinePanel.soloTrack")}
        onclick={() => void timeline.setTrackSolo(track.id, !track.solo)}
      >
        S
      </button>
    {/if}
  </span>
</div>

<style>
  .tl-track-header {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 100%;
    padding: 0 6px;
    background: var(--surface-2);
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    overflow: hidden;
  }
  .tl-track-header.locked {
    background: hsl(38 92% 60% / 0.06);
  }
  .tl-th-icon {
    font-size: 12px;
    flex-shrink: 0;
  }
  .tl-th-name {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tl-th-controls {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }
  .tl-th-btn {
    width: 20px;
    height: 20px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    background: transparent;
    border-radius: var(--radius-sm);
    font-size: 10px;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
  }
  .tl-th-btn:hover {
    background: var(--elevated);
    color: var(--foreground);
  }
  .tl-th-btn.active {
    background: hsl(213 94% 68% / 0.15);
    border-color: var(--accent);
    color: var(--foreground);
  }
  .tl-th-solo.active {
    background: hsl(38 92% 60% / 0.2);
    border-color: var(--warn);
  }
</style>
