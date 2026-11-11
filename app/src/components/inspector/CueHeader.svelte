<script lang="ts">
  import LinkSimple from '~icons/ph/link-simple-bold';
  import LinkSimpleBreak from '~icons/ph/link-simple-break-bold';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import * as Popover from '$lib/components/ui/popover';

  import { api, type ControlKind, type Cue } from '../../lib/api';
  import { unlinkCues } from '../../lib/commands';
  import { controlLabels, isControl, typeColors, typeTitles } from '../../lib/control';
  import { cueLabel } from '../../lib/cue';
  import { type CueEdit, isMixed } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { optionsOf } from '../../lib/options';
  import { attempt } from '../../lib/store.svelte';
  import ColorField from '../fields/ColorField.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';

  // The head of the inspector, outside the tabs: colour, number and title, type and script reference; the
  // link to other cues. With several cues: what they are, and the fields all of them can share here.
  let { edit, linked }: { edit: CueEdit; linked: Cue[] } = $props();

  const single = $derived(edit.multi ? null : edit.cues[0]);
  const color = $derived(edit.value<string | null>('color'));
  const scriptRef = $derived(edit.value<string>('script_ref'));
  // "2 × Sound", "1 × Playlist" for a selection.
  const counts = $derived(
    [...new Set(edit.cues.map((cue) => cue.type))].map(
      (type) => [type, edit.cues.filter((cue) => cue.type === type).length] as const,
    ),
  );

  /** Another control type; targets, duration and curve stay where both have them. */
  function setType(cue: Cue, next: ControlKind) {
    if (next !== cue.type) attempt(api.edit({ op: 'set_cue_type', ids: [cue.id], kind: next }));
  }
</script>

{#snippet swatch()}
  <Popover.Root>
    <Popover.Trigger>
      {#snippet child({ props })}
        <button
          {...props}
          class="h-8 w-3.5 shrink-0 rounded-sm border border-line-strong bg-[repeating-linear-gradient(135deg,var(--color-line)_0_3px,transparent_3px_6px)]"
          style:background={typeof color === 'string' ? color : undefined}
          aria-label={t('field.color')}
          disabled={edit.locked}
        ></button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Content align="start" class="w-auto">
      <ColorField
        value={typeof color === 'string' ? color : null}
        mixed={isMixed(color)}
        onchange={(next) => edit.set({ color: next })}
      />
    </Popover.Content>
  </Popover.Root>
{/snippet}

{#snippet script()}
  <Label class="gap-1.5 text-xs text-muted-foreground">
    {t('header_cue.script')}
    <Input
      class="min-h-7 w-36 py-0 text-xs"
      aria-label={t('header_cue.script_ref')}
      placeholder={isMixed(scriptRef) ? t('field.mixed') : undefined}
      value={isMixed(scriptRef) ? '' : scriptRef}
      disabled={edit.locked}
      onchange={(e) => edit.set({ script_ref: e.currentTarget.value })}
    />
  </Label>
{/snippet}

<div class="flex flex-col gap-2 px-4 pt-3 pb-2.5">
  {#if single}
    <div class="flex items-center gap-2">
      {@render swatch()}
      {#if single.type !== 'scene'}
        <Input
          class="w-16 shrink-0 text-center font-mono text-primary"
          aria-label={t('header_cue.number')}
          value={single.number}
          disabled={edit.locked}
          onchange={(e) => edit.set({ number: e.currentTarget.value })}
        />
      {/if}
      <Input
        class="grow font-medium"
        aria-label={t('cuelist.col_title')}
        value={single.title}
        disabled={edit.locked}
        onchange={(e) => edit.set({ title: e.currentTarget.value })}
      />
    </div>
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
      {#if isControl(single)}
        <SimpleSelect
          class="w-40"
          aria-label={t('cuelist.col_type')}
          disabled={edit.locked}
          value={single.type as ControlKind}
          options={optionsOf(controlLabels)}
          onchange={(next) => setType(single, next)}
        />
      {:else}
        <Badge variant="outline" class={typeColors[single.type]}>{typeTitles[single.type]}</Badge>
      {/if}
      {@render script()}
    </div>
    {#if single.type === 'scene'}
      <p class="m-0 text-xs text-muted-foreground">{t('header_cue.scene_hint')}</p>
    {/if}
  {:else}
    <div class="flex items-center gap-2">
      {@render swatch()}
      <span class="text-sm font-semibold">{t('header_cue.selected', { count: edit.cues.length })}</span>
    </div>
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
      {#each counts as [type, count] (type)}
        <Badge variant="outline" class={typeColors[type]}>{count} × {typeTitles[type]}</Badge>
      {/each}
      {@render script()}
    </div>
  {/if}
</div>

{#if single && linked.length}
  {@const ids = [single.id]}
  <div
    class="mx-4 mb-2.5 flex items-center gap-2.5 rounded-lg border border-line bg-panel-2 px-3 py-2 text-xs text-fg-2"
  >
    <LinkSimple class="size-4 shrink-0 text-primary" />
    <span class="min-w-0 grow">
      {t('header_cue.linked', { cues: linked.map(cueLabel).join(', '), count: linked.length + 1 })}
      <LinkSimpleBreak class="inline size-3 align-[-1px] text-dim" />.
    </span>
    <Button size="sm" disabled={edit.locked} onclick={() => unlinkCues(ids)}>{t('header_cue.unlink')}</Button>
  </div>
{:else if edit.multi}
  <p class="mx-4 mt-0 mb-2.5 text-xs text-muted-foreground">
    {t('header_cue.multi_hint', { count: edit.cues.length })}
  </p>
{/if}
