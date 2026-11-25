<script lang="ts">
  import Warning from '~icons/ph/warning-bold';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Label } from '$lib/components/ui/label';
  import { Switch } from '$lib/components/ui/switch';

  import { api, type LibrariesView, type LibrarySummary } from '../../lib/api';
  import { errorText } from '../../lib/errors';
  import { t } from '../../lib/i18n/index.svelte';
  import { libraries, localized, pickLibraryFile, setView } from '../../lib/library.svelte';
  import { ask, attempt } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsBadge from './SettingsBadge.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // The cue libraries of this machine: the bundled ones and those installed, to enable, disable, remove and add.
  // Applies at once, apart from the draft; shows never depend on a library (their cues hold all of their data).

  const list = $derived(libraries.view?.libraries ?? []);

  async function change(promise: Promise<LibrariesView>) {
    const view = await attempt(promise);
    if (view) setView(view);
  }

  async function remove(library: LibrarySummary) {
    const name = library.id ? localized(library.name) : library.file;
    const answer = await ask(
      t('library_tab.remove_title'),
      t('library_tab.remove_text', { name }),
      [
        { label: t('common.cancel'), value: 'cancel', focus: true },
        { label: t('common.delete'), value: 'remove', danger: true },
      ],
      'cancel',
    );
    if (answer !== 'remove') return;
    await change(library.id ? api.removeLibrary(library.id) : api.removeLibraryFile(library.file));
  }

  const meta = (library: LibrarySummary) =>
    [
      library.author && t('library.by', { author: library.author }),
      t('library.revision', { revision: library.revision }),
      t('library.preset_count', { count: library.presets }),
      library.languages.join(', ').toUpperCase(),
    ]
      .filter(Boolean)
      .join(' · ');
</script>

<SettingsPanel tab="library" description={t('library_tab.description')}>
  <SettingGroup title={t('library_tab.installed')}>
    {#snippet badge()}<SettingsBadge kind="now" />{/snippet}
    {#each list as library (`${library.source}/${library.file}`)}
      <SettingRow
        label={library.id ? localized(library.name) : library.file}
        keywords="{t('library_tab.keywords')} {library.file} {library.author}"
      >
        {#snippet badge()}
          {#if library.source === 'bundled'}
            <Badge variant="outline" class="tracking-wide uppercase">{t('library_tab.bundled')}</Badge>
          {/if}
        {/snippet}
        {#if library.error}
          <span class="flex items-start gap-2 text-xs text-stop-soft">
            <Warning class="mt-0.5 size-3.5 shrink-0" />{t('library_tab.broken', {
              error: errorText({ kind: 'library_file', path: library.file, error: library.error }),
            })}
          </span>
        {:else}
          {#if localized(library.description)}
            <span class="text-xs leading-normal text-fg-2">{localized(library.description)}</span>
          {/if}
          <span class={hint}>{meta(library)}</span>
          {#if !library.active}
            <span class={hint}>{t('library_tab.superseded')}</span>
          {/if}
        {/if}
        <div class="flex flex-wrap items-center gap-3 pt-0.5">
          {#if library.id && library.active}
            <Label class="font-normal text-fg-2">
              <Switch
                aria-label={t('library_tab.offer')}
                checked={library.enabled}
                onCheckedChange={(on) => change(api.setLibraryEnabled(library.id!, on))}
              />
              {t('library_tab.offer')}
            </Label>
          {/if}
          {#if library.source === 'installed'}
            <Button size="sm" variant="ghost" onclick={() => remove(library)}>{t('library_tab.remove')}</Button>
          {/if}
        </div>
      </SettingRow>
    {/each}
  </SettingGroup>

  <SettingGroup title={t('library_tab.add')}>
    <SettingRow label={t('library_tab.import')} keywords={t('library_tab.import_keywords')}>
      <div class="flex flex-wrap gap-2">
        <Button size="sm" onclick={pickLibraryFile}>{t('library_tab.import_button')}</Button>
        <Button size="sm" variant="ghost" onclick={() => attempt(api.openLibraryFolder())}>
          {t('library_tab.open_folder')}
        </Button>
        <Button size="sm" variant="ghost" onclick={() => change(api.rescanLibraries())}>
          {t('library_tab.rescan')}
        </Button>
      </div>
      <span class={hint}>{t('library_tab.import_hint')}</span>
      {#if libraries.view?.folder}
        <span class="font-mono text-[11px] break-all text-muted-foreground select-text">{libraries.view.folder}</span>
      {/if}
    </SettingRow>
  </SettingGroup>
</SettingsPanel>
