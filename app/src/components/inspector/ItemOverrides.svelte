<script lang="ts">
  import SlidersHorizontal from '~icons/ph/sliders-horizontal-bold';
  import { Button } from '$lib/components/ui/button';
  import * as Popover from '$lib/components/ui/popover';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { PlaylistCue, TimingWarning } from '../../lib/api';
  import { hasOverrides, type ItemPatch, withItem } from '../../lib/cue';
  import type { CueEdit } from '../../lib/cueEdit';
  import { fmtNumber, options, t } from '../../lib/i18n/index.svelte';
  import { RATE_MAX, RATE_MIN, speedOf, TRANSPOSE_MAX } from '../../lib/speed';
  import { field } from '../../lib/ui';
  import NumberField from '../fields/NumberField.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';

  // The settings one playlist item plays with instead of the playlist's: a button by the item, its fields in a
  // popover. An empty field plays the playlist's value (shown as the placeholder).
  let { edit, cue, index, timing }: { edit: CueEdit; cue: PlaylistCue; index: number; timing: TimingWarning[] } =
    $props();

  const own = $derived(cue.items[index]);
  const file = $derived(own.file);
  const changed = $derived(hasOverrides(own));
  const speed = $derived(speedOf(cue));

  function set(patch: ItemPatch | null) {
    edit.set({ items: withItem(cue, index, patch) });
  }

  const pitchOptions = options<string>({
    '': 'overrides.like_playlist',
    keep: 'overrides.pitch_keep',
    follow: 'overrides.pitch_follow',
  });
  const pitch = $derived(own.preserve_pitch == null ? '' : own.preserve_pitch ? 'keep' : 'follow');
  const num = (value: number) => fmtNumber(value);
</script>

<Popover.Root>
  <Tooltip
    tip={timing.length > 0
      ? timing.map((warning) => warning.message).join(' · ')
      : changed
        ? t('overrides.own_settings')
        : t('overrides.settings_only')}
  >
    {#snippet child({ props: tipProps })}
      <Popover.Trigger {...tipProps}>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="ghost"
            size="icon-sm"
            class={timing.length > 0 ? 'text-stop-soft' : changed ? 'text-primary' : undefined}
            aria-label={t('overrides.settings_of', { number: index + 1 })}
            disabled={edit.locked}><SlidersHorizontal /></Button
          >
        {/snippet}
      </Popover.Trigger>
    {/snippet}
  </Tooltip>
  <Popover.Content align="end" class="flex w-72 flex-col gap-3">
    <div class="flex flex-col gap-0.5">
      <span class="truncate text-[13px] font-semibold"
        >{t('overrides.item', { number: index + 1, file: file.split(/[\\/]/).at(-1) ?? '' })}</span
      >
      <span class="text-xs text-muted-foreground">{t('overrides.empty_hint')}</span>
    </div>
    <div class="grid grid-cols-2 gap-x-3 gap-y-2">
      <NumberField
        label={t('check.field.start')}
        unit="s"
        step={0.1}
        min={0}
        optional
        placeholder={t('overrides.file_start')}
        value={own.start || null}
        onchange={(start) => set({ start })}
      />
      <NumberField
        label={t('overrides.end')}
        unit="s"
        step={0.1}
        min={0}
        optional
        placeholder={t('overrides.file_end')}
        value={own.end}
        onchange={(end) => set({ end })}
      />
      {#each timing as warning (warning.message)}
        <span class="col-span-2 text-xs text-stop-soft">{warning.message}</span>
      {/each}
      <NumberField
        label={t('control.volume')}
        unit="dB"
        step={0.5}
        min={-60}
        max={24}
        optional
        placeholder="±0"
        tip={t('overrides.volume_tip')}
        value={own.gain_db || null}
        onchange={(gain_db) => set({ gain_db })}
      />
      <NumberField
        label={t('control.pan')}
        step={0.1}
        min={-1}
        max={1}
        optional
        placeholder={num(cue.pan)}
        value={own.pan}
        onchange={(pan) => set({ pan })}
      />
      <NumberField
        label={t('control.rate')}
        unit="%"
        step={1}
        min={RATE_MIN}
        max={RATE_MAX}
        optional
        placeholder={num(speed.rate)}
        value={own.rate}
        onchange={(rate) => set({ rate })}
      />
      <NumberField
        label={t('effect.transposition')}
        unit={t('speed.semitones')}
        step={0.5}
        min={-TRANSPOSE_MAX}
        max={TRANSPOSE_MAX}
        optional
        placeholder={num(speed.transpose)}
        value={own.transpose}
        onchange={(transpose) => set({ transpose })}
      />
      <NumberField
        label={t('check.field.fade_in')}
        unit="s"
        step={0.5}
        min={0}
        optional
        placeholder={num(cue.fade_in)}
        value={own.fade_in}
        onchange={(fade_in) => set({ fade_in })}
      />
      <NumberField
        label={t('check.field.fade_out')}
        unit="s"
        step={0.5}
        min={0}
        optional
        placeholder={num(cue.fade_out)}
        value={own.fade_out}
        onchange={(fade_out) => set({ fade_out })}
      />
      <label class={[field, 'col-span-2']}>
        {t('overrides.pitch')}
        <SimpleSelect
          value={pitch}
          options={pitchOptions}
          aria-label={t('overrides.pitch')}
          onchange={(value) => set({ preserve_pitch: value === '' ? null : value === 'keep' })}
        />
      </label>
    </div>
    <Button size="sm" variant="outline" class="self-end" disabled={!changed} onclick={() => set(null)}
      >{t('overrides.like_playlist')}</Button
    >
  </Popover.Content>
</Popover.Root>
