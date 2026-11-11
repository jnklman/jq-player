<script lang="ts">
  import { Button } from '$lib/components/ui/button';

  import { api, type LocalSettings, type WatchMode } from '../../lib/api';
  import { fmtMegabytes } from '../../lib/format';
  import { options, t } from '../../lib/i18n/index.svelte';
  import { attempt } from '../../lib/store.svelte';
  import { bar, hint } from '../../lib/ui';
  import SimpleSelect from '../SimpleSelect.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsBadge from './SettingsBadge.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // How the media files are watched for changes, and the waveform peak cache of this machine. The cache applies at
  // once, not on "Apply": nothing of the draft is involved.
  let { draft = $bindable() }: { draft: LocalSettings } = $props();

  const watchOptions = options<WatchMode>({ events: 'media_tab.events', poll: 'media_tab.poll' });

  const LIMIT = 500 * 1024 * 1024;
  let size = $state<number | null>(null);
  let clearing = $state(false);
  attempt(api.peakCacheSize()).then((bytes) => (size = bytes ?? null));

  async function clear() {
    clearing = true;
    const left = await attempt(api.clearPeakCache());
    if (left !== undefined) size = left;
    clearing = false;
  }
</script>

<SettingsPanel tab="media">
  <SettingGroup title={t('media_tab.changes')}>
    <SettingRow label={t('media_tab.watch')} keywords={t('media_tab.watch_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('media_tab.watch')}
        bind:value={draft.media_watch}
        options={watchOptions}
      />
      <span class={hint}>{t('media_tab.watch_hint')}</span>
    </SettingRow>
  </SettingGroup>

  <SettingGroup title={t('media_tab.waveforms')}>
    <SettingRow label={t('media_tab.peak_cache')} keywords={t('media_tab.peak_keywords')}>
      {#snippet badge()}<SettingsBadge kind="now" />{/snippet}
      <span class="pt-1.5 text-xs text-fg-2">
        {t('media_tab.cache_size', { size: size === null ? '…' : fmtMegabytes(size) })}
      </span>
      <span class="{bar} w-full max-w-90">
        <span class="block h-full w-full origin-left bg-primary" style:transform="scaleX({(size ?? 0) / LIMIT})"></span>
      </span>
      <div><Button size="sm" disabled={clearing || size === 0} onclick={clear}>{t('media_tab.clear')}</Button></div>
      <span class={hint}>{t('media_tab.cache_hint')}</span>
    </SettingRow>
  </SettingGroup>
</SettingsPanel>
