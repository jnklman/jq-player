<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { t } from '../../lib/i18n/index.svelte';
  import { boundKeys } from '../../lib/keys.svelte';
  import { gotoCue, ui } from '../../lib/store.svelte';

  // Jumps to a cue by its number on Enter: the selection or the playhead, as the selection mode has it.
  let number = $state('');
  let missing = $state(false);

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      missing = !gotoCue(number);
      if (!missing) (e.currentTarget as HTMLInputElement).blur();
      else (e.currentTarget as HTMLInputElement).select();
    } else if (e.key === 'Escape') {
      (e.currentTarget as HTMLInputElement).blur();
    }
  }

  function reset() {
    missing = false;
    number = '';
  }
</script>

<Tooltip
  tabindex={0}
  keepOpen={missing}
  keys={missing ? undefined : boundKeys(ui.view?.settings.keys ?? [], 'focus_goto') || undefined}
  tip={missing ? t('goto.missing', { number: number.trim() }) : t('goto.tip')}
>
  {#snippet child({ props })}
    <Input
      {...props}
      data-goto-field
      class="h-8 w-20"
      placeholder={t('goto.placeholder')}
      aria-label={t('goto.tip')}
      aria-invalid={missing}
      bind:value={number}
      oninput={() => (missing = false)}
      onblur={(e) => {
        // The tooltip's own blur closes it (opened by focusing the field with the keyboard).
        (props.onblur as ((e: FocusEvent) => void) | undefined)?.(e);
        reset();
      }}
      {onkeydown}
    />
  {/snippet}
</Tooltip>
