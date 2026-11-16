import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppInfo,
  CssRect,
  DockLayout,
  MonitorInfo,
  Settings,
  SettingsPatch,
} from "./types";

/** Typed wrappers around the backend commands in src-tauri/src/commands.rs. */
export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (patch: SettingsPatch) => invoke<Settings>("update_settings", { patch }),
  getDockLayout: () => invoke<DockLayout | null>("get_dock_layout"),
  setHitRegions: (rects: CssRect[]) => invoke<void>("set_hit_regions", { rects }),
  dockDragStart: () => invoke<void>("dock_drag_start"),
  dockDragMove: () => invoke<void>("dock_drag_move"),
  dockDragEnd: () => invoke<void>("dock_drag_end"),
  dockFocus: () => invoke<void>("dock_focus"),
  dockReleaseFocus: () => invoke<void>("dock_release_focus"),
  listMonitors: () => invoke<MonitorInfo[]>("list_monitors"),
  appInfo: () => invoke<AppInfo>("app_info"),
  openSettings: () => invoke<void>("open_settings"),
  restoreTaskbar: () => invoke<void>("restore_taskbar"),
  reloadApp: () => invoke<void>("reload_app"),
  quitApp: () => invoke<void>("quit_app"),
};

/** Backend events. Each returns a promise of its unlisten function. */
export const events = {
  settingsChanged: (cb: (settings: Settings) => void): Promise<UnlistenFn> =>
    listen<Settings>("settings://changed", (e) => cb(e.payload)),
  dockLayout: (cb: (layout: DockLayout) => void): Promise<UnlistenFn> =>
    listen<DockLayout>("dock://layout", (e) => cb(e.payload)),
  dockBlur: (cb: () => void): Promise<UnlistenFn> => listen("dock://blur", () => cb()),
  appInfo: (cb: (info: AppInfo) => void): Promise<UnlistenFn> =>
    listen<AppInfo>("app://info", (e) => cb(e.payload)),
};

/** Log a failed command without throwing; UI handlers call this fire-and-forget. */
export function report(action: string) {
  return (error: unknown) => console.error(`${action} failed:`, error);
}
