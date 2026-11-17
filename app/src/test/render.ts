// Rendering components as the app shows them (inside the tooltip provider), with a user to click and type.

import { render } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import type { Component } from 'svelte';

import Harness from './Harness.svelte';
import { settle } from './store';

/** Mounts `component` with `props` and lets it settle; `user` clicks and types like a person (no delays). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- any component with any props
export async function renderUi(component: Component<any>, props: Record<string, unknown> = {}) {
  const user = userEvent.setup({ delay: null, pointerEventsCheck: 0 });
  const result = render(Harness, { props: { component, props } });
  await settle();
  return { ...result, user };
}
