<script lang="ts">
  import * as Resizable from '$lib/components/ui/resizable';

  import type { LayoutNode } from '../../lib/layout.svelte';
  import { getPanelHosts } from './context';
  import { minShares } from './fit.svelte';
  import LayoutTree from './LayoutTree.svelte';
  import { layout } from './panels';

  let { node }: { node: LayoutNode } = $props();

  // Rebuilt (keyed) whenever panels move, so sizes only need to be read once
  // here. The panels themselves survive that (`hosts.ts`).
  const hosts = getPanelHosts();

  /** Smallest pane, in percent of its split. */
  const MIN_SIZE = 8;

  // A split keeps its panes high enough for panels that need a height of
  // their own and wide enough for those with a least width
  // (`fit.svelte.ts`), whatever the window's size.
  let groupRef = $state<HTMLElement | null>(null);
  let size = $state(0);
  $effect(() => {
    const element = groupRef;
    if (!element || node.type !== 'split') return;
    const vertical = node.direction === 'vertical';
    const observer = new ResizeObserver(
      ([entry]) => (size = vertical ? entry.contentRect.height : entry.contentRect.width),
    );
    observer.observe(element);
    return () => observer.disconnect();
  });
  const mins = $derived(node.type === 'split' ? minShares(node.children, node.direction, size, MIN_SIZE) : []);

  // Only sizes set at a divider are kept. paneforge also reports the layout
  // it forces when the limits change with the window's height; kept, a pane
  // grown in a low window would stay that big in a high one, and one given
  // 0 % (fitting its panels) would no longer follow them.
  let dragging = false;
  let keyed = false;
  function onKeyResize() {
    keyed = true;
    queueMicrotask(() => (keyed = false));
  }
</script>

{#if node.type === 'panel'}
  <div class="contents" {@attach hosts.slot(node.panel)}></div>
{:else}
  {@const group = node}
  <Resizable.PaneGroup
    bind:ref={groupRef}
    direction={group.direction}
    onLayoutChange={(sizes) => {
      if (dragging || keyed) layout.resize(group, sizes);
    }}
  >
    {#each group.children as child, i (i)}
      {#if i > 0}
        <Resizable.Handle onDraggingChange={(on) => (dragging = on)} onkeydown={onKeyResize} />
      {/if}
      <Resizable.Pane defaultSize={group.sizes[i]} minSize={mins[i] ?? MIN_SIZE} order={i}>
        <LayoutTree node={child} />
      </Resizable.Pane>
    {/each}
  </Resizable.PaneGroup>
{/if}
