import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Dock } from "./dock/Dock";
import { GlassLab } from "./glass/GlassLab";
import { SettingsApp } from "./settings/SettingsApp";
import { startAppStore } from "./stores/app";
import "./index.css";

// One bundle serves every window; the window label picks the UI.
const label = getCurrentWebviewWindow().label;
const root = document.documentElement;
root.dataset.window = label;
// Until the settings load, follow Windows' light/dark setting.
root.dataset.scheme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";

startAppStore();

// Dev builds expose the stores so scripted checks can render widgets with
// sample data.
if (import.meta.env.DEV) {
  void import("./stores/online").then(({ useOnline }) => {
    (window as unknown as { __online?: typeof useOnline }).__online = useOnline;
  });
}

const ui = label === "settings" ? <SettingsApp /> : label === "glasslab" ? <GlassLab /> : <Dock />;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{ui}</React.StrictMode>,
);
