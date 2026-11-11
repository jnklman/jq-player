<script lang="ts">
  import ArrowClockwise from '~icons/ph/arrow-clockwise-bold';
  import ArrowSquareOut from '~icons/ph/arrow-square-out-bold';
  import FolderOpen from '~icons/ph/folder-open-bold';
  import { Button } from '$lib/components/ui/button';
  import * as Popover from '$lib/components/ui/popover';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { chooseMediaFolder, missingMediaFiles, relinkMissing, rescanMedia } from '../lib/media';
  import { attempt, mode, ui } from '../lib/store.svelte';

  const missing = $derived(ui.view ? missingMediaFiles(ui.view).length : 0);

  // Copies into the media folder, overall.
  const copying = $derived.by(() => {
    const imports = ui.runtime?.imports ?? [];
    if (imports.length === 0) return null;
    const progress = imports.reduce((sum, copy) => sum + copy.progress, 0) / imports.length;
    return { count: imports.length, percent: Math.round(progress * 100) };
  });

  let mediaMenuOpen = $state(false);
  const canChangeMediaFolder = $derived(
    mode() === 'edit' &&
      !ui.view?.recovery &&
      !ui.runtime?.searching &&
      !ui.runtime?.imports.length &&
      !ui.runtime?.active.length &&
      !ui.runtime?.waiting.length &&
      !ui.runtime?.gaps.length,
  );

  async function chooseFolder() {
    if (ui.view && (await chooseMediaFolder(ui.view))) mediaMenuOpen = false;
  }

  function scanMedia() {
    mediaMenuOpen = false;
    rescanMedia();
  }

  async function openMediaFolder() {
    if (await attempt(api.openMediaDir())) mediaMenuOpen = false;
  }
</script>

<footer
  class="flex flex-wrap items-center justify-end gap-4 border-t border-line bg-panel px-5 py-2.5 text-xs text-muted-foreground"
>
  {#if copying}
    <span>
      {t('footer.copying', { count: copying.count })}
      {copying.percent} %
    </span>
  {/if}
  {#if ui.runtime?.searching}<span>{t('footer.searching')}</span>{/if}
  <span class={missing > 0 || !ui.view?.media_dir_exists ? 'text-stop-soft' : ''}>
    {t('footer.media_folder')}
    <Popover.Root bind:open={mediaMenuOpen}>
      <Tooltip tip={t('footer.media_tip')}>
        {#snippet child({ props: tipProps })}
          <Popover.Trigger {...tipProps}>
            {#snippet child({ props })}
              <button
                {...props}
                class="rounded-sm underline decoration-dotted underline-offset-2 hover:text-fg"
                aria-label={t('footer.media_folder_named', { name: ui.view?.show.media_root ?? '' })}
              >
                {ui.view?.show.media_root}
              </button>
            {/snippet}
          </Popover.Trigger>
        {/snippet}
      </Tooltip>
      <Popover.Content align="end" side="top" class="flex w-56 flex-col gap-1 p-2" aria-label={t('footer.media_label')}>
        <Button variant="ghost" class="justify-start" disabled={!canChangeMediaFolder} onclick={chooseFolder}>
          <FolderOpen class="size-4" />
          {t('footer.choose_folder')}
        </Button>
        <Button variant="ghost" class="justify-start" disabled={!ui.view?.media_dir_exists} onclick={openMediaFolder}>
          <ArrowSquareOut class="size-4" />
          {t('footer.open_in_manager')}
        </Button>
        <Button variant="ghost" class="justify-start" onclick={scanMedia}>
          <ArrowClockwise class="size-4" />
          {t('footer.rescan')}
        </Button>
      </Popover.Content>
    </Popover.Root>
    · {#if ui.view && !ui.view.media_dir_exists}
      {t('footer.folder_not_found')}
    {:else if missing}
      {t('footer.files_missing', { count: missing })}
    {:else}
      {t('footer.all_found')}
    {/if}
  </span>
  {#if missing > 0 && mode() === 'edit' && !ui.runtime?.searching}
    <Button variant="link" class="min-h-0 border-0 p-0 text-xs" onclick={relinkMissing}>{t('footer.relink')}</Button>
  {/if}
</footer>
