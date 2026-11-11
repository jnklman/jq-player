<script lang="ts">
  import CaretDown from '~icons/ph/caret-down-bold';
  import Clipboard from '~icons/ph/clipboard-bold';
  import Copy from '~icons/ph/copy-bold';
  import LinkSimple from '~icons/ph/link-simple-bold';
  import ListNumbers from '~icons/ph/list-numbers-bold';
  import Plus from '~icons/ph/plus-bold';
  import RowsPlusBottom from '~icons/ph/rows-plus-bottom-bold';
  import Scissors from '~icons/ph/scissors-bold';
  import SelectionPlus from '~icons/ph/selection-plus-bold';
  import SelectionSlash from '~icons/ph/selection-slash-bold';
  import SpeakerHigh from '~icons/ph/speaker-high-bold';
  import Trash from '~icons/ph/trash-bold';
  import Waveform from '~icons/ph/waveform-bold';
  import { buttonVariants } from '$lib/components/ui/button';
  import { ButtonGroup, ButtonGroupButton } from '$lib/components/ui/button-group';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Separator } from '$lib/components/ui/separator';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import {
    addCueAndSelect,
    copyCues,
    cutCues,
    deleteCues,
    duplicateCue,
    groupAction,
    linkCues,
    linkSource,
    pasteCues,
    pasteOverCues,
    renumber,
    toggleGroup,
    unlinkCues,
  } from '../../lib/commands';
  import { addGroups } from '../../lib/control';
  import { cueLabel } from '../../lib/cue';
  import { addIcons } from '../../lib/cueIcons';
  import { t } from '../../lib/i18n/index.svelte';
  import { allCues, cues, mode, selection, ui } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';
  import GotoField from './GotoField.svelte';
  import SelectionModePicker from './SelectionModePicker.svelte';

  // The cue list's header actions: adding, the clipboard, links, undo (edit
  // mode) and the selection mode. Everything goes through `lib/commands`, so
  // the buttons do exactly what their shortcuts do.
  const view = $derived(ui.view!);
  const editing = $derived(mode() === 'edit');
  const selected = $derived(selection());
  const selectedCues = $derived(allCues().filter((cue) => selected.includes(cue.id)));
  // Linking takes the settings of the cue in the inspector.
  const source = $derived(selectedCues.find((cue) => cue.id === ui.selected));
  const linkTarget = $derived(linkSource());
  const grouping = $derived(groupAction(selected));

  const sceneCount = $derived(cues().filter((cue) => cue.type === 'scene').length);
  const cueCount = $derived(cues().length - sceneCount);
</script>

{#snippet divider()}
  <Separator orientation="vertical" class="mx-1 h-5 bg-line-strong data-vertical:self-center" />
{/snippet}

{#if editing}
  <div class="flex flex-wrap items-center justify-end gap-1.5">
    <ButtonGroup>
      <ButtonGroupButton tip={t('toolbar.add_sound_tip')} onclick={() => addCueAndSelect({ label: '', kind: 'sound' })}>
        <Waveform class="text-muted-foreground" />{t('type.sound')}
      </ButtonGroupButton>
      <ButtonGroupButton
        tip={t('toolbar.add_volume_tip')}
        onclick={() => addCueAndSelect({ label: '', kind: 'volume' })}
      >
        <SpeakerHigh class="text-muted-foreground" />{t('control.volume')}
      </ButtonGroupButton>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger class={buttonVariants()}>
          <Plus class="text-muted-foreground" />{t('toolbar.more')}<CaretDown />
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end">
          {#each addGroups as group, g (group.label)}
            {#if g > 0}<DropdownMenu.Separator />{/if}
            <DropdownMenu.Group>
              <DropdownMenu.GroupHeading>{group.label}</DropdownMenu.GroupHeading>
              {#each group.items as item (item.kind)}
                {@const Icon = addIcons[item.kind]}
                <DropdownMenu.Item onSelect={() => addCueAndSelect(item)}>
                  <Icon class="text-muted-foreground" />{item.label}
                </DropdownMenu.Item>
              {/each}
            </DropdownMenu.Group>
          {/each}
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </ButtonGroup>

    {@render divider()}
    <ButtonGroup>
      <ButtonGroupButton
        size="icon"
        tip={t('cuemenu.copy')}
        keys="{t('key.ctrl')} + C"
        disabled={selected.length === 0}
        aria-label={t('cuemenu.copy')}
        onclick={() => copyCues()}
      >
        <Copy />
      </ButtonGroupButton>
      <ButtonGroupButton
        size="icon"
        tip={t('cuemenu.cut')}
        keys="{t('key.ctrl')} + X"
        disabled={selected.length === 0}
        aria-label={t('cuemenu.cut')}
        onclick={() => cutCues()}
      >
        <Scissors />
      </ButtonGroupButton>
      <ButtonGroupButton
        size="icon"
        tip={t('cuemenu.paste')}
        keys="{t('key.ctrl')} + V"
        disabled={!view.can_paste}
        aria-label={t('cuemenu.paste')}
        onclick={() => pasteCues()}
      >
        <Clipboard />
      </ButtonGroupButton>
      <DropdownMenu.Root>
        <Tooltip tip={t('toolbar.more_paste')}>
          {#snippet child({ props: tipProps })}
            <DropdownMenu.Trigger
              {...tipProps}
              class={buttonVariants({ size: 'icon' })}
              disabled={!view.can_paste}
              aria-label={t('toolbar.more_paste')}
            >
              <CaretDown />
            </DropdownMenu.Trigger>
          {/snippet}
        </Tooltip>
        <DropdownMenu.Content align="start">
          <DropdownMenu.Item disabled={selected.length === 0} onSelect={() => pasteOverCues()}>
            {t('cuemenu.paste_replace')}
          </DropdownMenu.Item>
          <DropdownMenu.Item disabled={selected.length === 0} onSelect={() => pasteOverCues(selected, true)}>
            {t('cuemenu.paste_swap')}
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
      <ButtonGroupButton
        size="icon"
        tip={t('group.duplicate')}
        disabled={!ui.selected}
        aria-label={t('group.duplicate')}
        onclick={() => duplicateCue()}
      >
        <RowsPlusBottom />
      </ButtonGroupButton>
      <ButtonGroupButton
        size="icon"
        tip={t('common.delete')}
        keys={t('key.delete')}
        disabled={selected.length === 0}
        aria-label={t('common.delete')}
        onclick={() => deleteCues()}
      >
        <Trash />
      </ButtonGroupButton>
    </ButtonGroup>
    <ButtonGroup>
      <DropdownMenu.Root>
        <Tooltip tip={t('cuemenu.link')}>
          {#snippet child({ props: tipProps })}
            <DropdownMenu.Trigger
              {...tipProps}
              class={buttonVariants({ size: 'icon' })}
              disabled={!ui.selected}
              aria-label={t('cuemenu.link')}
            >
              <LinkSimple />
            </DropdownMenu.Trigger>
          {/snippet}
        </Tooltip>
        <DropdownMenu.Content align="end">
          <DropdownMenu.Item disabled={!linkTarget} onSelect={() => linkTarget && linkCues(linkTarget.id)}>
            {t('cuemenu.link_selection', { cue: source ? cueLabel(source) : '–' })}
          </DropdownMenu.Item>
          <DropdownMenu.Item disabled={source?.type === 'group'} onSelect={() => duplicateCue(ui.selected, true)}>
            {t('cuemenu.duplicate_linked')}
          </DropdownMenu.Item>
          <DropdownMenu.Item disabled={!view.can_paste} onSelect={() => pasteCues(true)}>
            {t('cuemenu.paste_linked')}
            <DropdownMenu.Shortcut>{t('key.ctrl')} + {t('key.shift')} + V</DropdownMenu.Shortcut>
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Item disabled={!selectedCues.some((cue) => cue.link)} onSelect={() => unlinkCues()}>
            {t('cuemenu.unlink')}
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
      <ButtonGroupButton
        size="icon"
        tip={grouping.ungroup ? t('cuemenu.ungroup') : t('cuemenu.group')}
        disabled={!grouping.enabled}
        aria-label={grouping.ungroup ? t('cuemenu.ungroup') : t('cuemenu.group')}
        onclick={() => toggleGroup()}
      >
        {#if grouping.ungroup}
          <SelectionSlash />
        {:else}
          <SelectionPlus />
        {/if}
      </ButtonGroupButton>
    </ButtonGroup>
    <Tooltip
      tip={t('cuemenu.renumber')}
      class={buttonVariants({ size: 'icon' })}
      disabled={!cues().length}
      aria-label={t('cuemenu.renumber')}
      onclick={renumber}><ListNumbers /></Tooltip
    >

    {@render divider()}
    <GotoField />
    <SelectionModePicker />
  </div>
{:else}
  <div class="flex items-center justify-end gap-3">
    <span class={hint}>
      {t('status.cues', { count: cueCount })}
      {#if sceneCount > 0}
        {t('toolbar.in_scenes', { count: sceneCount })}
      {/if}
    </span>
    <GotoField />
    <SelectionModePicker />
  </div>
{/if}
