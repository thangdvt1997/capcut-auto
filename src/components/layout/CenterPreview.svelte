<!-- Video Preview center panel (master-prompt §9). Real in Phase 3: renders
     `VideoPlayer`, an actual HTML5 <video>-backed preview against whatever
     media is selected in the Media Library. -->
<script lang="ts">
  import VideoPlayer from "../preview/VideoPlayer.svelte";
</script>

<div class="preview">
  <VideoPlayer />
</div>

<style>
  .preview {
    height: 100%;
    background: var(--background);
  }
</style>
