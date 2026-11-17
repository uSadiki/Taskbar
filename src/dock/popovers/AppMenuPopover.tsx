import { Plus, X } from "lucide-react";
import { api, report } from "../../ipc/api";
import { useApps } from "../../stores/apps";
import { usePopover } from "../popover";

/** Right-click menu of an app: its windows, a new window, and closing. */
export function AppMenuPopover({ appId }: { appId: string }) {
  const group = useApps((s) => s.groups.find((g) => g.id === appId));
  const close = usePopover((s) => s.close);

  // The app closed its last window and isn't pinned: nothing left to show.
  if (!group) return <div className="p-4 text-[13px] text-[var(--ink-muted)]">This app has closed.</div>;

  const run = (action: () => Promise<void>, name: string) => () => {
    close();
    action().catch(report(name));
  };
  const count = group.windows.length;

  return (
    <div className="glass-menu" role="menu">
      <div className="truncate px-2.5 pt-1.5 pb-1 text-[12px] text-[var(--ink-muted)]">{group.name}</div>
      {group.windows.map((w) => (
        <div key={w.hwnd} className="flex items-center gap-1">
          <button
            role="menuitem"
            className="min-w-0 flex-1"
            title={w.title}
            onClick={run(() => api.appsWindowAction(w.hwnd, "focus"), "apps_window_action")}
          >
            <span className={`truncate ${w.active ? "font-semibold" : ""}`}>{w.title}</span>
          </button>
          <button
            className="glass-button flex-none"
            aria-label={`Close ${w.title}`}
            onClick={() => api.appsWindowAction(w.hwnd, "close").catch(report("apps_window_action"))}
          >
            <X size={14} aria-hidden />
          </button>
        </div>
      ))}
      {count > 0 && <hr />}
      <button role="menuitem" onClick={run(() => api.appsLaunch(group.id), "apps_launch")}>
        <Plus size={16} aria-hidden /> {count > 0 ? "New window" : "Open"}
      </button>
      {count > 0 && (
        <button role="menuitem" onClick={run(() => api.appsCloseAll(group.id), "apps_close_all")}>
          <X size={16} aria-hidden /> {count === 1 ? "Close window" : `Close all ${count} windows`}
        </button>
      )}
    </div>
  );
}
