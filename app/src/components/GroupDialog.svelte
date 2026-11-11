<script lang="ts">
  import CaretDown from '~icons/ph/caret-down-bold';
  import CaretUp from '~icons/ph/caret-up-bold';
  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import type { CueId } from '../lib/api';
  import { addCue, duplicateCue, edit } from '../lib/commands';
  import { addGroups, type AddItem, mainListOnly, typeLabel } from '../lib/control';
  import { t } from '../lib/i18n/index.svelte';
  import { cueById, dialogEscape, mode } from '../lib/store.svelte';
  import { empty, hint, panel } from '../lib/ui';
  import Inspector from './inspector/Inspector.svelte';

  let { groupId, onclose }: { groupId: CueId; onclose: () => void } = $props();

  const group = $derived.by(() => {
    const cue = cueById(groupId);
    return cue?.type === 'group' ? cue : undefined;
  });
  const children = $derived(group?.cues ?? []);
  const locked = $derived(mode() === 'show');
  // The first cue is open from the start.
  // svelte-ignore state_referenced_locally
  let selected = $state<CueId | null>(group?.cues[0]?.id ?? null);
  const index = $derived(children.findIndex((cue) => cue.id === selected));

  /** New cues go after the selected one and become selected (no groups in a group). */
  async function add(item: AddItem) {
    const at = index >= 0 ? index + 1 : children.length;
    const id = await addCue(item, { index: at, group: groupId });
    if (id) selected = id;
  }

  function move(by: number) {
    if (selected && index + by >= 0 && index + by < children.length) {
      edit({ op: 'move', ids: [selected], to: index + by });
    }
  }

  function remove() {
    if (!selected) return;
    const next = children[index + 1]?.id ?? children[index - 1]?.id ?? null;
    edit({ op: 'delete', ids: [selected] });
    selected = next;
  }

  $effect(() => {
    // A group deleted meanwhile (undo, another edit) closes the dialog.
    if (!group) onclose();
  });
</script>

<!-- Esc closes the dialog only with "close_first" (else it stays a panic), a click outside always. -->
<Dialog.Root open onOpenChange={(open) => !open && onclose()}>
  <Dialog.Content class="h-[min(760px,calc(100%-2rem))] max-w-6xl" escapeKeydownBehavior={dialogEscape()}>
    <Dialog.Header>
      <Dialog.Title>
        {t('group.title', { number: group?.number ?? '' })} · {group?.title || t('group.untitled')}
        <span class="ml-2 font-normal text-muted-foreground">
          {group?.mode === 'all' ? t('status.simultaneous') : t('status.sequential')}
        </span>
      </Dialog.Title>
      <Dialog.Close class={buttonVariants({ variant: 'ghost', size: 'icon-sm' })} aria-label={t('common.close')}
        ><XMark /></Dialog.Close
      >
    </Dialog.Header>
    <div class="flex min-h-0 flex-1 gap-4 p-4">
      <section class="flex w-80 shrink-0 flex-col gap-2" aria-label={t('group.cues_label')}>
        <ScrollArea class="flex-1 rounded-lg border border-line" role="listbox">
          {#each children as cue (cue.id)}
            <button
              type="button"
              role="option"
              aria-selected={selected === cue.id}
              class={[
                'flex w-full items-center gap-2 border-b border-row-line px-3 py-2 text-left text-[13px] focus-visible:-outline-offset-2',
                selected === cue.id ? 'bg-selected' : 'hover:bg-hover',
              ]}
              onclick={() => (selected = cue.id)}
            >
              <span class="w-10 shrink-0 font-mono text-muted-foreground">{cue.number}</span>
              <span class="min-w-0 grow truncate">{cue.title || t('group.untitled')}</span>
              <span class="text-xs text-muted-foreground">{typeLabel(cue)}</span>
            </button>
          {:else}
            <div class={empty}>{t('group.empty')}</div>
          {/each}
        </ScrollArea>
        <fieldset class="m-0 flex flex-col gap-2 border-0 p-0" disabled={locked}>
          <div class="flex gap-1.5">
            <Button aria-label={t('group.move_up')} disabled={index <= 0} onclick={() => move(-1)}><CaretUp /></Button>
            <Button
              aria-label={t('group.move_down')}
              disabled={index < 0 || index >= children.length - 1}
              onclick={() => move(1)}><CaretDown /></Button
            >
            <Button disabled={!selected} onclick={() => duplicateCue(selected)}>{t('group.duplicate')}</Button>
            <Button variant="destructive" disabled={!selected} onclick={remove}>{t('common.delete')}</Button>
          </div>
          <div class="flex flex-wrap gap-1.5">
            <Button onclick={() => add({ label: '', kind: 'sound' })}>{t('group.add_sound')}</Button>
            <Button onclick={() => add({ label: '', kind: 'volume' })}>{t('group.add_volume')}</Button>
            <DropdownMenu.Root>
              <DropdownMenu.Trigger class={buttonVariants()}>
                {t('group.add_more')}
                <CaretDown class="size-3.5 text-muted-foreground" />
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="start">
                {#each addGroups as menuGroup, g (menuGroup.label)}
                  {#if g > 0}<DropdownMenu.Separator />{/if}
                  <DropdownMenu.Group>
                    <DropdownMenu.GroupHeading>{menuGroup.label}</DropdownMenu.GroupHeading>
                    {#each menuGroup.items.filter((item) => !mainListOnly(item)) as item (item.kind)}
                      <DropdownMenu.Item onSelect={() => add(item)}>{item.label}</DropdownMenu.Item>
                    {/each}
                  </DropdownMenu.Group>
                {/each}
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          </div>
        </fieldset>
        {#if group?.mode === 'all'}
          <p class="{hint} m-0">{t('group.hint_all')}</p>
        {:else}
          <p class="{hint} m-0">{t('group.hint_sequence')}</p>
        {/if}
      </section>

      <div class="flex min-w-0 flex-1 flex-col">
        {#if selected && index >= 0}
          <Inspector cueId={selected} />
        {:else}
          <div class="{panel} {empty} flex-1">{t('group.select_left')}</div>
        {/if}
      </div>
    </div>

    <Dialog.Footer>
      <Button variant="default" onclick={onclose}>{t('common.done')}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
