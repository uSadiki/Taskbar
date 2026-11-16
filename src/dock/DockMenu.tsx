import { useEffect, type Ref } from "react";
import { Power, RefreshCw, Settings as SettingsIcon, Undo2 } from "lucide-react";
import { api, events, report } from "../ipc/api";

interface DockMenuProps {
  ref: Ref<HTMLDivElement>;
  /** Horizontal centre of the menu, in CSS pixels from the window's left edge. */
  x: number;
  /** Distance from the dock's screen edge to the near side of the menu. */
  offset: number;
  edge: "top" | "bottom";
  windowWidth: number;
  onClose: () => void;
}

const MENU_WIDTH = 200;

/**
 * The dock's right-click menu. It mirrors the tray menu, because the tray is
 * out of reach while the Windows taskbar is fully hidden.
 */
export function DockMenu({ ref, x, offset, edge, windowWidth, onClose }: DockMenuProps) {
  // The dock doesn't normally take focus. Take it while the menu is open so
  // Esc works and clicking another window closes the menu.
  useEffect(() => {
    api.dockFocus().catch(report("dock_focus"));
    let unlisten: (() => void) | undefined;
    let closed = false;
    void events.dockBlur(onClose).then((fn) => {
      if (closed) fn();
      else unlisten = fn;
    });
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      closed = true;
      unlisten?.();
      window.removeEventListener("keydown", onKey);
      api.dockReleaseFocus().catch(report("dock_release_focus"));
    };
  }, [onClose]);

  const run = (action: () => Promise<void>, name: string) => () => {
    onClose();
    action().catch(report(name));
  };

  const left = Math.min(Math.max(x - MENU_WIDTH / 2, 8), windowWidth - MENU_WIDTH - 8);

  return (
    <div
      ref={ref}
      className="dock-menu"
      role="menu"
      style={{ left, width: MENU_WIDTH, [edge]: offset }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <button role="menuitem" onClick={run(api.openSettings, "open_settings")}>
        <SettingsIcon size={16} aria-hidden /> Settings
      </button>
      <button role="menuitem" onClick={run(api.restoreTaskbar, "restore_taskbar")}>
        <Undo2 size={16} aria-hidden /> Restore taskbar
      </button>
      <button role="menuitem" onClick={run(api.reloadApp, "reload_app")}>
        <RefreshCw size={16} aria-hidden /> Reload
      </button>
      <hr />
      <button role="menuitem" onClick={run(api.quitApp, "quit_app")}>
        <Power size={16} aria-hidden /> Quit Glassdock
      </button>
    </div>
  );
}
