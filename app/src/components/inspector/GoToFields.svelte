<script lang="ts">
  import * as ToggleGroup from '$lib/components/ui/toggle-group';

  import type { CueInfo, GoToCue } from '../../lib/api';
  import type { CueEdit } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { cueById, cues } from '../../lib/store.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';
  import FieldGroup from './FieldGroup.svelte';
  import FieldRow from './FieldRow.svelte';

  // Target and jump of a go-to cue.
  let { cue, info, edit }: { cue: GoToCue; info: CueInfo | undefined; edit: CueEdit } = $props();

  // Targets: top-level cues (a group as a whole), not the go-to cue itself.
  const targets = $derived(cues().filter((other) => other.id !== cue.id));
  // A target that is no longer valid: gone, or inside a group (only top-level cues are).
  const invalid = $derived.by(() => {
    if (!cue.target || targets.some((other) => other.id === cue.target)) return [];
    const nested = cueById(cue.target);
    const label = nested ? t('goto_fields.in_group', { number: nested.number }) : t('goto_fields.target_missing');
    return [{ value: cue.target, label }];
  });
</script>

<FieldGroup title={t('inspector.tab.goto')}>
  <FieldRow label={t('goto_fields.target_cue')} field="target">
    <SimpleSelect
      aria-label={t('goto_fields.target_cue')}
      value={cue.target ?? ''}
      options={[
        { value: '', label: t('goto_fields.no_target') },
        ...targets.map((other) => ({ value: other.id, label: `${other.number} · ${other.title}` })),
        ...invalid,
      ]}
      onchange={(target) => edit.set({ target: target || null })}
    />
    {#if info?.target_missing}
      <span class="text-xs text-stop-soft">{t('goto_fields.target_gone')}</span>
    {/if}
  </FieldRow>
  <FieldRow label={t('goto_fields.then')}>
    <ToggleGroup.Root
      type="single"
      variant="outline"
      size="sm"
      aria-label={t('effect.jump')}
      bind:value={() => (cue.start ? 'start' : 'select'), (jump) => jump && edit.set({ start: jump === 'start' })}
    >
      <ToggleGroup.Item value="select">{t('goto_fields.select_only')}</ToggleGroup.Item>
      <ToggleGroup.Item value="start">{t('goto_fields.select_start')}</ToggleGroup.Item>
    </ToggleGroup.Root>
    <span class="text-xs text-muted-foreground">
      {cue.start ? t('goto_fields.like_go') : t('goto_fields.playhead_on_target')}
    </span>
  </FieldRow>
</FieldGroup>
