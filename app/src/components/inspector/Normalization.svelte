<script lang="ts">
  import { Button } from '$lib/components/ui/button';

  import type { CueInfo, PlaylistCue, SoundCue } from '../../lib/api';
  import { edit as editShow } from '../../lib/commands';
  import { fmtDb, fmtLufs } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { ui } from '../../lib/store.svelte';
  import FieldRow from './FieldRow.svelte';

  // Loudness of a sound or playlist cue's files and their normalization (one row).
  let {
    cue,
    info,
    onreset,
  }: {
    cue: SoundCue | PlaylistCue;
    info: CueInfo | undefined;
    /** Removes the offsets. */
    onreset: () => void;
  } = $props();

  const view = $derived(ui.view!);
  const loudness = $derived(info?.loudness ?? []);
  const measured = $derived(loudness.filter((file) => file !== null));
  const normalizedItems = $derived(
    cue.type === 'playlist' ? cue.items.filter((item) => item.normalize_db != null).length : 0,
  );
  const normalized = $derived(cue.type === 'sound' ? cue.normalize_db != null : normalizedItems > 0);
</script>

<FieldRow label={t('normalization.loudness')} field="normalize">
  <span class="flex min-h-8 flex-col justify-center text-xs text-fg-2">
    {#if cue.type === 'sound'}
      {@const file = loudness[0]}
      {#if file}
        <span>{fmtLufs(file.lufs)} · {t('normalization.true_peak', { peak: fmtDb(file.peak_db) })}</span>
        {#if cue.normalize_db != null}
          <span class="text-muted-foreground"
            >{t('normalization.offset', {
              offset: fmtDb(cue.normalize_db),
              target: fmtLufs(view.show.loudness_target),
            })}</span
          >
        {:else if file.suggested_db != null}
          <span class="text-muted-foreground">
            {t('normalization.suggestion', {
              offset: fmtDb(file.suggested_db),
              target: fmtLufs(view.show.loudness_target),
            })}
          </span>
        {/if}
      {:else}
        <span class="text-muted-foreground"
          >{info?.measuring ? t('normalization.measuring') : t('normalization.unknown')}</span
        >
      {/if}
    {:else}
      <span
        >{t('normalization.measured', { done: measured.length, count: loudness.length })}{info?.measuring
          ? ' …'
          : ''}</span
      >
      <span class="text-muted-foreground">{t('normalization.normalized', { count: normalizedItems })}</span>
    {/if}
  </span>
  {#if info?.normalize_stale}
    <span class="text-xs text-stop-soft">{t('normalization.stale')}</span>
  {/if}
  <span class="flex flex-wrap gap-1.5">
    <Button size="sm" disabled={measured.length === 0} onclick={() => editShow({ op: 'normalize', ids: [cue.id] })}
      >{normalized ? t('normalization.recalculate') : t('normalization.normalize')}</Button
    >
    <Button size="sm" variant="ghost" disabled={!normalized} onclick={onreset}>{t('normalization.remove')}</Button>
  </span>
</FieldRow>
