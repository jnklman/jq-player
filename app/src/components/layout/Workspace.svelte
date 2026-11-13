<script lang="ts">
  import { onMount } from 'svelte';

  import { Button } from '$lib/components/ui/button';

  import { t } from '../../lib/i18n/index.svelte';
  import { DOCK_SIZE, type DropZone, type Edge, type PanelId } from '../../lib/layout.svelte';
  import { ui } from '../../lib/store.svelte';
  import { grabbing } from '../../lib/ui';
  import { servePanelWindows } from '../../lib/windows.svelte';
  import { setArrange, setPanelHosts } from './context';
  import { PanelHosts, shownPanels } from './hosts';
  import LayoutTree from './LayoutTree.svelte';
  import PanelHost from './PanelHost.svelte';
  import { layout, panelById, panels } from './panels';

  // The main view's panels. While the layout is being edited (switched in
  // the "Ansicht" menu, independent of the show mode), a panel is moved by
  // dragging its title: onto another panel near an edge places it on that
  // side, its middle swaps the two; onto the margin of the workspace docks
  // it along that whole edge. Resizing is always possible.

  type Drop =
    { kind: 'panel'; target: PanelId; zone: DropZone; rect: DOMRect } | { kind: 'edge'; edge: Edge; rect: DOMRect };

  interface Drag {
    panel: PanelId;
    x: number;
    y: number;
    drop: Drop | null;
  }

  // The layout is the same for every show, so it is read once per mount
  // (the welcome screen has none) and only ever written afterwards.
  layout.restore(ui.view?.settings.layout ?? null);
  // The panel windows of the current mode are open while a show is (the welcome screen has none).
  onMount(() => servePanelWindows(layout));

  let drag = $state<Drag | null>(null);
  let main = $state<HTMLElement>();
  let area = $state<HTMLElement>();

  /** Pointer travel (px) before a press on a title counts as a drag. */
  const THRESHOLD = 6;
  /** How far (px) the edge zones reach from the margin into the panels. */
  const EDGE_REACH = 12;

  /** The nearest of the given edges and its distance. */
  const nearest = (distances: [Edge, number][]) => distances.reduce((best, next) => (next[1] < best[1] ? next : best));

  function aim(x: number, y: number, panel: PanelId): Drop | null {
    if (main && area) {
      const outer = main.getBoundingClientRect();
      const rect = area.getBoundingClientRect();
      const inside = x >= outer.left && x <= outer.right && y >= outer.top && y <= outer.bottom;
      const [edge, distance] = nearest([
        ['left', x - rect.left],
        ['right', rect.right - x],
        ['top', y - rect.top],
        ['bottom', rect.bottom - y],
      ]);
      if (inside && distance < EDGE_REACH) return { kind: 'edge', edge, rect };
    }
    const element = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-panel]');
    const target = element?.dataset.panel;
    if (!element || !target || target === panel) return null;
    const rect = element.getBoundingClientRect();
    const fx = (x - rect.left) / rect.width;
    const fy = (y - rect.top) / rect.height;
    // The nearest edge within the outer third, else the middle.
    const [edge, distance] = nearest([
      ['left', fx],
      ['right', 1 - fx],
      ['top', fy],
      ['bottom', 1 - fy],
    ]);
    return { kind: 'panel', target, zone: distance < 1 / 3 ? edge : 'center', rect };
  }

  function start(panel: PanelId, e: PointerEvent) {
    if (e.button !== 0 || !layout.editing) return;
    const handle = e.currentTarget as HTMLElement;
    const from = { x: e.clientX, y: e.clientY };
    handle.setPointerCapture(e.pointerId);

    const move = (e: PointerEvent) => {
      if (!drag && Math.hypot(e.clientX - from.x, e.clientY - from.y) < THRESHOLD) return;
      grabbing(true);
      drag = { panel, x: e.clientX, y: e.clientY, drop: aim(e.clientX, e.clientY, panel) };
    };
    const finish = (commit: boolean) => {
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', up);
      handle.removeEventListener('pointercancel', cancel);
      handle.removeEventListener('lostpointercapture', cancel);
      const drop = commit ? drag?.drop : null;
      if (drop?.kind === 'panel') layout.move(panel, drop.target, drop.zone);
      else if (drop?.kind === 'edge') layout.dock(panel, drop.edge);
      drag = null;
      grabbing(false);
    };
    const up = () => finish(true);
    const cancel = () => finish(false);
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', up);
    handle.addEventListener('pointercancel', cancel);
    // The capture is gone without a pointerup (e.g. the handle left the page): nothing is dropped.
    handle.addEventListener('lostpointercapture', cancel);
  }

  setArrange({ enabled: () => layout.editing, start });

  setPanelHosts(new PanelHosts());
  const shown = $derived(
    shownPanels(
      layout.root,
      panels.map((def) => def.id),
    ),
  );

  /** The part of the target the dragged panel would take. */
  function preview(rect: DOMRect, zone: DropZone, share: number) {
    const { left, top, width, height } = rect;
    switch (zone) {
      case 'left':
        return { left, top, width: width * share, height };
      case 'right':
        return { left: left + width * (1 - share), top, width: width * share, height };
      case 'top':
        return { left, top, width, height: height * share };
      case 'bottom':
        return { left, top: top + height * (1 - share), width, height: height * share };
      case 'center':
        return { left, top, width, height };
    }
  }

  const box = $derived.by(() => {
    const drop = drag?.drop;
    if (!drop) return null;
    return drop.kind === 'panel'
      ? preview(drop.rect, drop.zone, 1 / 2)
      : preview(drop.rect, drop.edge, DOCK_SIZE / 100);
  });
</script>

<main bind:this={main} class="min-h-0 flex-1 px-5 py-4">
  <div bind:this={area} class="h-full min-h-0">
    {#key layout.root}
      <LayoutTree node={layout.root} />
    {/key}
  </div>
  <!-- Each panel's element moves into its slot; the wrappers stay here, so the list can change around them. -->
  <div class="hidden">
    {#each shown as id (id)}
      <div><PanelHost {id} /></div>
    {/each}
  </div>
</main>

{#if layout.editing}
  <div
    class="fixed bottom-5 left-1/2 z-30 flex -translate-x-1/2 items-center gap-3 rounded-full border border-input bg-popover py-1.5 pr-1.5 pl-4 text-[13px] shadow-2xl"
  >
    <span class="text-muted-foreground">
      {t('layout.edit_hint')}
    </span>
    <Button variant="default" size="sm" class="rounded-full" onclick={() => (layout.editing = false)}
      >{t('common.done')}</Button
    >
  </div>
{/if}

{#if drag}
  {#if box}
    <div
      class="pointer-events-none fixed z-40 rounded-xl border-2 border-primary bg-primary/15 transition-all duration-100"
      style:left="{box.left}px"
      style:top="{box.top}px"
      style:width="{box.width}px"
      style:height="{box.height}px"
    ></div>
  {/if}
  <div
    class="pointer-events-none fixed z-50 rounded-lg border border-input bg-popover px-3 py-1.5 text-[13px] font-semibold shadow-2xl"
    style:left="{drag.x + 12}px"
    style:top="{drag.y + 12}px"
  >
    {panelById(drag.panel)?.title}
    {#if drag.drop?.kind === 'panel' && drag.drop.zone === 'center'}
      <span class="font-normal text-muted-foreground">· {t('layout.swap')}</span>
    {:else if drag.drop?.kind === 'edge'}
      <span class="font-normal text-muted-foreground">· {t('layout.to_edge')}</span>
    {/if}
  </div>
{/if}
