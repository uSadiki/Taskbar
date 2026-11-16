import { FlaskConical, Power, RefreshCw, Settings as SettingsIcon, Undo2 } from "lucide-react";
import { api, report } from "../../ipc/api";
import { usePopover } from "../popover";

/**
 * The dock's right-click menu. It mirrors the tray menu, because the tray is
 * out of reach while the Windows taskbar is fully hidden.
 */
export function MenuPopover() {
  const close = usePopover((s) => s.close);
  const run = (action: () => Promise<void>, name: string) => () => {
    close();
    action().catch(report(name));
  };

  return (
    <div className="glass-menu" role="menu">
      <button role="menuitem" onClick={run(api.openSettings, "open_settings")}>
        <SettingsIcon size={16} aria-hidden /> Settings
      </button>
      <button role="menuitem" onClick={run(api.restoreTaskbar, "restore_taskbar")}>
        <Undo2 size={16} aria-hidden /> Restore taskbar
      </button>
      <button role="menuitem" onClick={run(api.reloadApp, "reload_app")}>
        <RefreshCw size={16} aria-hidden /> Reload
      </button>
      {import.meta.env.DEV && (
        <button role="menuitem" onClick={run(api.openGlassLab, "open_glass_lab")}>
          <FlaskConical size={16} aria-hidden /> Glass lab
        </button>
      )}
      <hr />
      <button role="menuitem" onClick={run(api.quitApp, "quit_app")}>
        <Power size={16} aria-hidden /> Quit Glassdock
      </button>
    </div>
  );
}
