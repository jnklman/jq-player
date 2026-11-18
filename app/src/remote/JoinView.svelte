<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  import { t } from '../lib/i18n/index.svelte';
  import { join, remote } from './remote.svelte';

  // A device the host does not know asks to join, by a name the operator recognises.
  let name = $state('');
  const MAX_NAME = 40;
</script>

<form
  class="m-auto flex w-full max-w-sm flex-col gap-3 p-4"
  onsubmit={(e) => {
    e.preventDefault();
    if (name.trim()) void join(name.trim());
  }}
>
  <h1 class="text-lg font-semibold">{t('remote.join_title')}</h1>
  <p class="text-[13px] text-muted-foreground">{t('remote.join_text')}</p>
  <Label for="remote-name" class="text-xs text-muted-foreground">{t('remote.name')}</Label>
  <Input
    id="remote-name"
    bind:value={name}
    maxlength={MAX_NAME}
    autocomplete="off"
    placeholder={t('remote.name_placeholder')}
  />
  {#if remote.error}<p class="text-[13px] text-stop-soft" role="alert">{remote.error}</p>{/if}
  {#if remote.unreachable}<p class="text-[13px] text-stop-soft" role="alert">{t('remote.unreachable')}</p>{/if}
  <Button type="submit" variant="default" size="lg" disabled={!name.trim() || remote.joining}>{t('remote.join')}</Button
  >
  <p class="text-xs text-muted-foreground">{t('remote.insecure_hint')}</p>
</form>
