<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { ScrollArea } from '$lib/components/ui/scroll-area';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { Output } from '../lib/api';
  import { edit } from '../lib/commands';
  import { t } from '../lib/i18n/index.svelte';
  import { mode, ui } from '../lib/store.svelte';
  import { field, hint } from '../lib/ui';
  import EditDialog from './EditDialog.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const locked = $derived(mode() === 'show');
  // Edited as a whole and saved as one undo step.
  const initial = JSON.stringify(ui.view!.show.outputs);
  let outputs = $state<Output[]>(JSON.parse(initial));
  const dirty = $derived(JSON.stringify(outputs) !== initial);
  const valid = $derived(outputs.every((output) => output.name.trim() !== ''));

  function add() {
    outputs.push({
      id: crypto.randomUUID().slice(0, 8),
      name: t('outputs.name_numbered', { number: outputs.length + 1 }),
    });
  }
</script>

<EditDialog
  title={t('outputs.title')}
  class="max-w-110"
  {dirty}
  {valid}
  invalidTip={t('outputs.invalid_tip')}
  {locked}
  onapply={async () => !!(await edit({ op: 'set_outputs', outputs: $state.snapshot(outputs) }))}
  {onclose}
>
  <ScrollArea
    ><fieldset class="m-0 flex min-w-0 flex-col gap-3 border-0 p-4" disabled={locked}>
      <p class="{hint} m-0">{t('outputs.text')}</p>
      {#each outputs as output, i (output.id)}
        <div class="grid grid-cols-[1fr_auto] items-end gap-2">
          <Label class={field}>{t('outputs.name')}<Input bind:value={output.name} /></Label>
          <!-- The tooltip explains the disabled button, so it sits on a wrapper. -->
          <Tooltip tip={output.id === 'main' ? t('outputs.main_stays') : undefined}>
            {#snippet child({ props })}
              <span {...props} class="inline-flex">
                <Button
                  variant="ghost"
                  size="icon-sm"
                  class="text-base"
                  aria-label={t('outputs.remove', { name: output.name })}
                  disabled={output.id === 'main'}
                  onclick={() => outputs.splice(i, 1)}><XMark /></Button
                >
              </span>
            {/snippet}
          </Tooltip>
        </div>
      {/each}
      <Button class="self-start" onclick={add}>{t('outputs.add')}</Button>
    </fieldset></ScrollArea
  >
</EditDialog>
