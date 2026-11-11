<script lang="ts">
  import ArrowCounterClockwise from '~icons/ph/arrow-counter-clockwise';
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Kbd } from '$lib/components/ui/kbd';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { Chord, MidiBinding } from '../../lib/api';
  import { fmtMidi } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { keyText } from '../../lib/keys.svelte';
  import { useSetting } from '../../lib/settingsView.svelte';
  import KeyCapture from '../KeyCapture.svelte';

  // One global action in the shortcut table: up to two keys, the learned
  // MIDI control (null `midi` = keys only) and a reset to its default keys.
  // A fixed key (Esc for panic) is shown by the name; the two keys come on top.
  let {
    label,
    keywords = '',
    fixedKey,
    keys,
    midi,
    learning,
    canLearn,
    learnTip,
    changed,
    message,
    onkey,
    onreset,
    onlearn,
    onunbind,
  }: {
    label: string;
    keywords?: string;
    /** A key the action always has; it cannot be changed. */
    fixedKey?: Chord;
    keys: Chord[];
    /** The learned binding; undefined = none yet; null = the action takes no MIDI. */
    midi: MidiBinding | undefined | null;
    learning: boolean;
    canLearn: boolean;
    /** Why learning is not possible. */
    learnTip?: string;
    /** The keys differ from the defaults. */
    changed: boolean;
    /** A note on the last key change (`error`: refused). */
    message?: { error: boolean; text: string };
    onkey: (index: number, chord: Chord | null) => void;
    onreset: () => void;
    onlearn: (on: boolean) => void;
    onunbind: () => void;
  } = $props();

  let el = $state<HTMLElement | null>(null);
  const setting = useSetting(
    () =>
      [
        label,
        keywords,
        ...(fixedKey ? [keyText(fixedKey)] : []),
        ...keys.map(keyText),
        midi ? fmtMidi(midi.trigger) : '',
      ].join(' '),
    () => el,
  );
</script>

<div
  bind:this={el}
  role="group"
  aria-label={label}
  hidden={!setting.visible}
  class="grid grid-cols-[minmax(10rem,1.1fr)_1fr_1fr_minmax(9.5rem,1fr)_1.75rem] items-center gap-2 border-b border-row-line py-1 text-[13px] last:border-b-0 max-sm:grid-cols-[1fr_1fr_1.75rem]"
>
  <span class="flex items-center gap-1.5 font-semibold max-sm:col-span-full">
    {label}
    {#if fixedKey}
      <Tooltip tip={t('shortcuts.fixed_tip')}>
        {#snippet child({ props })}
          <Kbd {...props} aria-label={t('shortcuts.fixed_key', { key: keyText(fixedKey) })}>{keyText(fixedKey)}</Kbd>
        {/snippet}
      </Tooltip>
    {/if}
    {#if changed}<span class="size-1.75 shrink-0 rounded-full bg-primary" aria-label={t('settings.changed')}
      ></span>{/if}
  </span>
  <KeyCapture
    value={keys[0] ?? null}
    label={t('shortcuts.first_key', { label })}
    placeholder={fixedKey ? t('shortcuts.no_other') : t('trigger.unassigned')}
    clearable={false}
    onchange={(chord) => onkey(0, chord)}
  />
  <KeyCapture
    value={keys[1] ?? null}
    label={t('shortcuts.second_key', { label })}
    placeholder="–"
    clearable={false}
    disabled={keys.length === 0}
    onchange={(chord) => onkey(keys.length === 0 ? 0 : 1, chord)}
  />
  <div class="flex min-w-0 items-center gap-1 max-sm:col-span-2">
    {#if midi === null}
      <span class="text-xs text-muted-foreground">{t('actions.scope.keys_only')}</span>
    {:else if learning}
      <span class="truncate text-xs text-primary motion-safe:animate-pulse">{t('trigger.press_controller')}</span>
      <Button variant="ghost" size="sm" onclick={() => onlearn(false)}>{t('common.cancel')}</Button>
    {:else if midi}
      <span class="truncate font-mono text-[11.5px] text-fg-2">{fmtMidi(midi.trigger)}</span>
      <Button variant="ghost" size="icon-xs" aria-label={t('shortcuts.remove_midi', { label })} onclick={onunbind}
        ><XMark /></Button
      >
    {:else}
      <Tooltip tip={canLearn ? undefined : learnTip}>
        {#snippet child({ props })}
          <span {...props} class="inline-flex">
            <Button size="sm" disabled={!canLearn} onclick={() => onlearn(true)}>{t('trigger.learn')}</Button>
          </span>
        {/snippet}
      </Tooltip>
    {/if}
  </div>
  {#if changed}
    <Tooltip tip={t('shortcuts.default_keys')}>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="icon-xs"
          aria-label={t('shortcuts.default_keys_of', { label })}
          onclick={onreset}><ArrowCounterClockwise /></Button
        >
      {/snippet}
    </Tooltip>
  {:else}
    <span></span>
  {/if}
  {#if message}
    <span
      class={[
        'col-start-2 col-end-[-1] px-2 pb-0.5 text-xs max-sm:col-start-1',
        message.error ? 'text-stop-soft' : 'text-fade-soft',
      ]}>{message.text}</span
    >
  {/if}
</div>
