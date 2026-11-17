import { useEffect, useRef, type MouseEvent } from "react";
import {
  motion,
  useMotionValue,
  useReducedMotion,
  useSpring,
  useTransform,
  type MotionValue,
} from "motion/react";
import { api, report } from "../../ipc/api";
import type { AppGroup, Edge } from "../../ipc/types";
import { ensureIcon, startAppsStore, useApps } from "../../stores/apps";
import { anchorOf, usePopover } from "../popover";

/** Largest hover scale, and how far from the pointer the effect reaches (CSS pixels). */
const MAGNIFY = 1.22;
const REACH = 96;
/** At most this many window dots under an icon. */
const MAX_DOTS = 3;

interface AppIconProps {
  group: AppGroup;
  size: number;
  edge: Edge;
  /** Pointer x in window coordinates, or Infinity when the pointer is elsewhere. */
  pointerX: MotionValue<number>;
  magnify: boolean;
}

function AppIcon({ group, size, edge, pointerX, magnify }: AppIconProps) {
  const ref = useRef<HTMLButtonElement>(null);
  const icon = useApps((s) => s.icons[group.id]);
  const toggle = usePopover((s) => s.toggle);
  const menuOpen = usePopover((s) => s.id === `app:${group.id}`);

  useEffect(() => ensureIcon(group.id), [group.id]);

  // Grow toward the pointer, like a row of icons under a lens. The pointer
  // and the icon are both measured in window coordinates (`offsetLeft` would
  // be relative to the dock's full-width strip, not to this row).
  const target = useTransform(pointerX, (x) => {
    const el = ref.current;
    if (!magnify || !el || !Number.isFinite(x)) return 1;
    const box = el.getBoundingClientRect();
    const distance = Math.abs(x - (box.left + box.width / 2));
    const closeness = Math.max(0, 1 - distance / REACH);
    return 1 + (MAGNIFY - 1) * closeness * closeness;
  });
  const scale = useSpring(target, { stiffness: 380, damping: 26, mass: 0.4 });

  const running = group.windows.length > 0;
  const label = running
    ? `${group.name}, ${group.windows.length} window${group.windows.length === 1 ? "" : "s"}`
    : group.name;

  const onAuxClick = (event: MouseEvent) => {
    // Middle click: another instance.
    if (event.button === 1) {
      event.preventDefault();
      api.appsLaunch(group.id).catch(report("apps_launch"));
    }
  };

  const onContextMenu = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    toggle(`app:${group.id}`, anchorOf(event.currentTarget));
  };

  return (
    <button
      ref={ref}
      type="button"
      className="app-icon"
      style={{ width: size + 6, height: "100%" }}
      title={group.name}
      aria-label={label}
      aria-pressed={group.active}
      aria-expanded={menuOpen}
      onClick={() => api.appsActivate(group.id).catch(report("apps_activate"))}
      onAuxClick={onAuxClick}
      onContextMenu={onContextMenu}
    >
      <motion.span
        className="app-icon-image"
        style={{
          width: size,
          height: size,
          scale,
          transformOrigin: edge === "bottom" ? "50% 100%" : "50% 0%",
        }}
        whileTap={{ scale: 0.9 }}
      >
        {icon ? (
          <img src={icon} alt="" width={size} height={size} draggable={false} />
        ) : (
          <span className="app-icon-missing" aria-hidden>
            {group.name.charAt(0).toUpperCase()}
          </span>
        )}
      </motion.span>
      <span className="app-dots" data-edge={edge} aria-hidden>
        {group.active ? (
          <span className="app-dot app-dot-active" />
        ) : (
          group.windows.slice(0, MAX_DOTS).map((w) => <span key={w.hwnd} className="app-dot" />)
        )}
      </span>
    </button>
  );
}

/** Pinned apps and open windows, grouped by app. */
export function AppsWidget({ size, edge }: { size: number; edge: Edge }) {
  const groups = useApps((s) => s.groups);
  const pointerX = useMotionValue(Infinity);
  const reduced = useReducedMotion() ?? false;

  useEffect(startAppsStore, []);

  if (groups.length === 0) return null;

  return (
    <div
      className="apps-widget"
      role="toolbar"
      aria-label="Apps"
      onPointerMove={(e) => pointerX.set(e.clientX)}
      onPointerLeave={() => pointerX.set(Infinity)}
    >
      {groups.map((group) => (
        <AppIcon
          key={group.id}
          group={group}
          size={size}
          edge={edge}
          pointerX={pointerX}
          magnify={!reduced}
        />
      ))}
    </div>
  );
}
