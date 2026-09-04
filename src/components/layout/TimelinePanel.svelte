<!--
  Timeline dock (master-prompt §48 diagram: V1/V2/A1/CC tracks). Used to be
  a static mockup with no clip data or interaction (see git history); this
  is the real Phase 4 timeline engine's mounting point now —
  `components/timeline/Timeline.svelte` owns the ruler, track rows, clips,
  playhead, selection, zoom/scroll, and keyboard shortcuts, all backed by
  `stores/timeline.svelte.ts` and the real `commands::timeline::*` IPC
  surface.
-->
<script lang="ts">
  import Timeline from "../timeline/Timeline.svelte";
</script>

<Timeline />
