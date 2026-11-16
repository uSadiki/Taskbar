import { useLayoutEffect, useRef, type RefObject } from "react";
import { api, report } from "../ipc/api";
import type { CssRect } from "../ipc/types";
import { sameRects, toCssRect, visibleRects } from "../lib/regions";

/**
 * Tell the backend which rectangles of the dock window are in use. The window
 * is as wide as the monitor and tall enough for popovers, but only these
 * rectangles are visible and clickable; everything else passes clicks through
 * to whatever is behind it.
 *
 * Re-reports whenever `deps` change (layout, open menus) and whenever one of
 * the elements resizes.
 */
export function useHitRegions(refs: RefObject<HTMLElement | null>[], deps: unknown[]) {
  const last = useRef<CssRect[]>([]);

  useLayoutEffect(() => {
    const elements = refs.map((r) => r.current).filter((el): el is HTMLElement => el !== null);

    const send = (force: boolean) => {
      const rects = visibleRects(elements.map((el) => toCssRect(el.getBoundingClientRect())));
      if (!force && sameRects(rects, last.current)) return;
      last.current = rects;
      api.setHitRegions(rects).catch(report("set_hit_regions"));
    };

    const observer = new ResizeObserver(() => send(false));
    elements.forEach((el) => observer.observe(el));
    const onResize = () => send(false);
    window.addEventListener("resize", onResize);

    // The backend falls back to the whole reserved strip after it moves the
    // window, so always re-report when the layout changes, even if the
    // rectangles happen to be the same in CSS pixels.
    send(true);

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", onResize);
    };
    // The caller owns the dependency list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
