<script lang="ts">
  import { loadKeyLabels } from '../lib/keys.svelte';
  import { notePointer, onKeydown, onKeyup, releaseAllKeys, releaseKey } from '../lib/shortcuts';

  // The keyboard of a window of the app (main window and panel windows alike), so GO, the global shortcuts and
  // the cue hotkeys work in each of them. Keys are taken in the capture phase, before the focused element sees
  // them (see `lib/shortcuts.ts`).
  loadKeyLabels();
</script>

<svelte:window
  onpointerdowncapture={notePointer}
  onkeydowncapture={onKeydown}
  onkeyupcapture={(e) => releaseKey(e.code)}
  onkeyup={onKeyup}
  onfocus={() => loadKeyLabels()}
  onblur={releaseAllKeys}
/>
<svelte:document
  onvisibilitychange={() => {
    if (document.hidden) releaseAllKeys();
  }}
/>
