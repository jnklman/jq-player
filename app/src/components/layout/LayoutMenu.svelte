<script lang="ts">
  import Layout from '~icons/ph/layout-bold';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { t } from '../../lib/i18n/index.svelte';
  import type { Placement } from '../../lib/layout.svelte';
  import { mode } from '../../lib/store.svelte';
  import { windowSupport } from '../../lib/window.svelte';
  import { layout, panels } from './panels';

  // Each panel: switched off, in the main window, or in a window of its own
  // (normal or overlay). The main window keeps at least one panel. Without
  // overlays (Wayland), the window menu of the title bar keeps a panel window on top.
  const placements = $derived<{ value: Placement; label: string }[]>([
    { value: 'main', label: t('layout.in_main') },
    { value: 'window', label: t('layout.own_window') },
    ...(windowSupport.overlay ? [{ value: 'overlay' as const, label: t('layout.overlay_long') }] : []),
    { value: 'hidden', label: t('layout.hidden') },
  ]);
  /** Where the panel is; without overlays, an overlay of the layout opens as a normal window. */
  const shown = (id: string): Placement => {
    const placement = layout.placement(id);
    return placement === 'overlay' && !windowSupport.overlay ? 'window' : placement;
  };

  const short = (placement: Placement) =>
    ({
      main: '',
      window: t('layout.short_window'),
      overlay: t('layout.short_overlay'),
      hidden: t('layout.short_hidden'),
    })[placement];
</script>

<DropdownMenu.Root>
  <Tooltip tip={t('layout.view')} side="bottom">
    {#snippet child({ props: tipProps })}
      <DropdownMenu.Trigger {...tipProps}>
        {#snippet child({ props })}
          <button
            {...props}
            class="flex size-7 items-center justify-center rounded-full border border-input bg-secondary text-fg hover:bg-accent"
            aria-label={t('layout.view')}
          >
            <Layout class="size-4" />
          </button>
        {/snippet}
      </DropdownMenu.Trigger>
    {/snippet}
  </Tooltip>
  <DropdownMenu.Content align="end">
    <!-- Edit and show mode each have their own layout, panel windows included. -->
    <DropdownMenu.Label
      >{t('layout.panels')} · {mode() === 'show' ? t('layout.show_mode') : t('layout.editing')}</DropdownMenu.Label
    >
    {#each panels as def (def.id)}
      {@const placement = shown(def.id)}
      {@const stays = !layout.canLeave(def.id)}
      <DropdownMenu.Sub>
        <DropdownMenu.SubTrigger class="justify-between gap-6">
          <span class={placement === 'hidden' ? 'text-muted-foreground' : ''}>{def.title}</span>
          <span class="ml-auto text-xs text-muted-foreground">{short(placement)}</span>
        </DropdownMenu.SubTrigger>
        <DropdownMenu.SubContent>
          <!-- Bound through the layout: the menu stays open on a choice, and the placement also changes from outside
               (a panel window closed, its own menu), which a plain `value` would not follow. -->
          <DropdownMenu.RadioGroup
            bind:value={() => shown(def.id), (next) => next && layout.place(def.id, next as Placement)}
          >
            {#each placements as option (option.value)}
              <DropdownMenu.RadioItem value={option.value} disabled={stays && option.value !== 'main'}>
                {option.label}
              </DropdownMenu.RadioItem>
            {/each}
          </DropdownMenu.RadioGroup>
        </DropdownMenu.SubContent>
      </DropdownMenu.Sub>
    {/each}
    <DropdownMenu.Separator />
    <DropdownMenu.CheckboxItem bind:checked={layout.editing}>{t('layout.edit')}</DropdownMenu.CheckboxItem>
    <DropdownMenu.Item onSelect={() => layout.reset()}>{t('layout.reset')}</DropdownMenu.Item>
  </DropdownMenu.Content>
</DropdownMenu.Root>
