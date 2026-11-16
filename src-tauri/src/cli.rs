//! Command-line modes. One executable plays several roles so the installer
//! ships a single file.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The dock itself. `safe` skips AppBar registration and taskbar changes.
    App { safe: bool },
    /// Put the taskbar back the way it was and exit.
    Restore,
    /// Start a detached watchdog for the given process id and exit.
    WatchdogLauncher(u32),
    /// Wait for the given process to exit, then restore the taskbar if needed.
    Watchdog(u32),
}

/// `args` is the full argument list including the program name.
pub fn parse(args: &[String], env_safe: bool) -> Mode {
    let mut safe = env_safe;
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--restore" => return Mode::Restore,
            "--watchdog-launcher" => {
                if let Some(pid) = iter.next().and_then(|v| v.parse().ok()) {
                    return Mode::WatchdogLauncher(pid);
                }
            }
            "--watchdog" => {
                if let Some(pid) = iter.next().and_then(|v| v.parse().ok()) {
                    return Mode::Watchdog(pid);
                }
            }
            "--safe" => safe = true,
            _ => {}
        }
    }
    Mode::App { safe }
}

/// True when the environment asks for safe mode (`GLASSDOCK_SAFE=1`).
pub fn env_safe() -> bool {
    std::env::var("GLASSDOCK_SAFE")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("glassdock.exe")
            .chain(list.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn defaults_to_the_app() {
        assert_eq!(parse(&args(&[]), false), Mode::App { safe: false });
    }

    #[test]
    fn safe_mode_from_flag_or_environment() {
        assert_eq!(parse(&args(&["--safe"]), false), Mode::App { safe: true });
        assert_eq!(parse(&args(&[]), true), Mode::App { safe: true });
    }

    #[test]
    fn restore_wins_over_other_flags() {
        assert_eq!(parse(&args(&["--safe", "--restore"]), false), Mode::Restore);
    }

    #[test]
    fn watchdog_modes_need_a_pid() {
        assert_eq!(
            parse(&args(&["--watchdog", "4242"]), false),
            Mode::Watchdog(4242)
        );
        assert_eq!(
            parse(&args(&["--watchdog-launcher", "17"]), false),
            Mode::WatchdogLauncher(17)
        );
        // A missing or malformed pid falls back to running the app.
        assert_eq!(
            parse(&args(&["--watchdog"]), false),
            Mode::App { safe: false }
        );
        assert_eq!(
            parse(&args(&["--watchdog", "abc"]), false),
            Mode::App { safe: false }
        );
    }

    #[test]
    fn unknown_arguments_are_ignored() {
        assert_eq!(
            parse(&args(&["--whatever", "x"]), false),
            Mode::App { safe: false }
        );
    }
}
