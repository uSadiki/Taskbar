import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import {
  BellOff,
  Bluetooth,
  Moon,
  SlidersHorizontal,
  Sun,
  Volume2,
  VolumeX,
  Wifi,
  type LucideIcon,
} from "lucide-react";
import { api, report } from "../../ipc/api";
import { useApp } from "../../stores/app";
import { refreshToggles, startSystemStore, useSystem } from "../../stores/system";
import { anchorOf, usePopover } from "../popover";

/** Dock tile that opens the Quick Toggles popover. */
export function QuickTogglesTile({ size }: { size: number }) {
  const toggle = usePopover((s) => s.toggle);
  const open = usePopover((s) => s.id === "toggles");
  useEffect(startSystemStore, []);
  return (
    <button
      type="button"
      className="dock-tile"
      data-widget="toggles"
      style={{ width: size, height: size, background: open ? "var(--fill-hover)" : undefined }}
      aria-label="Quick toggles"
      aria-expanded={open}
      onClick={(e) => toggle("toggles", anchorOf(e.currentTarget))}
    >
      <SlidersHorizontal size={Math.round(size * 0.48)} strokeWidth={1.75} aria-hidden />
    </button>
  );
}

function Switch({
  Icon,
  label,
  on,
  hint,
  onChange,
}: {
  Icon: LucideIcon;
  label: string;
  /** null: not available on this PC. */
  on: boolean | null;
  hint?: string;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on ?? false}
      disabled={on === null}
      className="toggle-switch"
      onClick={() => onChange(!on)}
    >
      <span className="toggle-switch-icon">
        <Icon size={18} aria-hidden />
      </span>
      <span className="min-w-0 text-left leading-tight">
        <span className="block truncate text-[13px] font-semibold">{label}</span>
        <span className="block truncate text-[11.5px] text-[var(--ink-muted)]">
          {on === null ? "Not available" : (hint ?? (on ? "On" : "Off"))}
        </span>
      </span>
    </button>
  );
}

/**
 * A slider that follows the system value but, while being dragged, shows the
 * user's value and sends it at most every 50 ms.
 */
function LiveSlider({
  label,
  value,
  onCommit,
  children,
}: {
  label: string;
  value: number;
  onCommit: (value: number) => void;
  children: ReactNode;
}) {
  const [local, setLocal] = useState(value);
  const dragging = useRef(false);
  const timer = useRef(0);
  const pending = useRef<number | null>(null);
  useEffect(() => {
    if (!dragging.current) setLocal(value);
  }, [value]);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const change = (next: number) => {
    setLocal(next);
    pending.current = next;
    if (timer.current) return;
    timer.current = window.setTimeout(() => {
      timer.current = 0;
      if (pending.current !== null) onCommit(pending.current);
      pending.current = null;
    }, 50);
  };

  return (
    <div className="flex items-center gap-3">
      {children}
      <input
        type="range"
        className="glass-slider"
        aria-label={label}
        min={0}
        max={100}
        value={local}
        style={{ "--fill": `${local}%` } as CSSProperties}
        onPointerDown={() => (dragging.current = true)}
        onPointerUp={() => (dragging.current = false)}
        onChange={(e) => change(Number(e.target.value))}
      />
      <span className="w-8 text-right text-[12px] tabular-nums text-[var(--ink-muted)]">{local}</span>
    </div>
  );
}

export function QuickTogglesPopover() {
  const radios = useSystem((s) => s.radios);
  const audio = useSystem((s) => s.audio);
  const brightness = useSystem((s) => s.brightness);
  const darkMode = useSystem((s) => s.darkMode);
  const muted = useApp((s) => s.settings?.notifications.muted ?? false);
  const update = useApp((s) => s.update);

  // Brightness and dark mode can change outside the dock; read them fresh.
  useEffect(refreshToggles, []);

  const setRadio = (radio: "wifi" | "bluetooth") => (on: boolean) =>
    api.radioSet(radio, on).catch(report("radio_set"));

  return (
    <div className="p-4">
      <div className="grid grid-cols-2 gap-2">
        <Switch Icon={Wifi} label="Wi-Fi" on={radios?.wifi ?? null} onChange={setRadio("wifi")} />
        <Switch
          Icon={Bluetooth}
          label="Bluetooth"
          on={radios?.bluetooth ?? null}
          onChange={setRadio("bluetooth")}
        />
        <Switch
          Icon={darkMode ? Moon : Sun}
          label="Dark mode"
          on={darkMode}
          onChange={(dark) =>
            api.darkModeSet(dark).then(
              (now) => useSystem.setState({ darkMode: now }),
              report("dark_mode_set"),
            )
          }
        />
        <Switch
          Icon={BellOff}
          label="Quiet"
          on={muted}
          hint={muted ? "Dock alerts held" : "Dock alerts on"}
          onChange={(on) => void update({ notifications: { muted: on } })}
        />
      </div>

      {audio?.available && (
        <div className="mt-4 border-t border-[var(--stroke)] pt-3">
          <LiveSlider
            label="Volume"
            value={audio.volume}
            onCommit={(v) => api.audioSetVolume(v).catch(report("audio_set_volume"))}
          >
            <button
              type="button"
              className="glass-button flex-none"
              aria-label={audio.muted ? "Unmute" : "Mute"}
              aria-pressed={audio.muted}
              onClick={() => api.audioSetMuted(!audio.muted).catch(report("audio_set_muted"))}
            >
              {audio.muted ? <VolumeX size={18} /> : <Volume2 size={18} />}
            </button>
          </LiveSlider>
          {audio.devices.length > 1 && (
            <div role="radiogroup" aria-label="Output device" className="mt-2 flex flex-col gap-0.5">
              {audio.devices.map((device) => (
                <button
                  key={device.id}
                  type="button"
                  role="radio"
                  aria-checked={device.isDefault}
                  className="device-option"
                  title={device.name}
                  onClick={() => api.audioSetDevice(device.id).catch(report("audio_set_device"))}
                >
                  <span className="device-option-mark" aria-hidden />
                  <span className="truncate">{device.name}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {brightness !== null && (
        <div className="mt-3 border-t border-[var(--stroke)] pt-3">
          <LiveSlider
            label="Brightness"
            value={brightness}
            onCommit={(v) => {
              useSystem.setState({ brightness: v });
              api.brightnessSet(v).catch(report("brightness_set"));
            }}
          >
            <span className="grid h-7 w-7 flex-none place-items-center">
              <Sun size={18} aria-hidden />
            </span>
          </LiveSlider>
        </div>
      )}
    </div>
  );
}
