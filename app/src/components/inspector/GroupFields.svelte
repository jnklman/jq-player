<script lang="ts">
  import { Button } from '$lib/components/ui/button';

  import { editGroup } from '../../lib/commands';
  import type { CueEdit } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { hint } from '../../lib/ui';
  import EffectRows from './EffectRows.svelte';
  import FieldGroup from './FieldGroup.svelte';
  import FieldRow from './FieldRow.svelte';

  // A group's start mode and, for one group, its cues; the cues are edited in the group dialog.
  let { edit }: { edit: CueEdit } = $props();

  const cue = $derived(!edit.multi && edit.cues[0].type === 'group' ? edit.cues[0] : null);
</script>

<FieldGroup title={t('inspector.tab.group')}>
  <EffectRows {edit} />
  {#if cue}
    <FieldRow label={t('control_main.cues')}>
      <ul class="m-0 flex list-none flex-col gap-0.5 p-0 pt-1.5 text-[13px] text-fg-2">
        {#each cue.cues as child (child.id)}
          <li class="truncate">
            <span class="font-mono text-primary">{child.number}</span>
            {child.title}
          </li>
        {:else}
          <li class={hint}>{t('group_fields.empty')}</li>
        {/each}
      </ul>
      <Button size="sm" class="self-start" onclick={() => editGroup(cue.id)}>
        {t('group_fields.edit', { count: cue.cues.length })}
      </Button>
    </FieldRow>
  {/if}
</FieldGroup>
