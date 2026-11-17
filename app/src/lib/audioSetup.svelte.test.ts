import { flushSync } from 'svelte';
import { describe, expect, it } from 'vitest';

import { backend } from '../test/backend';
import { settings, view } from '../test/fixtures';
import { settle } from '../test/store';
import type { DeviceInfo, LocalSettings, ShowView } from './api';
import { AudioSetup, fits } from './audioSetup.svelte';

const device = (id: string, extra: Partial<DeviceInfo> = {}): DeviceInfo => ({
  id,
  name: id,
  description: id,
  default_config: null,
  max_channels: 8,
  buffer_range: [64, 1024],
  ...extra,
});

/** A setup over a reactive draft; `devices` are what the backend lists for every host. */
async function setup(draftExtra: Partial<LocalSettings> = {}, viewExtra: Parameters<typeof view>[1] = {}) {
  backend.on('audio_devices', ({ host }) => ({
    hosts: ['WASAPI', 'ASIO', 'JACK'],
    host: host ?? 'WASAPI',
    devices: [device('desk'), device('phones', { max_channels: 2, buffer_range: null })],
  }));
  const draft = $state(settings(draftExtra));
  const current: ShowView = view([], viewExtra);
  const audio = new AudioSetup(
    () => draft,
    () => current,
  );
  await settle();
  return { audio, draft, current };
}

describe('AudioSetup', () => {
  it('lists the devices of the draft host, keeping only the latest answer', async () => {
    const { audio, draft } = await setup({ host: 'ASIO' });
    expect(backend.invoked('audio_devices')).toEqual([{ host: 'ASIO' }]);
    expect(audio.devices?.host).toBe('ASIO');
    expect(audio.loading).toBe(false);

    audio.setHost('');
    expect(draft.host).toBeNull();
    expect(draft.device).toBeNull();
    let release!: () => void;
    backend.on('audio_devices', () => new Promise((resolve) => (release = () => resolve({ host: 'old' }))));
    void audio.load('JACK');
    backend.on('audio_devices', () => ({ hosts: [], host: 'new', devices: [] }));
    await audio.load('WASAPI');
    release();
    await settle();
    expect(audio.devices?.host).toBe('new');
  });

  it('lists the devices of the draft host anew on reload', async () => {
    const { audio, draft } = await setup({ host: 'ASIO' });
    draft.device = 'new';
    expect(audio.unknownDevice).toBe(true);
    backend.on('audio_devices', ({ host }) => ({ hosts: [], host, devices: [device('new')] }));
    audio.reload();
    await settle();
    expect(backend.invoked('audio_devices')).toEqual([{ host: 'ASIO' }, { host: 'ASIO' }]);
    expect(audio.selectedDevice?.id).toBe('new');
  });

  it('keeps an unknown device as its own option', async () => {
    const { audio, draft } = await setup({ device: 'desk' });
    expect(audio.selectedDevice?.id).toBe('desk');
    expect(audio.unknownDevice).toBe(false);
    draft.device = 'unplugged';
    flushSync();
    expect(audio.unknownDevice).toBe(true);
  });

  it('tells when the device will be reopened', async () => {
    const { audio, draft } = await setup();
    expect(audio.audioChanged).toBe(false);
    draft.buffer_frames = 256;
    expect(audio.audioChanged).toBe(true);
  });

  it('offers the buffer sizes the device takes, and the size it runs at by default', async () => {
    const { audio, draft } = await setup();
    expect(audio.bufferSizes).toEqual([32, 64, 128, 256, 512, 1024, 2048, 4096]);
    expect(audio.defaultFrames).toBe(480);
    expect(audio.latency(480)).toBe('10.0 ms');
    draft.device = 'desk';
    expect(audio.bufferSizes).toEqual([64, 128, 256, 512, 1024]);
    expect(audio.defaultFrames).toBeNull();
    expect(audio.hostSetsBuffer).toBe(false);
    draft.host = 'JACK';
    expect(audio.hostSetsBuffer).toBe(true);
  });

  it('switches the preview between off, the main device and another device', async () => {
    const { audio, draft } = await setup({ device: 'desk' });
    expect(audio.previewMode).toBe('off');
    audio.setPreviewMode('main');
    expect([draft.preview_device, draft.preview_channel]).toEqual([null, 3]);
    expect(audio.previewMode).toBe('main');
    audio.setPreviewMode('device');
    expect([draft.preview_device, draft.preview_channel]).toEqual(['phones', 3]);
    expect(audio.previewMode).toBe('device');
    expect(audio.previewLimit).toBe(2);
    audio.setPreviewMode('off');
    expect([draft.preview_device, draft.preview_channel]).toEqual([null, null]);
  });

  it('checks that the channel pairs fit', async () => {
    const { audio, draft } = await setup({ device: 'phones' });
    expect(audio.mainMax).toBe(2);
    expect(audio.channelsValid).toBe(true);
    draft.channels = 4;
    expect(audio.tooManyChannels).toBe(true);
    expect(audio.channelsValid).toBe(false);
    draft.channels = null;
    draft.outputs = { main: 2 };
    expect(audio.channelsValid).toBe(false);
  });

  it('knows the channels of the open default device only', async () => {
    const { audio, draft } = await setup();
    expect(audio.mainMax).toBe(2);
    draft.host = 'ASIO';
    expect(audio.mainMax).toBeNull();
  });

  it('warns about a preview pair on a show output', async () => {
    const { audio, draft } = await setup({ channels: 8 });
    audio.setPreviewMode('main');
    expect(audio.previewOverlaps).toBe(false);
    draft.preview_channel = 2;
    expect(audio.previewOverlaps).toBe(true);
  });

  it('reports a device list that cannot be read', async () => {
    backend.fail('audio_devices');
    const draft = $state(settings());
    const audio = new AudioSetup(
      () => draft,
      () => view(),
    );
    await settle();
    expect(audio.devices).toBeNull();
  });
});

describe('fits', () => {
  it('needs both channels of the pair within the limit', () => {
    expect(fits(1, 2)).toBe(true);
    expect(fits(2, 2)).toBe(false);
    expect(fits(7, null)).toBe(true);
  });
});
