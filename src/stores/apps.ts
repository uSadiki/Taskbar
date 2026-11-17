import { create } from "zustand";
import { api, events, report } from "../ipc/api";
import type { AppGroup } from "../ipc/types";

interface AppsState {
  groups: AppGroup[];
  /** Object URLs of icons already fetched, by app id. `null` means no icon. */
  icons: Record<string, string | null>;
}

export const useApps = create<AppsState>(() => ({ groups: [], icons: {} }));

const requested = new Set<string>();

/** Fetch an app's icon once and keep it as an object URL. */
export function ensureIcon(id: string) {
  if (requested.has(id)) return;
  requested.add(id);
  api.appsIcon(id).then(
    (buffer) => {
      const url =
        buffer.byteLength > 0
          ? URL.createObjectURL(new Blob([buffer], { type: "image/png" }))
          : null;
      useApps.setState((s) => ({ icons: { ...s.icons, [id]: url } }));
    },
    (error) => {
      requested.delete(id);
      report("apps_icon")(error);
    },
  );
}

let started = false;

/** Load the app list and follow changes. Idempotent. */
export function startAppsStore() {
  if (started) return;
  started = true;
  void events.appsChanged((groups) => useApps.setState({ groups }));
  api.appsList().then((groups) => useApps.setState({ groups }), report("apps_list"));
}
