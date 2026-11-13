// Outgoing MIDI messages: defaults per kind and a short description.

import type { MidiMessage } from './api';
import { options, t } from './i18n/index.svelte';

export const midiKinds = options<MidiMessage['kind']>({
  program_change: 'midi.kind.program_change',
  control_change: 'midi.kind.control_change',
  note_on: 'midi.kind.note_on',
  note_off: 'midi.kind.note_off',
  msc: 'midi.kind.msc',
});

/** A fresh message of `kind`, keeping the channel where both have one. */
export function midiMessageOf(kind: MidiMessage['kind'], from?: MidiMessage): MidiMessage {
  const channel = from && 'channel' in from ? from.channel : 0;
  switch (kind) {
    case 'program_change':
      return { kind, channel, program: 0 };
    case 'control_change':
      return { kind, channel, controller: 0, value: 127 };
    case 'note_on':
      return { kind, channel, note: 60, velocity: 127 };
    case 'note_off':
      return { kind, channel, note: 60 };
    case 'msc':
      return { kind, device: 127, command: 'go', cue: '', list: '' };
  }
}

/** e.g. "PC 5 · Channel 1" (programs counted from 1, like `fmtMidi`), "MSC GO 12.5". */
export function midiSummary(message: MidiMessage): string {
  switch (message.kind) {
    case 'program_change':
      return `PC ${message.program + 1} · ${t('midi.channel', { channel: message.channel + 1 })}`;
    case 'control_change':
      return `CC ${message.controller}=${message.value} · ${t('midi.channel', { channel: message.channel + 1 })}`;
    case 'note_on':
      return `${t('midi.note_on', { note: message.note })} · ${t('midi.channel', { channel: message.channel + 1 })}`;
    case 'note_off':
      return `${t('midi.note_off', { note: message.note })} · ${t('midi.channel', { channel: message.channel + 1 })}`;
    case 'msc':
      return `MSC ${message.command.toUpperCase()} ${message.cue || t('midi.msc_next')}`.trim();
  }
}
