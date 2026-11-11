<script lang="ts">
  import DotsSixVertical from '~icons/ph/dots-six-vertical';
  import FilmSlate from '~icons/ph/film-slate-bold';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { SceneCue } from '../../lib/api';
  import { selectScene } from '../../lib/commands';
  import { t } from '../../lib/i18n/index.svelte';
  import { plainText } from '../../lib/richText';
  import { extendSelection, select, toggleSelected } from '../../lib/store.svelte';
  import { cueGrid, dragRow, sceneRow } from './grid';

  // A scene marker of the list: a heading across the row. Playback passes
  // over it, so it has no number, type or status; a chain running past it
  // is drawn through it. It sticks below the list's header until the next
  // marker pushes it out (`top` set by the list), so its chain line stays inside the row.
  let {
    cue,
    through,
    done,
    selected,
    editing,
  }: {
    cue: SceneCue;
    /** A chain runs past the marker: `waiting` while its next cue waits to start. */
    through: { waiting: boolean } | null;
    /** Above the playhead. */
    done: boolean;
    selected: boolean;
    editing: boolean;
  } = $props();

  const base = $derived(selected ? 'var(--color-selected)' : 'var(--color-panel-2)');
  const tint = $derived(cue.color ? `color-mix(in oklab, ${cue.color} 22%, ${base})` : base);

  function onclick(e: MouseEvent) {
    if (editing && (e.ctrlKey || e.metaKey)) toggleSelected(cue.id);
    else if (editing && e.shiftKey) extendSelection(cue.id);
    else select(cue.id);
  }
</script>

<div
  class={[
    cueGrid,
    sceneRow,
    'sticky top-8 z-1 border-t border-t-line-strong',
    // The marker's colour tints the whole heading, over the stripe on the left.
    'bg-linear-to-r from-(--cue-color) from-[8px] to-(--cue-tint) to-[8px]',
    dragRow,
  ]}
  style:--cue-color={cue.color ?? 'transparent'}
  style:--cue-tint={tint}
  data-cue={cue.id}
  data-scene
  role="row"
  tabindex="-1"
  aria-selected={selected}
  {onclick}
  onmousedown={(e) => {
    if (e.shiftKey) e.preventDefault();
  }}
  ondblclick={() => selectScene(cue.id)}
  onkeydown={() => {}}
>
  <!-- The chain from the cue above goes on to the one below, top to bottom over the row lines. -->
  <span class="relative self-stretch">
    {#if through}
      <span
        class={[
          'pointer-events-none absolute -top-1.75 -bottom-1.75 left-[5.25px] w-[1.5px] bg-current',
          through.waiting ? 'text-primary' : 'text-dim',
        ]}
      ></span>
    {/if}
  </span>
  <span class={['flex text-[15px]', done ? 'text-dim' : 'text-fg-2']}><FilmSlate /></span>
  <span class="col-span-5 flex min-w-0 items-baseline gap-3">
    <span class={['truncate text-[15px] font-bold tracking-wide', done && 'text-muted-foreground']}
      >{cue.title || t('scene.untitled')}</span
    >
    {#if cue.notes}<span class="truncate text-xs text-muted-foreground">{plainText(cue.notes.split('\n')[0])}</span
      >{/if}
  </span>
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
