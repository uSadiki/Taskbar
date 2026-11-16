//! The notification-area icon and its menu.

use tauri::AppHandle;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let restore = MenuItem::with_id(app, "restore", "Restore taskbar", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Glassdock", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &restore, &reload, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("glassdock")
        .tooltip("Glassdock")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| crate::app::run_action(app, event.id().as_ref()));
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
