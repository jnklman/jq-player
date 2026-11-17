// The cue list's menus with several cues selected, its selection mode, and reordering by dragging (SortableJS faked:
// jsdom has no layout to drag in).

import { fireEvent, screen, within } from '@testing-library/svelte';
import type Sortable from 'sortablejs';
import { describe, expect, it, vi } from 'vitest';

const sortables = vi.hoisted(() => [] as { node: HTMLElement; options: Sortable.Options }[]);
vi.mock('sortablejs', () => ({
  default: {
    create: (node: HTMLElement, options: Sortable.Options) => {
      sortables.push({ node, options });
      return { options, destroy() {} };
    },
    ghost: null,
  },
}));

import type { EditOp } from '../../lib/api';
import { selectAll, ui } from '../../lib/store.svelte';
import { backend } from '../../test/backend';
import { runtime, sound, view, wait } from '../../test/fixtures';
import { renderUi } from '../../test/render';
import { loadShow, settle } from '../../test/store';
import CueList from './CueList.svelte';

const cues = () => [
  sound('a', { number: '1', title: 'Rain', link: 'L' }),
  wait('b', { number: '2', link: 'L' }),
  sound('c', { number: '3', title: 'Storm' }),
  wait('d', { number: '4' }),
];

/** Two sound cues, one of them linked: they can be linked with each other. */
const SOUNDS = ['a', 'c'];
const row = (id: string) => document.querySelector<HTMLElement>(`[data-cue="${id}"]`)!;
const lastEdit = () => backend.edits.at(-1) as EditOp | undefined;

async function list(extra: Parameters<typeof view>[1] = {}) {
  await loadShow(view(cues(), { can_paste: true, ...extra }), runtime());
  const result = await renderUi(CueList);
  selectAll(SOUNDS);
  await settle();
  return result;
}

/** Opens the submenu `name` of the open menu and returns its items. */
async function submenu(user: Awaited<ReturnType<typeof list>>['user'], name: string) {
  const trigger = screen
    .getAllByRole('menuitem', { hidden: true })
    .find((item) => item.textContent!.trim().startsWith(name))!;
  await user.hover(trigger);
  await settle();
  // jsdom takes the open submenu for hidden (no layout), so it is found by its slot.
  const content = [...document.querySelectorAll<HTMLElement>('[data-slot$="menu-sub-content"]')].at(-1)!;
  return within(content).getAllByRole('menuitem', { hidden: true });
}

describe('the context menu with several cues selected', () => {
  async function menu(user: Awaited<ReturnType<typeof list>>['user']) {
    await fireEvent.contextMenu(row('a'));
    await settle();
    return user;
  }

  it('links, duplicates linked, pastes linked and unlinks', async () => {
    const { user } = await list();
    const ops: string[] = [];
    for (const name of [/^Link selection/, /^Duplicate linked/, /^Paste linked/, /^Unlink/]) {
      await menu(user);
      const items = await submenu(user, 'Link');
      await user.click(items.find((item) => name.test(item.textContent!.trim()))!);
      await settle();
      ops.push(lastEdit()!.op);
      selectAll(SOUNDS);
    }
    expect(ops).toEqual(['link_cues', 'duplicate', 'paste', 'unlink_cues']);
  });

  it('cuts, pastes over and swaps, duplicates and groups', async () => {
    const { user } = await list();
    const ops: string[] = [];
    for (const name of ['Cut', 'Paste', 'Replace', 'Swap', 'Duplicate', 'Group']) {
      await menu(user);
      const item = screen
        .getAllByRole('menuitem')
        .find((element) => element.textContent!.trim().startsWith(name) && !element.hasAttribute('data-disabled'));
      expect(item, name).toBeDefined();
      await user.click(item!);
      await settle();
      ops.push(lastEdit()?.op ?? 'none');
      selectAll(SOUNDS);
    }
    expect(ops).toContain('cut');
    expect(ops).toContain('paste_replace');
    expect(ops).toContain('group');
  });

  it('adds cues above and below the selection', async () => {
    const { user } = await list();
    for (const where of ['Add above', 'Add below']) {
      await menu(user);
      const items = await submenu(user, where);
      await user.click(items.find((item) => /Wait/.test(item.textContent!))!);
      await settle();
      selectAll(SOUNDS);
    }
    const adds = backend.edits.filter((op) => op.op === 'add_wait') as Extract<EditOp, { op: 'add_wait' }>[];
    // Above and below the cue in the inspector (the first selected).
    expect(adds.map((op) => op.index)).toEqual([0, 1]);
  });
});

describe('the toolbar with several cues selected', () => {
  it('links and unlinks from its Link menu, and pastes over the selection', async () => {
    const { user } = await list();
    const ops: string[] = [];
    for (const name of [/^Link selection/, /^Duplicate linked/, /^Paste linked/, /^Unlink/]) {
      await user.click(screen.getByRole('button', { name: 'Link' }));
      await settle();
      await user.click(screen.getAllByRole('menuitem').find((item) => name.test(item.textContent!.trim()))!);
      await settle();
      ops.push(lastEdit()!.op);
      selectAll(SOUNDS);
    }
    expect(ops).toEqual(['link_cues', 'duplicate', 'paste', 'unlink_cues']);
    await user.click(screen.getByRole('button', { name: 'More paste options' }));
    await user.click(await screen.findByRole('menuitem', { name: /Replace/ }));
    await settle();
    expect(lastEdit()).toMatchObject({ op: 'paste_replace' });
  });

  it('switches how selection and playhead go together', async () => {
    const { user } = await list();
    await user.click(screen.getByRole('button', { name: 'Selection and playhead separate' }));
    await user.click(await screen.findByRole('menuitemradio', { name: 'Playhead follows selection' }));
    await settle();
    expect(backend.invoked('set_selection_mode')).toEqual([{ mode: 'playhead_follows' }]);
  });
});

describe('dragging cues', () => {
  async function drag(id: string, to: number) {
    const { options, node } = sortables.filter((entry) => entry.node.isConnected).at(-1)!;
    const item = row(id);
    const oldIndex = [...node.children].indexOf(item);
    const event = (extra: object) => ({ item, from: node, ...extra }) as Sortable.SortableEvent;
    options.onStart!(event({}));
    options.onEnd!(event({ oldIndex, newIndex: to }));
    await settle();
  }

  it('moves one cue to where it is dropped', async () => {
    await list();
    selectAll(['c']);
    await settle();
    await drag('c', 0);
    expect(lastEdit()).toEqual({ op: 'move', ids: ['c'], to: 0 });
  });

  it('moves the selected cues together when one of them is dragged', async () => {
    await list();
    selectAll(['a', 'b']);
    await settle();
    await drag('a', 3);
    expect(lastEdit()).toEqual({ op: 'move', ids: ['a', 'b'], to: 2 });
    expect(row('b').hidden).toBe(false);
  });

  it('shows the old order again when the move is refused', async () => {
    await list();
    backend.fail('edit', 'refused');
    vi.spyOn(console, 'error').mockImplementation(() => {});
    await drag('d', 0);
    expect(ui.notices.at(-1)?.message).toMatch(/refused/);
    expect(row('a').nextElementSibling).toBe(row('b'));
  });
});
