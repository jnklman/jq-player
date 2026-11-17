// The audio part of the settings dialog: the devices of the chosen host and
// whether the draft's channels fit them. Reads (and for host and preview
// changes writes) the dialog's draft; the open device comes from the view.

import { api, type AudioDevices, type LocalSettings, type ShowView } from './api';
import { fmtNumber, t } from './i18n/index.svelte';
import { type Option, withMissing } from './options';
import { attempt } from './store.svelte';

/** Buffer sizes the backend takes (powers of two). */
const BUFFER_SIZES = [32, 64, 128, 256, 512, 1024, 2048, 4096];

export class AudioSetup {
  devices = $state<AudioDevices | null>(null);
  loading = $state(false);
  private readonly draft: () => LocalSettings;
  private readonly view: () => ShowView;

  constructor(draft: () => LocalSettings, view: () => ShowView) {
    this.draft = draft;
    this.view = view;
    void this.load(draft().host);
  }

  /** Counts the loads, so only the latest one's answer is kept. */
  private loads = 0;

  async load(host: string | null) {
    const mine = ++this.loads;
    this.loading = true;
    const devices = (await attempt(api.audioDevices(host ?? undefined))) ?? null;
    if (mine !== this.loads) return;
    this.devices = devices;
    this.loading = false;
  }

  /** Lists the devices anew, e.g. after one was plugged in. */
  reload() {
    void this.load(this.draft().host);
  }

  /** Another host: its default device, and its device list. */
  setHost(host: string) {
    this.draft().host = host || null;
    this.draft().device = null;
    void this.load(this.draft().host);
  }

  readonly selectedDevice = $derived.by(() =>
    this.devices?.devices.find((device) => device.id === this.draft().device),
  );
  /** A device stored by name (or one that is unplugged) is kept as its own option. */
  readonly unknownDevice = $derived.by(
    () => this.draft().device !== null && this.devices !== null && !this.selectedDevice,
  );

  /** The device will be reopened on apply. */
  readonly audioChanged = $derived.by(() => {
    const current = this.view().settings;
    return (
      this.draft().host !== current.host ||
      this.draft().device !== current.device ||
      this.draft().channels !== current.channels ||
      this.draft().buffer_frames !== current.buffer_frames
    );
  });

  /** The host sets the buffer size itself (JACK: the server), the setting is ignored. */
  readonly hostSetsBuffer = $derived.by(() => (this.draft().host ?? this.devices?.host) === 'JACK');

  /** Buffer sizes within what the chosen device reports. */
  readonly bufferSizes = $derived.by(() => {
    const [min, max] = this.selectedDevice?.buffer_range ?? [32, 4096];
    return BUFFER_SIZES.filter((frames) => frames >= min && frames <= max);
  });

  /** The size the device runs at with "Standard", while the draft keeps the open device and it runs its own default;
   * null while unknown. */
  readonly defaultFrames = $derived.by(() => {
    const current = this.view();
    const draft = this.draft();
    const same =
      draft.host === current.settings.host &&
      draft.device === current.settings.device &&
      (current.settings.buffer_frames === null || this.hostSetsBuffer) &&
      !current.audio_error;
    return same ? current.audio_frames : null;
  });

  /** The hosts to choose from; one stored but no longer offered stays as its own option. */
  readonly hostOptions = $derived.by<Option<string>[]>(() =>
    withMissing(
      [
        { value: '', label: t('device.system_default') },
        ...(this.devices?.hosts ?? []).map((host) => ({ value: host, label: host })),
      ],
      this.devices ? this.draft().host : null,
      t('device.not_available', { name: this.draft().host ?? '' }),
    ),
  );

  /** The devices of the chosen host. */
  readonly deviceOptions = $derived.by<Option<string>[]>(() => {
    const device = this.draft().device;
    return [
      { value: '', label: this.loading ? t('device.searching') : t('audio.default_device') },
      ...(this.devices?.devices ?? []).map((info) => ({ value: info.id, label: info.name })),
      ...(this.unknownDevice && device ? [{ value: device, label: t('device.not_found', { name: device }) }] : []),
    ];
  });

  /** The buffer sizes the device takes, with their latency; '' is the default. */
  readonly bufferOptions = $derived.by<Option<string>[]>(() => {
    const frames = this.draft().buffer_frames;
    return withMissing(
      [
        {
          value: '',
          label:
            this.defaultFrames === null
              ? t('device.default')
              : t('device.default_frames', { frames: this.defaultFrames, latency: this.latency(this.defaultFrames) }),
        },
        ...this.bufferSizes.map((size) => ({ value: String(size), label: `${size} (${this.latency(size)})` })),
      ],
      frames === null ? null : String(frames),
      t('device.unsupported', { frames: frames ?? 0 }),
    );
  });

  /** The buffer select's value: none while the host sets the size. */
  readonly bufferValue = $derived.by(() => (this.hostSetsBuffer ? '' : String(this.draft().buffer_frames ?? '')));

  setBuffer(frames: string) {
    this.draft().buffer_frames = frames ? Number(frames) : null;
  }

  /** Milliseconds `frames` last at the open device's rate, e.g. "5,3 ms". */
  latency(frames: number): string {
    return `${fmtNumber((frames / this.view().audio.sample_rate) * 1000, 1)} ms`;
  }

  readonly previewMode = $derived.by<'off' | 'main' | 'device'>(() =>
    this.draft().preview_device ? 'device' : this.draft().preview_channel ? 'main' : 'off',
  );

  setPreviewMode(mode: 'off' | 'main' | 'device') {
    const draft = this.draft();
    if (mode === 'off') {
      draft.preview_device = null;
      draft.preview_channel = null;
    } else if (mode === 'main') {
      draft.preview_device = null;
      draft.preview_channel = draft.preview_channel ?? 3;
    } else {
      const other = this.devices?.devices.find((device) => device.id !== draft.device);
      draft.preview_device = draft.preview_device ?? other?.id ?? this.devices?.devices[0]?.id ?? null;
      draft.preview_channel = draft.preview_channel ?? 1;
    }
  }

  /** Most channels the chosen device takes; null while unknown. The open device knows it for "Standardgerät" too,
   * the list only for named ones. */
  readonly mainMax = $derived.by(() => {
    const current = this.view();
    if (this.draft().device !== null) return this.selectedDevice?.max_channels ?? null;
    return this.draft().host === current.settings.host && current.settings.device === null
      ? current.audio.max_channels
      : null;
  });

  /** Channel pairs must fit the set channel count, or else the device. */
  readonly mainLimit = $derived.by(() => this.draft().channels ?? this.mainMax);
  readonly previewLimit = $derived.by(() =>
    this.previewMode === 'device'
      ? (this.devices?.devices.find((device) => device.id === this.draft().preview_device)?.max_channels ?? null)
      : this.mainLimit,
  );
  readonly tooManyChannels = $derived.by(() => {
    const channels = this.draft().channels;
    return channels !== null && this.mainMax !== null && channels > this.mainMax;
  });

  readonly channelsValid = $derived.by(
    () =>
      !this.tooManyChannels &&
      this.view().show.outputs.every((output) => fits(this.draft().outputs[output.id] ?? 1, this.mainLimit)) &&
      (this.previewMode === 'off' || fits(this.draft().preview_channel ?? 1, this.previewLimit)),
  );

  /** A preview pair overlapping a show output would be heard in the hall. */
  readonly previewOverlaps = $derived.by(
    () =>
      this.previewMode === 'main' &&
      this.view().show.outputs.some(
        (output) => Math.abs((this.draft().outputs[output.id] ?? 1) - (this.draft().preview_channel ?? 1)) < 2,
      ),
  );
}

/** Whether the pair starting at `first` (1-based) exists within `limit`. */
export function fits(first: number, limit: number | null): boolean {
  return limit === null || first + 1 <= limit;
}
