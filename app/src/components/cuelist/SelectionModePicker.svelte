<script lang="ts">
  import CursorClick from '~icons/ph/cursor-click-bold';
  import { buttonVariants } from '$lib/components/ui/button';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type SelectionMode } from '../../lib/api';
  import { labels } from '../../lib/i18n/index.svelte';
  import { attempt, selectionMode } from '../../lib/store.svelte';

  // How the cue list's selection and the playhead go together (a machine setting).
  const modes = labels<SelectionMode>({
    separate: 'selmode.separate',
    playhead_follows: 'selmode.playhead_follows',
    selection_follows: 'selmode.selection_follows',
    linked: 'selmode.linked',
  });
  const current = $derived({ label: modes[selectionMode()] });
</script>

<DropdownMenu.Root>
  <Tooltip tip={current.label}>
    {#snippet child({ props: tipProps })}
      <DropdownMenu.Trigger {...tipProps} class={buttonVariants({ size: 'icon' })} aria-label={current.label}>
        <CursorClick />
      </DropdownMenu.Trigger>
    {/snippet}
  </Tooltip>
  <DropdownMenu.Content align="end">
    <DropdownMenu.RadioGroup
      bind:value={() => selectionMode(), (value) => attempt(api.setSelectionMode(value as SelectionMode))}
    >
      {#each Object.entries(modes) as [value, label] (value)}
        <DropdownMenu.RadioItem {value} closeOnSelect>{label}</DropdownMenu.RadioItem>
      {/each}
    </DropdownMenu.RadioGroup>
  </DropdownMenu.Content>
</DropdownMenu.Root>
