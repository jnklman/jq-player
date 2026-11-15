<script lang="ts">
  import { tick } from 'svelte';

  import SlidersHorizontal from '~icons/ph/sliders-horizontal-bold';
  import Textbox from '~icons/ph/textbox-bold';
  import { Button } from '$lib/components/ui/button';
  import Slider from '$lib/components/ui/slider/slider.svelte';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { t } from '../../lib/i18n/index.svelte';
  import NumberField from './NumberField.svelte';

  // A single slider with a button that swaps it for a number field, to enter an exact value. The choice holds
  // for this instance only. The number field works in the units it shows (`number`), which need not be the
  // slider's (a fader position against dB).
  type Props = {
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number | number[];
    /** Tooltip text for the thumb's value. */
    format?: (value: number) => string;
    /** Value a double click puts the slider back to. */
    reset?: number;
    disabled?: boolean;
    onValueChange?: (value: number) => void;
    onValueCommit?: (value: number) => void;
    number: {
      value: number;
      unit?: string;
      step?: number;
      min?: number;
      max?: number;
      integer?: boolean;
      onchange: (value: number) => void;
    };
  };

  let { label, number, ...slider }: Props = $props();

  let typing = $state(false);
  let box: HTMLDivElement | undefined = $state();

  async function toggle() {
    typing = !typing;
    if (!typing) return;
    await tick();
    box?.querySelector('input')?.select();
  }
</script>

<div bind:this={box} class="flex min-w-0 items-center gap-2">
  {#if typing}
    <NumberField
      class="w-28"
      aria-label={label}
      value={number.value}
      unit={number.unit}
      step={number.step}
      min={number.min}
      max={number.max}
      integer={number.integer}
      disabled={slider.disabled}
      onchange={number.onchange}
    />
  {:else}
    <Slider type="single" aria-label={label} class="flex-1" {...slider} />
  {/if}
  <Tooltip tip={typing ? t('field.use_slider') : t('field.enter_value')}>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="ghost"
        size="icon-xs"
        class="ml-auto shrink-0 text-muted-foreground"
        aria-label={typing ? t('field.use_slider') : t('field.enter_value')}
        aria-pressed={typing}
        onclick={toggle}
      >
        {#if typing}<SlidersHorizontal />{:else}<Textbox />{/if}
      </Button>
    {/snippet}
  </Tooltip>
</div>
