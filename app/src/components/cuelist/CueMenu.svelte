<script lang="ts">
  import type { Component } from 'svelte';

  import ArrowLineDown from '~icons/ph/arrow-line-down-bold';
  import ArrowLineUp from '~icons/ph/arrow-line-up-bold';
  import Checks from '~icons/ph/checks-bold';
  import Clipboard from '~icons/ph/clipboard-bold';
  import Copy from '~icons/ph/copy-bold';
  import LinkSimple from '~icons/ph/link-simple-bold';
  import ListNumbers from '~icons/ph/list-numbers-bold';
  import PencilSimple from '~icons/ph/pencil-simple-bold';
  import Plus from '~icons/ph/plus-bold';
  import RowsPlusBottom from '~icons/ph/rows-plus-bottom-bold';
  import Scissors from '~icons/ph/scissors-bold';
  import SelectionPlus from '~icons/ph/selection-plus-bold';
  import SelectionSlash from '~icons/ph/selection-slash-bold';
  import Swap from '~icons/ph/swap-bold';
  import Trash from '~icons/ph/trash-bold';
  import Waveform from '~icons/ph/waveform-bold';
  import * as ContextMenu from '$lib/components/ui/context-menu';

  import {
    addCueAndSelect,
    addToPlaylist,
    canOpenWaveform,
    copyCues,
    cutCues,
    deleteCues,
    duplicateCue,
    editGroup,
    groupAction,
    linkCues,
    linkSource,
    openWaveform,
    pasteCues,
    pasteOverCues,
    type Place,
    placeAfterSelection,
    placeBeforeSelection,
    renumber,
    replaceSoundFile,
    selectEveryCue,
    toggleGroup,
    unlinkCues,
  } from '../../lib/commands';
  import { addGroups, type AddItem } from '../../lib/control';
  import { cueLabel, isPlaceholder } from '../../lib/cue';
  import { addIcons } from '../../lib/cueIcons';
  import { t } from '../../lib/i18n/index.svelte';
  import { allCues, cues, selection, ui } from '../../lib/store.svelte';

  // Content of the cue list's context menu (edit mode): the actions of its
  // header, through the same `lib/commands`.
  const view = $derived(ui.view!);
  const selected = $derived(selection());
  const selectedCues = $derived(allCues().filter((cue) => selected.includes(cue.id)));
  const source = $derived(selectedCues.find((cue) => cue.id === ui.selected));
  const linkTarget = $derived(linkSource());
  // The cue the type-specific actions (open its dialog, …) are for: one selected cue.
  const single = $derived(selected.length === 1 ? source : undefined);
  const grouping = $derived(groupAction(selected));
  // "Above" needs a cue of the main list to go above.
  const inList = $derived(cues().some((cue) => cue.id === ui.selected));
</script>

<!-- The add menu, for a place relative to the selection (taken when an item is chosen). -->
{#snippet addMenu(label: string, Icon: Component, place: () => Place, disabled = false)}
  {@const add = (item: AddItem) => addCueAndSelect(item, place())}
  <ContextMenu.Sub>
    <ContextMenu.SubTrigger {disabled}><Icon class="text-muted-foreground" />{label}</ContextMenu.SubTrigger>
    <ContextMenu.SubContent class="max-h-(--bits-context-menu-content-available-height) overflow-y-auto">
      {#each addGroups as group, g (group.label)}
        {#if g > 0}<ContextMenu.Separator />{/if}
        <ContextMenu.Group>
          <ContextMenu.GroupHeading>{group.label}</ContextMenu.GroupHeading>
          {#each group.items as item (item.kind)}
            {@const ItemIcon = addIcons[item.kind]}
            <ContextMenu.Item onSelect={() => add(item)}>
              <ItemIcon class="text-muted-foreground" />{item.label}
            </ContextMenu.Item>
          {/each}
        </ContextMenu.Group>
      {/each}
    </ContextMenu.SubContent>
  </ContextMenu.Sub>
{/snippet}

<ContextMenu.Content class="min-w-56">
  {#if single?.type === 'group'}
    <ContextMenu.Item onSelect={() => editGroup(single.id)}>
      <PencilSimple class="text-muted-foreground" />{t('cuemenu.edit_group')}
    </ContextMenu.Item>
    <ContextMenu.Separator />
  {:else if single?.type === 'playlist'}
    <ContextMenu.Item onSelect={() => addToPlaylist(single.id)}>
      <Plus class="text-muted-foreground" />{t('cuemenu.add_item')}
    </ContextMenu.Item>
    <ContextMenu.Separator />
  {:else if single?.type === 'sound'}
    <ContextMenu.Item disabled={!canOpenWaveform(single)} onSelect={() => openWaveform(single.id)}>
      <Waveform class="text-muted-foreground" />{t('cuemenu.edit_waveform')}
    </ContextMenu.Item>
    <ContextMenu.Item onSelect={() => replaceSoundFile(single)}>
      <Swap class="text-muted-foreground" />{isPlaceholder(single)
        ? t('cuemenu.choose_file')
        : t('cuemenu.replace_file')}
    </ContextMenu.Item>
    <ContextMenu.Separator />
  {/if}

  {@render addMenu(t('cuemenu.add_above'), ArrowLineUp, placeBeforeSelection, !inList)}
  {@render addMenu(t('cuemenu.add_below'), ArrowLineDown, placeAfterSelection)}

  <ContextMenu.Separator />
  <ContextMenu.Item disabled={selected.length === 0} onSelect={() => copyCues()}>
    <Copy class="text-muted-foreground" />{t('cuemenu.copy')}<ContextMenu.Shortcut
      >{t('key.ctrl')} + C</ContextMenu.Shortcut
    >
  </ContextMenu.Item>
  <ContextMenu.Item disabled={selected.length === 0} onSelect={() => cutCues()}>
    <Scissors class="text-muted-foreground" />{t('cuemenu.cut')}<ContextMenu.Shortcut
      >{t('key.ctrl')} + X</ContextMenu.Shortcut
    >
  </ContextMenu.Item>
  <ContextMenu.Item disabled={!view.can_paste} onSelect={() => pasteCues()}>
    <Clipboard class="text-muted-foreground" />{t('cuemenu.paste')}<ContextMenu.Shortcut
      >{t('key.ctrl')} + V</ContextMenu.Shortcut
    >
  </ContextMenu.Item>
  <ContextMenu.Item inset disabled={!view.can_paste || selected.length === 0} onSelect={() => pasteOverCues()}>
    {t('cuemenu.paste_replace')}
  </ContextMenu.Item>
  <ContextMenu.Item
    inset
    disabled={!view.can_paste || selected.length === 0}
    onSelect={() => pasteOverCues(selected, true)}
  >
    {t('cuemenu.paste_swap')}
  </ContextMenu.Item>
  <ContextMenu.Item disabled={!ui.selected} onSelect={() => duplicateCue()}>
    <RowsPlusBottom class="text-muted-foreground" />{t('group.duplicate')}
  </ContextMenu.Item>

  <ContextMenu.Item disabled={!grouping.enabled} onSelect={() => toggleGroup()}>
    {#if grouping.ungroup}
      <SelectionSlash class="text-muted-foreground" />{t('cuemenu.ungroup')}
    {:else}
      <SelectionPlus class="text-muted-foreground" />{t('cuemenu.group')}
    {/if}
  </ContextMenu.Item>

  <ContextMenu.Sub>
    <ContextMenu.SubTrigger disabled={!ui.selected && !view.can_paste}>
      <LinkSimple class="text-muted-foreground" />{t('cuemenu.link')}
    </ContextMenu.SubTrigger>
    <ContextMenu.SubContent>
      <ContextMenu.Item disabled={!linkTarget} onSelect={() => linkTarget && linkCues(linkTarget.id)}>
        {t('cuemenu.link_selection', { cue: source ? cueLabel(source) : '–' })}
      </ContextMenu.Item>
      <ContextMenu.Item
        disabled={!ui.selected || source?.type === 'group'}
        onSelect={() => duplicateCue(ui.selected, true)}
      >
        {t('cuemenu.duplicate_linked')}
      </ContextMenu.Item>
      <ContextMenu.Item disabled={!view.can_paste} onSelect={() => pasteCues(true)}>
        {t('cuemenu.paste_linked')}<ContextMenu.Shortcut>{t('key.ctrl')} + {t('key.shift')} + V</ContextMenu.Shortcut>
      </ContextMenu.Item>
      <ContextMenu.Separator />
      <ContextMenu.Item disabled={!selectedCues.some((cue) => cue.link)} onSelect={() => unlinkCues()}>
        {t('cuemenu.unlink')}
      </ContextMenu.Item>
    </ContextMenu.SubContent>
  </ContextMenu.Sub>

  <ContextMenu.Separator />
  <ContextMenu.Item disabled={!cues().length} onSelect={selectEveryCue}>
    <Checks class="text-muted-foreground" />{t('cuemenu.select_all')}<ContextMenu.Shortcut
      >{t('key.ctrl')} + A</ContextMenu.Shortcut
    >
  </ContextMenu.Item>
  <ContextMenu.Item disabled={!cues().length} onSelect={renumber}>
    <ListNumbers class="text-muted-foreground" />{t('cuemenu.renumber')}
  </ContextMenu.Item>

  <ContextMenu.Separator />
  <ContextMenu.Item variant="destructive" disabled={selected.length === 0} onSelect={() => deleteCues()}>
    <Trash />{t('common.delete')}<ContextMenu.Shortcut>{t('key.delete')}</ContextMenu.Shortcut>
  </ContextMenu.Item>
</ContextMenu.Content>
