import { create } from "zustand";

/** The dock element a popover grows out of, in CSS pixels relative to the window. */
export interface Anchor {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface PopoverState {
  /** The open popover, or null. */
  id: string | null;
  /** Kept after closing so the glass can shrink back into it. */
  anchor: Anchor | null;
  /** Open `id` from `anchor`, or close it if it is already open. */
  toggle: (id: string, anchor: Anchor) => void;
  close: () => void;
}

export const usePopover = create<PopoverState>((set, get) => ({
  id: null,
  anchor: null,
  toggle: (id, anchor) => set(get().id === id ? { id: null } : { id, anchor }),
  close: () => set({ id: null }),
}));

export function anchorOf(element: Element): Anchor {
  const r = element.getBoundingClientRect();
  return { x: r.left, y: r.top, w: r.width, h: r.height };
}
