<script lang="ts">
  import { type CueEdit, isMixed } from '../../../lib/cueEdit';
  import { t } from '../../../lib/i18n/index.svelte';
  import RichTextField from '../../fields/RichTextField.svelte';

  // "Detail text": the formatted text field, as tall as the panel allows.
  let { edit }: { edit: CueEdit } = $props();

  const notes = $derived(edit.value<string>('notes'));
</script>

<div class="mt-3">
  <RichTextField
    class="h-72 min-h-40"
    label={t('inspector.tab.notes')}
    placeholder={isMixed(notes) ? t('field.mixed') : t('notes.placeholder')}
    value={isMixed(notes) ? '' : notes}
    disabled={edit.locked}
    onchange={(text) => edit.set({ notes: text })}
  />
</div>
