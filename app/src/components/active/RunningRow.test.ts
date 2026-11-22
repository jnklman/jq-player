// A running cue in the list of running cues: what it shows while it plays.

import { screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import { active, runtime, sound, view } from '../../test/fixtures';
import { renderUi } from '../../test/render';
import { loadShow } from '../../test/store';
import RunningRow from './RunningRow.svelte';

/** The fill of the progress bar (the bar next to the times). */
const fill = (container: HTMLElement) => container.querySelector<HTMLElement>('[style*="scaleX"]')!;

describe('RunningRow', () => {
  it('fills the bar with the silence between two loop passes', async () => {
    await loadShow(view([sound('s', { loop: 3, loop_delay: 2 })]), runtime());
    const pass = { pass: 1, count: 3, vamp: null };
    const playing = active('s', { progress: 0.5, pass, looping: true });
    const first = await renderUi(RunningRow, { running: playing });
    expect(fill(first.container).style.transform).toBe('scaleX(0.5)');
    expect(screen.queryByText(/Pause before pass/)).toBeNull();
    expect(screen.getByText('Loop 1/3')).toBeInTheDocument();
    first.unmount();

    const silent = { ...playing, progress: 1, loop_gap: { remaining: 1.5, progress: 0.25 } };
    const { container } = await renderUi(RunningRow, { running: silent });
    expect(fill(container).style.transform).toBe('scaleX(0.25)');
    expect(fill(container)).toHaveClass('bg-vamp-soft/50');
    expect(screen.getByText(/Pause before pass 2/)).toHaveTextContent('1.5 s');
    // It takes the place of the pass badge.
    expect(screen.queryByText('Loop 1/3')).toBeNull();
  });
});
