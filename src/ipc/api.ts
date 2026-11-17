import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppGroup,
  AppInfo,
  AudioState,
  BackdropInfo,
  BatteryState,
  CssRect,
  DockLayout,
  MediaCommand,
  MediaState,
  MonitorInfo,
  RadiosState,
  Settings,
  SettingsPatch,
  TogglesSnapshot,
  WindowAction,
  WindowOrigin,
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
  glassInfo: () => invoke<BackdropInfo>("glass_info"),
  /** 16-byte header (rev, width, height, reserved as u32 LE) then RGBA. */
  glassBackdrop: () => invoke<ArrayBuffer>("glass_backdrop"),
  openGlassLab: () => invoke<void>("open_glass_lab"),
  listMonitors: () => invoke<MonitorInfo[]>("list_monitors"),
  appsList: () => invoke<AppGroup[]>("apps_list"),
  /** PNG bytes; empty when the app has no icon. */
  appsIcon: (id: string) => invoke<ArrayBuffer>("apps_icon", { id }),
  appsActivate: (id: string) => invoke<void>("apps_activate", { id }),
  appsLaunch: (id: string) => invoke<void>("apps_launch", { id }),
  appsWindowAction: (hwnd: number, action: WindowAction) =>
    invoke<void>("apps_window_action", { hwnd, action }),
  appsCloseAll: (id: string) => invoke<void>("apps_close_all", { id }),
  mediaState: () => invoke<MediaState>("media_state"),
  /** Album art image bytes; empty when there is none. */
  mediaArt: () => invoke<ArrayBuffer>("media_art"),
  mediaCommand: (command: MediaCommand) => invoke<void>("media_command", { command }),
  batteryState: () => invoke<BatteryState>("battery_state"),
  togglesSnapshot: () => invoke<TogglesSnapshot>("toggles_snapshot"),
  radioSet: (radio: "wifi" | "bluetooth", on: boolean) => invoke<void>("radio_set", { radio, on }),
  audioSetVolume: (percent: number) => invoke<void>("audio_set_volume", { percent }),
  audioSetMuted: (muted: boolean) => invoke<void>("audio_set_muted", { muted }),
  audioSetDevice: (id: string) => invoke<void>("audio_set_device", { id }),
  brightnessSet: (percent: number) => invoke<void>("brightness_set", { percent }),
  /** Switches Windows' light/dark mode; resolves to the mode now in effect. */
  darkModeSet: (dark: boolean) => invoke<boolean>("dark_mode_set", { dark }),
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
  dockOrigin: (cb: (origin: WindowOrigin) => void): Promise<UnlistenFn> =>
    listen<WindowOrigin>("dock://origin", (e) => cb(e.payload)),
  appsChanged: (cb: (groups: AppGroup[]) => void): Promise<UnlistenFn> =>
    listen<AppGroup[]>("apps://changed", (e) => cb(e.payload)),
  mediaChanged: (cb: (state: MediaState) => void): Promise<UnlistenFn> =>
    listen<MediaState>("media://changed", (e) => cb(e.payload)),
  batteryChanged: (cb: (state: BatteryState) => void): Promise<UnlistenFn> =>
    listen<BatteryState>("battery://changed", (e) => cb(e.payload)),
  radiosChanged: (cb: (state: RadiosState) => void): Promise<UnlistenFn> =>
    listen<RadiosState>("radios://changed", (e) => cb(e.payload)),
  audioChanged: (cb: (state: AudioState) => void): Promise<UnlistenFn> =>
    listen<AudioState>("audio://changed", (e) => cb(e.payload)),
  backdrop: (cb: (info: BackdropInfo) => void): Promise<UnlistenFn> =>
    listen<BackdropInfo>("glass://backdrop", (e) => cb(e.payload)),
  appInfo: (cb: (info: AppInfo) => void): Promise<UnlistenFn> =>
    listen<AppInfo>("app://info", (e) => cb(e.payload)),
};

/** Log a failed command without throwing; UI handlers call this fire-and-forget. */
export function report(action: string) {
  return (error: unknown) => console.error(`${action} failed:`, error);
}
