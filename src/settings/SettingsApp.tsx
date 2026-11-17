import { useEffect, useState } from "react";
import {
  Bell,
  ChevronDown,
  ChevronUp,
  Info,
  LayoutGrid,
  Palette,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import { WIDGETS } from "../dock/widgets";
import { api, report } from "../ipc/api";
import type { AppInfo, MonitorInfo, Settings, SettingsPatch } from "../ipc/types";
import { useApp } from "../stores/app";
import { useScheme } from "../themes/useAppearance";
import { Appearance } from "./AppearanceSection";
import { WidgetSettings } from "./WidgetSettings";
import { Button, Notice, Row, Section, Segmented, Toggle } from "./controls";

type SectionId = "general" | "appearance" | "widgets" | "notifications" | "about";

const SECTIONS: { id: SectionId; label: string; Icon: LucideIcon }[] = [
  { id: "general", label: "General", Icon: SlidersHorizontal },
  { id: "appearance", label: "Appearance", Icon: Palette },
  { id: "widgets", label: "Widgets", Icon: LayoutGrid },
  { id: "notifications", label: "Notifications", Icon: Bell },
  { id: "about", label: "About", Icon: Info },
];

type Update = (patch: SettingsPatch) => Promise<void>;

export function SettingsApp() {
  const settings = useApp((s) => s.settings);
  const info = useApp((s) => s.info);
  const update = useApp((s) => s.update);
  const [section, setSection] = useState<SectionId>("general");
  useScheme(settings?.appearance.colorScheme);

  if (!settings) {
    return <div className="grid h-full place-items-center text-[var(--text-muted)]">Loading settings</div>;
  }

  return (
    <div className="flex h-full">
      <nav aria-label="Settings sections" className="w-52 flex-none border-r border-[var(--border)] p-3">
        <div className="px-3 pt-2 pb-4 text-[15px] font-semibold">Glassdock</div>
        {SECTIONS.map(({ id, label, Icon }) => (
          <button
            key={id}
            type="button"
            aria-current={section === id ? "page" : undefined}
            onClick={() => setSection(id)}
            className={`mb-0.5 flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${
              section === id
                ? "bg-[var(--surface-raised)] font-semibold shadow-sm"
                : "text-[var(--text-muted)] hover:bg-[var(--surface-sunken)] hover:text-[var(--text)]"
            }`}
          >
            <Icon size={16} aria-hidden /> {label}
          </button>
        ))}
      </nav>

      <main className="min-w-0 flex-1 overflow-y-auto px-8 py-7">
        <h1 className="mb-5 text-xl font-semibold">{SECTIONS.find((s) => s.id === section)?.label}</h1>
        {section === "general" && <General settings={settings} info={info} update={update} />}
        {section === "appearance" && <Appearance settings={settings} update={update} />}
        {section === "widgets" && <WidgetsPage settings={settings} update={update} />}
        {section === "notifications" && <Notifications settings={settings} update={update} />}
        {section === "about" && <About info={info} />}
      </main>
    </div>
  );
}

function General({ settings, info, update }: { settings: Settings; info: AppInfo | null; update: Update }) {
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  useEffect(() => {
    api.listMonitors().then(setMonitors, report("list_monitors"));
  }, []);

  const { general } = settings;
  return (
    <>
      {info?.safeMode && (
        <Notice>
          Safe mode is on. The dock floats without reserving screen space, and the Windows taskbar is
          left alone. The taskbar setting below takes effect on a normal start.
        </Notice>
      )}
      {info?.taskbarSuspended && !info.safeMode && (
        <Notice>
          The taskbar was restored and Glassdock is not managing it. Pick a taskbar mode below, or
          choose Reload from the tray menu, to manage it again.
        </Notice>
      )}

      <Section title="Windows taskbar">
        <Row
          label="While Glassdock runs"
          description="Auto-hide keeps the taskbar reachable at the screen edge. Hide completely removes it until Glassdock exits; the system tray is unreachable in that mode."
        >
          <Segmented
            label="Taskbar mode"
            value={general.taskbarMode}
            options={[
              { value: "autoHide", label: "Auto-hide" },
              { value: "hidden", label: "Hide completely" },
            ]}
            onChange={(taskbarMode) => update({ general: { taskbarMode } })}
          />
        </Row>
      </Section>

      <Section title="Dock">
        <Row label="Position" description="You can also drag the dock to the other edge.">
          <Segmented
            label="Position"
            value={general.position}
            options={[
              { value: "bottom", label: "Bottom" },
              { value: "top", label: "Top" },
            ]}
            onChange={(position) => update({ general: { position } })}
          />
        </Row>
        <Row label="Layout">
          <Segmented
            label="Layout"
            value={general.layout}
            options={[
              { value: "pill", label: "Pill" },
              { value: "full", label: "Full width" },
            ]}
            onChange={(layout) => update({ general: { layout } })}
          />
        </Row>
        <Row label="Monitor" description="If the chosen monitor is unplugged, the dock moves to the primary one.">
          <select
            aria-label="Monitor"
            value={general.monitor ?? ""}
            onChange={(e) => update({ general: { monitor: e.target.value || null } })}
            className="rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] px-2.5 py-1.5 text-[13px]"
          >
            <option value="">Primary monitor</option>
            {monitors.map((m, index) => (
              <option key={m.id} value={m.id}>
                {`Display ${index + 1}: ${m.rect.right - m.rect.left} × ${m.rect.bottom - m.rect.top}, ${Math.round((m.dpi / 96) * 100)}%${m.primary ? " (primary)" : ""}`}
              </option>
            ))}
          </select>
        </Row>
      </Section>

      <Section title="Startup">
        <Row label="Start at login" description="Arrives with the installer in phase 6.">
          <Toggle label="Start at login" checked={general.autostart} disabled onChange={() => {}} />
        </Row>
      </Section>
    </>
  );
}

function Widgets({ settings, update }: { settings: Settings; update: Update }) {
  const { widgets } = settings;

  const setEnabled = (index: number, enabled: boolean) =>
    update({ widgets: widgets.map((w, i) => (i === index ? { ...w, enabled } : w)) });

  const move = (index: number, delta: -1 | 1) => {
    const target = index + delta;
    if (target < 0 || target >= widgets.length) return;
    const next = [...widgets];
    [next[index], next[target]] = [next[target], next[index]];
    void update({ widgets: next });
  };

  return (
    <Section title="Shown in the dock, in this order">
      {widgets.map((widget, index) => {
        const { label, Icon, about } = WIDGETS[widget.id];
        return (
          <div key={widget.id} className="flex items-center gap-3 px-4 py-2.5">
            <Icon size={18} className="flex-none text-[var(--text-muted)]" aria-hidden />
            <div className="min-w-0 flex-1">
              <div className="font-medium">{label}</div>
              <div className="text-[12.5px] text-[var(--text-muted)]">{about}</div>
            </div>
            <Button label={`Move ${label} up`} disabled={index === 0} onClick={() => move(index, -1)}>
              <ChevronUp size={14} aria-hidden />
            </Button>
            <Button
              label={`Move ${label} down`}
              disabled={index === widgets.length - 1}
              onClick={() => move(index, 1)}
            >
              <ChevronDown size={14} aria-hidden />
            </Button>
            <Toggle label={`Show ${label}`} checked={widget.enabled} onChange={(on) => setEnabled(index, on)} />
          </div>
        );
      })}
    </Section>
  );
}

function WidgetsPage({ settings, update }: { settings: Settings; update: Update }) {
  return (
    <>
      <Widgets settings={settings} update={update} />
      <WidgetSettings settings={settings} update={update} />
    </>
  );
}

function Notifications({ settings, update }: { settings: Settings; update: Update }) {
  const { quietHours } = settings.notifications;
  const timeInput =
    "rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] px-2 py-1 text-[13px] disabled:opacity-45";
  return (
    <>
    <Section title="Alerts">
      <Row
        label="Claude usage limits"
        description="A toast above the dock when the 5-hour or weekly limit reaches 80% and again at 95%."
      >
        <Toggle
          label="Claude usage limit alerts"
          checked={settings.notifications.claudeLimits}
          onChange={(claudeLimits) => update({ notifications: { claudeLimits } })}
        />
      </Row>
      <Row
        label="Quiet"
        description="Holds every alert until it is switched off. Also in Quick Toggles."
      >
        <Toggle
          label="Quiet"
          checked={settings.notifications.muted}
          onChange={(muted) => update({ notifications: { muted } })}
        />
      </Row>
    </Section>
    <Section title="Quiet hours">
      <Row
        label="Hold alerts on a schedule"
        description="Alerts that come up during quiet hours are shown when they end."
      >
        <Toggle
          label="Quiet hours"
          checked={quietHours.enabled}
          onChange={(enabled) => update({ notifications: { quietHours: { ...quietHours, enabled } } })}
        />
      </Row>
      <Row label="Schedule">
        <div className="flex items-center gap-2">
          <input
            type="time"
            aria-label="Quiet hours start"
            className={timeInput}
            disabled={!quietHours.enabled}
            value={quietHours.start}
            onChange={(e) =>
              e.target.value &&
              update({ notifications: { quietHours: { ...quietHours, start: e.target.value } } })
            }
          />
          <span className="text-[var(--text-muted)]">to</span>
          <input
            type="time"
            aria-label="Quiet hours end"
            className={timeInput}
            disabled={!quietHours.enabled}
            value={quietHours.end}
            onChange={(e) =>
              e.target.value &&
              update({ notifications: { quietHours: { ...quietHours, end: e.target.value } } })
            }
          />
        </div>
      </Row>
    </Section>
    </>
  );
}

function About({ info }: { info: AppInfo | null }) {
  return (
    <>
      <Section title="Glassdock">
        <Row label="Version">{info?.version ?? "unknown"}</Row>
        <Row label="Mode">{info?.safeMode ? "Safe mode" : "Normal"}</Row>
        <Row label="Settings file" description={info?.settingsPath} />
        <Row label="Logs" description={info?.logDir} />
      </Section>
      <Section title="Controls">
        <Row
          label="Restore taskbar"
          description="Puts the Windows taskbar back the way it was and stops managing it until you reload."
        >
          <Button onClick={() => api.restoreTaskbar().catch(report("restore_taskbar"))}>Restore</Button>
        </Row>
        <Row label="Reload" description="Re-reads settings and re-registers the dock with Windows.">
          <Button onClick={() => api.reloadApp().catch(report("reload_app"))}>Reload</Button>
        </Row>
        <Row label="Quit" description="Closes Glassdock and restores the taskbar.">
          <Button onClick={() => api.quitApp().catch(report("quit_app"))}>Quit</Button>
        </Row>
      </Section>
    </>
  );
}
