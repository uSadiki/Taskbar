import { useCallback, useRef, useState, type MouseEvent } from "react";
import { useApp } from "../stores/app";
import { Clock } from "./Clock";
import { DockMenu } from "./DockMenu";
import { useDockDrag } from "./useDockDrag";
import { useHitRegions } from "./useHitRegions";
import { WIDGETS } from "./widgets";

/** Space between the dock and its right-click menu, in CSS pixels. */
const MENU_GAP = 6;

export function Dock() {
  const layout = useApp((s) => s.layout);
  const settings = useApp((s) => s.settings);
  const barRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuX, setMenuX] = useState<number | null>(null);

  const onPointerDown = useDockDrag();
  const closeMenu = useCallback(() => setMenuX(null), []);

  useHitRegions([barRef, menuRef], [layout, settings, menuX]);

  if (!layout || !settings) return null;

  const openMenu = (event: MouseEvent) => {
    event.preventDefault();
    setMenuX(event.clientX);
  };

  const tileSize = layout.dockHeight - 12;
  const enabled = settings.widgets.filter((w) => w.enabled);

  return (
    <div className="pointer-events-none fixed inset-0">
      {/* The reserved strip, at the window edge that touches the screen edge. */}
      <div
        className="absolute inset-x-0 flex justify-center"
        style={{ [layout.edge]: 0, height: layout.bandHeight, padding: layout.gap }}
      >
        <div
          ref={barRef}
          className="dock-bar pointer-events-auto"
          data-layout={layout.layout}
          data-edge={layout.edge}
          style={{ height: layout.dockHeight }}
          onPointerDown={onPointerDown}
          onContextMenu={openMenu}
          onClick={menuX === null ? undefined : closeMenu}
        >
          {enabled.map(({ id }) => {
            const { label, Icon, phase } = WIDGETS[id];
            return (
              <button
                key={id}
                type="button"
                className="dock-tile"
                style={{ width: tileSize, height: tileSize }}
                title={`${label} (arrives in phase ${phase})`}
                aria-label={label}
              >
                <Icon size={Math.round(tileSize * 0.5)} strokeWidth={1.75} aria-hidden />
              </button>
            );
          })}
          {layout.layout === "full" && <div className="flex-1" />}
          {layout.safeMode && (
            <span className="dock-badge" title="Safe mode: no AppBar, taskbar untouched">
              SAFE
            </span>
          )}
          <Clock />
        </div>
      </div>

      {menuX !== null && (
        <DockMenu
          ref={menuRef}
          x={menuX}
          offset={layout.bandHeight - layout.gap + MENU_GAP}
          edge={layout.edge}
          windowWidth={layout.windowWidth}
          onClose={closeMenu}
        />
      )}
    </div>
  );
}
