<script lang="ts">
  import { tick, untrack } from 'svelte';

  import { ScrollArea } from '$lib/components/ui/scroll-area';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type Cue } from '../../lib/api';
  import { edit as editShow } from '../../lib/commands';
  import { linkedCues } from '../../lib/cue';
  import { type CueEdit, patchFor, shared } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { type InspectorNotice, inspectorTabs, inspectorView, noticesOf, shownTab } from '../../lib/inspector.svelte';
  import { allCues, attempt, cueById, isTopLevel, ui } from '../../lib/store.svelte';
  import { bar } from '../../lib/ui';
  import { setLinked } from './context';
  import CueHeader from './CueHeader.svelte';
  import InspectorTabs from './InspectorTabs.svelte';
  import Notices from './Notices.svelte';
  import FlowTab from './tabs/FlowTab.svelte';
  import MainTab from './tabs/MainTab.svelte';
  import NotesTab from './tabs/NotesTab.svelte';
  import SoundTab from './tabs/SoundTab.svelte';
  import TriggerTab from './tabs/TriggerTab.svelte';

  // The fields of one cue (or of several cues) under a fixed head, in tabs that every cue type has in the same
  // order. The inspector makes a new one for every other set of cues (or another type), so the fields always
  // edit the cues they were made for: when the selection moves on, Svelte may update a branch once more before
  // removing it, and fields made for a sound cue must not meet a stopwatch cue then.
  let { cues: given, locked }: { cues: Cue[]; locked: boolean } = $props();

  // The cues as made for; a later list with other ids or types (a branch about to go) keeps the last one.
  const made = untrack(() => given.map((cue) => `${cue.id}:${cue.type}`).join());
  let last = untrack(() => given);
  const current = $derived.by(() => {
    if (given.map((cue) => `${cue.id}:${cue.type}`).join() === made) last = given;
    return last;
  });
  const multi = untrack(() => given.length > 1);

  // What a slider drag has changed so far (sent with `live`, not in the show until the drag ends). A show event
  // during the drag (another change, a media check) keeps it, so the thumb does not jump back and the drag's end
  // does not commit the old value.
  let dragged: Partial<Cue> = {};

  // Local copy of the one cue that the fields edit; every change is sent as one update.
  let draft = $state<Cue | null>(untrack(() => (multi ? null : ($state.snapshot(given[0]) as Cue))));
  $effect(() => {
    if (!multi) draft = Object.assign($state.snapshot(current[0]) as Cue, dragged);
  });

  // Bumped when the backend refuses a change to several cues, so their fields show the show again.
  let refused = $state(0);

  async function commit() {
    // The cue may be gone when a field commits as the body goes (see `Inspector`).
    if (!draft || !cueById(draft.id)) return;
    // A refused change (e.g. a taken hotkey) puts the fields back.
    if (!(await attempt(api.edit({ op: 'update_cue', cue: $state.snapshot(draft) as Cue })))) {
      draft = $state.snapshot(current[0]) as Cue;
    }
  }

  async function commitAll(cues: Cue[], patch: Parameters<CueEdit['set']>[0]) {
    const changed = cues.map((cue) => {
      const copy = $state.snapshot(cue) as Cue;
      return Object.assign(copy, patchFor(patch, copy));
    });
    if (!(await editShow({ op: 'update_cues', cues: changed }))) refused++;
  }

  const edit: CueEdit = {
    get cues() {
      return multi ? current : draft ? [draft] : [];
    },
    get multi() {
      return multi;
    },
    get locked() {
      return locked;
    },
    value: <T,>(key: string) => shared(edit.cues, key) as T,
    set(patch, live = false) {
      if (multi) return void commitAll(current, patch);
      if (!draft) return;
      const values = patchFor(patch, draft);
      Object.assign(draft, values);
      // While a slider is dragged running cues follow without an undo step; the drag ends with `set({})`.
      if (live) {
        Object.assign(dragged, values);
        void attempt(api.liveCue($state.snapshot(draft) as Cue));
      } else {
        dragged = {};
        void commit();
      }
    },
  };

  const single = $derived(multi ? null : draft);
  const view = $derived(ui.view!);
  const info = $derived(single ? view.cue_info[single.id] : undefined);
  const linked = $derived(linkedCues(single, allCues()));
  setLinked(() => linked.length > 0);
  // Previewing never touches the show, so it works in show mode too.
  const preview = $derived(single && ui.runtime?.preview?.cue === single.id ? ui.runtime.preview : null);

  const tabs = $derived(inspectorTabs(edit.cues, !!single && isTopLevel(single.id)));
  const tab = $derived(shownTab(tabs));
  const notices = $derived(single ? noticesOf(single, info, view.show.level_limits.max_db) : []);
  const problems = $derived(new Set(notices.map((notice) => notice.tab).filter((id) => id !== undefined)));

  let body = $state<HTMLElement | null>(null);

  /** Opens the notice's tab and shows its field, briefly highlighted. */
  async function reveal(notice: InspectorNotice) {
    if (notice.tab) inspectorView.tab = notice.tab;
    await tick();
    const row = notice.field ? body?.querySelector<HTMLElement>(`[data-field="${notice.field}"]`) : null;
    if (!row) return;
    const calm = matchMedia('(prefers-reduced-motion: reduce)').matches;
    row.scrollIntoView({ block: 'center', behavior: calm ? 'auto' : 'smooth' });
    if (!calm) {
      row.animate([{ backgroundColor: 'var(--color-stop-bg)' }, { backgroundColor: 'transparent' }], {
        duration: 1400,
        easing: 'ease-out',
      });
    }
  }
</script>

<CueHeader {edit} {linked} />
{#if preview}
  <Tooltip tip={preview.playing ? t('inspector.preview_playing') : t('inspector.preview_loading')}>
    {#snippet child({ props })}
      <span {...props} class="{bar} mx-4 mb-2.5">
        <span class="block h-full w-full origin-left bg-primary" style:transform="scaleX({preview.progress})"></span>
      </span>
    {/snippet}
  </Tooltip>
{/if}
<Notices {notices} {tabs} onreveal={reveal} />
<InspectorTabs {tabs} active={tab} {problems} onselect={(next) => (inspectorView.tab = next)} />
<ScrollArea class="min-h-0 flex-1">
  <div bind:this={body} role="tabpanel" aria-label={tabs.find((other) => other.id === tab)?.label}>
    {#key refused}
      <fieldset class="@container m-0 min-w-0 border-0 px-4 pb-4" disabled={locked}>
        {#if tab === 'main'}
          <MainTab {edit} {info} />
        {:else if tab === 'sound'}
          <SoundTab {edit} {info} />
        {:else if tab === 'flow'}
          <FlowTab {edit} {info} />
        {:else if tab === 'trigger'}
          <TriggerTab {edit} {info} />
        {:else}
          <NotesTab {edit} />
        {/if}
      </fieldset>
    {/key}
  </div>
</ScrollArea>
