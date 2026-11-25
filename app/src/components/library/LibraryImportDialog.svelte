<script lang="ts">
  import CircleNotch from '~icons/ph/circle-notch-bold';
  import Warning from '~icons/ph/warning-bold';
  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';

  import { api, type LibraryInspection } from '../../lib/api';
  import { cueIcons, libraryIcon } from '../../lib/cueIcons';
  import { errorText } from '../../lib/errors';
  import { t } from '../../lib/i18n/index.svelte';
  import { localized, presetTitle, setView } from '../../lib/library.svelte';
  import { attempt, notify } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';

  // A cue library file opened from outside (double-click) or picked in the settings: what it holds, and the offer to
  // install it (copy it into the libraries folder). Works without a show.
  let { path, onclose }: { path: string; onclose: () => void } = $props();

  let inspection = $state<LibraryInspection | null>(null);
  let error = $state<string | null>(null);
  let installing = $state(false);
  // svelte-ignore state_referenced_locally
  api.inspectLibrary(path).then(
    (found) => (inspection = found),
    (e) => (error = errorText(e)),
  );

  const library = $derived(inspection?.library ?? null);
  const groups = $derived.by(() => {
    if (!library) return [];
    const named = library.categories.map((category) => ({
      id: category.id,
      name: localized(category.name),
      presets: library.presets.filter((p) => p.category === category.id),
    }));
    const other = library.presets.filter((p) => !library.categories.some((c) => c.id === p.category));
    if (other.length) named.push({ id: '', name: t('library.uncategorized'), presets: other });
    return named.filter((group) => group.presets.length);
  });

  async function install() {
    installing = true;
    const view = await attempt(api.installLibrary(path));
    installing = false;
    if (!view) return;
    setView(view);
    notify({ level: 'warning', message: t('library.installed', { name: localized(library?.name) }) });
    onclose();
  }

  const fileName = $derived(path.split(/[\\/]/).pop() ?? path);
</script>

<Dialog.Root
  bind:open={
    () => true,
    (open) => {
      if (!open) onclose();
    }
  }
>
  <Dialog.Content class="h-[min(38rem,calc(100%-2rem))] max-w-160">
    <Dialog.Header>
      <Dialog.Title>{t('library.import_title')}</Dialog.Title>
      <Dialog.Close class={buttonVariants({ variant: 'ghost', size: 'icon-sm' })} aria-label={t('common.close')}>
        <XMark />
      </Dialog.Close>
    </Dialog.Header>
    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto">
      {#if error}
        <div
          class="m-4 flex items-start gap-3 rounded border border-stop-line bg-panel-2 px-4 py-3 text-[13px]"
          role="alert"
        >
          <Warning class="mt-0.5 size-4 shrink-0 text-stop-soft" />
          <div class="flex min-w-0 flex-col gap-1">
            <span class="font-semibold">{t('library.unusable', { file: fileName })}</span>
            <span class="text-fg-2 select-text">{error}</span>
          </div>
        </div>
      {:else if !library || !inspection}
        <div class="flex items-center gap-2 p-4 text-[13px] text-muted-foreground">
          <CircleNotch class="size-4 animate-spin motion-reduce:animate-none" />{t('library.reading')}
        </div>
      {:else}
        {@const Icon = libraryIcon}
        <div class="flex shrink-0 flex-col gap-2 border-b border-line p-4">
          <div class="flex items-center gap-3">
            <Icon class="size-8 shrink-0 text-primary" />
            <div class="flex min-w-0 flex-col">
              <span class="text-base font-semibold">{localized(library.name)}</span>
              <span class={hint}>
                {[
                  library.author && t('library.by', { author: library.author }),
                  t('library.revision', { revision: library.revision }),
                  t('library.preset_count', { count: library.presets.length }),
                  library.devices.length
                    ? library.devices.map((d) => `${localized(d.name)} :${d.port}`).join(', ')
                    : '',
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </div>
          </div>
          {#if localized(library.description)}
            <p class="m-0 text-[13px] leading-normal text-fg-2">{localized(library.description)}</p>
          {/if}
          {#if library.homepage}
            <span class="font-mono text-[11px] break-all text-muted-foreground select-text">{library.homepage}</span>
          {/if}
          <p class="m-0 text-[13px] {inspection.comparison === 'older' ? 'text-stop-soft' : 'text-fg-2'}" role="status">
            {#if inspection.comparison === 'new'}
              {t('library.compare.new')}
            {:else if inspection.comparison === 'newer'}
              {t('library.compare.newer', { installed: inspection.installed ?? 0 })}
            {:else if inspection.comparison === 'same'}
              {t('library.compare.same')}
            {:else}
              {t('library.compare.older', { installed: inspection.installed ?? 0 })}
            {/if}
          </p>
        </div>
        {#each groups as group (group.id)}
          <h4 class="m-0 px-4 pt-3 pb-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
            {group.name}
          </h4>
          <ul class="m-0 list-none px-2 pb-1">
            {#each group.presets as preset (preset.id)}
              {@const PresetIcon = cueIcons[preset.cue.type] ?? libraryIcon}
              <li class="flex items-start gap-2.5 rounded px-2 py-1">
                <PresetIcon class="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <span class="flex min-w-0 flex-col">
                  <span class="text-[13px]">{presetTitle(preset)}</span>
                  {#if localized(preset.description)}
                    <span class="text-[11px] text-muted-foreground">{localized(preset.description)}</span>
                  {/if}
                </span>
              </li>
            {/each}
          </ul>
        {/each}
      {/if}
    </div>
    <Dialog.Footer>
      <span class="{hint} mr-auto truncate">{fileName}</span>
      <Button onclick={onclose}>{t('common.cancel')}</Button>
      <Button variant="default" disabled={!inspection || installing} onclick={install}>
        {!inspection || inspection.comparison === 'new' ? t('library.install') : t('library.replace')}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
