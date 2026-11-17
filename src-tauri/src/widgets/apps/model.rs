//! Grouping windows into apps and deciding what a click does. Pure logic.

use serde::Serialize;

/// What is known about a pinned app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinMeta {
    /// Stable identity of the pin.
    pub key: String,
    pub name: String,
    pub aumid: Option<String>,
    /// The executable the pin launches, if it is a shortcut to one.
    pub target: Option<String>,
}

/// What is known about an open window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowMeta {
    pub hwnd: isize,
    pub title: String,
    pub exe: String,
    pub aumid: Option<String>,
    /// Display name of the app that owns the window.
    pub app_name: String,
    pub minimized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppWindow {
    pub hwnd: isize,
    pub title: String,
    pub minimized: bool,
    pub active: bool,
}

/// One icon in the dock: a pinned app, a running app, or both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGroup {
    pub id: String,
    pub name: String,
    pub pinned: bool,
    /// One of this app's windows is the foreground window.
    pub active: bool,
    pub windows: Vec<AppWindow>,
}

const EXPLORER_AUMID: &str = "Microsoft.Windows.Explorer";

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

fn parent_dir(path: &str) -> &str {
    path.rfind(['\\', '/']).map_or("", |i| &path[..i])
}

/// Squirrel-installed apps (Discord, Slack, ...) keep every version in its own
/// `app-<version>` folder, so the path changes with each update.
fn is_version_dir(segment: &str) -> bool {
    segment
        .to_ascii_lowercase()
        .strip_prefix("app-")
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// A path with any version folder removed, for comparing across updates.
fn without_version_dir(path: &str) -> String {
    path.split(['\\', '/'])
        .filter(|segment| !is_version_dir(segment))
        .collect::<Vec<_>>()
        .join("\\")
        .to_ascii_lowercase()
}

/// Identity of an unpinned app: its AppUserModelID if it has one, else its
/// executable.
pub fn window_key(window: &WindowMeta) -> String {
    match &window.aumid {
        Some(aumid) => format!("aumid:{}", aumid.to_ascii_lowercase()),
        None => format!("exe:{}", window.exe.to_ascii_lowercase()),
    }
}

pub fn pin_id(pin: &PinMeta) -> String {
    format!("pin:{}", pin.key.to_ascii_lowercase())
}

/// Does `window` belong to the pinned app `pin`?
pub fn pin_matches(pin: &PinMeta, window: &WindowMeta) -> bool {
    // Both declare an identity: that settles it either way. A browser's
    // installed web apps share its executable but not its identity.
    if let (Some(a), Some(b)) = (&pin.aumid, &window.aumid) {
        return same(a, b);
    }
    // File Explorer's pin is an Apps-folder item with no shortcut target, and
    // its windows declare no identity; the executable is the only link.
    if pin
        .aumid
        .as_deref()
        .is_some_and(|a| same(a, EXPLORER_AUMID))
        && same(file_name(&window.exe), "explorer.exe")
    {
        return true;
    }
    let Some(target) = &pin.target else {
        return false;
    };
    if same(target, &window.exe) {
        return true;
    }
    // The pin may point at an older version's folder than the one running.
    if without_version_dir(target) == without_version_dir(&window.exe) {
        return true;
    }
    // Or at the launcher, `<app>\Update.exe`, while the app runs from
    // `<app>\app-<version>\<name>.exe`.
    if same(file_name(target), "update.exe") {
        let root = parent_dir(target);
        let exe_dir = parent_dir(parent_dir(&window.exe));
        return !root.is_empty() && same(root, exe_dir);
    }
    false
}

/// Build the dock's app list: pins in pin order, each with its windows, then
/// running apps that are not pinned, in the order their first window appeared.
/// `windows` must be in first-seen order.
pub fn build_groups(pins: &[PinMeta], windows: &[WindowMeta], foreground: isize) -> Vec<AppGroup> {
    let to_window = |w: &WindowMeta| AppWindow {
        hwnd: w.hwnd,
        title: w.title.clone(),
        minimized: w.minimized,
        active: w.hwnd == foreground,
    };

    let mut groups: Vec<AppGroup> = pins
        .iter()
        .map(|pin| AppGroup {
            id: pin_id(pin),
            name: pin.name.clone(),
            pinned: true,
            active: false,
            windows: Vec::new(),
        })
        .collect();

    for window in windows {
        if let Some(index) = pins.iter().position(|pin| pin_matches(pin, window)) {
            groups[index].windows.push(to_window(window));
            continue;
        }
        let id = window_key(window);
        match groups.iter_mut().find(|g| !g.pinned && g.id == id) {
            Some(group) => group.windows.push(to_window(window)),
            None => groups.push(AppGroup {
                id,
                name: window.app_name.clone(),
                pinned: false,
                active: false,
                windows: vec![to_window(window)],
            }),
        }
    }

    for group in &mut groups {
        group.active = group.windows.iter().any(|w| w.active);
    }
    groups
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickAction {
    Launch,
    Focus(isize),
    Minimize(isize),
}

/// What a left click on an app does, the way the Windows taskbar behaves:
/// start it, bring it forward, minimize it if it is already in front, or step
/// to its next window if it has several.
pub fn click_action(group: &AppGroup) -> ClickAction {
    match group.windows.as_slice() {
        [] => ClickAction::Launch,
        [only] if only.active => ClickAction::Minimize(only.hwnd),
        [only] => ClickAction::Focus(only.hwnd),
        many => match many.iter().position(|w| w.active) {
            Some(current) => ClickAction::Focus(many[(current + 1) % many.len()].hwnd),
            None => ClickAction::Focus(many.iter().find(|w| !w.minimized).unwrap_or(&many[0]).hwnd),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(name: &str, aumid: Option<&str>, target: Option<&str>) -> PinMeta {
        PinMeta {
            key: aumid.or(target).unwrap_or(name).to_string(),
            name: name.into(),
            aumid: aumid.map(Into::into),
            target: target.map(Into::into),
        }
    }

    fn window(hwnd: isize, exe: &str, aumid: Option<&str>) -> WindowMeta {
        WindowMeta {
            hwnd,
            title: format!("window {hwnd}"),
            exe: exe.into(),
            aumid: aumid.map(Into::into),
            app_name: file_name(exe).trim_end_matches(".exe").into(),
            minimized: false,
        }
    }

    const CHROME: &str = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
    const CODE: &str = r"C:\Users\u\AppData\Local\Programs\Microsoft VS Code\Code.exe";

    #[test]
    fn matches_by_identity_when_both_have_one() {
        let terminal = pin(
            "Terminal",
            Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
            None,
        );
        let w = window(
            1,
            r"C:\Program Files\WindowsApps\x\WindowsTerminal.exe",
            Some("microsoft.windowsterminal_8wekyb3d8bbwe!app"),
        );
        assert!(pin_matches(&terminal, &w));
    }

    #[test]
    fn different_identities_do_not_match_even_with_the_same_executable() {
        // A web app installed from Chrome runs chrome.exe under its own identity.
        let chrome = pin("Chrome", Some("Chrome"), Some(CHROME));
        assert!(pin_matches(&chrome, &window(1, CHROME, Some("Chrome"))));
        assert!(!pin_matches(
            &chrome,
            &window(2, CHROME, Some("Chrome._crx_abc"))
        ));
    }

    #[test]
    fn matches_by_executable_when_an_identity_is_missing() {
        let code = pin("Visual Studio Code", None, Some(CODE));
        assert!(pin_matches(&code, &window(1, &CODE.to_uppercase(), None)));
        // The pin declares an identity but the window doesn't expose one.
        let chrome = pin("Chrome", Some("Chrome"), Some(CHROME));
        assert!(pin_matches(&chrome, &window(2, CHROME, None)));
        assert!(!pin_matches(&code, &window(3, CHROME, None)));
    }

    #[test]
    fn matches_squirrel_launchers_to_their_versioned_executable() {
        let discord = pin(
            "Discord",
            None,
            Some(r"C:\Users\u\AppData\Local\Discord\Update.exe"),
        );
        assert!(pin_matches(
            &discord,
            &window(
                1,
                r"C:\Users\u\AppData\Local\Discord\app-1.0.9219\Discord.exe",
                None
            )
        ));
        // Another Squirrel app's window must not match.
        assert!(!pin_matches(
            &discord,
            &window(
                2,
                r"C:\Users\u\AppData\Local\slack\app-4.1.0\slack.exe",
                None
            )
        ));
    }

    #[test]
    fn file_explorer_windows_belong_to_the_file_explorer_pin() {
        let explorer = pin("File Explorer", Some("Microsoft.Windows.Explorer"), None);
        assert!(pin_matches(
            &explorer,
            &window(1, r"C:\Windows\explorer.exe", None)
        ));
        assert!(!pin_matches(
            &explorer,
            &window(2, r"C:\Windows\notepad.exe", None)
        ));
        // Another identity-only pin does not claim Explorer's windows.
        let terminal = pin(
            "Terminal",
            Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
            None,
        );
        assert!(!pin_matches(
            &terminal,
            &window(3, r"C:\Windows\explorer.exe", None)
        ));
    }

    #[test]
    fn matches_a_pin_that_points_at_an_older_version_folder() {
        let discord = pin(
            "Discord",
            Some("com.squirrel.Discord.Discord"),
            Some(r"C:\Users\u\AppData\Local\Discord\app-1.0.9200\Discord.exe"),
        );
        assert!(pin_matches(
            &discord,
            &window(
                1,
                r"C:\Users\u\AppData\Local\Discord\app-1.0.9268\Discord.exe",
                None
            )
        ));
        // Another program in the same app's version folder does not match.
        assert!(!pin_matches(
            &discord,
            &window(
                2,
                r"C:\Users\u\AppData\Local\Discord\app-1.0.9268\helper.exe",
                None
            )
        ));
        // A folder that merely starts with "app-" is not a version folder.
        assert_eq!(
            without_version_dir(r"C:\app-data\x.exe"),
            r"c:\app-data\x.exe"
        );
    }

    #[test]
    fn a_pin_without_identity_or_target_matches_nothing() {
        assert!(!pin_matches(
            &pin("Mystery", None, None),
            &window(1, CHROME, None)
        ));
    }

    #[test]
    fn pins_come_first_then_running_apps_in_first_seen_order() {
        let pins = [
            pin("Chrome", Some("Chrome"), Some(CHROME)),
            pin("Code", None, Some(CODE)),
        ];
        let windows = [
            window(10, r"C:\Windows\notepad.exe", None),
            window(11, CODE, None),
            window(12, r"C:\Windows\notepad.exe", None),
            window(13, r"C:\Tools\other.exe", None),
        ];
        let groups = build_groups(&pins, &windows, 12);
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Chrome", "Code", "notepad", "other"]);

        assert!(groups[0].pinned && groups[0].windows.is_empty());
        assert_eq!(groups[1].windows.len(), 1);
        // Both Notepad windows share one icon; the foreground one marks it active.
        assert_eq!(
            groups[2].windows.iter().map(|w| w.hwnd).collect::<Vec<_>>(),
            [10, 12]
        );
        assert!(groups[2].active && !groups[2].pinned);
        assert!(groups[2].windows[1].active && !groups[2].windows[0].active);
        assert!(!groups[1].active);
    }

    #[test]
    fn group_ids_are_stable_and_distinguish_pinned_from_running() {
        let pins = [pin("Code", None, Some(CODE))];
        let windows = [window(1, r"C:\Windows\notepad.exe", None)];
        let groups = build_groups(&pins, &windows, 0);
        assert_eq!(groups[0].id, format!("pin:{}", CODE.to_ascii_lowercase()));
        assert_eq!(groups[1].id, r"exe:c:\windows\notepad.exe");
    }

    fn group(windows: &[(isize, bool, bool)]) -> AppGroup {
        AppGroup {
            id: "x".into(),
            name: "x".into(),
            pinned: true,
            active: windows.iter().any(|w| w.1),
            windows: windows
                .iter()
                .map(|&(hwnd, active, minimized)| AppWindow {
                    hwnd,
                    title: String::new(),
                    minimized,
                    active,
                })
                .collect(),
        }
    }

    #[test]
    fn click_launches_an_app_with_no_windows() {
        assert_eq!(click_action(&group(&[])), ClickAction::Launch);
    }

    #[test]
    fn click_focuses_a_background_window_and_minimizes_the_foreground_one() {
        assert_eq!(
            click_action(&group(&[(7, false, false)])),
            ClickAction::Focus(7)
        );
        assert_eq!(
            click_action(&group(&[(7, false, true)])),
            ClickAction::Focus(7)
        );
        assert_eq!(
            click_action(&group(&[(7, true, false)])),
            ClickAction::Minimize(7)
        );
    }

    #[test]
    fn click_cycles_through_several_windows() {
        let windows = [(1, false, false), (2, true, false), (3, false, false)];
        assert_eq!(click_action(&group(&windows)), ClickAction::Focus(3));
        // From the last window it wraps to the first.
        let windows = [(1, false, false), (2, false, false), (3, true, false)];
        assert_eq!(click_action(&group(&windows)), ClickAction::Focus(1));
    }

    #[test]
    fn click_prefers_a_visible_window_when_none_is_in_front() {
        let windows = [(1, false, true), (2, false, false)];
        assert_eq!(click_action(&group(&windows)), ClickAction::Focus(2));
        // All minimized: restore the first.
        let windows = [(1, false, true), (2, false, true)];
        assert_eq!(click_action(&group(&windows)), ClickAction::Focus(1));
    }
}
