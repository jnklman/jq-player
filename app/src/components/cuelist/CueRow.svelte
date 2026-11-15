<script lang="ts">
  import type { Component } from 'svelte';

  import DotsSixVertical from '~icons/ph/dots-six-vertical';
  import LinkSimple from '~icons/ph/link-simple-bold';
  import MidiIcon from '~icons/ph/piano-keys';
  import Play from '~icons/ph/play-fill';
  import Repeat from '~icons/ph/repeat-bold';
  import Warning from '~icons/ph/warning-bold';
  import { Kbd } from '$lib/components/ui/kbd';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type Cue } from '../../lib/api';
  import { typeColors, typeLabel } from '../../lib/control';
  import { cueLabel, isPlaceholder, linkedCues, secondTriggerLabels, vampSpans } from '../../lib/cue';
  import { placeholderIcon } from '../../lib/cueIcons';
  import { chainArrow, chainWaiting, cueStatus, cueWarnings, repeatBadge, rowRunning } from '../../lib/cueStatus';
  import { fmtMidi } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { keyText } from '../../lib/keys.svelte';
  import { plainText } from '../../lib/richText';
  import { allCues, extendSelection, isRunning, select, toggleSelected, ui } from '../../lib/store.svelte';
  import { bar } from '../../lib/ui';
  import { cueGrid, cueRow, dragRow, headRow } from './grid';

  // One cue of the list: chain arrow, playhead mark, number, title with its
  // badges and first line of notes, script reference, type and status.
  let {
    cue,
    next,
    beforeScene,
    incoming,
    head,
    done,
    selected,
    editing,
  }: {
    cue: Cue;
    /** The cue below, for the chain arrow. */
    next: Cue | undefined;
    /** The next row is a scene marker: the arrow stops at this row's bottom, the marker carries the chain on. */
    beforeScene: boolean;
    /** A chain comes in through the scene marker above: `waiting` while this cue waits to start. */
    incoming: { waiting: boolean } | null;
    /** At the playhead. */
    head: boolean;
    /** Above the playhead and over. */
    done: boolean;
    selected: boolean;
    editing: boolean;
  } = $props();

  // Only these booleans follow every runtime update; the status is rebuilt when one of them changes or while the
  // cue runs, not for every resting row of the list.
  const live = $derived(isRunning(cue));
  const status = $derived(cueStatus(cue, live));
  const arrow = $derived(chainArrow(cue, next));
  const arrowWaiting = $derived(chainWaiting(cue, next));
  const passes = $derived(repeatBadge(cue));
  const warnings = $derived(cueWarnings(cue));
  const running = $derived(rowRunning(cue));

  const background = $derived(
    head ? '' : status.vamp ? 'bg-row-vamp' : running ? 'bg-row-running' : selected ? 'bg-selected' : '',
  );
  const progressColor = $derived(
    status.copying
      ? 'bg-muted-foreground/50'
      : status.paused
        ? 'bg-pause'
        : status.vamp
          ? 'bg-vamp'
          : status.warn
            ? 'bg-fade'
            : 'bg-progress',
  );

  // Edit mode: Ctrl-click adds or removes a cue, Shift-click selects the
  // range from the selected cue (which stays the one in the inspector).
  function onclick(e: MouseEvent) {
    if (editing && (e.ctrlKey || e.metaKey)) toggleSelected(cue.id);
    else if (editing && e.shiftKey) extendSelection(cue.id);
    else select(cue.id);
  }
</script>

<!-- A small icon after the title, explained by its tooltip. -->
{#snippet badge(tip: string, Icon: Component)}
  <Tooltip {tip}>
    {#snippet child({ props })}
      <span {...props} class="pointer-events-auto flex shrink-0 text-fg-2">
        <Icon class="size-3.5" />
      </span>
    {/snippet}
  </Tooltip>
{/snippet}

<div
  class={[
    cueGrid,
    cueRow,
    // Cue colour as a stripe on the left, drawn under the playhead's accent frame.
    'bg-linear-to-r from-(--cue-color) from-[8px] to-transparent to-[8px]',
    background,
    head && headRow,
    dragRow,
  ]}
  style:--cue-color={cue.color ?? 'transparent'}
  data-cue={cue.id}
  role="row"
  tabindex="-1"
  aria-selected={selected}
  {onclick}
  onmousedown={(e) => {
    // Shift-click would also select the rows' text.
    if (e.shiftKey) e.preventDefault();
  }}
  ondblclick={() => api.action({ action: 'set_playhead', cue: cue.id })}
  onkeydown={() => {}}
>
  <!-- From this row's middle to the next one's, over the row line. Before a scene marker only to the row's
   bottom (the sticky marker would cover it); from a marker above, a line from the top with the arrowhead. -->
  <span class="relative self-stretch">
    {#if incoming}
      <span
        class={[
          'pointer-events-none absolute -top-1.5 left-0 h-6 w-full',
          incoming.waiting ? 'text-primary' : 'text-dim',
        ]}
      >
        <svg class="absolute inset-0 overflow-visible" width="12" height="24" aria-hidden="true">
          <line x1="6" y1="0" x2="6" y2="21" stroke="currentColor" stroke-width="1.5" />
          <path d="M3 17 L6 22 L9 17" fill="none" stroke="currentColor" stroke-width="1.5" />
        </svg>
      </span>
    {/if}
    {#if arrow}
      <span
        class={[
          'pointer-events-none absolute top-1/2 left-0 z-0 w-full',
          beforeScene ? 'h-6' : 'h-12',
          arrowWaiting ? 'text-primary' : 'text-dim',
        ]}
      >
        <svg class="absolute inset-0 overflow-visible" width="12" height={beforeScene ? 24 : 48} aria-hidden="true">
          <circle cx="6" cy="0" r="2.5" fill="currentColor" />
          {#if beforeScene}
            <line x1="6" y1="0" x2="6" y2="24" stroke="currentColor" stroke-width="1.5" />
          {:else}
            <line x1="6" y1="0" x2="6" y2="45" stroke="currentColor" stroke-width="1.5" />
            <path d="M3 41 L6 46 L9 41" fill="none" stroke="currentColor" stroke-width="1.5" />
          {/if}
        </svg>
        <span
          class={[
            'absolute top-1/2 left-2.5 -translate-y-1/2 rounded border bg-panel px-1 text-[10px] leading-4 font-semibold whitespace-nowrap',
            arrowWaiting ? 'border-primary text-primary' : 'border-line-strong text-fg-2',
          ]}>{arrow}</span
        >
      </span>
    {/if}
  </span>
  <span class="text-[13px] font-bold text-primary">
    {#if head}
      <Play />
    {/if}
  </span>
  <span class={['font-mono font-semibold', done && 'text-dim']}>{cue.number}</span>
  <!-- Title and first line of the detail text fill the fixed row height exactly (20 + 16 px). -->
  <span class="flex min-w-0 flex-col">
    <span class="flex h-5 min-w-0 items-center gap-2">
      <span class={['truncate font-medium', done && 'text-muted-foreground']}>{cue.title || t('group.untitled')}</span>
      {#if isPlaceholder(cue)}
        {@render badge(t('cuerow.placeholder_tip'), placeholderIcon)}
      {/if}
      {#if editing && cue.link}
        {@render badge(
          t('cuerow.linked_with', { cues: linkedCues(cue, allCues()).map(cueLabel).join(', ') }),
          LinkSimple,
        )}
      {/if}
      {#if cue.second_trigger.action !== 'ignore'}
        {@render badge(t('cuerow.second_trigger', { action: secondTriggerLabels[cue.second_trigger.action] }), Repeat)}
      {/if}
      {#if passes}
        <Tooltip tip={t('cuerow.repeat_tip')}>
          {#snippet child({ props })}
            <span {...props} class="pointer-events-auto shrink-0 text-xs font-semibold text-primary">{passes}</span>
          {/snippet}
        </Tooltip>
      {/if}
      {#if warnings.length}
        <Tooltip tip={warnings.join('\n')}>
          {#snippet child({ props })}
            <span {...props} class="pointer-events-auto flex shrink-0 text-stop-soft">
              <Warning class="size-3.5" />
            </span>
          {/snippet}
        </Tooltip>
      {/if}
      {#if cue.hotkey?.code}
        {@const hotkey = cue.hotkey}
        <Tooltip tip={t('cuerow.hotkey')}>
          {#snippet child({ props })}
            <Kbd {...props} class="pointer-events-auto shrink-0 py-0 text-fg-2">{keyText(hotkey)}</Kbd>
          {/snippet}
        </Tooltip>
      {/if}
      {#if cue.midi_trigger}
        {@render badge(t('cuerow.midi_trigger', { trigger: fmtMidi(cue.midi_trigger) }), MidiIcon)}
      {/if}
    </span>
    {#if cue.notes}<span class="truncate text-xs text-muted-foreground">{plainText(cue.notes.split('\n')[0])}</span
      >{/if}
  </span>
  <span class="truncate text-[13px] text-fg-2">{cue.script_ref}</span>
  <span class="text-xs font-semibold {typeColors[cue.type]}">{typeLabel(cue)}</span>
  <span class="flex flex-col gap-1">
    <!-- At most two lines: a go-to's target title can be long. -->
    <span class="flex min-w-0 justify-between gap-2 text-xs text-fg-2">
      <span
        class={[
          'line-clamp-2 min-w-0 font-mono wrap-break-word',
          status.bad && 'text-stop-soft',
          status.warn && 'font-semibold text-fade',
        ]}
      >
        {status.text}
      </span>
      {#if status.info}
        <span
          class={[
            'shrink-0 font-semibold',
            status.paused ? 'text-pause-soft' : status.vamp ? 'text-vamp-soft' : 'text-primary',
          ]}
        >
          {status.info}
        </span>
      {/if}
    </span>
    {#if status.progress != null}
      <span class="{bar} relative">
        <span class={['block h-full w-full origin-left', progressColor]} style:transform="scaleX({status.progress})"
        ></span>
        {#each vampSpans(cue, ui.view?.cue_info[cue.id]) as span, i (i)}
          <span
            class="absolute inset-y-0 bg-vamp-soft/35"
            style:left="{span.left * 100}%"
            style:width="{span.width * 100}%"
          ></span>
        {/each}
      </span>
    {/if}
  </span>
  <!-- Hidden rather than left out in show mode: a few hundred tooltips make the mode switch slow. -->
  <Tooltip tip={t('cuerow.drag')}>
    {#snippet child({ props })}
      <span
        {...props}
        class={['flex cursor-grab text-dim hover:text-fg-2 active:cursor-grabbing', !editing && 'hidden']}
        data-handle
      >
        <DotsSixVertical />
      </span>
    {/snippet}
  </Tooltip>
</div>
