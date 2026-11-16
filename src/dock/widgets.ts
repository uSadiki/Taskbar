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

/** Display name and placeholder glyph for each widget, and the phase it lands in. */
export const WIDGETS: Record<WidgetId, { label: string; Icon: LucideIcon; phase: number }> = {
  apps: { label: "Apps", Icon: LayoutGrid, phase: 3 },
  nowPlaying: { label: "Now Playing", Icon: Music, phase: 4 },
  weather: { label: "Weather", Icon: CloudSun, phase: 5 },
  github: { label: "GitHub Activity", Icon: GitBranch, phase: 5 },
  claude: { label: "Claude Usage", Icon: Gauge, phase: 5 },
  battery: { label: "Battery Ring", Icon: BatteryMedium, phase: 4 },
  toggles: { label: "Quick Toggles", Icon: SlidersHorizontal, phase: 4 },
};
