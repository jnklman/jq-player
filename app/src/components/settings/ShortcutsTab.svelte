<script lang="ts">
  import CaretRight from '~icons/ph/caret-right-bold';
  import { Button } from '$lib/components/ui/button';
  import { Kbd } from '$lib/components/ui/kbd';
  import { Label } from '$lib/components/ui/label';
  import * as ToggleGroup from '$lib/components/ui/toggle-group';

  import { api, type Chord, type ControlAction, type KeyBinding, type LocalSettings } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { keyText, ON_SELECTED, PANIC_KEY } from '../../lib/keys.svelte';
  import {
    ACTION_GROUPS,
    actionName,
    bindKey,
    displacedText,
    keysChanged,
    keysOf,
    resetKeys,
  } from '../../lib/settings';
  import { settingsView } from '../../lib/settingsView.svelte';
  import { attempt, ui } from '../../lib/store.svelte';
  import NumberField from '../fields/NumberField.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingsPanel from './SettingsPanel.svelte';
  import ShortcutRow from './ShortcutRow.svelte';

  // Keys and learned MIDI controls of the global actions in one table,
  // grouped by what they act on, with the step sizes next to their actions.
  // Keys are part of the draft; MIDI bindings are learned and saved at once.
  let {
    draft = $bindable(),
    openMidiPort,
  }: {
    draft: LocalSettings;
    /** Applies the chosen MIDI input (only it) so learning can start; true when it is open. */
    openMidiPort: () => Promise<boolean>;
  } = $props();

  const view = $derived(ui.view!);
  const settings = settingsView();

  type Filter = 'all' | 'set' | 'changed';
  let filter = $state<Filter>('all');
  // The note on the last key change of an action: a fixed key (error) or the keys it took from other
  // actions. The latter shows only while the action still has the key and it is not applied yet.
  let notes = $state<Partial<Record<ControlAction, { error?: string; displaced: KeyBinding[] }>>>({});
  function message(action: ControlAction): { error: boolean; text: string } | undefined {
    const entry = notes[action];
    if (!entry) return undefined;
    if (entry.error) return { error: true, text: entry.error };
    const text = displacedText(entry.displaced, action, draft.keys, view.settings.keys);
    return text ? { error: false, text } : undefined;
  }

  const midiOf = (action: ControlAction) => view.settings.midi.find((binding) => binding.action === action);
  const changed = (action: ControlAction) => keysChanged(draft.keys, view.default_keys, action);
  function shown(action: ControlAction): boolean {
    if (settings.searching || filter === 'all') return true;
    if (filter === 'changed') return changed(action);
    return keysOf(draft.keys, action).length > 0 || !!midiOf(action);
  }

  function setKey(action: ControlAction, index: number, chord: Chord | null) {
    const result = bindKey(draft.keys, action, index, chord, view.fixed_keys);
    notes[action] = { error: result.error, displaced: result.displaced };
    if (!result.error) draft.keys = result.keys;
  }

  function reset(action: ControlAction) {
    const result = resetKeys(draft.keys, view.default_keys, action);
    draft.keys = result.keys;
    notes[action] = { displaced: result.displaced };
  }

  // Learned bindings are saved by the backend at once, so they come from the view, never from the draft.
  function unbind(action: ControlAction) {
    const midi = view.settings.midi.filter((binding) => binding.action !== action);
    attempt(api.setSettings({ ...$state.snapshot(view.settings), midi }));
  }

  // A MIDI input chosen in the draft but not applied yet is opened first.
  async function learn(action: ControlAction, on: boolean) {
    if (on && draft.midi_port !== view.settings.midi_port && !(await openMidiPort())) return;
    await attempt(api.midiLearn(on ? { kind: 'action', action } : null));
  }

  // Leaving the dialog ends a pending learn.
  $effect(() => () => {
    if (ui.view?.midi.learning) api.midiLearn(null);
  });
</script>

{#snippet step(label: string, unit: string, value: number, min: number, stepSize: number, set: (value: number) => void)}
  <Label class="flex items-center gap-1.5 text-xs font-normal text-muted-foreground">
    {label}
    <NumberField class="h-7 w-16" aria-label={`${label} (${unit})`} {min} step={stepSize} {value} onchange={set} />
    {unit}
  </Label>
{/snippet}

<SettingsPanel
  tab="keys"
  description={t('shortcuts.description', {
    modifiers: `${t('key.ctrl')}, ${t('key.alt')} / ${t('key.shift')}`,
    del: t('key.delete'),
  })}
>
  {#if !settings.searching}
    <div class="flex flex-wrap items-center gap-2">
      <ToggleGroup.Root
        type="single"
        variant="outline"
        size="sm"
        aria-label={t('shortcuts.filter')}
        bind:value={() => filter, (value) => value && (filter = value as Filter)}
      >
        <ToggleGroup.Item value="all">{t('shortcuts.all')}</ToggleGroup.Item>
        <ToggleGroup.Item value="set">{t('shortcuts.assigned')}</ToggleGroup.Item>
        <ToggleGroup.Item value="changed">{t('shortcuts.changed')}</ToggleGroup.Item>
      </ToggleGroup.Root>
      <Button
        size="sm"
        class="ml-auto"
        onclick={() => {
          draft.keys = $state.snapshot(view.default_keys);
          notes = {};
        }}>{t('shortcuts.keys_default')}</Button
      >
    </div>
  {/if}

  <div class="flex flex-col gap-4">
    {#if !settings.searching}
      <div
        class="sticky top-0 z-10 grid grid-cols-[minmax(10rem,1.1fr)_1fr_1fr_minmax(9.5rem,1fr)_1.75rem] gap-2 border-b border-line bg-card py-1.5 text-[11px] tracking-[0.06em] text-muted-foreground uppercase max-sm:hidden"
      >
        <span>{t('field.action')}</span><span>{t('shortcuts.key')}</span><span>{t('shortcuts.key2')}</span><span
          >MIDI</span
        >
      </div>
    {/if}
    {#each ACTION_GROUPS as group (group.id)}
      {@const actions = group.actions.filter(shown)}
      {#if actions.length}
        <SettingGroup title={group.title}>
          {#snippet badge()}
            {#if group.scope}<span class="font-normal text-muted-foreground">{group.scope}</span>{/if}
          {/snippet}
          {#snippet aside()}
            {#if group.id === 'jump'}
              {@render step(t('shortcuts.back'), 's', draft.jump_back, 0.1, 1, (value) => (draft.jump_back = value))}
              {@render step(
                t('shortcuts.forward'),
                's',
                draft.jump_forward,
                0.1,
                1,
                (value) => (draft.jump_forward = value),
              )}
              {@render step(
                t('shortcuts.before_end'),
                's',
                draft.jump_end,
                0.1,
                1,
                (value) => (draft.jump_end = value),
              )}
            {:else if group.id === 'level'}
              {@render step(
                t('control.volume'),
                'dB',
                draft.volume_step,
                0.1,
                0.5,
                (value) => (draft.volume_step = value),
              )}
              {@render step(
                t('control.pan'),
                '%',
                Math.round(draft.pan_step * 100),
                1,
                5,
                (value) => (draft.pan_step = value / 100),
              )}
              {@render step(t('control.rate'), '%', draft.rate_step, 0.1, 1, (value) => (draft.rate_step = value))}
            {/if}
          {/snippet}
          {#each actions as action (action)}
            {@const learning = view.midi.learning?.kind === 'action' && view.midi.learning.action === action}
            <ShortcutRow
              label={actionName(action)}
              keywords="{t('shortcuts.keywords')} {action === 'panic' ? 'Escape' : ''} {group.keywords ?? ''}"
              fixedKey={action === 'panic' ? PANIC_KEY : undefined}
              keys={keysOf(draft.keys, action)}
              midi={ON_SELECTED.includes(action) ? null : midiOf(action)}
              {learning}
              canLearn={!!draft.midi_port}
              learnTip={t('shortcuts.learn_tip')}
              changed={changed(action)}
              message={message(action)}
              onkey={(index, chord) => setKey(action, index, chord)}
              onreset={() => reset(action)}
              onlearn={(on) => learn(action, on)}
              onunbind={() => unbind(action)}
            />
          {/each}
        </SettingGroup>
      {/if}
    {/each}
    {#if !settings.searching && ACTION_GROUPS.every((group) => !group.actions.some(shown))}
      <p class="m-0 py-4 text-center text-xs text-muted-foreground">
        {filter === 'changed' ? t('shortcuts.all_default') : t('shortcuts.none_assigned')}
      </p>
    {/if}
  </div>

  {#if !settings.searching}
    <details class="group rounded-lg border border-line bg-panel-2 px-3 py-2">
      <summary
        class="flex cursor-pointer list-none items-center gap-1.5 text-xs text-muted-foreground [&::-webkit-details-marker]:hidden"
      >
        <CaretRight
          class="shrink-0 text-[10px] transition-transform group-open:rotate-90 motion-reduce:transition-none"
        />
        {t('shortcuts.fixed_keys')}
      </summary>
      <div class="flex flex-wrap gap-1.5 pt-2">
        {#each view.fixed_keys as chord (keyText(chord))}<Kbd>{keyText(chord)}</Kbd>{/each}
      </div>
    </details>
  {/if}
</SettingsPanel>
