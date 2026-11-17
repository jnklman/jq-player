import { screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { active, control, runtime, sound, view } from '../test/fixtures';
import { renderUi } from '../test/render';
import { loadShow, settle } from '../test/store';
import Transport from './Transport.svelte';

const go = () => screen.getByRole('button', { name: /^GO/ });

describe('Transport', () => {
  it('names the next cue and sends GO and the global stops', async () => {
    const rain = sound('a', { number: '1', title: 'Rain', script_ref: 'p. 4' });
    await loadShow(view([rain]), runtime({ playhead: 'a', playhead_armed: true }));
    const { user } = await renderUi(Transport);
    expect(screen.getByText('Rain')).toBeInTheDocument();
    expect(screen.getByText('p. 4')).toBeInTheDocument();
    expect(go()).toHaveTextContent('Space');
    await user.click(go());
    await user.click(screen.getByRole('button', { name: /FADE ALL/i }));
    await user.click(screen.getByRole('button', { name: /PAUSE ALL/i }));
    await user.click(screen.getByRole('button', { name: /STOP ALL/i }));
    expect(backend.actions).toEqual([
      { action: 'go' },
      { action: 'fade_all' },
      { action: 'toggle_pause_all' },
      { action: 'stop_all' },
    ]);
    // Without a key of its own, the stop button shows the panic key.
    expect(screen.getByRole('button', { name: /STOP ALL/i })).toHaveTextContent('Esc');
  });

  it('has no GO past the end of the list or without cues', async () => {
    await loadShow(view([sound('a')]), runtime());
    await renderUi(Transport);
    expect(go()).toBeDisabled();
    expect(screen.getByText('End of the cue list')).toBeInTheDocument();
    await loadShow(view([]), runtime());
    await settle();
    expect(screen.getByText('No cues')).toBeInTheDocument();
  });

  it('offers to resume while everything is paused', async () => {
    await loadShow(view([sound('a')]), runtime({ active: [active('a', { paused: true })], any_paused: true }));
    await renderUi(Transport);
    expect(screen.getByRole('button', { name: /RESUME ALL/i })).toBeInTheDocument();
  });

  it('shows what a control cue acts on', async () => {
    const cues = [sound('a', { number: '1', title: 'Music' }), control('f', 'volume', { number: '2' })];
    await loadShow(view(cues), runtime({ playhead: 'f', playhead_armed: true }));
    await renderUi(Transport);
    expect(screen.getByText('Fade → 1')).toBeInTheDocument();
  });

  it('shows "loading" only once arming takes long', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      await loadShow(view([sound('a')]), runtime({ playhead: 'a', playhead_armed: false }));
      await renderUi(Transport);
      expect(screen.queryByText('loading …')).toBeNull();
      vi.advanceTimersByTime(400);
      await settle();
      expect(screen.getByText('loading …')).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it('flashes GO when the lockout drops a GO', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      await loadShow(view([sound('a')]), runtime({ playhead: 'a', go_rejected: 3 }));
      await renderUi(Transport);
      expect(go()).not.toHaveTextContent('too fast');
      await backend.state(runtime({ playhead: 'a', go_rejected: 4 }));
      expect(go()).toHaveTextContent('too fast – dropped');
      vi.advanceTimersByTime(400);
      await settle();
      expect(go()).not.toHaveTextContent('too fast');
    } finally {
      vi.useRealTimers();
    }
  });
});
