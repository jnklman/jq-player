<script lang="ts">
  import * as ToggleGroup from '$lib/components/ui/toggle-group';

  import type { OthersEffect } from '../../lib/api';
  import { othersLabels } from '../../lib/cue';
  import type { CueEdit } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { othersOptions } from '../../lib/options';
  import { isTopLevel, ui } from '../../lib/store.svelte';
  import NumberField from '../fields/NumberField.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';
  import FieldGroup from './FieldGroup.svelte';
  import FieldRow from './FieldRow.svelte';

  // What a cue with media does to the other running cues when it starts (exclusive, ducking), or the show's
  // default for that.
  let { edit }: { edit: CueEdit } = $props();

  const view = $derived(ui.view!);
  const single = $derived(edit.multi ? null : edit.cues[0]);
  const topLevel = $derived(edit.cues.every((cue) => isTopLevel(cue.id)));
  const choices = $derived(new Set(edit.cues.map((cue) => cue.others?.effect ?? 'show')));
  const choice = $derived(choices.size === 1 ? [...choices][0] : '');

  function setChoice(next: 'show' | OthersEffect['effect']) {
    edit.set((cue) => ({ others: next === 'show' ? null : { ...(cue.others ?? view.show.others), effect: next } }));
  }

  function setOthers(patch: Partial<OthersEffect>) {
    edit.set((cue) => (cue.others ? { others: { ...cue.others, ...patch } } : {}));
  }
</script>

<FieldGroup title={t('others_group.on_start')}>
  <FieldRow label={t('others_group.other_cues')} own>
    <SimpleSelect
      aria-label={t('others_group.other_running')}
      value={choice as 'show' | OthersEffect['effect']}
      placeholder={t('field.mixed')}
      options={[
        {
          value: 'show' as const,
          label: topLevel
            ? t('others_group.show_default', { effect: othersLabels[view.show.others.effect] })
            : t('others_group.show_default_group'),
        },
        ...othersOptions,
      ]}
      onchange={setChoice}
    />
    <span class="text-xs text-muted-foreground">{t('others_group.hint')}</span>
  </FieldRow>
  {#if single?.others && single.others.effect !== 'none'}
    {@const others = single.others}
    <FieldRow label={t('type.fade')} sub own>
      <NumberField
        class="w-28"
        aria-label={t('type.fade')}
        unit="s"
        min={0}
        value={others.fade}
        onchange={(fade) => setOthers({ fade })}
      />
    </FieldRow>
    <FieldRow label={t('showmenu.scope')} sub own>
      <ToggleGroup.Root
        type="single"
        variant="outline"
        size="sm"
        aria-label={t('showmenu.scope')}
        bind:value={() => others.scope, (scope) => scope && setOthers({ scope: scope as OthersEffect['scope'] })}
      >
        <ToggleGroup.Item value="all">{t('options.others_scope.all')}</ToggleGroup.Item>
        <ToggleGroup.Item value="output">{t('options.others_scope.output')}</ToggleGroup.Item>
      </ToggleGroup.Root>
    </FieldRow>
    {#if others.effect === 'duck'}
      <FieldRow label={t('others_group.duck')} sub own>
        <NumberField
          class="w-28"
          aria-label={t('others_group.duck')}
          unit="dB"
          step={1}
          min={-60}
          max={0}
          value={others.duck_db}
          onchange={(duck_db) => setOthers({ duck_db })}
        />
      </FieldRow>
    {/if}
  {/if}
</FieldGroup>
