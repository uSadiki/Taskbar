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
    /** Alert at 80% and 95% of the Claude usage limits. */
    claudeLimits: boolean;
  };
  weather: {
    location: WeatherLocation | null;
    fahrenheit: boolean;
  };
  claude: {
    /** Register Glassdock as Claude Code's status line command. */
    statuslineBridge: boolean;
  };
}

export interface WeatherLocation {
  name: string;
  latitude: number;
  longitude: number;
}

export interface Place extends WeatherLocation {
  region: string | null;
  country: string | null;
}

export interface WeatherCurrent {
  temperature: number;
  feelsLike: number;
  humidity: number;
  precipitation: number;
  code: number;
  windSpeed: number;
  windDirection: number;
  isDay: boolean;
  uvIndex: number;
}

export interface WeatherHour {
  time: string;
  temperature: number;
  code: number;
  precipitationChance: number;
  isDay: boolean;
}

export interface WeatherDay {
  date: string;
  code: number;
  high: number;
  low: number;
  precipitationChance: number;
  sunrise: string;
  sunset: string;
  uvIndexMax: number;
}

export interface Forecast {
  location: string;
  fahrenheit: boolean;
  current: WeatherCurrent;
  hours: WeatherHour[];
  days: WeatherDay[];
  fetchedAt: number;
}

export interface WeatherState {
  configured: boolean;
  forecast: Forecast | null;
  error: string | null;
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
  /** A development build: autostart and the Claude bridge don't register. */
  devBuild: boolean;
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

// ---- GitHub Activity --------------------------------------------------------

export type GithubStatus = "noToken" | "ok" | "invalidToken" | "rateLimited" | "offline" | "error";

export interface ContributionDay {
  date: string;
  count: number;
  /** 0 (none) to 4 (most). */
  level: number;
}

export interface PullRequest {
  title: string;
  url: string;
  number: number;
  repo: string;
  draft: boolean;
  updatedAt: string;
}

export interface GithubEvent {
  kind: "push" | "pr" | "issue" | "review" | "comment" | "create" | "other";
  summary: string;
  repo: string;
  url: string;
  createdAt: string;
}

export interface GithubData {
  login: string;
  totalYear: number;
  days: ContributionDay[];
  today: number;
  currentStreak: number;
  longestStreak: number;
  authored: PullRequest[];
  authoredTotal: number;
  reviewRequests: PullRequest[];
  reviewTotal: number;
  events: GithubEvent[];
  fetchedAt: number;
}

export interface GithubState {
  status: GithubStatus;
  message: string | null;
  data: GithubData | null;
}

// ---- Claude Usage -----------------------------------------------------------

export interface LimitWindow {
  usedPercentage: number;
  /** Unix seconds. */
  resetsAt: number;
}

export interface Limits {
  fiveHour: LimitWindow | null;
  sevenDay: LimitWindow | null;
  updatedAt: number;
}

export type Bridge = "installed" | "otherCommand" | "off" | "devBuild" | "noClaudeCode" | "error";

export interface UsageTotals {
  tokens: number;
  output: number;
  /** Estimated API cost, US dollars. */
  cost: number;
  requests: number;
}

export interface UsageBlock extends UsageTotals {
  start: number;
  end: number;
  tokensPerMinute: number;
  costPerHour: number;
  projectedCost: number;
}

export interface ClaudeState {
  limits: Limits | null;
  bridge: Bridge | null;
  fiveHourRunsOutAt: number | null;
  block: UsageBlock | null;
  days: (UsageTotals & { date: string })[];
  models: (UsageTotals & { model: string; label: string })[];
  week: UsageTotals;
  hasLogs: boolean;
}

// ---- Alerts -----------------------------------------------------------------

export interface Alert {
  id: string;
  title: string;
  body: string;
  level: "info" | "warning" | "critical";
  /** Popover to open when the toast is clicked. */
  open: string | null;
}
