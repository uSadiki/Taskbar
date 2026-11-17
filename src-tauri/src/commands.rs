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
use crate::widgets::apps::{self, WindowAction, model::AppGroup};
use crate::widgets::battery::{self, BatteryState};
use crate::widgets::media::{self, MediaCommand, MediaState};
use crate::widgets::toggles::{self, TogglesSnapshot, radios::RadioName};

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

// ---- Apps widget ----------------------------------------------------------

#[tauri::command]
pub fn apps_list() -> Vec<AppGroup> {
    apps::groups()
}

/// PNG icon for an app, as raw bytes. Empty when the app has no icon.
#[tauri::command]
pub async fn apps_icon(id: String) -> tauri::ipc::Response {
    tauri::ipc::Response::new(apps::icon(id).unwrap_or_default())
}

/// Left click: launch, focus, minimize or step to the next window.
#[tauri::command]
pub fn apps_activate(id: String) {
    apps::activate(id);
}

/// Middle click: start a new instance.
#[tauri::command]
pub fn apps_launch(id: String) {
    apps::launch(id);
}

#[tauri::command]
pub fn apps_window_action(hwnd: isize, action: WindowAction) {
    apps::window_action(hwnd, action);
}

#[tauri::command]
pub fn apps_close_all(id: String) {
    apps::close_all(id);
}

// ---- Now Playing ----------------------------------------------------------

#[tauri::command]
pub fn media_state() -> MediaState {
    media::state()
}

/// The current album art as raw image bytes; empty when there is none.
#[tauri::command]
pub async fn media_art() -> tauri::ipc::Response {
    tauri::ipc::Response::new(media::art())
}

#[tauri::command]
pub fn media_command(command: MediaCommand) {
    media::command(command);
}

// ---- Battery --------------------------------------------------------------

#[tauri::command]
pub fn battery_state() -> BatteryState {
    battery::state()
}

// ---- Quick Toggles --------------------------------------------------------

/// Async: reading brightness waits on the shell worker.
#[tauri::command]
pub async fn toggles_snapshot() -> TogglesSnapshot {
    toggles::snapshot()
}

#[tauri::command]
pub fn radio_set(radio: RadioName, on: bool) {
    toggles::radios::set(radio, on);
}

#[tauri::command]
pub fn audio_set_volume(percent: u8) {
    toggles::audio::set_volume(percent);
}

#[tauri::command]
pub fn audio_set_muted(muted: bool) {
    toggles::audio::set_muted(muted);
}

#[tauri::command]
pub fn audio_set_device(id: String) {
    toggles::audio::set_default_device(id);
}

#[tauri::command]
pub fn brightness_set(percent: u8) {
    toggles::set_brightness(percent);
}

/// Switch Windows between light and dark mode. Returns the mode now in effect.
#[tauri::command]
pub fn dark_mode_set(dark: bool) -> bool {
    toggles::theme::set_dark(dark);
    toggles::theme::is_dark()
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
