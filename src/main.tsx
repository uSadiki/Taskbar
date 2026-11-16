import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Dock } from "./dock/Dock";
import { SettingsApp } from "./settings/SettingsApp";
import { startAppStore } from "./stores/app";
import "./index.css";

// One bundle serves every window; the window label picks the UI.
const label = getCurrentWebviewWindow().label;
document.documentElement.dataset.window = label;

startAppStore();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{label === "settings" ? <SettingsApp /> : <Dock />}</React.StrictMode>,
);
