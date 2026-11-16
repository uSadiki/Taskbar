import { create } from "zustand";
import { api, events, report } from "../ipc/api";
import type { AppInfo, DockLayout, Settings, SettingsPatch } from "../ipc/types";

interface AppState {
  settings: Settings | null;
  layout: DockLayout | null;
  info: AppInfo | null;
  /** Send a merge patch to the backend; the store updates from its reply. */
  update: (patch: SettingsPatch) => Promise<void>;
}

export const useApp = create<AppState>((set) => ({
  settings: null,
  layout: null,
  info: null,
  update: async (patch) => {
    try {
      set({ settings: await api.updateSettings(patch) });
    } catch (error) {
      report("update_settings")(error);
    }
  },
}));

let started = false;

/** Load the initial state and subscribe to backend events. Idempotent. */
export function startAppStore() {
  if (started) return;
  started = true;

  // Subscribe first so nothing emitted during the initial fetch is missed.
  void events.settingsChanged((settings) => useApp.setState({ settings }));
  void events.dockLayout((layout) => useApp.setState({ layout }));
  void events.appInfo((info) => useApp.setState({ info }));

  api.getSettings().then((settings) => useApp.setState({ settings }), report("get_settings"));
  api.appInfo().then((info) => useApp.setState({ info }), report("app_info"));
  api.getDockLayout().then((layout) => {
    if (layout) useApp.setState({ layout });
  }, report("get_dock_layout"));
}
