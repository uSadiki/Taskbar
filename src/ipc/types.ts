// Mirrors the serde types in src-tauri/src (settings/mod.rs, shell/dock.rs,
// shell/monitors.rs). Keep the two in step.

export type TaskbarMode = "autoHide" | "hidden";
export type Edge = "bottom" | "top";
export type DockLayoutKind = "pill" | "full";
export type DockSize = "small" | "medium" | "large";
export type ColorScheme = "system" | "light" | "dark";

export type WidgetId =
  | "apps"
  | "nowPlaying"
  | "weather"
  | "github"
  | "claude"
  | "battery"
  | "toggles";

export interface WidgetEntry {
  id: WidgetId;
  enabled: boolean;
}

export interface Settings {
  version: number;
  general: {
    autostart: boolean;
    taskbarMode: TaskbarMode;
    position: Edge;
    layout: DockLayoutKind;
    /** Monitor device name; null means the primary monitor. */
    monitor: string | null;
  };
  appearance: {
    theme: string;
    colorScheme: ColorScheme;
    accent: string | null;
    dockSize: DockSize;
    /** Glass slider overrides; null keeps the theme's value. */
    glass: {
      refraction: number | null;
      edgeWidth: number | null;
      blur: number | null;
      chroma: number | null;
      highlight: number | null;
    };
  };
  widgets: WidgetEntry[];
  notifications: {
    quietHours: { enabled: boolean; start: string; end: string };
  };
}

/** A JSON merge patch: any subset of the settings tree; null clears a field. */
export type SettingsPatch = DeepPartial<Settings>;

type DeepPartial<T> = T extends unknown[]
  ? T
  : T extends object
    ? { [K in keyof T]?: DeepPartial<T[K]> | null }
    : T;

/** Everything in logical (CSS) pixels. */
export interface DockLayout {
  edge: Edge;
  layout: DockLayoutKind;
  dockHeight: number;
  gap: number;
  bandHeight: number;
  windowWidth: number;
  windowHeight: number;
  scale: number;
  monitorId: string;
  safeMode: boolean;
  /** Window origin on its monitor, physical pixels. */
  originX: number;
  originY: number;
  /** Monitor size, physical pixels. */
  monitorWidth: number;
  monitorHeight: number;
}

export interface WindowOrigin {
  originX: number;
  originY: number;
}

export interface BackdropInfo {
  rev: number;
  width: number;
  height: number;
  available: boolean;
  error: string | null;
}

export interface PhysicalRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface MonitorInfo {
  id: string;
  rect: PhysicalRect;
  work: PhysicalRect;
  dpi: number;
  primary: boolean;
}

export interface AppInfo {
  version: string;
  safeMode: boolean;
  taskbarSuspended: boolean;
  logDir: string;
  settingsPath: string;
}

/** A rectangle in CSS pixels relative to the window. */
export interface CssRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
