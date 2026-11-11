<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { t } from '../lib/i18n/index.svelte';
  import { parseNumber, roundMs } from '../lib/input';

  // The marks of a sound cue, each one an editable time, plus a button that
  // adds one at `next` (null: no position, the button is disabled).
  let {
    marks,
    next,
    nextTip,
    disabled = false,
    onchange,
  }: {
    marks: number[];
    next: number | null;
    nextTip: string;
    disabled?: boolean;
    onchange: (marks: number[]) => void;
  } = $props();

  /** Sorted, each time once. */
  function tidy(list: number[]): number[] {
    return [...new Set(list.map(roundMs))].sort((left, right) => left - right);
  }

  function edit(index: number, e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const value = parseNumber(input.value);
    if (value === null || value < 0) {
      // Not a time: the field shows the mark again.
      input.value = String(roundMs(marks[index]));
      return;
    }
    onchange(tidy(marks.map((mark, i) => (i === index ? value : mark))));
  }
</script>

<div class="flex flex-wrap items-center gap-1.5">
  {#each marks as mark, i (i)}
    <span class="flex items-center gap-0.5">
      <Input
        class="w-24 font-mono text-xs"
        type="number"
        step="0.01"
        min="0"
        aria-label={t('marks.mark_secs', { number: i + 1 })}
        value={roundMs(mark)}
        {disabled}
        onchange={(e) => edit(i, e)}
      />
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t('marks.remove', { number: i + 1 })}
        {disabled}
        onclick={() => onchange(marks.filter((_, index) => index !== i))}><XMark /></Button
      >
    </span>
  {/each}
  <Tooltip
    tip={nextTip}
    class={buttonVariants()}
    disabled={disabled || next == null}
    onclick={() => next != null && onchange(tidy([...marks, next]))}>{t('marks.add')}</Tooltip
  >
</div>
