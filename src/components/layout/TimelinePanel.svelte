<!--
  Timeline dock (master-prompt §48 diagram: V1/V2/A1/CC tracks). Static
  mockup of the track-header/lane structure the real Phase 4 timeline engine
  will render into — no clip data, no interaction, clearly labeled.
-->
<script lang="ts">
  const mockTracks: { label: string; kind: "video" | "audio" | "caption" }[] = [
    { label: "V2", kind: "video" },
    { label: "V1", kind: "video" },
    { label: "A1", kind: "audio" },
    { label: "CC", kind: "caption" },
  ];
</script>

<div class="timeline-panel">
  <div class="tl-header">
    <span>Timeline</span>
    <span class="muted-2 mono">— Phase 4 (clip editing, undo/redo, zoom)</span>
  </div>
  <div class="tl-tracks">
    {#each mockTracks as track (track.label)}
      <div class="tl-track">
        <div class="tl-track-label">{track.label}</div>
        <div class="tl-track-lane {track.kind}"></div>
      </div>
    {/each}
  </div>
</div>
