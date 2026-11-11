<script lang="ts">
  import { onDestroy } from 'svelte';

  import { Button } from '$lib/components/ui/button';
  import { Label } from '$lib/components/ui/label';

  import { t } from '../../lib/i18n/index.svelte';
  import { field, hint } from '../../lib/ui';

  // A cue colour: a colour picker and "none". `mixed`: the selected cues differ.
  let {
    value,
    mixed = false,
    onchange,
  }: { value: string | null; mixed?: boolean; onchange: (color: string | null) => void } = $props();

  // While the picker is dragged, WebKitGTK fires `change` for every step. Only the colour it settles on is
  // sent (one edit, one undo step); a pending colour goes out when the field closes.
  const settle = 250;
  let pending: string | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function pick(color: string) {
    pending = color;
    clearTimeout(timer);
    timer = setTimeout(flush, settle);
  }

  function flush() {
    clearTimeout(timer);
    timer = undefined;
    if (pending !== null && pending !== value) onchange(pending);
    pending = null;
  }

  function clear() {
    clearTimeout(timer);
    pending = null;
    onchange(null);
  }

  onDestroy(flush);
</script>

<Label class={field}>
  {t('field.color')}
  <span class="flex items-center gap-2">
    <input
      class="h-8 w-15 rounded-lg border border-line-strong bg-canvas p-0.5"
      type="color"
      value={value ?? '#14171b'}
      oninput={(e) => pick(e.currentTarget.value)}
      onchange={(e) => pick(e.currentTarget.value)}
    />
    <Button disabled={!mixed && !value} onclick={clear}>{t('keycapture.none')}</Button>
    {#if mixed}<span class={hint}>{t('field.mixed')}</span>{/if}
  </span>
</Label>
