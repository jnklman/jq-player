<script lang="ts">
  import { type Component, untrack } from 'svelte';

  import TextB from '~icons/ph/text-b';
  import TextItalic from '~icons/ph/text-italic';
  import TextStrikethrough from '~icons/ph/text-strikethrough';
  import TextUnderline from '~icons/ph/text-underline';
  import { ButtonGroup, ButtonGroupButton } from '$lib/components/ui/button-group';

  import { t } from '../../lib/i18n/index.svelte';
  import type { Format } from '../../lib/richText';
  import { applyFormat, createRichTextEditor, type RichTextEditor } from '../../lib/richTextEditor';
  import { FORMAT_KEYS } from '../../lib/shortcuts';

  // A multi-line text with simple formatting: a formatting bar and the editor,
  // which shows the formats as you type (lib/richTextEditor.ts). Like a
  // textarea, `onchange` follows when the field loses the focus.
  let {
    value,
    placeholder = '',
    disabled = false,
    label,
    class: className = '',
    onchange,
  }: {
    value: string;
    placeholder?: string;
    disabled?: boolean;
    label: string;
    class?: string;
    onchange: (value: string) => void;
  } = $props();

  let host = $state<HTMLDivElement | null>(null);
  let editor = $state<RichTextEditor | null>(null);

  const formats: { format: Format; label: string; icon: Component }[] = $derived([
    { format: 'bold', label: t('richtext.bold'), icon: TextB },
    { format: 'italic', label: t('richtext.italic'), icon: TextItalic },
    { format: 'underline', label: t('richtext.underline'), icon: TextUnderline },
    { format: 'strike', label: t('richtext.strike'), icon: TextStrikethrough },
  ]);

  $effect(() => {
    if (!host) return;
    // Created once; later values reach it through the effects below.
    const created = untrack(() =>
      createRichTextEditor(host!, { text: value, placeholder, disabled, label, onchange: (text) => onchange(text) }),
    );
    editor = created;
    return () => {
      created.destroy();
      editor = null;
    };
  });

  $effect(() => editor?.load(value));
  $effect(() => editor?.setPlaceholder(placeholder));
  $effect(() => editor?.setDisabled(disabled));
</script>

<div class="flex flex-col gap-2">
  <ButtonGroup aria-label={t('richtext.formatting')}>
    {#each formats as { format, label: name, icon: Icon } (format)}
      <ButtonGroupButton
        size="icon-sm"
        tip={name}
        keys={FORMAT_KEYS[format]}
        aria-label={name}
        {disabled}
        onmousedown={(e: MouseEvent) => e.preventDefault()}
        onclick={() => editor && applyFormat(editor.view, format)}
      >
        <Icon />
      </ButtonGroupButton>
    {/each}
  </ButtonGroup>
  <div
    bind:this={host}
    class={[
      'w-full min-w-0 resize-y overflow-auto rounded-lg border border-input bg-background px-2.5 py-2 text-sm text-foreground select-text has-focus-visible:outline-2 has-focus-visible:outline-offset-1 has-focus-visible:outline-ring',
      disabled && 'text-fg-2',
      className,
    ]}
  ></div>
</div>
