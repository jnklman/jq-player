// Tailwind class sets used by several components. Component-specific
// classes stay inline in the markup; buttons, fields, menus, tags and keys
// are the shadcn-svelte components in `lib/components/ui`.

/** Marks a string as Tailwind classes, so Prettier sorts them and the editor
 * completes them. Returns the string unchanged. */
export const tw = (strings: TemplateStringsArray, ...values: unknown[]): string =>
  String.raw({ raw: strings }, ...values);

export const panel = tw`rounded-xl border border-line bg-panel`;
/** Empty space in a title bar that moves the window, with `data-tauri-drag-region="deep"`: on Windows the native
 * title bar (see `styles.css`), which a region with controls can't be. */
export const dragFill = tw`min-w-0 grow self-stretch`;
export const panelHead = tw`flex items-center justify-between gap-3 border-b border-line px-4 py-3`;
export const panelTitle = tw`m-0 text-sm font-semibold`;
export const hint = tw`text-xs text-muted-foreground`;
export const empty = tw`p-4 text-[13px] text-muted-foreground`;

/** Track of a progress bar; the fill is a child span with a width style. */
export const bar = tw`block h-1.5 overflow-hidden rounded-full bg-line`;

export const field = tw`flex flex-col items-stretch gap-1 text-xs text-muted-foreground`;
export const check = tw`flex flex-row items-center gap-2 text-[13px] text-fg-2`;

/** Keeps the grabbing cursor over the whole window while a drag runs. */
export function grabbing(on: boolean) {
  document.documentElement.toggleAttribute('data-grabbing', on);
}
