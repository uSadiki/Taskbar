import { useEffect } from "react";
import { Zap } from "lucide-react";
import type { BatteryState } from "../../ipc/types";
import { formatDuration } from "../../lib/time";
import { startSystemStore, useSystem } from "../../stores/system";
import { anchorOf, usePopover } from "../popover";

/** Ring colour by charge level; charging is always the "good" colour. */
export function batteryColor(battery: BatteryState): string {
  const percent = battery.percent ?? 0;
  if (battery.charging) return "var(--good)";
  if (percent <= 15) return "var(--bad)";
  if (percent <= 35) return "var(--warn)";
  return "var(--good)";
}

/** One line describing what the battery is doing. */
export function batteryStatus(battery: BatteryState): string {
  if (battery.charging) {
    return battery.secondsToFull !== null
      ? `Charging, ${formatDuration(battery.secondsToFull)} until full`
      : "Charging";
  }
  if (battery.plugged) return battery.percent === 100 ? "Fully charged" : "Plugged in, not charging";
  return battery.secondsRemaining !== null
    ? `${formatDuration(battery.secondsRemaining)} remaining`
    : "On battery";
}

/**
 * `pulse` animates the ring while charging. Only the popover uses it: any
 * endless animation in the dock bar makes the whole dock window redraw every
 * frame.
 */
function Ring({
  battery,
  size,
  stroke,
  pulse = false,
}: {
  battery: BatteryState;
  size: number;
  stroke: number;
  pulse?: boolean;
}) {
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const percent = battery.percent ?? 0;
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
      <circle
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="none"
        stroke="var(--stroke)"
        strokeWidth={stroke}
      />
      <circle
        className={pulse && battery.charging ? "battery-charging" : undefined}
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="none"
        stroke={batteryColor(battery)}
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={circumference * (1 - percent / 100)}
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
        style={{ transition: "stroke-dashoffset 400ms ease, stroke 400ms ease" }}
      />
    </svg>
  );
}

/** Dock tile: a ring with the charge level inside. Hidden on a PC without a battery. */
export function BatteryTile({ size }: { size: number }) {
  const battery = useSystem((s) => s.battery);
  const toggle = usePopover((s) => s.toggle);
  const open = usePopover((s) => s.id === "battery");
  useEffect(startSystemStore, []);

  if (!battery?.present || battery.percent === null) return null;

  return (
    <button
      type="button"
      className="dock-tile relative"
      data-widget="battery"
      style={{ width: size, height: size, background: open ? "var(--fill-hover)" : "transparent" }}
      aria-label={`Battery ${battery.percent}%. ${batteryStatus(battery)}`}
      aria-expanded={open}
      onClick={(e) => toggle("battery", anchorOf(e.currentTarget))}
    >
      <Ring battery={battery} size={size - 6} stroke={3} />
      <span className="absolute inset-0 grid place-items-center text-[11px] font-semibold tabular-nums">
        {battery.charging ? <Zap size={13} fill="currentColor" aria-hidden /> : battery.percent}
      </span>
    </button>
  );
}

export function BatteryPopover() {
  const battery = useSystem((s) => s.battery);
  if (!battery?.present || battery.percent === null) {
    return <div className="p-5 text-[13px] text-[var(--ink-muted)]">This PC has no battery.</div>;
  }
  return (
    <div className="p-5">
      <div className="flex items-center gap-4">
        <div className="relative flex-none">
          <Ring battery={battery} size={76} stroke={6} pulse />
          {battery.charging && (
            <Zap size={22} fill="currentColor" className="absolute inset-0 m-auto" aria-hidden />
          )}
        </div>
        <div className="min-w-0">
          <div className="text-[38px] font-light leading-none tabular-nums">
            {battery.percent}
            <span className="text-[20px] text-[var(--ink-muted)]">%</span>
          </div>
          <div className="mt-1.5 text-[13px] text-[var(--ink-muted)]">{batteryStatus(battery)}</div>
        </div>
      </div>
      <dl className="mt-4 grid grid-cols-[1fr_auto] gap-y-1.5 border-t border-[var(--stroke)] pt-3 text-[13px]">
        <dt className="text-[var(--ink-muted)]">Power source</dt>
        <dd>{battery.plugged ? "Mains" : "Battery"}</dd>
        <dt className="text-[var(--ink-muted)]">Battery saver</dt>
        <dd>{battery.saver ? "On" : "Off"}</dd>
      </dl>
    </div>
  );
}
