import { fireEvent, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { panelsIn } from '../../lib/layout.svelte';
import { windowSupport } from '../../lib/window.svelte';
import { backend } from '../../test/backend';
import { runtime, sound, view } from '../../test/fixtures';
import { renderUi } from '../../test/render';
import { loadShow, settle } from '../../test/store';
import LayoutMenu from './LayoutMenu.svelte';
import { layout } from './panels';
import Workspace from './Workspace.svelte';

// jsdom has no layout: every element covers 0–1000 × 0–800 px, and the element under the pointer is set per test.
let under: Element | null = null;
beforeEach(() => {
  layout.reset();
  layout.editing = false;
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
    DOMRect.fromRect({ x: 0, y: 0, width: 1000, height: 800 }),
  );
  document.elementFromPoint = () => under;
});
afterEach(() => {
  vi.restoreAllMocks();
  under = null;
});

const showView = () => view([sound('a', { number: '1', title: 'Rain' })]);

describe('Workspace', () => {
  async function mount() {
    await loadShow(showView(), runtime());
    const result = await renderUi(Workspace);
    layout.editing = true;
    await settle();
    const handle = (id: string) =>
      result.container.querySelector<HTMLElement>(
        `[data-panel="${id}"] [data-slot="panel-frame"] [role="presentation"]`,
      )!;
    const panel = (id: string) => result.container.querySelector<HTMLElement>(`[data-panel="${id}"]`)!;
    return { ...result, handle, panel };
  }

  /** Drags panel `id`'s title from the middle to (`x`, `y`), over `target`, and lets go with `end`. */
  async function drag(handle: HTMLElement, x: number, y: number, target: Element | null, end = 'pointerUp') {
    await fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 500, clientY: 400 });
    // Too short to count as a drag.
    await fireEvent.pointerMove(handle, { pointerId: 1, clientX: 502, clientY: 401 });
    under = target;
    await fireEvent.pointerMove(handle, { pointerId: 1, clientX: x, clientY: y });
    if (end === 'blur') await fireEvent.blur(window);
    else await fireEvent[end as 'pointerUp'](handle, { pointerId: 1 });
    await settle();
  }

  it('puts every panel of the layout into its slot', async () => {
    const { panel } = await mount();
    for (const id of panelsIn(layout.root)) expect(panel(id)).toBeInTheDocument();
  });

  it('moves a panel next to another one, on each side', async () => {
    const { handle, panel } = await mount();
    const move = vi.spyOn(layout, 'move');
    for (const [x, y, zone] of [
      [50, 400, 'left'],
      [950, 400, 'right'],
      [500, 50, 'top'],
      [500, 750, 'bottom'],
    ] as const) {
      // Away from the workspace's own edges, which dock.
      vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (this: Element) {
        return this.tagName === 'MAIN' || this.parentElement?.tagName === 'MAIN'
          ? DOMRect.fromRect({ x: -500, y: -500, width: 3000, height: 3000 })
          : DOMRect.fromRect({ x: 0, y: 0, width: 1000, height: 800 });
      });
      await drag(handle('cues'), x, y, panel('active'));
      expect(move).toHaveBeenLastCalledWith('cues', 'active', zone);
    }
  });

  it('swaps two panels when dropped in the middle, and shows it while dragging', async () => {
    const { handle, panel } = await mount();
    const move = vi.spyOn(layout, 'move');
    await fireEvent.pointerDown(handle('cues'), { button: 0, pointerId: 1, clientX: 500, clientY: 400 });
    under = panel('active');
    await fireEvent.pointerMove(handle('cues'), { pointerId: 1, clientX: 505, clientY: 404 });
    await settle();
    expect(screen.getByText(/· swap/)).toBeInTheDocument();
    await fireEvent.pointerUp(handle('cues'), { pointerId: 1 });
    expect(move).toHaveBeenCalledWith('cues', 'active', 'center');
  });

  it('docks a panel along an edge of the workspace', async () => {
    const { handle } = await mount();
    const dock = vi.spyOn(layout, 'dock');
    await fireEvent.pointerDown(handle('cues'), { button: 0, pointerId: 1, clientX: 500, clientY: 400 });
    await fireEvent.pointerMove(handle('cues'), { pointerId: 1, clientX: 4, clientY: 400 });
    await settle();
    expect(screen.getByText(/· to the edge/)).toBeInTheDocument();
    await fireEvent.pointerUp(handle('cues'), { pointerId: 1 });
    expect(dock).toHaveBeenCalledWith('cues', 'left');
  });

  it('drops nothing onto itself, outside any panel, or when the drag is cut off', async () => {
    const { handle, panel } = await mount();
    const move = vi.spyOn(layout, 'move');
    const dock = vi.spyOn(layout, 'dock');
    await drag(handle('cues'), 500, 400, panel('cues'));
    await drag(handle('cues'), 500, 400, null);
    await drag(handle('cues'), 500, 400, panel('active'), 'pointerCancel');
    await drag(handle('cues'), 500, 400, panel('active'), 'blur');
    expect(move).not.toHaveBeenCalled();
    expect(dock).not.toHaveBeenCalled();
  });

  it('moves nothing outside layout editing, and ends it with "Done"', async () => {
    const { handle, user } = await mount();
    const move = vi.spyOn(layout, 'move');
    await fireEvent.pointerDown(handle('cues'), { button: 2, pointerId: 1 });
    await user.click(screen.getByRole('button', { name: 'Done' }));
    expect(layout.editing).toBe(false);
    await settle();
    expect(move).not.toHaveBeenCalled();
  });
});

describe('LayoutMenu', () => {
  async function openMenu() {
    await loadShow(showView());
    const result = await renderUi(LayoutMenu);
    await result.user.click(screen.getByRole('button', { name: 'View' }));
    await settle();
    return { ...result, menu: screen.getByRole('menu') };
  }

  async function openSub(user: Awaited<ReturnType<typeof openMenu>>['user'], name: RegExp) {
    const trigger = screen
      .getAllByRole('menuitem', { hidden: true })
      .find((item) => name.test(item.textContent!.trim()))!;
    trigger.focus();
    await user.keyboard('{ArrowRight}');
    await settle();
    return screen.getAllByRole('menu').at(-1)!;
  }

  it('lists the panels with where they are', async () => {
    const { menu } = await openMenu();
    expect(within(menu).getByText(/Panels · Editing/)).toBeInTheDocument();
    expect(within(menu).getByRole('menuitem', { name: /Inspector/ })).toBeInTheDocument();
    expect(within(menu).getByRole('menuitem', { name: /Stopwatches.*off/ })).toBeInTheDocument();
  });

  it('moves a panel into a window of its own and hides another', async () => {
    const place = vi.spyOn(layout, 'place');
    const { user } = await openMenu();
    let sub = await openSub(user, /^Running cues/);
    await user.click(within(sub).getByRole('menuitemradio', { name: 'Own window' }));
    expect(place).toHaveBeenLastCalledWith('active', 'window');
    await user.keyboard('{Escape}');
    await settle();
    if (!screen.queryByRole('menu')) await user.click(screen.getByRole('button', { name: 'View' }));
    await settle();
    sub = await openSub(user, /^Inspector/);
    await user.click(within(sub).getByRole('menuitemradio', { name: 'Hidden' }));
    expect(place).toHaveBeenLastCalledWith('inspector', 'hidden');
  });

  it('shows an overlay as a window where overlays are not supported', async () => {
    layout.place('active', 'overlay');
    windowSupport.overlay = false;
    try {
      const { user } = await openMenu();
      expect(screen.getByRole('menuitem', { name: /Running cues.*Window/ })).toBeInTheDocument();
      const sub = await openSub(user, /^Running cues/);
      expect(within(sub).queryByRole('menuitemradio', { name: /Overlay/ })).toBeNull();
    } finally {
      windowSupport.overlay = true;
    }
  });

  it('starts layout editing and resets the layout', async () => {
    const reset = vi.spyOn(layout, 'reset');
    const { user, menu } = await openMenu();
    await user.click(within(menu).getByRole('menuitemcheckbox', { name: 'Edit layout' }));
    expect(layout.editing).toBe(true);
    await user.click(screen.getByRole('button', { name: 'View' }));
    await settle();
    await user.click(screen.getByRole('menuitem', { name: 'Reset layout' }));
    expect(reset).toHaveBeenCalled();
  });

  it('names the show mode layout in show mode', async () => {
    await loadShow(view([], { settings: { show_mode: true } }));
    const { user } = await renderUi(LayoutMenu);
    await user.click(screen.getByRole('button', { name: 'View' }));
    await settle();
    expect(screen.getByText(/Panels · Show mode/)).toBeInTheDocument();
    expect(backend.calls.length).toBeGreaterThan(0);
  });
});
