import { fireEvent, screen, within } from '@testing-library/svelte';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import type { EditOp, SoundCue } from '../../lib/api';
import { redo, undo } from '../../lib/commands';
import { backend } from '../../test/backend';
import { active, runtime, sound, view } from '../../test/fixtures';
import { renderUi } from '../../test/render';
import { loadShow, settle } from '../../test/store';
import Waveform from './Waveform.svelte';
import WaveformDialog from './WaveformDialog.svelte';

// jsdom lays nothing out: every element is 1000 × 200 px from x = 0, observers report at once, canvases draw nowhere.
const WIDTH = 1000;
const restore: (() => void)[] = [];
beforeAll(() => {
  const size = (name: 'clientWidth' | 'clientHeight' | 'offsetWidth' | 'scrollWidth', value: number) => {
    const before = Object.getOwnPropertyDescriptor(HTMLElement.prototype, name);
    Object.defineProperty(HTMLElement.prototype, name, { configurable: true, get: () => value });
    restore.push(() => before && Object.defineProperty(HTMLElement.prototype, name, before));
  };
  size('clientWidth', WIDTH);
  size('offsetWidth', WIDTH);
  size('clientHeight', 200);
  // Wider than the view, so the scroll area shows its scrollbar.
  size('scrollWidth', 2 * WIDTH);
  const rect = vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({
    left: 0,
    top: 0,
    right: WIDTH,
    bottom: 200,
    width: WIDTH,
    height: 200,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
  const canvas = vi
    .spyOn(HTMLCanvasElement.prototype, 'getContext')
    .mockReturnValue(new Proxy({}, { get: () => () => {} }) as unknown as CanvasRenderingContext2D);
  const Observer = globalThis.ResizeObserver;
  globalThis.ResizeObserver = class {
    constructor(private callback: ResizeObserverCallback) {}
    observe(target: Element) {
      const entry = { target, contentRect: DOMRect.fromRect({ width: WIDTH, height: 200 }) };
      this.callback([entry as unknown as ResizeObserverEntry], this as unknown as ResizeObserver);
    }
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  restore.push(() => {
    rect.mockRestore();
    canvas.mockRestore();
    globalThis.ResizeObserver = Observer;
  });
});
afterAll(() => restore.forEach((undo) => undo()));

const peaks = () => ({ duration: 10, peaks: Array.from({ length: 50 }, (_, i) => (i % 5) / 5) });

let errors: MockInstance;
beforeEach(() => {
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  expect(errors).not.toHaveBeenCalled();
  vi.useRealTimers();
});

const cue = (extra: Partial<SoundCue> = {}) =>
  sound('s', {
    number: '1',
    title: 'Rain',
    end: 8,
    vamps: [{ start: 2, end: 4 }],
    marks: [5],
    fade_in: 1,
    fade_out: 1,
    end_fade: true,
    ...extra,
  });

/** x in px for `secs` of the 10 s file over the 1000 px wide element. */
const x = (secs: number) => (secs / 10) * WIDTH;

describe('Waveform (inspector)', () => {
  async function mount(props: { locked?: boolean; onexpand?: () => void; file?: string } = {}) {
    const onchange = vi.fn();
    const shown = cue({ file: props.file ?? `small-${Math.random()}.wav` });
    await loadShow(view([shown]), runtime({ active: [active('s', { progress: 0.5 })] }));
    const result = await renderUi(Waveform, { cue: shown, version: 1, locked: false, onchange, ...props });
    await settle();
    return { ...result, onchange };
  }

  it('draws the peaks, handles and the running position', async () => {
    backend.on('waveform', peaks);
    const onexpand = vi.fn();
    const { container, user } = await mount({ onexpand });
    expect(container.querySelector('svg path')?.getAttribute('d')).toMatch(/^M/);
    expect(screen.getByRole('button', { name: 'Start' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Mark 1' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Large view/ }));
    expect(onexpand).toHaveBeenCalled();
  });

  it('shows why the waveform failed', async () => {
    backend.fail('waveform', 'cannot decode');
    await mount();
    expect(screen.getByText('cannot decode')).toBeInTheDocument();
  });

  it('drags the start and reports it, but not a plain click', async () => {
    backend.on('waveform', peaks);
    const { container, onchange } = await mount();
    const area = container.querySelector('[role="presentation"]')!;
    const start = screen.getByRole('button', { name: 'Start' });
    await fireEvent.pointerDown(start, { pointerId: 1, clientX: 0 });
    await fireEvent.pointerUp(area, { pointerId: 1 });
    expect(onchange).not.toHaveBeenCalled();

    await fireEvent.pointerDown(start, { pointerId: 1, clientX: 0 });
    await fireEvent.pointerMove(area, { pointerId: 1, clientX: x(1.5) });
    await fireEvent.pointerUp(area, { pointerId: 1 });
    expect(onchange).toHaveBeenCalledWith(expect.objectContaining({ start: 1.5, end: 8 }));
  });

  it('drags vamps and marks', async () => {
    backend.on('waveform', peaks);
    const { container, onchange } = await mount();
    const area = container.querySelector('[role="presentation"]')!;
    for (const [name, to] of [
      ['Vamp 1 start', 2.5],
      ['Vamp 1 end', 3.5],
      ['Mark 1', 6],
      ['End', 9],
    ] as const) {
      await fireEvent.pointerDown(screen.getByRole('button', { name }), { pointerId: 1 });
      await fireEvent.pointerMove(area, { pointerId: 1, clientX: x(to) });
      await fireEvent.pointerCancel(area, { pointerId: 1 });
    }
    expect(onchange).toHaveBeenCalledTimes(4);
    expect(onchange.mock.calls[3][0]).toMatchObject({ end: 9 });
  });

  it('sets and removes a mark with a double-click', async () => {
    backend.on('waveform', peaks);
    const { container, onchange } = await mount();
    const area = container.querySelector('[role="presentation"]')!;
    await fireEvent.dblClick(area, { clientX: x(7) });
    expect(onchange).toHaveBeenLastCalledWith(expect.objectContaining({ marks: [5, 7] }));
    await fireEvent.dblClick(area, { clientX: x(5) });
    expect(onchange).toHaveBeenLastCalledWith(expect.objectContaining({ marks: [] }));
  });

  it('cannot be edited when locked', async () => {
    backend.on('waveform', peaks);
    const { container, onchange } = await mount({ locked: true });
    expect(screen.queryByRole('button', { name: 'Start' })).toBeNull();
    await fireEvent.dblClick(container.querySelector('[role="presentation"]')!, { clientX: x(7) });
    expect(onchange).not.toHaveBeenCalled();
  });

  it('drops the peaks of a file replaced meanwhile', async () => {
    let answer: (value: unknown) => void = () => {};
    backend.on('waveform', () => new Promise((resolve) => (answer = resolve)));
    const shown = cue({ file: 'slow.wav' });
    await loadShow(view([shown]));
    const { rerender } = await renderUi(Waveform, { cue: shown, version: 1, locked: false, onchange: () => {} });
    const first = answer;
    backend.on('waveform', peaks);
    await rerender({
      component: Waveform,
      props: { cue: { ...shown, file: 'other.wav' }, version: 1, locked: false, onchange: () => {} },
    });
    await settle();
    first({ duration: 99, peaks: [] });
    await settle();
    expect(screen.queryByText('Computing waveform …')).toBeNull();
  });
});

describe('Waveform dialog', () => {
  async function open(extra: { locked?: boolean; preview?: boolean; cue?: Partial<SoundCue> } = {}) {
    backend.on('waveform', peaks);
    backend.on('waveform_range', ({ from, to }) => ({ duration: (to as number) - (from as number), peaks: [0.5, 1] }));
    const onclose = vi.fn();
    const shown = cue({ file: `big-${Math.random()}.wav`, ...extra.cue });
    const v = view([shown], {
      settings: { show_mode: extra.locked ?? false, preview_channel: extra.preview === false ? null : 3 },
    });
    v.cue_info.s.file_duration = 10;
    await loadShow(v, runtime());
    const result = await renderUi(WaveformDialog, { cueId: 's', onclose });
    await settle();
    const dialog = screen.getByRole('dialog');
    const content = dialog.querySelector('[role="presentation"]') as HTMLElement;
    return { ...result, dialog, content, onclose };
  }

  const lastUpdate = () =>
    (backend.edits.filter((op) => op.op === 'update_cue').at(-1) as Extract<EditOp, { op: 'update_cue' }>)?.cue;

  it('draws the ruler and every handle of the cue', async () => {
    const { dialog } = await open();
    for (const name of ['Start', 'End', 'Vamp 1 start', 'Vamp 1 end', 'Mark 1', 'Fade-in', 'Fade-out']) {
      expect(within(dialog).getByRole('button', { name })).toBeInTheDocument();
    }
    expect(within(dialog).getAllByText('Vamp 1').length).toBeGreaterThan(0);
  });

  it('drags handles into the draft and writes them on "Done"', async () => {
    const { dialog, content, onclose, user } = await open();
    const drag = async (name: string, to: number) => {
      await fireEvent.pointerDown(within(dialog).getByRole('button', { name }), { pointerId: 1, button: 0 });
      await fireEvent.pointerMove(content, { pointerId: 1, clientX: x(to) });
      await fireEvent.pointerUp(content, { pointerId: 1 });
      await settle();
    };
    await drag('Fade-in', 2);
    await drag('Start', 0.5);
    // Let go where it was: nothing changes.
    await drag('Mark 1', 5);
    expect(backend.edits).toEqual([]);
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    await settle();
    expect(lastUpdate()).toMatchObject({ id: 's', start: 0.5, fade_in: 2 });
    expect(onclose).toHaveBeenCalled();
  });

  it('ignores other buttons than the main one', async () => {
    const { dialog, content } = await open();
    await fireEvent.pointerDown(within(dialog).getByRole('button', { name: 'Start' }), { pointerId: 1, button: 2 });
    await fireEvent.pointerDown(content, { pointerId: 1, button: 2 });
    await fireEvent.pointerUp(content, { pointerId: 1 });
    expect(within(dialog).getByText('Cursor: click in the waveform')).toBeInTheDocument();
  });

  it('places the cursor with a click, scrubs, and sets fields to it', async () => {
    const { dialog, content, user } = await open();
    await fireEvent.pointerDown(content, { pointerId: 1, button: 0, clientX: x(3) });
    await fireEvent.pointerMove(content, { pointerId: 1, clientX: x(6) });
    await fireEvent.pointerUp(content, { pointerId: 1 });
    await settle();
    expect(within(dialog).getByText('Cursor 6.000 s')).toBeInTheDocument();

    for (const name of [
      'Start to position',
      'End to position',
      'Fade-in to position',
      'Fade-out from position',
      'Vamp 1 start to position',
      'Vamp 1 end to position',
    ]) {
      await user.click(within(dialog).getByRole('button', { name }));
    }
    await user.click(within(dialog).getByRole('button', { name: '+ Vamp' }));
    await user.click(within(dialog).getByRole('button', { name: 'Remove vamp 1' }));
    await user.click(within(dialog).getByRole('button', { name: /Done/ }));
    await settle();
    expect(lastUpdate()).toMatchObject({ start: 6, end: 6.01, fade_in: 0, fade_out: 0.01, end_fade: true });
  });

  it('sets the times in the fields, and undoes and redoes them in the draft', async () => {
    const { dialog, user } = await open();
    const start = within(dialog).getByRole('spinbutton', { name: 'Start (s)' });
    await user.clear(start);
    await user.type(start, '1.25');
    await user.tab();
    const end = within(dialog).getByRole('spinbutton', { name: 'End (s, empty = end of file)' });
    await user.clear(end);
    await user.tab();
    for (const name of ['Fade-in (s)', 'Fade-out (s)']) {
      const field = within(dialog).getByRole('spinbutton', { name });
      await user.clear(field);
      await user.type(field, '0.5');
      await user.tab();
    }
    for (const name of ['Vamp 1 start', 'Vamp 1 end']) {
      const field = within(dialog).getByRole('spinbutton', { name });
      await user.clear(field);
      await user.type(field, name.endsWith('start') ? '4.5' : '7');
      await user.tab();
    }
    await user.click(within(dialog).getByRole('checkbox', { name: /End fade/ }));
    await settle();
    expect(start).toHaveValue(1.25);

    undo();
    await settle();
    expect(within(dialog).getByRole('checkbox', { name: /End fade/ })).toBeChecked();
    redo();
    await settle();
    expect(within(dialog).getByRole('checkbox', { name: /End fade/ })).not.toBeChecked();

    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    await settle();
    expect(lastUpdate()).toMatchObject({
      start: 1.25,
      end: null,
      fade_in: 0.5,
      fade_out: 0.5,
      end_fade: false,
      vamps: [{ start: 4.5, end: 7 }],
    });
  });

  it('sets and removes marks with a double-click', async () => {
    const { dialog, content, user } = await open();
    await fireEvent.dblClick(content, { clientX: x(7) });
    await fireEvent.dblClick(content, { clientX: x(5) });
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    await settle();
    expect(lastUpdate()).toMatchObject({ marks: [7] });
  });

  it('zooms with the buttons and Ctrl+wheel, and scrolls with the wheel', async () => {
    const { dialog, user } = await open();
    const zoomIn = within(dialog).getByRole('button', { name: 'Zoom in' });
    const zoomOut = within(dialog).getByRole('button', { name: 'Zoom out' });
    const whole = within(dialog).getByRole('button', { name: 'Whole file' });
    expect(zoomOut).toBeDisabled();
    await user.click(zoomIn);
    await settle();
    expect(zoomOut).toBeEnabled();
    const viewport = dialog.querySelector('[data-slot="scroll-area-viewport"]') as HTMLElement;
    const before = viewport.scrollLeft;
    await fireEvent.wheel(viewport, { deltaY: 100 });
    expect(viewport.scrollLeft).toBe(before + 100);
    await fireEvent.scroll(viewport);
    await fireEvent.wheel(viewport, { deltaY: -500, ctrlKey: true });
    await settle();
    await user.click(zoomOut);
    await user.click(whole);
    await settle();
    expect(whole).toBeDisabled();
  });

  it('scrolls with the scrollbar by pointer and keys', async () => {
    const { dialog, user } = await open();
    await user.click(within(dialog).getByRole('button', { name: 'Zoom in' }));
    await settle();
    const viewport = dialog.querySelector('[data-slot="scroll-area-viewport"]') as HTMLElement;
    await fireEvent.pointerEnter(dialog.querySelector('[data-slot="scroll-area"]')!);
    // bits-ui shows the scrollbar a frame after it measured the overflow.
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await settle();
    const bar = within(dialog).getByRole('scrollbar', { name: 'Scroll the waveform horizontally' });
    // Zoomed in twice: 2000 px of content in a 1000 px view, a 500 px thumb with 500 px of travel.
    await fireEvent.pointerDown(bar, { pointerId: 1, button: 0, clientX: 600 });
    expect(viewport.scrollLeft).toBe(700);
    await fireEvent.pointerMove(bar, { pointerId: 1, clientX: 700 });
    expect(viewport.scrollLeft).toBe(900);
    await fireEvent.pointerUp(bar, { pointerId: 1 });
    await fireEvent.pointerMove(bar, { pointerId: 1, clientX: 100 });
    expect(viewport.scrollLeft).toBe(900);
    const keys: [string, number][] = [
      ['Home', 0],
      ['ArrowRight', 40],
      ['ArrowLeft', 0],
      ['PageDown', 1000],
      ['PageUp', 0],
      ['End', 1000],
      ['a', 1000],
    ];
    for (const [key, left] of keys) {
      await fireEvent.keyDown(bar, { key });
      expect(viewport.scrollLeft, key).toBe(left);
    }
  });

  it('plays the preview from the cursor and the edges, and follows it', async () => {
    const { dialog, content, user } = await open();
    await user.click(within(dialog).getByRole('button', { name: /Before the end/ }));
    await user.click(within(dialog).getByRole('button', { name: /Start to end/ }));
    await fireEvent.keyDown(dialog, { key: ' ' });
    expect(backend.invoked('preview_at')).toEqual([
      { cue: 's', at: 3, until: null },
      { cue: 's', at: 0, until: 8 },
      { cue: 's', at: 0, until: null },
    ]);
    await backend.state(runtime({ preview: { cue: 's', playing: true, elapsed: 1, progress: 0.1, position: 7.5 } }));
    expect(within(dialog).getByText('▶ 7.500 s')).toBeInTheDocument();
    // Scrubbing while it plays plays from there.
    await fireEvent.pointerDown(content, { pointerId: 1, button: 0, clientX: x(2) });
    await fireEvent.pointerUp(content, { pointerId: 1 });
    expect(backend.invoked('preview_at').at(-1)).toEqual({ cue: 's', at: 2, until: null });
    // Zoomed in, the view follows the playhead.
    await user.click(within(dialog).getByRole('button', { name: 'Zoom in' }));
    await backend.state(runtime({ preview: { cue: 's', playing: true, elapsed: 1, progress: 0.1, position: 9 } }));
    await user.click(within(dialog).getByRole('button', { name: /Stop/ }));
    expect(backend.actions).toContainEqual({ action: 'stop_preview' });
    // Space on a text field stays there; keyup is held back for the player.
    await fireEvent.keyUp(window, { key: ' ' });
  });

  it('stops its preview when closed', async () => {
    const { unmount } = await open();
    await backend.state(runtime({ preview: { cue: 's', playing: true, elapsed: 1, progress: 0.1, position: 2 } }));
    unmount();
    expect(backend.actions).toEqual([{ action: 'stop_preview' }]);
  });

  it('trims the end onto the audible part', async () => {
    backend.on('audible_range', () => ({ start: 0, end: 9.5 }));
    const { dialog, user } = await open();
    await user.click(within(dialog).getByRole('button', { name: /Trim end/ }));
    await settle();
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    await settle();
    expect(lastUpdate()).toMatchObject({ end: 9.5 });
  });

  it('shows the rate of a cue that plays faster', async () => {
    const { dialog } = await open({ cue: { rate: 120 } });
    expect(within(dialog).getByText(/120/)).toBeInTheDocument();
  });

  it('is read-only in show mode', async () => {
    const { dialog, content } = await open({ locked: true });
    expect(within(dialog).queryByRole('button', { name: 'Start' })).toBeNull();
    await fireEvent.dblClick(content, { clientX: x(7) });
    expect(within(dialog).getByRole('button', { name: /Trim start/ })).toBeDisabled();
  });

  it('closes when its cue goes away', async () => {
    const { onclose } = await open();
    await loadShow(view([]));
    await settle();
    expect(onclose).toHaveBeenCalled();
  });

  it('shows a failed waveform', async () => {
    const v = view([cue({ file: 'broken.wav' })], { settings: { preview_channel: 3 } });
    await loadShow(v);
    backend.fail('waveform', 'broken file');
    await renderUi(WaveformDialog, { cueId: 's', onclose: () => {} });
    await settle();
    expect(screen.getByText('broken file')).toBeInTheDocument();
  });
});
