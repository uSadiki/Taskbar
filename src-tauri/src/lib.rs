//! Glassdock: a liquid glass smart dock for Windows.

mod app;
mod cli;
mod commands;
mod error;
mod filelog;
mod geometry;
mod glass;
mod paths;
mod settings;
mod shell;
mod widgets;

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    match cli::parse(&args, cli::env_safe()) {
        cli::Mode::Restore => shell::restore_command(),
        cli::Mode::Quit => shell::quit_command(),
        cli::Mode::WatchdogLauncher(pid) => shell::watchdog::launch_detached(pid),
        cli::Mode::Watchdog(pid) => shell::watchdog::run(pid),
        cli::Mode::App { safe } => app::run(safe),
    }
}
