<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Kbd } from '$lib/components/ui/kbd';

  import type { Chord } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { chordOf, keyCapture, keyParts } from '../lib/keys.svelte';

  // A key field: click, then press the key (with modifiers); the press is
  // taken by the field, not run as a shortcut. Del removes the key (it is a
  // fixed key, so nothing is lost). Clicking again or leaving the field cancels.
  let {
    value,
    onchange,
    placeholder,
    disabled = false,
    clearable = true,
    label,
    class: className = '',
  }: {
    value: Chord | null;
    onchange: (chord: Chord | null) => void;
    placeholder?: string;
    disabled?: boolean;
    /** An × next to the field removes the key (besides Del). */
    clearable?: boolean;
    /** Accessible name, e.g. "GO, first key". */
    label: string;
    class?: string;
  } = $props();

  let recording = $state(false);
  // The key just recorded: its keyup must not click the focused button.
  let swallow: string | null = null;

  function setRecording(on: boolean) {
    recording = on;
    keyCapture.active = on;
  }

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.repeat) return;
    const remove = e.code === 'Delete' && !e.ctrlKey && !e.altKey && !e.shiftKey;
    const chord = remove ? null : chordOf(e);
    if (!remove && !chord) return;
    swallow = e.code;
    setRecording(false);
    onchange(chord);
  }

  function onkeyup(e: KeyboardEvent) {
    if (swallow !== e.code) return;
    swallow = null;
    e.preventDefault();
    e.stopPropagation();
  }

  $effect(() => () => {
    if (recording) keyCapture.active = false;
  });
</script>

<svelte:window onkeydowncapture={onkeydown} onkeyupcapture={onkeyup} />

<span class="flex min-w-0 items-center gap-1 {className}">
  <button
    type="button"
    class={[
      'flex h-7 min-w-0 grow cursor-pointer items-center gap-1 overflow-hidden rounded-md border px-2 text-left text-xs whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40',
      recording
        ? 'border-primary bg-primary/10 text-primary'
        : 'border-dashed border-transparent enabled:hover:border-input enabled:hover:bg-background',
      !recording && (value ? 'text-fg' : 'text-muted-foreground'),
    ]}
    aria-label={label}
    {disabled}
    onclick={() => setRecording(!recording)}
    onblur={() => recording && setRecording(false)}
  >
    {#if recording}
      <span class="truncate">{t('keycapture.press')}</span>
    {:else if value}
      {#each keyParts(value) as part, index (index)}<Kbd>{part}</Kbd>{/each}
    {:else}
      <span class="truncate">{placeholder ?? t('keycapture.none')}</span>
    {/if}
  </button>
  {#if clearable && value && !recording}
    <Button
      variant="ghost"
      size="icon-xs"
      aria-label={t('keycapture.remove', { label })}
      {disabled}
      onclick={() => onchange(null)}><XMark /></Button
    >
  {/if}
</span>
