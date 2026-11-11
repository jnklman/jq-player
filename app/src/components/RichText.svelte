<script lang="ts">
  import { parseRichText, pieceClass } from '../lib/richText';

  // A detail text with its formatting (see lib/richText.ts), inline; line
  // breaks need `whitespace-pre-wrap` on the parent.
  let { text }: { text: string } = $props();

  const segments = $derived(parseRichText(text));
</script>

{#each segments as segment, i (i)}<span class={pieceClass(segment)}>{segment.text}</span>{/each}
