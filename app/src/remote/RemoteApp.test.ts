import { screen, within } from '@testing-library/svelte';
import { beforeEach, describe, expect, it } from 'vitest';

import { canControl, type RemoteState, ui } from '$lib/store.svelte';

import { backend } from '../test/backend';
import { active, runtime, scene, sound, view } from '../test/fixtures';
import { renderUi } from '../test/render';
import { loadShow, settle } from '../test/store';
import { onRemoteKey } from './keys';
import { remote } from './remote.svelte';
import RemoteApp from './RemoteApp.svelte';
import RemoteCueList from './RemoteCueList.svelte';

// The remote page against the fake backend (Tauri's mock stands in for the WebSocket: the same commands and
// events); `ui.remote` is what the socket would report.

const go = () => screen.getByRole('button', { name: /^GO/ });

function as(role: RemoteState['role'], extra: Partial<RemoteState> = {}) {
  ui.remote = { role, connected: true, locked: false, ...extra };
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
    await user.click(screen.getByRole('button', { name: 'Set as next' }));
    await user.click(screen.getByRole('button', { name: 'Start' }));
    await user.click(within(screen.getByRole('listbox').parentElement!).getByRole('button', { name: 'Stop' }));
    expect(backend.actions).toEqual([
      { action: 'set_playhead', cue: 'b' },
      { action: 'start_cue', cue: 'b' },
      { action: 'stop_cue', cue: 'b' },
    ]);
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
