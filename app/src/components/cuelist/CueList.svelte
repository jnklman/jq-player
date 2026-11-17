<script lang="ts" module>
  // Scroll position of the list, kept across remounts.
  let scrolled = { top: 0, left: 0 };
</script>

<script lang="ts">
  import Stop from '~icons/ph/stop-fill';
  import * as ContextMenu from '$lib/components/ui/context-menu';
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import { api, type Cue, type ShowView } from '../../lib/api';
  import { addDroppedAudio, edit } from '../../lib/commands';
  import { LIST_HEADER_HEIGHT, rowScrollDelta } from '../../lib/cueScroll';
  import { chainWaiting, rowRunning } from '../../lib/cueStatus';
  import { dropSlot, edgeScroller, onFileDrop } from '../../lib/fileDrop';
  import { t } from '../../lib/i18n/index.svelte';
  import { isAudioFile } from '../../lib/media';
  import { sortable } from '../../lib/sortable';
  import { cues, mode, moveSelection, select, selection, selectionMode, ui } from '../../lib/store.svelte';
  import { empty } from '../../lib/ui';
  import PanelFrame from '../layout/PanelFrame.svelte';
  import CueMenu from './CueMenu.svelte';
  import CueRow from './CueRow.svelte';
  import CueToolbar from './CueToolbar.svelte';
  import { CUE_ROW_HEIGHT, cueGrid, cueRow, headRow, INCOMING_TIP, SCENE_ROW_HEIGHT, THROUGH_LINE } from './grid';
  import SceneRow from './SceneRow.svelte';

  const editing = $derived(mode() === 'edit');
  // Order after a drop, shown until the backend's answer arrives so the row
  // doesn't jump back for a moment. Any new show view replaces it.
  let moved = $state.raw<{ view: ShowView; cues: Cue[] } | null>(null);
  const list = $derived(moved && moved.view === ui.view ? moved.cues : cues());
  const playhead = $derived(ui.runtime?.playhead);
  const head = $derived(playhead ? list.findIndex((cue) => cue.id === playhead) : list.length);
  const selected = $derived(selection());
  const selectedIds = $derived(new Set(selected));

  // For each row the nearest cue above and below that is not a scene marker, from the list alone (not the runtime
  // state), so the rows' chain arrows don't search the list again on every state update.
  const playable = $derived.by(() => {
    const before: (Cue | undefined)[] = new Array(list.length);
    const after: (Cue | undefined)[] = new Array(list.length);
    let last: Cue | undefined;
    for (let i = 0; i < list.length; i++) {
      before[i] = last;
      if (list[i].type !== 'scene') last = list[i];
    }
    last = undefined;
    for (let i = list.length - 1; i >= 0; i--) {
      after[i] = last;
      if (list[i].type !== 'scene') last = list[i];
    }
    return { before, after };
  });

  /** The cue a chain from row `i` runs on to: the next one past scene markers. */
  function nextPlayable(i: number): Cue | undefined {
    return playable.after[i];
  }

  function rowHeight(cue: Cue | undefined): number {
    return cue?.type === 'scene' ? SCENE_ROW_HEIGHT : CUE_ROW_HEIGHT;
  }

  /** Whether the scene marker in row `i` is over: the playhead has passed the next marker, not just this one. */
  function sceneDone(i: number): boolean {
    const next = list.findIndex((cue, at) => at > i && cue.type === 'scene');
    return next >= 0 && next < head;
  }

  // Shared answers of `chainThrough`: the same object while nothing changes, so the rows aren't updated on every
  // state update.
  const THROUGH = { waiting: false };
  const THROUGH_WAITING = { waiting: true };

  /** Whether a chain runs past the scene marker in row `i`, and whether its next cue waits to start. */
  function chainThrough(i: number): { waiting: boolean } | null {
    const before = playable.before[i];
    const after = playable.after[i];
    if (!before?.advance.start_next || !after) return null;
    return chainWaiting(before, after) ? THROUGH_WAITING : THROUGH;
  }

  // The list follows the playhead: whenever it moves (GO, arrow keys, OSC/MIDI,
  // double-click) to a row that isn't fully visible, the list scrolls just far
  // enough to show it. `rowScrollDelta` keeps it clear of the sticky header and a stuck scene marker.
  // The scroller is the viewport of the scroll area.
  let scroller = $state<HTMLElement | null>(null);

  // Hiding the panel unmounts the list; shown again, it comes back at the
  // position it was left at.
  $effect(() => {
    const element = scroller;
    if (!element) return;
    element.scrollTo(scrolled);
    const remember = () => (scrolled = { top: element.scrollTop, left: element.scrollLeft });
    element.addEventListener('scroll', remember, { passive: true });
    return () => element.removeEventListener('scroll', remember);
  });

  // The list follows the playhead: whenever it moves (GO, arrow keys, OSC/MIDI,
  // double-click) to a row that isn't fully visible including the next three rows
  // (scene markers are lower than cues, the end row counts as one). Only when it moves: any other show update
  // (an edit, undo, a saved setting) must not pull the list back while the operator works further down. The list is
  // read only until the playhead's row has been found, in case the playhead arrives before its cue.
  const ROW_OFFSET = 3;
  let followed: string | undefined;
  $effect(() => {
    if (!playhead) followed = undefined;
    if (!playhead || !scroller || playhead === followed) return;
    const at = list.findIndex((cue) => cue.id === playhead);
    const playHeadRow = scroller.querySelector<HTMLElement>(`[data-cue="${playhead}"]`);
    if (at < 0 || !playHeadRow) return;
    followed = playhead;
    let following = 0;
    for (let row = at + 1; row <= Math.min(at + ROW_OFFSET, list.length); row++) following += rowHeight(list[row]);
    const delta = rowScrollDelta(scroller, playHeadRow, following);
    if (!delta) return;
    scroller.scrollBy({ top: delta, behavior: Math.abs(delta) > CUE_ROW_HEIGHT ? 'smooth' : 'instant' });
  });

  // A sticky scene marker is pushed out by the next one instead of being covered:
  // its `top` shrinks (below zero) as the next marker comes up under it. Plain CSS
  // would need a wrapper per scene, which the flat sortable list doesn't have.
  let rows = $state<HTMLElement | null>(null);
  $effect(() => {
    const element = scroller;
    const container = rows;
    void list; // again after every change of the list
    if (!element || !container) return;
    const push = () => {
      // Where each marker would sit unstuck, from the row heights (sticky rows keep theirs).
      let y = container.getBoundingClientRect().top - element.getBoundingClientRect().top;
      // All reads first, then the writes: reading a height after setting a `top` would lay the list out again per marker.
      const scenes: { row: HTMLElement; top: number; height: number }[] = [];
      for (const row of container.children) {
        if (!(row instanceof HTMLElement)) continue;
        const height = row.offsetHeight;
        if (row.dataset.scene !== undefined) scenes.push({ row, top: y, height });
        y += height;
      }
      // A marker away from its place is stuck (or being pushed out): the cue above it has gone under the header and
      // the one below slides under the marker. The chain line through it (`--chain-cut` shortens it from the top)
      // draws back with the arrow below and is gone when that arrow's head is covered, never hanging on its own.
      scenes.forEach(({ row, top, height }, i) => {
        const next = scenes[i + 1]?.top ?? Infinity;
        const stuck = Math.min(LIST_HEADER_HEIGHT, next - height);
        row.style.top = `${stuck}px`;
        const cut = Math.min(THROUGH_LINE, (Math.max(0, stuck - top) * THROUGH_LINE) / INCOMING_TIP);
        row.style.setProperty('--chain-cut', `${cut}px`);
      });
    };
    push();
    element.addEventListener('scroll', push, { passive: true });
    return () => element.removeEventListener('scroll', push);
  });

  // Mouse wheel per `settings.wheel` (edit and show mode alike): scrolls, or
  // moves the selection or playhead one cue per notch; Ctrl+wheel always
  // scrolls. A touchpad's small deltas add up to a step. Not passive, so the
  // list does not scroll as well.
  const WHEEL_STEP_PX = 40;
  $effect(() => {
    const element = scroller;
    if (!element) return;
    let pending = 0;
    const wheel = (e: WheelEvent) => {
      const wheelMode = ui.view?.settings.wheel ?? 'scroll';
      if (wheelMode === 'scroll' || e.ctrlKey || e.deltaY === 0) return;
      e.preventDefault();
      pending += e.deltaMode === WheelEvent.DOM_DELTA_PIXEL ? e.deltaY : Math.sign(e.deltaY) * WHEEL_STEP_PX;
      if (Math.abs(pending) < WHEEL_STEP_PX) return;
      const step = Math.sign(pending);
      pending = 0;
      if (wheelMode === 'playhead') api.action({ action: step > 0 ? 'playhead_next' : 'playhead_prev' });
      else moveSelection(step);
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  });

  /** The cues a drag of `id` moves: the selected ones of the list, in list
   * order, if `id` is one of them; otherwise just `id`. */
  function dragged(id: string): string[] {
    const group = selectedIds.has(id) ? list.filter((cue) => selectedIds.has(cue.id)) : [];
    return group.length > 1 ? group.map((cue) => cue.id) : [id];
  }

  /** For a group drag: the other rows that go along, hidden while dragging. */
  function dragGroup(item: HTMLElement): { others: HTMLElement[]; label: string } | null {
    const id = item.dataset.cue;
    const ids = id ? dragged(id) : [];
    if (ids.length < 2) return null;
    const going = new Set(ids);
    const others = [...(item.parentElement?.children ?? [])].filter(
      (row): row is HTMLElement => row instanceof HTMLElement && row !== item && going.has(row.dataset.cue ?? ''),
    );
    return { others, label: t('status.cues', { count: ids.length }) };
  }

  // `to` is the index after removing the dragged cue; `move` wants the index
  // among the cues that stay.
  async function onMove(item: HTMLElement, to: number) {
    const id = item.dataset.cue;
    if (!id || !list.some((cue) => cue.id === id) || !ui.view) return;
    const ids = dragged(id);
    const going = new Set(ids);
    const before = list.filter((cue) => cue.id !== id).slice(0, to);
    const index = before.filter((cue) => !going.has(cue.id)).length;
    const rest = list.filter((cue) => !going.has(cue.id));
    rest.splice(index, 0, ...list.filter((cue) => going.has(cue.id)));
    moved = { view: ui.view, cues: rest };
    if (!(await edit({ op: 'move', ids, to: index }))) moved = null;
  }

  // Audio files dragged onto the list in edit mode become sound cues where they are dropped. While they are
  // over the window the list is outlined with a hint, as the cursor can't show where a drop is refused (the
  // webview picks it once for the whole window); over the list a line marks the place (its height in the
  // list's content). Near the list's top or bottom edge it scrolls, so a long list can be dropped into anywhere.
  // The line lies under the sticky header and scene markers, like the rows; next to a scene marker it sits
  // wholly beside it instead of on the boundary, where the marker would cover half of it.
  // The outline follows the panel's rounded bottom corners: `rounded-xl` less its 1 px border (a popped-out
  // panel's window has 10 px, or square corners with a native frame).
  type DropLine = { top: number; side: 'center' | 'above' | 'below' };
  let fileDrag = $state(false);
  let dropLine = $state<DropLine | null>(null);

  /** Where files dragged to the point would go: `null` if none is audio or the point isn't on the list. */
  function dropTarget(x: number, y: number, paths: string[]): (DropLine & { index: number }) | null {
    if (!scroller || !rows || !paths.some(isAudioFile)) return null;
    const hit = document.elementFromPoint(x, y);
    if (!hit || !scroller.contains(hit)) return null;
    const slot = dropSlot(rows, y);
    const side =
      list[slot.index]?.type === 'scene' ? 'above' : list[slot.index - 1]?.type === 'scene' ? 'below' : 'center';
    return { index: slot.index, top: rows.offsetTop + slot.top, side };
  }

  $effect(() => {
    const element = scroller;
    if (!editing || !element) return;
    // The last hover, to place the line again as the list scrolls under a pointer that holds still.
    let last: { x: number; y: number; paths: string[] } | null = null;
    const hover = () => {
      const target = last && dropTarget(last.x, last.y, last.paths);
      dropLine = target && { top: target.top, side: target.side };
      return target;
    };
    const edges = edgeScroller(element, hover, LIST_HEADER_HEIGHT);
    const stop = onFileDrop({
      over: (x, y, paths) => {
        fileDrag = paths.some(isAudioFile);
        last = { x, y, paths };
        edges.update(hover() ? y : null);
      },
      leave: () => {
        last = null;
        edges.stop();
        fileDrag = false;
        dropLine = null;
      },
      drop: (x, y, paths) => {
        const target = dropTarget(x, y, paths);
        if (target) void addDroppedAudio(paths, { index: target.index });
      },
    });
    return () => {
      stop();
      edges.stop();
      fileDrag = false;
      dropLine = null;
    };
  });

  // A right-click on a cue outside the selection selects it first, so the
  // context menu acts on the cue under the pointer.
  function oncontextmenu(e: MouseEvent) {
    const id = (e.target as Element).closest<HTMLElement>('[data-cue]')?.dataset.cue;
    if (id && !selectedIds.has(id)) select(id);
  }
</script>

{#snippet toolbar()}
  <CueToolbar />
{/snippet}

<PanelFrame title={t('cuelist.title')} actions={toolbar}>
  <!-- Context menu in edit mode only. Keep the trigger's `select-none`: WebKit treats
   `select-auto` as `text`, so dragging the scrollbar would select the rows' text. -->
  <ContextMenu.Root>
    <ContextMenu.Trigger disabled={!editing} class="relative flex min-h-0 flex-1 flex-col" {oncontextmenu}>
      <!-- A click on empty space in the list (not on a cue row) clears the selection, unless it is linked to the playhead.
       No tab stop: the arrow keys move the playhead or selection, never scroll. -->
      <ScrollArea
        orientation="both"
        class="flex-1 focus:outline-hidden"
        bind:viewportRef={scroller}
        role="presentation"
        tabindex={-1}
        onclick={(e) => {
          const target = e.target as Element;
          if (selectionMode() !== 'linked' && !target.closest('[data-cue], [data-slot="scroll-area-scrollbar"]'))
            select(null);
        }}
        ><div class="relative pb-3">
          <div
            class={[
              cueGrid,
              'sticky top-0 z-2 h-8 border-b border-line bg-panel py-2 text-[11px] font-semibold tracking-[0.8px] text-muted-foreground uppercase',
            ]}
          >
            <span></span>
            <span></span>
            <span>{t('cuelist.col_number')}</span>
            <span>{t('cuelist.col_title')}</span>
            <span>{t('playhead.script_ref')}</span>
            <span>{t('cuelist.col_type')}</span>
            <span>{t('cuelist.col_status')}</span>
          </div>

          <div bind:this={rows} data-cue-rows {@attach sortable(onMove, { handle: '[data-handle]' }, dragGroup)}>
            {#each list as cue, i (cue.id)}
              {#if cue.type === 'scene'}
                <SceneRow
                  {cue}
                  through={chainThrough(i)}
                  done={sceneDone(i)}
                  selected={selectedIds.has(cue.id)}
                  {editing}
                />
              {:else}
                <CueRow
                  {cue}
                  next={nextPlayable(i)}
                  beforeScene={list[i + 1]?.type === 'scene'}
                  incoming={list[i - 1]?.type === 'scene' ? chainThrough(i - 1) : null}
                  head={i === head}
                  done={i < head && !rowRunning(cue)}
                  selected={selectedIds.has(cue.id)}
                  {editing}
                />
              {/if}
            {/each}
          </div>

          {#if dropLine}
            <div
              data-drop-line={dropLine.side}
              class={[
                'pointer-events-none absolute inset-x-0 z-0 h-0.5 rounded-full bg-primary',
                dropLine.side === 'center' && '-translate-y-1/2',
                dropLine.side === 'above' && '-translate-y-full',
              ]}
              style:top="{dropLine.top}px"
            ></div>
          {/if}

          {#if !list.length}
            <div class={empty}>{editing ? t('cuelist.empty_edit') : t('playhead.no_cues')}</div>
          {:else}
            <div
              class={[cueGrid, cueRow, head >= list.length && headRow]}
              role="row"
              tabindex="-1"
              onkeydown={() => {}}
            >
              <span></span>
              <span class="text-[13px] font-bold text-primary">
                {#if head >= list.length}
                  <Stop />
                {/if}
              </span>
              <span class="font-mono font-semibold"></span>
              <span class="flex min-w-0 flex-col gap-0.5">
                <span class="truncate font-medium">{t('cuelist.end')}</span>
              </span>
            </div>
          {/if}
        </div></ScrollArea
      >
      {#if fileDrag}
        <div
          data-drop-zone
          class="pointer-events-none absolute inset-0 z-4 flex items-end justify-center rounded-b-[11px] bg-primary/5 pb-4 ring-2 ring-primary/70 ring-inset in-data-native-frame:rounded-none"
        >
          <span
            class={[
              'rounded-full bg-primary px-3 py-1 text-xs font-semibold text-primary-foreground shadow-lg transition-opacity',
              dropLine !== null && 'opacity-60',
            ]}>{t('cuelist.drop_hint')}</span
          >
        </div>
      {/if}
    </ContextMenu.Trigger>
    {#if editing}<CueMenu />{/if}
  </ContextMenu.Root>
</PanelFrame>
