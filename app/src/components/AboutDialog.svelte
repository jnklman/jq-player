<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';

  import ArrowCircleUp from '~icons/ph/arrow-circle-up-bold';
  import CheckCircle from '~icons/ph/check-circle-bold';
  import CircleNotch from '~icons/ph/circle-notch-bold';
  import Warning from '~icons/ph/warning-bold';
  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';

  import appIcon from '../../assets/app-icon.svg';
  import { api, events, type UpdateInfo } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { dialogEscape } from '../lib/store.svelte';
  import { hint } from '../lib/ui';

  let { onclose }: { onclose: () => void } = $props();

  const SOURCE = 'https://github.com/jnklman/jq-player';

  let version = $state('');
  getVersion().then(
    (v) => (version = v),
    () => {},
  );

  type UpdateState =
    | { kind: 'idle' }
    | { kind: 'checking' }
    | { kind: 'current' }
    | { kind: 'available'; info: UpdateInfo }
    | { kind: 'installing'; percent: number | null }
    | { kind: 'error'; message: string };
  let update = $state<UpdateState>({ kind: 'idle' });

  async function checkUpdate() {
    update = { kind: 'checking' };
    try {
      const info = await api.checkUpdate();
      update = info ? { kind: 'available', info } : { kind: 'current' };
    } catch (e) {
      update = { kind: 'error', message: String(e) };
    }
  }

  async function installUpdate() {
    update = { kind: 'installing', percent: null };
    const unlisten = await events.onUpdateProgress(({ downloaded, total }) => {
      update = { kind: 'installing', percent: total ? Math.min(100, Math.floor((downloaded / total) * 100)) : null };
    });
    try {
      await api.installUpdate();
    } catch (e) {
      update = { kind: 'error', message: String(e) };
    } finally {
      unlisten();
    }
  }

  type Tab = 'gpl' | 'third';
  let tab = $state<Tab>('gpl');
  /** The package whose license text is unfolded ("section/name@version"). */
  let unfolded = $state<string | null>(null);

  // 860 KB of license texts: loaded when the dialog opens, not at startup.
  let licenses = $state.raw<typeof import('../lib/licenses.json') | null>(null);
  import('../lib/licenses.json').then(
    (module) => (licenses = module.default),
    () => {},
  );
  const gpl = $derived(licenses ? licenses.texts[licenses.gpl] : '');
  const count = $derived(licenses?.sections.reduce((n, section) => n + section.packages.length, 0) ?? 0);
</script>

<Dialog.Root
  bind:open={
    () => true,
    (open) => {
      if (!open) onclose();
    }
  }
>
  <Dialog.Content class="h-168 max-w-160" escapeKeydownBehavior={dialogEscape()}>
    <Dialog.Header>
      <Dialog.Title>{t('about.title')}</Dialog.Title>
      <Dialog.Close class={buttonVariants({ variant: 'ghost', size: 'icon-sm' })} aria-label={t('common.close')}>
        <XMark />
      </Dialog.Close>
    </Dialog.Header>
    {#if update.kind !== 'idle'}
      <div
        class="m-2 mb-0 flex shrink-0 items-center gap-3 rounded border px-4 py-2 text-[13px] {update.kind === 'error'
          ? 'border-stop-line bg-panel-2 text-stop-soft'
          : 'border-border bg-panel-2 text-fg-2'}"
        role="status"
      >
        {#if update.kind === 'error'}
          <Warning class="size-4 shrink-0" />
        {:else if update.kind === 'current'}
          <CheckCircle class="size-4 shrink-0 text-go" />
        {:else if update.kind === 'available'}
          <ArrowCircleUp class="size-4 shrink-0 text-primary" />
        {:else}
          <CircleNotch class="size-4 shrink-0 animate-spin text-primary motion-reduce:animate-none" />
        {/if}
        <span class="min-w-0 grow">
          {#if update.kind === 'checking'}
            {t('update.checking')}
          {:else if update.kind === 'current'}
            {t('update.current')}
          {:else if update.kind === 'available'}
            {t('update.available', { version: update.info.version })}
          {:else if update.kind === 'installing'}
            {update.percent === null ? t('update.installing') : t('update.downloading', { percent: update.percent })}
          {:else if update.kind === 'error'}
            {update.message}
          {/if}
        </span>
        {#if update.kind === 'available'}
          <Button onclick={installUpdate}>{t('update.install')}</Button>
        {/if}
      </div>
    {/if}
    <div class="flex shrink-0 flex-col gap-3 border-b border-border p-4">
      <div class="flex items-center gap-4">
        <img src={appIcon} alt="" class="size-16" />
        <div class="flex min-w-0 flex-col gap-1">
          <div class="flex items-baseline gap-2 leading-none">
            <span class="text-[28px] font-extrabold tracking-tight">jQ</span>
            <span class="text-xl text-fg-2">Player</span>
            {#if version}
              <span class="ml-1 font-mono text-xs text-muted-foreground">{t('about.version', { version })}</span>
            {/if}
          </div>
          <span class="text-[13px] text-fg-2">{t('about.tagline')}</span>
          <span class={hint}>© 2026 Julius Henkelmann</span>
        </div>
        <Button
          class="ml-auto shrink-0 self-start"
          variant="outline"
          disabled={update.kind === 'checking' || update.kind === 'installing'}
          onclick={checkUpdate}
        >
          {update.kind === 'checking' ? t('update.checking') : t('update.check')}
        </Button>
      </div>
      <p class="m-0 text-[13px] leading-normal text-fg-2">
        {t('about.license_text')}
        {t('about.source')} <span class="font-mono select-text">{SOURCE}</span>.
      </p>
      <div class="flex gap-1.5">
        <Button variant={tab === 'gpl' ? 'default' : 'ghost'} onclick={() => (tab = 'gpl')}>GNU GPL v3</Button>
        <Button variant={tab === 'third' ? 'default' : 'ghost'} onclick={() => (tab = 'third')}>
          {t('about.libraries', { count })}
        </Button>
      </div>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto">
      {#if tab === 'gpl'}
        <pre class="m-0 p-4 font-mono text-[11px] leading-snug whitespace-pre-wrap text-fg-2 select-text">{gpl}</pre>
      {:else}
        <p class="{hint} m-0 px-4 pt-3">
          {t('about.libraries_text')}
        </p>
        {#each licenses?.sections ?? [] as section (section.id)}
          <h3 class="m-0 px-4 pt-4 pb-1.5 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
            {section.title}
          </h3>
          <ul class="m-0 list-none p-0">
            {#each section.packages as pkg (`${pkg.name}@${pkg.version}`)}
              {@const key = `${section.id}/${pkg.name}@${pkg.version}`}
              {@const open = unfolded === key}
              <li class="border-t border-line first:border-t-0">
                <button
                  class="flex w-full items-baseline gap-3 px-4 py-1.5 text-left text-[13px] hover:bg-hover"
                  aria-expanded={open}
                  onclick={() => (unfolded = open ? null : key)}
                >
                  <span class="min-w-0 truncate font-medium">{pkg.name}</span>
                  <span class="shrink-0 font-mono text-[11px] text-muted-foreground">{pkg.version}</span>
                  <span class="ml-auto shrink-0 pl-3 text-xs text-muted-foreground"
                    >{pkg.license ?? t('about.unknown_license')}</span
                  >
                </button>
                {#if open}
                  <div class="flex flex-col gap-2 bg-background/40 px-4 py-2">
                    {#if 'repository' in pkg}
                      <span class="font-mono text-[11px] break-all text-muted-foreground select-text">
                        {pkg.repository}
                      </span>
                    {/if}
                    {#if 'note' in pkg}<span class={hint}>{pkg.note}</span>{/if}
                    {#each pkg.texts as text (text)}
                      <pre
                        class="m-0 font-mono text-[11px] leading-snug whitespace-pre-wrap text-fg-2 select-text">{licenses
                          ?.texts[text]}</pre>
                    {/each}
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        {/each}
      {/if}
    </div>
    <Dialog.Footer>
      <Button onclick={onclose}>{t('common.close')}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
