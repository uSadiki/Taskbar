import { useCallback, useEffect, useRef, type PointerEvent as ReactPointerEvent } from "react";
import { api, report } from "../ipc/api";

/** Vertical movement, in CSS pixels, before a press becomes a drag. */
const DRAG_THRESHOLD = 6;

/**
 * Drag the dock between the top and bottom edge. A press anywhere on the dock
 * that moves more than a few pixels vertically starts a drag: the backend
 * moves the window with the cursor and snaps it to the nearer edge on release.
 * Presses that don't move stay ordinary clicks.
 */
export function useDockDrag() {
  const cleanup = useRef<(() => void) | null>(null);

  useEffect(() => () => cleanup.current?.(), []);

  return useCallback((event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    cleanup.current?.();

    const bar = event.currentTarget;
    const pointerId = event.pointerId;
    const startY = event.screenY;
    let dragging = false;
    let frame = 0;

    const onMove = (e: PointerEvent) => {
      if (e.pointerId !== pointerId) return;
      if (!dragging) {
        if (Math.abs(e.screenY - startY) < DRAG_THRESHOLD) return;
        dragging = true;
        // From here on the pointer may leave the window; capture keeps the
        // events coming.
        bar.setPointerCapture(pointerId);
        api.dockDragStart().catch(report("dock_drag_start"));
      }
      if (!frame) {
        frame = requestAnimationFrame(() => {
          frame = 0;
          api.dockDragMove().catch(report("dock_drag_move"));
        });
      }
    };

    const finish = (e: PointerEvent) => {
      if (e.pointerId !== pointerId) return;
      stop();
      if (dragging) {
        // The press ended as a drag: swallow the click it would produce.
        window.addEventListener("click", swallowClick, { capture: true, once: true });
        setTimeout(() => window.removeEventListener("click", swallowClick, { capture: true }), 0);
        api.dockDragEnd().catch(report("dock_drag_end"));
      }
    };

    const stop = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      if (frame) cancelAnimationFrame(frame);
      if (bar.hasPointerCapture(pointerId)) bar.releasePointerCapture(pointerId);
      cleanup.current = null;
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    cleanup.current = stop;
  }, []);
}

function swallowClick(event: MouseEvent) {
  event.stopPropagation();
  event.preventDefault();
}
