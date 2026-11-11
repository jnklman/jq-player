<script lang="ts">
  import { Label } from '$lib/components/ui/label';
  import { Switch } from '$lib/components/ui/switch';

  import { curveOptions } from '../../lib/cue';
  import { type CueEdit, isMixed, shown } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import NumberField from '../fields/NumberField.svelte';
  import FieldRow from './FieldRow.svelte';
  import NumberRow from './fields/NumberRow.svelte';
  import SelectRow from './fields/SelectRow.svelte';

  // Fade-in, fade-out (with the end fade of sound cues) and curve of sound and playlist cues.
  let { edit, fadeInHint }: { edit: CueEdit; fadeInHint?: string } = $props();

  const fadeOut = $derived(edit.value<number>('fade_out'));
  const endFade = $derived(edit.value<boolean>('end_fade'));
  const allSound = $derived(edit.cues.every((cue) => cue.type === 'sound'));
</script>

<NumberRow {edit} key="fade_in" label={t('check.field.fade_in')} unit="s" min={0}>
  {#if fadeInHint}<span class="text-xs text-muted-foreground">{fadeInHint}</span>{/if}
</NumberRow>
<FieldRow label={t('check.field.fade_out')}>
  <span class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
    <NumberField
      class="w-28"
      aria-label={t('check.field.fade_out')}
      unit="s"
      min={0}
      optional={isMixed(fadeOut)}
      placeholder={isMixed(fadeOut) ? t('field.mixed') : ''}
      value={shown(fadeOut)}
      onchange={(fade_out) => fade_out !== null && edit.set({ fade_out })}
    />
    {#if allSound}
      <Label class="font-normal text-fg-2">
        <Switch
          aria-label={t('fade.end_fade')}
          checked={endFade === true}
          onCheckedChange={(end_fade) => edit.set({ end_fade })}
        />
        {t('fade.fade_before_end')}
        {#if isMixed(endFade)}<span class="text-xs text-muted-foreground">({t('field.mixed')})</span>{/if}
      </Label>
    {/if}
  </span>
</FieldRow>
<SelectRow {edit} key="curve" label={t('field.curve')} options={curveOptions} />
