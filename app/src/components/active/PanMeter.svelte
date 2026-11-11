<script lang="ts">
  import { panLabel } from '../../lib/control';

  // Pan from the middle out, as it plays now (pan cues move it).
  let { pan, label }: { pan: number; label: string } = $props();
</script>

<span
  class="relative h-1 min-w-0 flex-1 rounded-sm bg-line"
  role="meter"
  aria-label={label}
  aria-valuemin={-1}
  aria-valuemax={1}
  aria-valuenow={pan}
  aria-valuetext={panLabel(pan)}
>
  <span
    class="absolute inset-y-0 bg-fade"
    style:left="{50 + Math.min(0, pan) * 50}%"
    style:width="{Math.abs(pan) * 50}%"
  ></span>
  <span class="absolute -inset-y-0.5 left-1/2 w-px -translate-x-1/2 bg-dim"></span>
  <span class="absolute -inset-y-0.5 w-0.5 -translate-x-1/2 rounded-full bg-fade-soft" style:left="{50 + pan * 50}%">
  </span>
</span>
