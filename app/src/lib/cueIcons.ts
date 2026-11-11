// One icon per cue type, for the add menu (and anywhere else a type needs a symbol).

import type { Component } from 'svelte';

import ArrowCounterClockwise from '~icons/ph/arrow-counter-clockwise-bold';
import ArrowElbowDownRight from '~icons/ph/arrow-elbow-down-right-bold';
import ArrowsLeftRight from '~icons/ph/arrows-left-right-bold';
import Broadcast from '~icons/ph/broadcast-bold';
import FileDashed from '~icons/ph/file-dashed-bold';
import FilmSlate from '~icons/ph/film-slate-bold';
import Gauge from '~icons/ph/gauge-bold';
import Hourglass from '~icons/ph/hourglass-bold';
import Pause from '~icons/ph/pause-bold';
import PianoKeys from '~icons/ph/piano-keys-bold';
import Playlist from '~icons/ph/playlist-bold';
import Power from '~icons/ph/power-bold';
import SignOut from '~icons/ph/sign-out-bold';
import SkipForward from '~icons/ph/skip-forward-bold';
import SpeakerHigh from '~icons/ph/speaker-high-bold';
import Stack from '~icons/ph/stack-bold';
import Timer from '~icons/ph/timer-bold';
import Waveform from '~icons/ph/waveform-bold';

import type { Cue } from './api';
import type { AddItem } from './control';

export const cueIcons: Record<Cue['type'], Component> = {
  sound: Waveform,
  playlist: Playlist,
  start_stop: Power,
  pause: Pause,
  volume: SpeakerHigh,
  pan: ArrowsLeftRight,
  rate: Gauge,
  position: SkipForward,
  exit_loop: SignOut,
  reset: ArrowCounterClockwise,
  stopwatch: Timer,
  wait: Hourglass,
  group: Stack,
  goto: ArrowElbowDownRight,
  osc: Broadcast,
  midi: PianoKeys,
  scene: FilmSlate,
};

/** Placeholders: sound cues without a file. */
export const placeholderIcon = FileDashed;

/** By the add menu's kinds. */
export const addIcons: Record<AddItem['kind'], Component> = { ...cueIcons, placeholder: placeholderIcon };
