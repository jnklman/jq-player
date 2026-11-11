<script lang="ts">
  import type { LocalSettings } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { hint } from '../../lib/ui';
  import NumberField from '../fields/NumberField.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // Times and safeguards during the performance.
  let { draft = $bindable() }: { draft: LocalSettings } = $props();

  type Seconds = 'fade_all' | 'panic_fade' | 'go_lockout' | 'end_warning';
</script>

{#snippet seconds(label: string, key: Seconds, step: number)}
  <div class="flex items-center gap-2">
    <NumberField
      class="w-24"
      aria-label="{label} (s)"
      min={0}
      {step}
      value={draft[key]}
      onchange={(value) => (draft[key] = value)}
    />
    <span class="text-xs text-muted-foreground">s</span>
  </div>
{/snippet}

<SettingsPanel tab="show" description={t('show_tab.description')}>
  <SettingGroup title={t('show_tab.fade_panic')}>
    <SettingRow label={t('action.fade_all')} keywords={t('show_tab.fade_keywords')}>
      {@render seconds(t('action.fade_all'), 'fade_all', 0.5)}
    </SettingRow>
    <SettingRow label={t('settings.problem.panic_fade')} keywords={t('show_tab.panic_keywords')}>
      {@render seconds(t('settings.problem.panic_fade'), 'panic_fade', 0.5)}
      <span class={hint}>{t('show_tab.panic_hint')}</span>
    </SettingRow>
  </SettingGroup>

  <SettingGroup title={t('show_tab.protection')}>
    <SettingRow label={t('settings.problem.go_lockout')} keywords={t('show_tab.lockout_keywords')}>
      {@render seconds(t('settings.problem.go_lockout'), 'go_lockout', 0.1)}
      <span class={hint}>{t('show_tab.lockout_hint')}</span>
    </SettingRow>
    <SettingRow label={t('show_tab.warning_time')} keywords={t('show_tab.warning_keywords')}>
      {@render seconds(t('show_tab.warning_time'), 'end_warning', 1)}
      <span class={hint}>{t('show_tab.warning_hint')}</span>
    </SettingRow>
  </SettingGroup>
</SettingsPanel>
