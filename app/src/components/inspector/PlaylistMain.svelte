<script lang="ts">
  import CaretDown from '~icons/ph/caret-down-bold';
  import CaretUp from '~icons/ph/caret-up-bold';
  import Swap from '~icons/ph/swap-bold';
  import Warning from '~icons/ph/warning-bold';
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { CueInfo, PlaylistCue } from '../../lib/api';
  import { addToPlaylist, replacePlaylistFile } from '../../lib/commands';
  import type { CueEdit } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { shuffleOptions } from '../../lib/options';
  import FadeRows from './FadeRows.svelte';
  import FieldGroup from './FieldGroup.svelte';
  import FieldRow from './FieldRow.svelte';
  import NumberRow from './fields/NumberRow.svelte';
  import SelectRow from './fields/SelectRow.svelte';
  import SwitchRow from './fields/SwitchRow.svelte';
  import ItemOverrides from './ItemOverrides.svelte';

  // The first tab of playlist cues: the tracks, the gap, repeat and shuffle, the fades. With several playlists all but
  // the tracks.
  let { edit, info }: { edit: CueEdit; info: CueInfo | undefined } = $props();

  const cue = $derived(edit.multi ? null : (edit.cues[0] as PlaylistCue));

  function moveFile(cue: PlaylistCue, i: number, by: number) {
    const to = i + by;
    if (to < 0 || to >= cue.items.length) return;
    const items = [...cue.items];
    [items[i], items[to]] = [items[to], items[i]];
    edit.set({ items });
  }
</script>

{#if cue}
  <FieldGroup title={cue.shuffle === 'off' ? t('playlist.items_in_order') : t('playlist.items_shuffled')}>
    {#snippet aside()}{t('status.items', { count: cue.items.length })}{/snippet}
    <FieldRow field="files">
      {#each cue.items as { file }, i (i)}
        {@const missing = info?.missing_items[i] ?? false}
        <div class="flex items-center gap-1.5 text-[13px]">
          <span class="w-6 shrink-0 text-right font-mono text-muted-foreground">
            {#if missing}
              <Warning class="ml-auto text-stop-soft" aria-label={t('status.file_missing')} />
            {:else}
              {i + 1}
            {/if}
          </span>
          <span class={['flex min-w-0 grow items-center gap-1', missing ? 'text-stop-soft' : 'text-fg-2']}>
            <Tooltip tip={missing ? t('playlist.file_missing', { file }) : file} side="right">
              {#snippet child({ props })}
                <span {...props} class="truncate">{file.split('/').at(-1)}</span>
              {/snippet}
            </Tooltip>
          </span>
          <ItemOverrides {edit} {cue} index={i} timing={info?.item_timing[i] ?? []} />
          <Tooltip tip={t('cuemenu.replace_file')}>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="ghost"
                size="icon-sm"
                aria-label={t('playlist.replace_item', { number: i + 1 })}
                onclick={() => replacePlaylistFile(cue, i)}><Swap /></Button
              >
            {/snippet}
          </Tooltip>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t('playlist.item_up', { number: i + 1 })}
            disabled={i === 0}
            onclick={() => moveFile(cue, i, -1)}><CaretUp /></Button
          >
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t('playlist.item_down', { number: i + 1 })}
            disabled={i === cue.items.length - 1}
            onclick={() => moveFile(cue, i, 1)}><CaretDown /></Button
          >
          <Button
            variant="ghost"
            size="icon-sm"
            class="shrink-0 text-base"
            aria-label={t('playlist.remove_item', { number: i + 1 })}
            onclick={() => edit.set({ items: cue.items.filter((_, j) => j !== i) })}><XMark /></Button
          >
        </div>
      {/each}
      <Button size="sm" class="self-start" onclick={() => addToPlaylist(cue.id)}>{t('playlist.add_item')}</Button>
    </FieldRow>
  </FieldGroup>
{/if}

<FieldGroup title={t('inspector.tab.playback')}>
  <NumberRow {edit} key="gap" label={t('playlist.gap')} unit="s" step={0.5} min={0} />
  <SwitchRow {edit} key="repeat" label={t('playlist.at_end')} text={t('playlist.restart')} />
  <SelectRow {edit} key="shuffle" label={t('playlist.shuffle')} options={shuffleOptions} />
</FieldGroup>

<FieldGroup title={t('playlist.fades')}>
  <FadeRows {edit} fadeInHint={t('playlist.per_item')} />
</FieldGroup>
