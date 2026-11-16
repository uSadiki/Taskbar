//! Commands the windows can invoke.
//!
//! Synchronous commands run on the main thread, which is where the shell code
//! expects to be. `open_settings` is async because creating a webview window
//! from a synchronous command deadlocks on Windows.

use serde_json::{Value, json};
use tauri::{AppHandle, State};

use crate::app;
use crate::error::AppResult;
use crate::geometry::RectF;
use crate::glass::{self, BackdropInfo};
use crate::settings::{Settings, SettingsStore};
use crate::shell::dock::{self, AppInfo, DockLayout};
use crate::shell::monitors::{self, MonitorInfo};

#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> Settings {
    store.get()
}

/// Apply a JSON merge patch to the settings and return the result.
#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    store: State<'_, SettingsStore>,
    patch: Value,
) -> AppResult<Settings> {
    let (old, new) = store.apply_patch(&patch)?;
    app::settings_changed(&app, &old, &new);
    // Choosing a taskbar mode, even the one already selected, ends a manual
    // "Restore taskbar" and puts the taskbar back under Glassdock's control.
    let picked_taskbar_mode = patch.pointer("/general/taskbarMode").is_some();
    if picked_taskbar_mode && old.general.taskbar_mode == new.general.taskbar_mode {
        dock::resume_taskbar();
    }
    Ok(new)
}

#[tauri::command]
pub fn get_dock_layout() -> Option<DockLayout> {
    dock::layout()
}

/// The frontend reports which rectangles of the dock window are in use
/// (the dock itself, an open menu or popover), in CSS pixels.
#[tauri::command]
pub fn set_hit_regions(rects: Vec<RectF>) {
    dock::set_regions(rects);
}

#[tauri::command]
pub fn dock_drag_start() {
    dock::drag_start();
}

#[tauri::command]
pub fn dock_drag_move() {
    dock::drag_move();
}

/// Finish a drag: snap to the edge the dock was dropped on.
#[tauri::command]
pub fn dock_drag_end(app: AppHandle, store: State<'_, SettingsStore>) -> AppResult<()> {
    let Some(edge) = dock::drag_end() else {
        return Ok(());
    };
    if store.get().general.position == edge {
        // Dropped on the edge it came from: snap back.
        dock::reposition();
    } else {
        let (old, new) = store.apply_patch(&json!({ "general": { "position": edge } }))?;
        app::settings_changed(&app, &old, &new);
    }
    Ok(())
}

#[tauri::command]
pub fn dock_focus() {
    dock::focus();
}

#[tauri::command]
pub fn dock_release_focus() {
    dock::release_focus();
}

#[tauri::command]
pub fn glass_info() -> BackdropInfo {
    glass::info()
}

/// The wallpaper pixels behind the dock's monitor: a 16-byte header followed
/// by RGBA. Sent as raw bytes, not JSON. Async so the copy happens off the
/// main thread.
#[tauri::command]
pub async fn glass_backdrop() -> tauri::ipc::Response {
    tauri::ipc::Response::new(glass::packed())
}

#[tauri::command]
pub fn list_monitors() -> Vec<MonitorInfo> {
    monitors::enumerate()
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    dock::app_info()
}

#[tauri::command]
pub async fn open_settings(app: AppHandle) -> AppResult<()> {
    app::open_settings(&app)
}

#[tauri::command]
pub async fn open_glass_lab(app: AppHandle) -> AppResult<()> {
    app::open_glass_lab(&app)
}

#[tauri::command]
pub fn restore_taskbar() {
    dock::suspend_taskbar();
}

#[tauri::command]
pub fn reload_app() {
    dock::reload();
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app::quit(&app);
}
