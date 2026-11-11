<script lang="ts">
  import type { Component } from 'svelte';

  import Broom from '~icons/ph/broom-bold';
  import CaretRight from '~icons/ph/caret-right-bold';
  import Check from '~icons/ph/check-bold';
  import LockSimple from '~icons/ph/lock-simple-bold';
  import Network from '~icons/ph/network-bold';
  import SpeakerHigh from '~icons/ph/speaker-high-bold';
  import Timer from '~icons/ph/timer-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type OthersEffect } from '../lib/api';
  import { edit, setLevelLimits } from '../lib/commands';
  import { t } from '../lib/i18n/index.svelte';
  import { parseNumber } from '../lib/input';
  import { cleanMediaFolder } from '../lib/media';
  import { othersOptions, othersScopeOptions } from '../lib/options';
  import { attempt, mode, ui } from '../lib/store.svelte';
  import { check, field, hint } from '../lib/ui';
  import NumberField from './fields/NumberField.svelte';
  import SimpleSelect from './SimpleSelect.svelte';

  // Content of the popover under the show name in the header: settings of the
  // open show only. Opening and creating shows is in the app menu.
  let { onclose }: { onclose: () => void } = $props();

  const view = $derived(ui.view!);
  const locked = $derived(mode() === 'show');
  let name = $state(ui.view!.name);
  const canRename = $derived(!locked && name.trim() !== '' && name.trim() !== view.name);

  const others = $derived(view.show.others);
  const modeLocked = $derived(view.show.show_mode_locked);
  const limits = $derived(view.show.level_limits);

  function openDialog(dialog: 'outputsOpen' | 'devicesOpen' | 'stopwatchesOpen') {
    onclose();
    ui[dialog] = true;
  }

  function setOthers(patch: Partial<OthersEffect>) {
    edit({ op: 'set_others', others: { ...others, ...patch } });
  }

  /** Sets a limit (asking first if cues lie outside); a refused or cancelled one puts the field back. */
  async function setLimit(key: 'max_db' | 'min_db', input: HTMLInputElement) {
    const db = parseNumber(input.value);
    if (db === null || !(await setLevelLimits({ ...limits, [key]: db }))) input.value = String(limits[key]);
  }

  function setNormalization(target: number, onImport: boolean) {
    edit({ op: 'set_normalization', target, on_import: onImport });
  }

  /** Normalizes every sound and playlist cue (groups included). */
  function normalizeAll() {
    edit({ op: 'normalize', ids: view.show.cues.map((cue) => cue.id) });
  }

  async function rename() {
    if (canRename && (await attempt(api.renameShow(name.trim())))) onclose();
  }

  function cleanMedia() {
    onclose();
    void cleanMediaFolder();
  }
</script>

{#snippet row(Icon: Component, label: string, onclick: () => void, count?: number, disabled = false)}
  <Button variant="ghost" class="justify-start px-2 text-fg" {disabled} {onclick}>
    <Icon class="size-4 text-muted-foreground" />
    {label}
    {#if count != null}<span class="ml-auto text-xs text-muted-foreground tabular-nums">{count}</span>{/if}
    {#if disabled && locked}
      <LockSimple class={['size-3 text-muted-foreground', count == null && 'ml-auto']} />
    {:else}
      <CaretRight class={['size-3 text-muted-foreground', count == null && 'ml-auto']} />
    {/if}
  </Button>
{/snippet}

<!-- Marks what show mode keeps from being edited. -->
{#snippet lock(tip = t('showmenu.locked_tip'))}
  {#if locked}
    <Tooltip {tip}>
      {#snippet child({ props })}
        <span {...props} class="inline-flex text-muted-foreground" aria-label={tip}><LockSimple class="size-3" /></span>
      {/snippet}
    </Tooltip>
  {/if}
{/snippet}

{#snippet heading(text: string, lockTip?: string)}
  <span class="flex items-center gap-1.5 text-[10.5px] font-semibold tracking-[0.07em] text-muted-foreground uppercase">
    {text}
    {@render lock(lockTip)}
  </span>
{/snippet}

<!-- Identity of the show and the lock, which can be lifted in show mode too. -->
<div class="flex flex-col gap-2.5">
  <div class="flex flex-col gap-1">
    <Label class={field}>
      <span class="flex items-center gap-1.5">{t('showmenu.name')} {@render lock()}</span>
      <div class="flex gap-1.5">
        <Input
          bind:value={name}
          disabled={locked}
          onkeydown={(e) => {
            if (e.key === 'Enter') rename();
          }}
        />
        {#if canRename}
          <Tooltip
            tip={t('showmenu.rename')}
            keys="Enter"
            class={buttonVariants({ variant: 'default', size: 'icon' })}
            aria-label={t('showmenu.rename')}
            onclick={rename}
          >
            <Check />
          </Tooltip>
        {/if}
      </div>
    </Label>
    <span class="{hint} break-all">{view.path}</span>
  </div>
  <div class="flex flex-col gap-1">
    <Label class={check}>
      <Checkbox bind:checked={() => modeLocked, (checked) => edit({ op: 'lock_show_mode', locked: checked })} />
      {t('showmenu.lock_mode')}
    </Label>
    <span class={hint}>{t('showmenu.lock_mode_hint')}</span>
  </div>
</div>

<div class="flex flex-col gap-1 border-t border-line pt-3">
  {@render heading(t('showmenu.setup'), t('showmenu.view_only'))}
  <div class="-mx-2 flex flex-col">
    {@render row(SpeakerHigh, t('outputs.title'), () => openDialog('outputsOpen'), view.show.outputs.length)}
    {@render row(Network, t('devices.title'), () => openDialog('devicesOpen'), view.show.devices.length)}
    {@render row(Timer, t('stopwatches.title'), () => openDialog('stopwatchesOpen'), view.show.stopwatches.length)}
  </div>
</div>

<div class="flex flex-col gap-2.5 border-t border-line pt-3">
  {@render heading(t('showmenu.cue_start'))}
  <Label class={field}>
    {t('showmenu.other_cues')}
    <SimpleSelect
      disabled={locked}
      value={others.effect}
      options={othersOptions}
      onchange={(effect) => setOthers({ effect })}
    />
  </Label>
  {#if others.effect !== 'none'}
    <div class="grid grid-cols-2 gap-2">
      <NumberField
        label={t('showmenu.fade_secs')}
        min={0}
        step={0.5}
        disabled={locked}
        value={others.fade}
        onchange={(fade) => setOthers({ fade })}
      />
      {#if others.effect === 'duck'}
        <NumberField
          label={t('showmenu.duck_db')}
          min={-60}
          max={0}
          step={1}
          disabled={locked}
          value={others.duck_db}
          onchange={(duck_db) => setOthers({ duck_db })}
        />
      {/if}
      <Label class="{field} col-span-2">
        {t('showmenu.scope')}
        <SimpleSelect
          disabled={locked}
          value={others.scope}
          options={othersScopeOptions}
          onchange={(scope) => setOthers({ scope })}
        />
      </Label>
    </div>
  {/if}
  <span class="{hint} -mt-1">{t('showmenu.others_hint')}</span>
</div>

<div class="flex flex-col gap-2.5 border-t border-line pt-3">
  {@render heading(t('showmenu.limits'))}
  <div class="grid grid-cols-2 gap-2">
    <Label class={field}>
      {t('showmenu.min_db')}
      <Input
        type="number"
        step="1"
        disabled={locked}
        value={limits.min_db}
        onchange={(e) => setLimit('min_db', e.currentTarget)}
      />
    </Label>
    <Label class={field}>
      {t('showmenu.max_db')}
      <Input
        type="number"
        step="1"
        disabled={locked}
        value={limits.max_db}
        onchange={(e) => setLimit('max_db', e.currentTarget)}
      />
    </Label>
  </div>
  <span class="{hint} -mt-1">{t('showmenu.limits_hint')}</span>
</div>

<div class="flex flex-col gap-2.5 border-t border-line pt-3">
  {@render heading(t('showmenu.normalization'))}
  <NumberField
    label={t('showmenu.target_loudness')}
    step={1}
    min={-40}
    max={-6}
    disabled={locked}
    value={view.show.loudness_target}
    onchange={(target) => setNormalization(target, view.show.normalize_on_import)}
  />
  <Label class={check}>
    <Checkbox
      disabled={locked}
      bind:checked={
        () => view.show.normalize_on_import, (checked) => setNormalization(view.show.loudness_target, checked)
      }
    />
    {t('showmenu.normalize_new')}
  </Label>
  <span class="{hint} -mt-1">{t('showmenu.normalize_hint')}</span>
  <Button disabled={locked} onclick={normalizeAll}>{t('showmenu.normalize_all')}</Button>
</div>

<div class="border-t border-line pt-2">
  <div class="-mx-2 flex flex-col">
    {@render row(Broom, t('media.clean_title'), cleanMedia, undefined, locked)}
  </div>
</div>
