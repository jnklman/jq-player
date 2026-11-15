import { beforeEach } from 'vitest';

import { i18n } from './i18n/index.svelte';

// The UI starts in the system language (`navigator.languages`); tests expect English texts and numbers
// whatever the machine's locale is. A test of another language sets `i18n.locale` itself.
beforeEach(() => {
  i18n.locale = 'en';
});
