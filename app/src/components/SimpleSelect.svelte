<script lang="ts" generics="T extends string">
  import * as Select from '$lib/components/ui/select';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { Option } from '../lib/options';

  // A shadcn select over a list of options, used like a native <select>:
  // `bind:value` and/or `onchange`, which runs after the value is set.
  let {
    value = $bindable(),
    options,
    onchange,
    disabled = false,
    placeholder = '',
    tip,
    class: className = '',
    id,
    'aria-label': ariaLabel,
  }: {
    value: T;
    options: Option<T>[];
    onchange?: (value: T) => void;
    disabled?: boolean;
    placeholder?: string;
    /** Tooltip on the closed select. */
    tip?: string | null;
    class?: string;
    id?: string;
    'aria-label'?: string;
  } = $props();

  // bits-ui reads "" as "nothing selected", but "" is a real value here
  // (e.g. "Systemstandard"), so it travels under another name.
  const EMPTY = '\u0000empty';
  const encode = (raw: string) => (raw === '' ? EMPTY : raw);
  const decode = (raw: string) => (raw === EMPTY ? '' : raw) as T;

  const selected = $derived(options.find((option) => option.value === value));
</script>

<Select.Root
  type="single"
  {disabled}
  bind:value={
    () => encode(value),
    (raw) => {
      value = decode(raw);
      onchange?.(value);
    }
  }
>
  <!-- The tooltip sits on a wrapper: a disabled trigger gets no pointer events. -->
  <Tooltip tip={tip ?? undefined}>
    {#snippet child({ props })}
      <span {...props} class={['flex min-w-0', className]}>
        <Select.Trigger {id} aria-label={ariaLabel} class="disabled:pointer-events-none">
          <span class={['truncate', !selected && 'text-muted-foreground']}>{selected?.label ?? placeholder}</span>
        </Select.Trigger>
      </span>
    {/snippet}
  </Tooltip>
  <Select.Content>
    {#each options as option (option.value)}
      <Select.Item value={encode(option.value)} label={option.label} disabled={option.disabled} />
    {/each}
  </Select.Content>
</Select.Root>
