import { useEffect, useLayoutEffect, useState, type RefObject } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, Info, X } from "lucide-react";
import type { GlassShape } from "../glass/renderer";
import { scene } from "../glass/scene";
import { events } from "../ipc/api";
import type { Alert, DockLayout } from "../ipc/types";
import { useTone, type Appearance } from "../themes/useAppearance";
import { anchorOf, usePopover } from "./popover";

const WIDTH = 320;
const MARGIN = 12;
const GAP = 10;
/** Toasts dismiss themselves after this long; critical ones stay longer. */
const LIFETIME = { info: 7000, warning: 10_000, critical: 20_000 };

/**
 * The dock's alerts. One toast at a time, at the window's far side above the
 * dock, on its own glass. A newer alert replaces the current one.
 */
export function Toasts({
  layout,
  appearance,
  hostRef,
  onChange,
}: {
  layout: DockLayout;
  appearance: Appearance;
  hostRef: RefObject<HTMLDivElement | null>;
  /** Called when a toast appears or goes, so the clickable region can follow. */
  onChange: (visible: boolean) => void;
}) {
  const [alert, setAlert] = useState<Alert | null>(null);
  const [shape, setShape] = useState<GlassShape | null>(null);
  const togglePopover = usePopover((s) => s.toggle);

  useEffect(() => {
    const unlisten = events.alert(setAlert);
    return () => void unlisten.then((fn) => fn());
  }, []);

  useEffect(() => {
    onChange(alert !== null);
    if (!alert) return;
    const timer = window.setTimeout(() => setAlert(null), LIFETIME[alert.level] ?? LIFETIME.info);
    return () => window.clearTimeout(timer);
  }, [alert, onChange]);

  const offset = layout.bandHeight - layout.gap + GAP;
  useLayoutEffect(() => {
    const element = hostRef.current;
    if (!alert || !element) {
      scene.remove("toast");
      setShape(null);
      return;
    }
    const measure = () => {
      const next: GlassShape = {
        x: element.offsetLeft,
        y: element.offsetTop,
        w: element.offsetWidth,
        h: element.offsetHeight,
        r: 18,
        frost: 0.5,
      };
      if (next.h === 0) return;
      scene.set("toast", next, { instant: true });
      setShape(next);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, [alert, hostRef, offset, layout.edge, layout.windowWidth]);

  const tone = useTone(shape, layout, appearance);
  const Icon = alert?.level === "info" ? Info : AlertTriangle;
  const color = alert?.level === "critical" ? "var(--bad)" : alert?.level === "warning" ? "var(--warn)" : "var(--accent)";

  const openTarget = () => {
    if (!alert?.open) return;
    const target = document.querySelector(`[data-widget="${alert.open}"]`);
    if (target) togglePopover(alert.open, anchorOf(target));
    setAlert(null);
  };

  return (
    <div
      ref={hostRef}
      data-tone={tone}
      className="glass-content pointer-events-auto absolute"
      style={{ left: layout.windowWidth - WIDTH - MARGIN, width: WIDTH, [layout.edge]: offset }}
    >
      <AnimatePresence>
        {alert && (
          <motion.div
            key={alert.id + alert.title}
            role="alert"
            className="flex cursor-default items-start gap-3 p-3.5"
            initial={{ opacity: 0, y: layout.edge === "bottom" ? 12 : -12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, transition: { duration: 0.12 } }}
            onClick={openTarget}
          >
            <Icon size={20} className="mt-0.5 flex-none" style={{ color }} aria-hidden />
            <div className="min-w-0 flex-1">
              <div className="text-[13.5px] font-semibold">{alert.title}</div>
              {alert.body && <div className="mt-0.5 text-[12.5px] text-[var(--ink-muted)]">{alert.body}</div>}
            </div>
            <button
              type="button"
              className="glass-button flex-none"
              aria-label="Dismiss"
              onClick={(e) => {
                e.stopPropagation();
                setAlert(null);
              }}
            >
              <X size={14} />
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
