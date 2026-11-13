<script lang="ts">
  import type { HTMLInputAttributes, HTMLInputTypeAttribute } from 'svelte/elements';

  import CaretDown from '~icons/ph/caret-down-bold';
  import CaretUp from '~icons/ph/caret-up-bold';
  import { t } from '$lib/i18n/index.svelte';
  import { cn, type WithElementRef } from '$lib/utils.js';

  type InputType = Exclude<HTMLInputTypeAttribute, 'file'>;

  type Props = WithElementRef<
    Omit<HTMLInputAttributes, 'type'> & ({ type: 'file'; files?: FileList } | { type?: InputType; files?: undefined })
  > & {
    /** type="number": unit shown inside the field, right of the number ("s", "dB"). */
    unit?: string;
  };

  let {
    ref = $bindable(null),
    value = $bindable(),
    type,
    files = $bindable(),
    class: className,
    'data-slot': dataSlot = 'input',
    unit,
    ...restProps
  }: Props = $props();

  // The step arrows of a number field: as the arrow keys, then sent as an edit and a change (as the native
  // ones, which look different in every engine and are hidden).
  function stepBy(direction: number) {
    const input = ref as HTMLInputElement | null;
    if (!input) return;
    try {
      if (direction > 0) input.stepUp();
      else input.stepDown();
    } catch {
      return; // step="any"
    }
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }
</script>

{#if type === 'file'}
  <input
    bind:this={ref}
    data-slot={dataSlot}
    class={cn(
      'min-h-8 w-full min-w-0 rounded-lg border border-input bg-background px-2.5 text-sm text-foreground select-text file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-dim disabled:text-fg-2 aria-invalid:border-destructive',
      className,
    )}
    type="file"
    bind:files
    bind:value
    {...restProps}
  />
{:else if type === 'number'}
  <!-- The box is a wrapper, so a unit and the arrows take their own width. Without a unit the arrows show on
       hover and focus; with one they take its place while the field has focus. `class` styles the box, the
       input inherits its font. -->
  <span
    data-slot={dataSlot}
    class={cn(
      'group flex min-h-8 w-full min-w-0 items-center rounded-lg border border-input bg-background pr-1 text-sm text-foreground has-aria-invalid:border-destructive has-[input:focus-visible]:outline-2 has-[input:focus-visible]:outline-offset-1 has-[input:focus-visible]:outline-ring has-[input:focus-visible]:outline-solid',
      className,
    )}
  >
    <input
      bind:this={ref}
      class={[
        'min-w-0 flex-1 [appearance:textfield] self-stretch bg-transparent pr-1 pl-2.5 outline-hidden select-text placeholder:font-sans placeholder:text-xs placeholder:text-dim disabled:text-fg-2 [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none',
        unit && 'text-right',
      ]}
      type="number"
      bind:value
      {...restProps}
    />
    <span class="grid shrink-0 place-items-center">
      {#if unit}
        <span
          class={[
            'pointer-events-none pr-0.5 font-sans text-xs text-dim [grid-area:1/1]',
            !restProps.disabled && 'group-focus-within:invisible',
          ]}>{unit}</span
        >
      {/if}
      {#if !restProps.disabled}
        <span
          class={['invisible flex flex-col [grid-area:1/1] group-focus-within:visible', !unit && 'group-hover:visible']}
        >
          {#each [1, -1] as direction (direction)}
            <button
              type="button"
              tabindex={-1}
              aria-label={direction > 0 ? t('input.increase') : t('input.decrease')}
              class="flex h-3.5 w-4 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-fg"
              onmousedown={(e) => e.preventDefault()}
              onclick={() => stepBy(direction)}
            >
              {#if direction > 0}<CaretUp class="size-3" />{:else}<CaretDown class="size-3" />{/if}
            </button>
          {/each}
        </span>
      {/if}
    </span>
  </span>
{:else}
  <input
    bind:this={ref}
    data-slot={dataSlot}
    class={cn(
      'min-h-8 w-full min-w-0 rounded-lg border border-input bg-background px-2.5 text-sm text-foreground select-text file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-dim disabled:text-fg-2 aria-invalid:border-destructive',
      className,
    )}
    {type}
    bind:value
    {...restProps}
  />
{/if}
