<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import FilePlus from '~icons/ph/file-plus-bold';
  import FloppyDiskBack from '~icons/ph/floppy-disk-back-bold';
  import FloppyDisk from '~icons/ph/floppy-disk-bold';
  import FolderOpen from '~icons/ph/folder-open-bold';
  import Info from '~icons/ph/info-bold';
  import Power from '~icons/ph/power-bold';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import appIcon from '../../assets/app-icon.svg';
  import { t } from '../lib/i18n/index.svelte';
  import { createShow, openShow, saveShowAs } from '../lib/shows';
  import { saveShow, ui } from '../lib/store.svelte';

  // Menu behind the app name in the header: everything about switching shows.
  // Settings of the open show live in the show menu next to it.
</script>

<DropdownMenu.Root>
  <Tooltip tip={t('appmenu.tip')} side="bottom">
    {#snippet child({ props: tipProps })}
      <DropdownMenu.Trigger {...tipProps}>
        {#snippet child({ props })}
          <button
            {...props}
            class="-ml-1.5 flex shrink-0 items-center gap-2.5 rounded-md px-1.5 py-0.5 hover:bg-hover aria-expanded:bg-hover"
            aria-label={t('appmenu.label')}
          >
            <img src={appIcon} alt="" class="size-6" />
            <span class="flex items-baseline gap-1 text-[15px] leading-none">
              <span class="font-extrabold tracking-tight">jQ</span>
              <span class="text-fg-2">Player</span>
            </span>
          </button>
        {/snippet}
      </DropdownMenu.Trigger>
    {/snippet}
  </Tooltip>
  <DropdownMenu.Content align="start" class="min-w-56">
    <DropdownMenu.Item disabled={ui.openingShow} onSelect={createShow}>
      <FilePlus />
      {t('appmenu.new_show')}
      <DropdownMenu.Shortcut>{t('key.ctrl')} + N</DropdownMenu.Shortcut>
    </DropdownMenu.Item>
    <DropdownMenu.Item disabled={ui.openingShow} onSelect={openShow}>
      <FolderOpen />
      {ui.openingShow ? t('appmenu.opening') : t('appmenu.open')}
      <DropdownMenu.Shortcut>{t('key.ctrl')} + O</DropdownMenu.Shortcut>
    </DropdownMenu.Item>
    <DropdownMenu.Separator />
    <DropdownMenu.Item onSelect={saveShow}>
      <FloppyDisk />
      {t('common.save')}
      <DropdownMenu.Shortcut>{t('key.ctrl')} + S</DropdownMenu.Shortcut>
    </DropdownMenu.Item>
    <DropdownMenu.Item disabled={ui.openingShow} onSelect={saveShowAs}>
      <FloppyDiskBack />
      {ui.savingAs ? t('appmenu.saving') : t('appmenu.save_as')}
      <DropdownMenu.Shortcut>{t('key.ctrl')} + {t('key.shift')} + S</DropdownMenu.Shortcut>
    </DropdownMenu.Item>
    <DropdownMenu.Separator />
    <DropdownMenu.Item onSelect={() => (ui.aboutOpen = true)}>
      <Info />
      {t('about.title')}
    </DropdownMenu.Item>
    <DropdownMenu.Item onSelect={async () => await getCurrentWindow().close()}>
      <Power />
      {t('leave.quit')}
    </DropdownMenu.Item>
  </DropdownMenu.Content>
</DropdownMenu.Root>
