<script lang="ts">
  import type { Snippet } from 'svelte';

  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { t } from '../lib/i18n/index.svelte';
  import { askUnsaved, dialogEscape } from '../lib/store.svelte';

  // A dialog that edits a draft and applies it as a whole: "Cancel"
  // discards it, the apply button sends it. Closing any other way (×, Esc,
  // a click outside) asks about unsaved changes first. Esc closes only with
  // `settings.escape = "close_first"`; else it stays a panic.
  let {
    title,
    class: className = '',
    dirty,
    valid = true,
    invalidTip,
    locked = false,
    applyLabel,
    closeOnApply = true,
    focusDialog = false,
    onapply,
    onclose,
    status,
    children,
  }: {
    title: string | Snippet;
    /** Classes of the dialog box, e.g. its width. */
    class?: string;
    /** The draft differs from what it was opened with. */
    dirty: boolean;
    /** The draft can be applied; else the apply button is disabled with `invalidTip`. */
    valid?: boolean;
    invalidTip?: string;
    /** Show mode: nothing can be applied. */
    locked?: boolean;
    applyLabel?: string;
    /** Applying closes the dialog; else it stays open, with nothing to apply until the next change. */
    closeOnApply?: boolean;
    /** The focus starts on the dialog itself, not on its first button (×). */
    focusDialog?: boolean;
    /** Sends the draft; true when done, which closes the dialog (with `closeOnApply`). */
    onapply: () => Promise<boolean> | boolean;
    onclose: () => void;
    /** Left in the footer, e.g. how much was changed. */
    status?: Snippet;
    children: Snippet;
  } = $props();

  let saving = $state(false);
  let content = $state<HTMLElement | null>(null);

  async function apply() {
    saving = true;
    try {
      if ((await onapply()) && closeOnApply) onclose();
    } finally {
      saving = false;
    }
  }

  async function leave() {
    if (dirty && !locked) {
      const answer = await askUnsaved(valid);
      if (answer === 'apply') return apply();
      if (answer !== 'discard') return;
    }
    onclose();
  }
</script>

<Dialog.Root
  bind:open={
    () => true,
    (open) => {
      if (!open) void leave();
    }
  }
>
  <Dialog.Content
    bind:ref={content}
    class={['outline-none', className]}
    tabindex={focusDialog ? -1 : undefined}
    escapeKeydownBehavior={dialogEscape()}
    onOpenAutoFocus={(e) => {
      if (!focusDialog) return;
      e.preventDefault();
      content?.focus();
    }}
  >
    <Dialog.Header>
      <Dialog.Title>
        {#if typeof title === 'string'}{title}{:else}{@render title()}{/if}
      </Dialog.Title>
      <Dialog.Close class={buttonVariants({ variant: 'ghost', size: 'icon-sm' })} aria-label={t('common.close')}
        ><XMark /></Dialog.Close
      >
    </Dialog.Header>
    {@render children()}
    <Dialog.Footer>
      {#if status}<div class="mr-auto flex items-center">{@render status()}</div>{/if}
      <Button onclick={onclose}>{closeOnApply || dirty ? t('common.cancel') : t('common.close')}</Button>
      <!-- The tooltip explains the disabled button, so it sits on a wrapper. -->
      <Tooltip tip={valid ? undefined : invalidTip}>
        {#snippet child({ props })}
          <span {...props} class="inline-flex">
            <Button variant="default" disabled={saving || locked || !valid || (!closeOnApply && !dirty)} onclick={apply}
              >{applyLabel ?? t('common.apply')}</Button
            >
          </span>
        {/snippet}
      </Tooltip>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
