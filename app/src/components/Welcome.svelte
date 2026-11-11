<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  import appIcon from '../../assets/app-icon.svg';
  import { api, SHOW_EXTENSION } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { newShow, openShow, openShowFile } from '../lib/shows';
  import { ask, attempt, ui } from '../lib/store.svelte';
  import { check, dragFill, field, hint, panel } from '../lib/ui';
  import WindowControls from './WindowControls.svelte';

  let name = $state('');
  let createFolder = $state(true);
  const canCreate = $derived(name.trim() !== '' && !ui.openingShow);
  const intro = $derived(t('welcome.intro').split('{ext}'));

  function create() {
    if (canCreate) newShow(name.trim(), createFolder);
  }

  // On startup the backend may offer the last show ("ask", or after it
  // crashed the app on the last start); it does so only once.
  attempt(api.startupShow()).then(async (path) => {
    if (!path) return;
    const choice = await ask(t('welcome.open_last'), path, [
      { label: t('welcome.open'), value: 'open', focus: true },
      { label: t('welcome.not_now'), value: 'skip' },
    ]);
    if (choice === 'open') openShowFile(path);
  });
</script>

<!-- Title bar stand-in: the window has no system decorations. -->
<div class="fixed inset-x-0 top-0 flex px-5 py-3" data-tauri-drag-region="deep">
  <div class={dragFill} data-tauri-drag-region="deep"></div>
  <WindowControls />
</div>

<div class="grid h-full place-items-center p-4">
  <div class="{panel} flex w-full max-w-115 flex-col gap-4 p-7">
    <div class="flex items-center gap-5">
      <img src={appIcon} alt="" class="size-20" />
      <div class="flex flex-col gap-2">
        <h1 class="m-0 flex items-baseline gap-2.5 leading-none">
          <span class="text-[40px] font-extrabold tracking-[-1.5px]">jQ</span>
          <span class="text-[28px] font-normal tracking-[-0.75px] text-fg-2">Player</span>
        </h1>
        <div class="flex items-center gap-2 text-[13px] text-muted-foreground">
          <span class="size-2 rounded-full bg-go"></span>
          {t('welcome.tagline')}
        </div>
      </div>
    </div>
    <p class="{hint} m-0 leading-normal">
      {intro[0]}<span class="font-mono">.{SHOW_EXTENSION}</span>{intro[1]}
    </p>
    <Button variant="go" class="min-h-11 text-sm font-bold" disabled={ui.openingShow} onclick={openShow}>
      {ui.openingShow ? t('appmenu.opening') : t('appmenu.open')}
    </Button>
    <div class="flex flex-col gap-2 border-t border-line pt-4">
      <Label class={field}>
        {t('shows.new_title')}
        <Input
          bind:value={name}
          onkeydown={(e) => e.key === 'Enter' && create()}
          placeholder={t('welcome.show_name')}
        />
      </Label>
      <Label class={check}>
        <Checkbox bind:checked={createFolder} />
        {t('welcome.own_folder')}
      </Label>
      <Button class="min-h-10" disabled={!canCreate} onclick={create}>{t('welcome.create')}</Button>
    </div>
    <Button variant="link" class="min-h-0 self-center p-0 text-xs" onclick={() => (ui.aboutOpen = true)}>
      {t('welcome.about')}
    </Button>
  </div>
</div>
