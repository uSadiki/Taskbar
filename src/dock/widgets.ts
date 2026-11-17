import {
  BatteryMedium,
  CloudSun,
  Gauge,
  GitBranch,
  LayoutGrid,
  Music,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import type { WidgetId } from "../ipc/types";

/** Display name, Settings icon and a one-line description of each widget. */
export const WIDGETS: Record<WidgetId, { label: string; Icon: LucideIcon; about: string }> = {
  apps: { label: "Apps", Icon: LayoutGrid, about: "Pinned apps and open windows" },
  nowPlaying: { label: "Now Playing", Icon: Music, about: "Shown while something is playing" },
  weather: { label: "Weather", Icon: CloudSun, about: "Shown once a city is chosen" },
  github: { label: "GitHub Activity", Icon: GitBranch, about: "Shown once a token is added" },
  claude: { label: "Claude Usage", Icon: Gauge, about: "Limits and usage from Claude Code" },
  battery: { label: "Battery Ring", Icon: BatteryMedium, about: "Hidden on a PC without a battery" },
  toggles: { label: "Quick Toggles", Icon: SlidersHorizontal, about: "Wi-Fi, Bluetooth, sound, brightness" },
};
