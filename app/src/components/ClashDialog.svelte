<script lang="ts">
  import * as AlertDialog from '$lib/components/ui/alert-dialog';
  import { Button } from '$lib/components/ui/button';
  import { ScrollArea } from '$lib/components/ui/scroll-area';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { ImportClash, OnClash } from '../lib/api';
  import { labels, t } from '../lib/i18n/index.svelte';
  import type { ClashQuestion } from '../lib/store.svelte';
  import { hint } from '../lib/ui';
  import SimpleSelect from './SimpleSelect.svelte';

  let { question }: { question: ClashQuestion } = $props();

  // Renaming keeps both files, so it is the default.
  let choices = $state<Record<string, OnClash>>({});
  const choiceOf = (clash: ImportClash): OnClash => choices[clash.path] ?? 'rename';
  let accept = $state<HTMLElement | null>(null);

  const choiceLabels = labels<OnClash>({
    rename: 'clash.rename',
    use_existing: 'clash.use_existing',
    skip: 'clash.skip',
  });

  /** Applies `choice` to every file it makes sense for. */
  function setAll(choice: OnClash) {
    for (const clash of question.clashes) {
      if (choice !== 'use_existing' || !clash.picked) choices[clash.path] = choice;
    }
  }

  function split(path: string): [string, string] {
    const at = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    return [path.slice(at + 1), path.slice(0, at)];
  }

  function done() {
    question.answer(Object.fromEntries(question.clashes.map((clash) => [clash.path, choiceOf(clash)])));
  }
</script>

<AlertDialog.Root open>
  <AlertDialog.Content
    class="max-w-160"
    escapeKeydownBehavior="ignore"
    onOpenAutoFocus={(e) => {
      e.preventDefault();
      accept?.focus();
    }}
  >
    <AlertDialog.Header>
      <AlertDialog.Title>{t('clash.title')}</AlertDialog.Title>
    </AlertDialog.Header>
    <AlertDialog.Description class="pb-0">
      {t('clash.text', { count: question.clashes.length })}
    </AlertDialog.Description>
    {#if question.clashes.length > 1}
      <div class="flex flex-wrap items-center gap-2 px-4 pt-3">
        <span class={hint}>{t('clash.for_all')}</span>
        {#each Object.entries(choiceLabels) as [choice, label] (choice)}
          <Button onclick={() => setAll(choice as OnClash)}>{label}</Button>
        {/each}
      </div>
    {/if}
    <ScrollArea>
      <ul class="m-0 flex list-none flex-col gap-3 p-4">
        {#each question.clashes as clash (clash.path)}
          {@const [name, dir] = split(clash.path)}
          <li class="flex items-center gap-3">
            <div class="flex min-w-0 grow flex-col gap-0.5">
              <Tooltip tip={clash.path}>
                {#snippet child({ props })}
                  <span {...props} class="truncate text-[13px] text-fg">{name}</span>
                {/snippet}
              </Tooltip>
              <Tooltip tip={dir}>
                {#snippet child({ props })}
                  <span {...props} class="{hint} truncate">
                    {clash.picked ? t('clash.same_picked') : t('clash.existing', { name: clash.existing })}
                  </span>
                {/snippet}
              </Tooltip>
            </div>
            <SimpleSelect
              class="w-56 shrink-0"
              aria-label={t('clash.action_for', { name })}
              value={choiceOf(clash)}
              options={Object.entries(choiceLabels)
                .filter(([choice]) => choice !== 'use_existing' || !clash.picked)
                .map(([choice, label]) => ({ value: choice as OnClash, label }))}
              onchange={(choice) => (choices[clash.path] = choice)}
            />
          </li>
        {/each}
      </ul>
    </ScrollArea>
    <AlertDialog.Footer>
      <Button onclick={() => question.answer(null)}>{t('common.cancel')}</Button>
      <Button variant="default" bind:ref={accept} onclick={done}>{t('common.apply')}</Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
