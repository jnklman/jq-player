import { screen, within } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { canControl, type RemoteState, ui } from '$lib/store.svelte';

import { backend } from '../test/backend';
import { active, chord, runtime, scene, sound, view } from '../test/fixtures';
import { renderUi } from '../test/render';
import { loadShow, settle } from '../test/store';
import { hostKeys, onRemoteKey, withKey } from './keys';
import { remote } from './remote.svelte';
import RemoteApp from './RemoteApp.svelte';
import RemoteCueList from './RemoteCueList.svelte';

// The remote page against the fake backend (Tauri's mock stands in for the WebSocket: the same commands and
// events); `ui.remote` is what the socket would report.

const go = () => screen.getByRole('button', { name: /^GO/ });

function as(role: RemoteState['role'], extra: Partial<RemoteState> = {}) {
  ui.remote = { role, connected: true, locked: false, keys: null, preview: false, ...extra };
}

async function showLoaded() {
  const cues = [
    scene('s', { title: 'Act 1' }),
    sound('a', { number: '1', title: 'Rain' }),
    sound('b', { number: '2', title: 'Thunder' }),
  ];
  await loadShow(view(cues), runtime({ playhead: 'a', playhead_armed: true, active: [active('b')] }));
}

describe('RemoteApp', () => {
  beforeEach(() => {
    remote.phase = 'live';
    remote.name = 'Tablet';
  });

  it('only shows the show to a device that watches', async () => {
    await showLoaded();
    as('view');
    await renderUi(RemoteApp);
    expect(screen.getByText('Rain')).toBeInTheDocument();
    expect(go()).toBeDisabled();
    for (const name of [/FADE ALL/i, /PAUSE ALL/i, /STOP ALL/i]) {
      expect(screen.getByRole('button', { name })).toBeDisabled();
    }
    expect(screen.getByRole('button', { name: /stop cue 2/i })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Lock controls' })).not.toBeInTheDocument();
  });

  it('lets a device that operates send GO, until its controls are locked', async () => {
    await showLoaded();
    as('control');
    const { user } = await renderUi(RemoteApp);
    await user.click(go());
    expect(backend.actions).toEqual([{ action: 'go' }]);
    await user.click(screen.getByRole('button', { name: 'Lock controls' }));
    expect(ui.remote?.locked).toBe(true);
    expect(go()).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Unlock controls' }));
    expect(go()).toBeEnabled();
  });

  it('covers everything while the connection is lost', async () => {
    await showLoaded();
    as('control', { connected: false });
    await renderUi(RemoteApp);
    expect(screen.getByRole('alert')).toHaveTextContent('Disconnected – display out of date');
    expect(go()).toBeDisabled();
    expect(canControl()).toBe(false);
  });

  it('waits for the host to let it in', async () => {
    as('pending');
    await renderUi(RemoteApp);
    expect(screen.getByText('Waiting for approval')).toBeInTheDocument();
    expect(screen.getByText(/“Tablet” asked to join/)).toBeInTheDocument();
  });

  it('asks a device the host does not know for its name', async () => {
    remote.phase = 'join';
    await renderUi(RemoteApp);
    expect(screen.getByRole('button', { name: 'Ask to join' })).toBeDisabled();
  });
});

describe('RemoteCueList', () => {
  it('marks the playhead and offers actions on a picked cue only to a device that operates', async () => {
    await showLoaded();
    as('view');
    const { user } = await renderUi(RemoteCueList);
    const rows = screen.getAllByRole('option');
    expect(rows.map((row) => row.textContent?.includes('Rain'))).toEqual([true, false]);
    expect(rows[0].className).toContain('outline-primary');
    expect(screen.getByText('Act 1')).toBeInTheDocument();
    await user.click(rows[1]);
    expect(rows[1]).toHaveAttribute('aria-selected', 'true');
    expect(screen.queryByRole('button', { name: 'Set as next' })).not.toBeInTheDocument();
    expect(backend.actions).toEqual([]);

    as('control');
    await settle();
    expect(screen.queryByRole('button', { name: 'Start' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Set as next' }));
    expect(backend.actions).toEqual([{ action: 'set_playhead', cue: 'b' }]);
    expect(rows[1], 'the pick is let go').toHaveAttribute('aria-selected', 'false');
    expect(screen.queryByRole('button', { name: 'Set as next' })).not.toBeInTheDocument();
  });
});

describe('local preview', () => {
  it('plays a sound cue on the device only if the host lets it', async () => {
    await showLoaded();
    as('view');
    backend.on('local_preview', () => ({ id: 'abc', url: '/media/abc.wav', duration: 2 }));
    // jsdom has no media playback.
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue();
    const { user } = await renderUi(RemoteCueList);
    await user.click(screen.getAllByRole('option')[1]);
    expect(screen.queryByRole('button', { name: 'Preview' })).not.toBeInTheDocument();

    as('view', { preview: true });
    await settle();
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    await settle();
    expect(backend.invoked('local_preview')).toEqual([{ cue: 'b' }]);
    const player = screen.getByRole('group', { name: 'Preview of 2 Thunder' });
    expect(player.querySelector('audio')).toHaveAttribute('src', '/media/abc.wav');
    expect(play, 'starts at once').toHaveBeenCalled();
    expect(within(player).getByText('0:00 / 0:02')).toBeInTheDocument();
    expect(within(player).getByText('rendering 0 %'), 'nothing heard of the host yet').toBeInTheDocument();
    remote.progress = { id: 'abc', rendered: 2, done: true, failed: false };
    await settle();
    expect(within(player).queryByText(/rendering/)).not.toBeInTheDocument();
    remote.progress = { id: 'abc', rendered: 1, done: false, failed: true };
    await settle();
    expect(within(player).getByText('Cannot be played')).toBeInTheDocument();
    const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
    as('view', { preview: true, connected: false });
    await settle();
    expect(pause, 'a covered player does not go on playing').toHaveBeenCalled();
    pause.mockRestore();
    as('view', { preview: true });
    await settle();
    expect(within(player).getByRole('slider')).toBeInTheDocument();
    await user.click(within(player).getByRole('button', { name: 'Close preview' }));
    expect(screen.queryByRole('group', { name: 'Preview of 2 Thunder' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    await settle();
    screen
      .getByRole('group', { name: 'Preview of 2 Thunder' })
      .querySelector('audio')!
      .dispatchEvent(new Event('ended'));
    await settle();
    expect(screen.queryByRole('group', { name: 'Preview of 2 Thunder' }), 'closes once played through').toBeNull();
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    await settle();
    play.mockRestore();
    expect(backend.actions, 'nothing happens on the host').toEqual([]);

    as('view', { preview: false });
    await settle();
    expect(screen.queryByRole('group', { name: 'Preview of 2 Thunder' })).not.toBeInTheDocument();
  });
});

describe('remote keys', () => {
  it('sends the global actions by the host bindings, only while the device operates', async () => {
    await showLoaded();
    as('view');
    const space = () => new KeyboardEvent('keydown', { code: 'Space', key: ' ', cancelable: true });
    expect(onRemoteKey(space())).toBe(false);
    as('control');
    expect(onRemoteKey(space())).toBe(true);
    expect(onRemoteKey(new KeyboardEvent('keydown', { code: 'Escape', key: 'Escape' }))).toBe(true);
    // Editing keys do nothing here.
    expect(onRemoteKey(new KeyboardEvent('keydown', { code: 'KeyZ', key: 'z', ctrlKey: true }))).toBe(false);
    await settle();
    expect(backend.actions).toEqual([{ action: 'go' }, { action: 'panic' }]);
  });
});

describe('own keys', () => {
  beforeEach(() => {
    remote.phase = 'live';
  });

  it('replace the host bindings on the device', async () => {
    await showLoaded();
    as('control', { keys: [{ chord: chord('F1'), action: 'stop_all' }] });
    const key = (code: string) => new KeyboardEvent('keydown', { code, key: code, cancelable: true });
    expect(onRemoteKey(key('Space')), "the host's GO key").toBe(false);
    expect(onRemoteKey(key('F1'))).toBe(true);
    expect(onRemoteKey(key('Escape')), 'Esc always panics').toBe(true);
    await settle();
    expect(backend.actions).toEqual([{ action: 'stop_all' }, { action: 'panic' }]);
  });

  it('start from the host keys the remote page carries out, and move a key to the action it is set for', async () => {
    await showLoaded();
    expect(hostKeys()).toEqual([{ chord: chord('Space', { label: ' ' }), action: 'go' }]);
    let keys = withKey(hostKeys(), 'go', 1, chord('F1'));
    expect(keys.map((binding) => binding.chord.code)).toEqual(['Space', 'F1']);
    keys = withKey(keys, 'stop_all', 0, chord('F1'));
    expect(keys).toEqual([
      { chord: chord('Space', { label: ' ' }), action: 'go' },
      { chord: chord('F1'), action: 'stop_all' },
    ]);
    expect(withKey(keys, 'go', 0, null)).toEqual([{ chord: chord('F1'), action: 'stop_all' }]);
    expect(withKey(keys, 'go', 1, chord('Space', { label: ' ' })), 'the same key twice counts once').toHaveLength(2);
  });

  it('are set on the device and sent to the host', async () => {
    await showLoaded();
    as('control');
    const { user } = await renderUi(RemoteApp);
    await user.click(screen.getByRole('button', { name: 'Keys of this device' }));
    const dialog = screen.getByRole('dialog');
    const first = within(dialog).getByRole('button', { name: 'STOP ALL, first key' });
    expect(first).toBeDisabled();
    await user.click(within(dialog).getByRole('checkbox', { name: 'Own keys for this device' }));
    await user.click(first);
    await user.keyboard('[F2]');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    await settle();
    expect(backend.invoked('set_own_keys')).toEqual([
      {
        keys: [
          { chord: chord('Space', { label: ' ' }), action: 'go' },
          expect.objectContaining({ chord: expect.objectContaining({ code: 'F2' }), action: 'stop_all' }),
        ],
      },
    ]);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(backend.actions, 'recording a key is not operating').toEqual([]);
  });
});
