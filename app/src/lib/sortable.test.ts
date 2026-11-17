import type Sortable from 'sortablejs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// SortableJS itself needs a real layout; the fake keeps the options it was created with, so a test plays its callbacks.
const created = vi.hoisted(() => ({
  options: null as Sortable.Options | null,
  destroyed: 0,
  ghost: null as HTMLElement | null,
}));
vi.mock('sortablejs', () => {
  const create = (_node: HTMLElement, options: Sortable.Options) => {
    created.options = options;
    return { options, destroy: () => created.destroyed++ };
  };
  return {
    default: {
      create,
      get ghost() {
        return created.ghost;
      },
    },
  };
});

import { sortable } from './sortable';

function list(count = 3) {
  const node = document.createElement('ul');
  for (let i = 0; i < count; i++) node.append(Object.assign(document.createElement('li'), { id: `i${i}` }));
  document.body.append(node);
  return node;
}

const item = (node: HTMLElement, i: number) => node.children[i] as HTMLElement;
type Event = Sortable.SortableEvent;
const start = (item: HTMLElement) => created.options!.onStart!({ item } as Event);
const end = (node: HTMLElement, item: HTMLElement, oldIndex: number | undefined, newIndex: number | undefined) =>
  created.options!.onEnd!({ item, from: node, oldIndex, newIndex } as Event);
const grabbing = () => document.documentElement.hasAttribute('data-grabbing');

beforeEach(() => {
  created.options = null;
  created.destroyed = 0;
  created.ghost = null;
  document.body.innerHTML = '';
});

describe('sortable', () => {
  it('puts the moved element back and reports where it went', () => {
    const node = list();
    const onMove = vi.fn();
    const detach = sortable(onMove, { handle: '.grip' })(node);
    expect(created.options).toMatchObject({ forceFallback: true, handle: '.grip' });

    const dragged = item(node, 0);
    start(dragged);
    expect(grabbing()).toBe(true);
    // SortableJS moved it to the end meanwhile.
    node.append(dragged);
    end(node, dragged, 0, 2);
    expect(grabbing()).toBe(false);
    expect([...node.children].map((child) => child.id)).toEqual(['i0', 'i1', 'i2']);
    expect(onMove).toHaveBeenCalledWith(dragged, 2);

    (detach as () => void)();
    expect(created.destroyed).toBe(1);
  });

  it('reports nothing for a drag that ends in place or without indices', () => {
    const node = list();
    const onMove = vi.fn();
    sortable(onMove)(node);
    start(item(node, 1));
    end(node, item(node, 1), 1, 1);
    start(item(node, 1));
    end(node, item(node, 1), undefined, 2);
    expect(onMove).not.toHaveBeenCalled();
  });

  it('hides the elements moving along in a group drag and labels the drag', () => {
    const node = list();
    const onMove = vi.fn();
    created.ghost = document.createElement('li');
    sortable(onMove, {}, (dragged) => ({ others: [item(node, 2)], label: `${dragged.id} +1` }))(node);
    const dragged = item(node, 0);
    start(dragged);
    expect(item(node, 2).hidden).toBe(true);
    expect(dragged.dataset.dragCount).toBe('i0 +1');
    expect(created.ghost.dataset.dragCount).toBe('i0 +1');
    // A group drag counts even in place: the others join the dragged one.
    end(node, dragged, 0, 0);
    expect(item(node, 2).hidden).toBe(false);
    expect(dragged.dataset.dragCount).toBeUndefined();
    expect(onMove).toHaveBeenCalledWith(dragged, 0);
  });

  it('cancels a drag when the window loses focus', () => {
    const node = list();
    const onMove = vi.fn();
    sortable(onMove, { supportPointer: true } as Sortable.Options)(node);
    const released: string[] = [];
    document.addEventListener('pointerup', () => released.push('pointerup'));
    const dragged = item(node, 0);
    start(dragged);
    window.dispatchEvent(new FocusEvent('blur'));
    expect(released).toEqual(['pointerup']);
    node.append(dragged);
    end(node, dragged, 0, 2);
    expect(onMove).not.toHaveBeenCalled();
    expect(item(node, 0)).toBe(dragged);
    // The next drag is reported again.
    start(dragged);
    end(node, dragged, 0, 1);
    expect(onMove).toHaveBeenCalledWith(dragged, 1);
  });

  it('releases with a mouseup without pointer events, and unhides a group when detached mid-drag', () => {
    const node = list();
    const detach = sortable(vi.fn(), {}, () => ({ others: [item(node, 1)], label: '2' }))(node) as () => void;
    const released: string[] = [];
    document.addEventListener('mouseup', () => released.push('mouseup'));
    start(item(node, 0));
    window.dispatchEvent(new FocusEvent('blur'));
    expect(released).toEqual(['mouseup']);
    detach();
    expect(item(node, 1).hidden).toBe(false);
    expect(grabbing()).toBe(false);
  });
});
