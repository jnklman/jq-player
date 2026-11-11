<script lang="ts">
  import * as AlertDialog from '$lib/components/ui/alert-dialog';
  import { Button } from '$lib/components/ui/button';

  import type { Question } from '../lib/store.svelte';

  let { question }: { question: Question } = $props();

  let buttons = $state<(HTMLElement | null)[]>([]);
  const focused = $derived(
    Math.max(
      0,
      question.choices.findIndex((choice) => choice.focus),
    ),
  );
</script>

<!-- Esc stays STOP ALL and never answers; only the buttons do. -->
<AlertDialog.Root open>
  <AlertDialog.Content
    escapeKeydownBehavior="ignore"
    onOpenAutoFocus={(e) => {
      e.preventDefault();
      buttons[focused]?.focus();
    }}
  >
    <AlertDialog.Header>
      <AlertDialog.Title>{question.title}</AlertDialog.Title>
    </AlertDialog.Header>
    <AlertDialog.Description class="wrap-break-word whitespace-pre-line">{question.text}</AlertDialog.Description>
    <AlertDialog.Footer>
      {#each question.choices as choice, i (choice.value)}
        <Button
          bind:ref={() => buttons[i] ?? null, (el) => (buttons[i] = el)}
          variant={choice.danger ? 'destructive' : choice.focus ? 'default' : 'outline'}
          onclick={() => question.answer(choice.value)}>{choice.label}</Button
        >
      {/each}
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
