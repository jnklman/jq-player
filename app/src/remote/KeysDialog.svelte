<script lang="ts">
  import { untrack } from 'svelte';

  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Kbd } from '$lib/components/ui/kbd';
  import { Label } from '$lib/components/ui/label';

  import KeyCapture from '../components/KeyCapture.svelte';
  import { api, type KeyBinding } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { ACTION_NAMES, keyText, PANIC_KEY } from '../lib/keys.svelte';
  import { ui } from '../lib/store.svelte';
  import { check, hint } from '../lib/ui';
  import { hostKeys, keysOf, REMOTE_ACTIONS, type RemoteAction, withKey } from './keys';

  // The keys of this device (a keyboard at a tablet): the host's, or keys of its own for the show's global actions,
  // kept by the host with the device. Saved as a whole; the host checks them like its own (no reserved key, none
  // twice) and says why it refuses.
  let { onclose }: { onclose: () => void } = $props();

  const initial = untrack(() => ui.remote?.keys ?? null);
  let own = $state(initial !== null);
  let draft = $state<KeyBinding[]>(untrack(() => initial ?? hostKeys()));
  let error = $state<string | null>(null);
  let saving = $state(false);

  const shown = $derived(own ? draft : hostKeys());

  function setKey(action: RemoteAction, index: number, chord: KeyBinding['chord'] | null) {
    draft = withKey(draft, action, index, chord);
    error = null;
  }

  async function save() {
    saving = true;
    error = null;
    try {
      await api.setOwnKeys(own ? $state.snapshot(draft) : null);
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root
  bind:open={
    () => true,
    (open) => {
      if (!open) onclose();
    }
  }
>
  <Dialog.Content class="max-w-xl">
    <Dialog.Header>
      <Dialog.Title>{t('remote.keys_title')}</Dialog.Title>
      <Dialog.Close class={buttonVariants({ variant: 'ghost', size: 'icon-sm' })} aria-label={t('common.close')}
        ><XMark /></Dialog.Close
      >
    </Dialog.Header>
    <div class="flex min-h-0 flex-col gap-3 overflow-y-auto px-4 py-3">
      <Label class={check}>
        <Checkbox
          checked={own}
          onCheckedChange={(on) => {
            own = on;
            error = null;
          }}
        />
        {t('remote.keys_own')}
      </Label>
      <p class={hint}>{own ? t('remote.keys_own_hint') : t('remote.keys_host_hint')}</p>
      <div role="list" class="flex flex-col">
        {#each REMOTE_ACTIONS as action (action)}
          {@const keys = keysOf(shown, action)}
          {@const label = ACTION_NAMES[action]}
          <div
            role="listitem"
            aria-label={label}
            class="grid grid-cols-[minmax(8rem,1.2fr)_1fr_1fr] items-center gap-2 border-b border-row-line py-1 text-[13px] last:border-b-0 max-sm:grid-cols-2"
          >
            <span class="flex items-center gap-1.5 font-semibold max-sm:col-span-full">
              {label}
              {#if action === 'panic'}
                <Kbd aria-label={t('shortcuts.fixed_key', { key: keyText(PANIC_KEY) })}>{keyText(PANIC_KEY)}</Kbd>
              {/if}
            </span>
            <KeyCapture
              value={keys[0] ?? null}
              label={t('shortcuts.first_key', { label })}
              placeholder={action === 'panic' ? t('shortcuts.no_other') : t('trigger.unassigned')}
              clearable={false}
              disabled={!own}
              onchange={(chord) => setKey(action, 0, chord)}
            />
            <KeyCapture
              value={keys[1] ?? null}
              label={t('shortcuts.second_key', { label })}
              placeholder="–"
              clearable={false}
              disabled={!own || keys.length === 0}
              onchange={(chord) => setKey(action, keys.length === 0 ? 0 : 1, chord)}
            />
          </div>
        {/each}
      </div>
      {#if error}<p class="text-xs text-stop-soft" role="alert">{error}</p>{/if}
    </div>
    <Dialog.Footer>
      <Button onclick={onclose}>{t('common.cancel')}</Button>
      <Button variant="default" disabled={saving} onclick={save}>{t('common.save')}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
