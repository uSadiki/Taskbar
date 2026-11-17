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
    /** Hold the dock's own alerts until switched off. */
    muted: boolean;
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

export interface AppWindow {
  hwnd: number;
  title: string;
  minimized: boolean;
  active: boolean;
}

/** One icon in the Apps widget: a pinned app, a running app, or both. */
export interface AppGroup {
  id: string;
  name: string;
  pinned: boolean;
  active: boolean;
  windows: AppWindow[];
}

export type WindowAction = "focus" | "minimize" | "close";

export interface MediaState {
  available: boolean;
  app: string;
  title: string;
  artist: string;
  album: string;
  playing: boolean;
  /** Position when last reported, and when that was (Unix ms). */
  positionMs: number;
  durationMs: number;
  updatedAtMs: number;
  canToggle: boolean;
  canNext: boolean;
  canPrevious: boolean;
  canSeek: boolean;
  /** null when the player doesn't support the control. */
  shuffle: boolean | null;
  repeat: "none" | "track" | "list" | null;
  artRev: number;
}

export type MediaCommand =
  | { type: "toggle" | "next" | "previous" }
  | { type: "seek"; positionMs: number }
  | { type: "shuffle"; on: boolean }
  | { type: "repeat"; mode: "none" | "track" | "list" };

export interface BatteryState {
  present: boolean;
  percent: number | null;
  plugged: boolean;
  charging: boolean;
  saver: boolean;
  secondsRemaining: number | null;
  secondsToFull: number | null;
}

export interface RadiosState {
  /** null: no such radio, or Windows denied control of it. */
  wifi: boolean | null;
  bluetooth: boolean | null;
}

export interface OutputDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface AudioState {
  available: boolean;
  /** 0 to 100. */
  volume: number;
  muted: boolean;
  devices: OutputDevice[];
}

export interface TogglesSnapshot {
  radios: RadiosState;
  audio: AudioState;
  /** null when no display supports brightness control. */
  brightness: number | null;
  darkMode: boolean;
}

/** A rectangle in CSS pixels relative to the window. */
export interface CssRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
