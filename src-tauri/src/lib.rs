//! Glassdock: a liquid glass smart dock for Windows.

mod alerts;
mod app;
mod autostart;
mod cli;
mod commands;
mod error;
mod filelog;
mod geometry;
mod glass;
mod net;
mod paths;
mod settings;
mod shell;
mod widgets;

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    match cli::parse(&args, cli::env_safe()) {
        cli::Mode::Restore => shell::restore_command(),
        cli::Mode::Quit => shell::quit_command(),
        cli::Mode::Statusline => widgets::claude::statusline::run(),
        cli::Mode::UninstallCleanup => uninstall_cleanup(),
        cli::Mode::WatchdogLauncher(pid) => shell::watchdog::launch_detached(pid),
        cli::Mode::Watchdog(pid) => shell::watchdog::run(pid),
        cli::Mode::App { safe } => app::run(safe),
    }
}

/// `glassdock.exe --uninstall-cleanup`, run by the uninstaller after the dock
/// has been asked to quit.
fn uninstall_cleanup() {
    shell::restore_command();
    let bridge = widgets::claude::statusline::sync(false);
    autostart::remove();
    filelog::line(
        "uninstall",
        &format!("cleanup ran; Claude Code status line: {bridge:?}; autostart entry removed"),
    );
}
