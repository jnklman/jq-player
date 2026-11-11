<script lang="ts">
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Label } from '$lib/components/ui/label';

  import type { EscapeMode, LocalSettings, OpenLast, WheelMode } from '../../lib/api';
  import { AUTO, LANGUAGES, options, t } from '../../lib/i18n/index.svelte';
  import { settingsView } from '../../lib/settingsView.svelte';
  import { check, hint } from '../../lib/ui';
  import SimpleSelect from '../SimpleSelect.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // How the program starts, reacts to the mouse and Esc and warns about a lost focus.
  let { draft = $bindable() }: { draft: LocalSettings } = $props();

  const openLastOptions = options<OpenLast>({
    off: 'ui_tab.open_off',
    last: 'ui_tab.open_last',
    ask: 'ui_tab.open_ask',
  });
  const wheelOptions = options<WheelMode>({
    scroll: 'ui_tab.wheel_scroll',
    selection: 'ui_tab.wheel_selection',
    playhead: 'ui_tab.wheel_playhead',
  });
  const escapeOptions = options<EscapeMode>({
    stop_all: 'ui_tab.escape_panic',
    close_first: 'ui_tab.escape_close',
  });
  // "Automatic" follows the system language; each language is named in itself, so it can be found in any.
  const languageOptions = $derived([
    { value: AUTO, label: t('ui_tab.language_auto') },
    ...LANGUAGES.map((language) => ({ value: language.code as string, label: language.name })),
  ]);

  const settings = settingsView();
</script>

<SettingsPanel tab="ui">
  <SettingGroup>
    <SettingRow label={t('ui_tab.language')} keywords={t('ui_tab.language_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('ui_tab.language')}
        bind:value={draft.language}
        options={languageOptions}
      />
      <span class={hint}>{t('ui_tab.language_hint')}</span>
    </SettingRow>
    <SettingRow label={t('ui_tab.startup')} keywords={t('ui_tab.startup_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('ui_tab.startup')}
        bind:value={draft.open_last}
        options={openLastOptions}
      />
    </SettingRow>
    <SettingRow label={t('ui_tab.wheel')} keywords={t('ui_tab.wheel_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('ui_tab.wheel')}
        bind:value={draft.wheel}
        options={wheelOptions}
      />
    </SettingRow>
    <SettingRow label={t('ui_tab.escape')} keywords={t('ui_tab.escape_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('ui_tab.escape')}
        bind:value={draft.escape}
        options={escapeOptions}
      />
      <span class={hint}>
        {t('ui_tab.escape_hint')}
        <button class="cursor-pointer text-primary hover:underline" onclick={() => settings.open('keys')}
          >{t('settings.tab.keys')}</button
        >.
      </span>
    </SettingRow>
    <SettingRow label={t('ui_tab.focus')} keywords={t('ui_tab.focus_keywords')}>
      <Label class="{check} pt-1.5">
        <Checkbox bind:checked={draft.focus_warning} />
        {t('ui_tab.focus_warning')}
      </Label>
      <span class={hint}>{t('ui_tab.focus_hint')}</span>
    </SettingRow>
    <SettingRow label={t('ui_tab.updates')} keywords={t('ui_tab.updates_keywords')}>
      <Label class="{check} pt-1.5">
        <Checkbox bind:checked={draft.check_updates} />
        {t('ui_tab.check_updates')}
      </Label>
      <span class={hint}>{t('ui_tab.updates_hint')}</span>
    </SettingRow>
  </SettingGroup>
</SettingsPanel>
