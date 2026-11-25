// Names of the fields a cue made from a library preset can differ in (`PresetState.modified`: keys of the cue, its
// type's fields included), for the inspector's preset box.

import { t, type TextKey } from './i18n/index.svelte';

const NAMES: Record<string, TextKey> = {
  title: 'preset.field.title',
  script_ref: 'preset.field.script_ref',
  notes: 'preset.field.notes',
  color: 'preset.field.color',
  advance: 'preset.field.advance',
  passes: 'preset.field.passes',
  pre_wait: 'preset.field.pre_wait',
  second_trigger: 'preset.field.second_trigger',
  release_trigger: 'preset.field.release_trigger',
  others: 'preset.field.others',
  type: 'preset.field.type',
  device: 'preset.field.device',
  messages: 'preset.field.messages',
  duration: 'preset.field.duration',
  targets: 'preset.field.targets',
};

/** A field's name in the current language; other fields by their key. */
export function presetFieldName(field: string): string {
  const key = NAMES[field];
  return key ? t(key) : field;
}
