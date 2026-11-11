<script lang="ts">
  import { Label } from '$lib/components/ui/label';
  import { Switch } from '$lib/components/ui/switch';
  import * as ToggleGroup from '$lib/components/ui/toggle-group';

  import type { ExitLoop } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { devampModeOptions } from '../../lib/options';
  import SimpleSelect from '../SimpleSelect.svelte';
  import FieldRow from './FieldRow.svelte';

  // How a loop is left: shared by the loop exit cue and the second trigger
  // "Exit loop", so both offer (and do) the same. Three rows.
  let { exit, sub = false, onchange }: { exit: ExitLoop; sub?: boolean; onchange: (exit: ExitLoop) => void } = $props();
</script>

<FieldRow label={t('exitloop.exit')} {sub}>
  <SimpleSelect
    aria-label={t('exitloop.exit')}
    value={exit.mode}
    options={devampModeOptions}
    onchange={(mode) => onchange({ ...exit, mode })}
  />
</FieldRow>
<FieldRow label={t('exitloop.level')} {sub}>
  <ToggleGroup.Root
    type="single"
    variant="outline"
    size="sm"
    aria-label={t('exitloop.level')}
    bind:value={() => exit.level, (level) => level && onchange({ ...exit, level: level as ExitLoop['level'] })}
  >
    <ToggleGroup.Item value="innermost">{t('options.exit_level.innermost')}</ToggleGroup.Item>
    <ToggleGroup.Item value="all">{t('exitloop.all')}</ToggleGroup.Item>
  </ToggleGroup.Root>
</FieldRow>
<FieldRow label={t('exitloop.then')} {sub}>
  <Label class="min-h-8 font-normal text-fg-2">
    <Switch
      aria-label={t('exitloop.stop_at_exit')}
      checked={exit.stop}
      onCheckedChange={(stop) => onchange({ ...exit, stop })}
    />
    {t('exitloop.stop_at_exit')}
  </Label>
  <span class="text-xs text-muted-foreground">
    {exit.stop ? t('exitloop.ends') : t('exitloop.continues')}
  </span>
</FieldRow>
