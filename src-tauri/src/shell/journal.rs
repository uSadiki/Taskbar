//! The taskbar restore journal.
//!
//! Before Glassdock changes anything about the Windows taskbar it writes the
//! taskbar's original state here. The file exists exactly while the taskbar is
//! in a state Glassdock put it in, so anything that finds it (a clean exit,
//! the watchdog, the next start, `--restore`) knows what to undo.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::settings::TaskbarMode;

pub const JOURNAL_VERSION: u32 = 1;

/// `ABM_GETSTATE` / `ABM_SETSTATE` bits.
pub const ABS_AUTOHIDE: u32 = 0x1;
pub const ABS_ALWAYSONTOP: u32 = 0x2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarJournal {
    pub version: u32,
    /// The Glassdock process that owns the current taskbar state.
    pub pid: u32,
    /// Taskbar state bits before Glassdock touched anything.
    pub original_state: u32,
    /// Whether the taskbar window was visible before Glassdock touched anything.
    pub taskbar_was_visible: bool,
    /// The mode Glassdock last applied.
    pub applied_mode: TaskbarMode,
}

/// What a restore has to do, derived from the journal alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestorePlan {
    pub state: u32,
    pub show_taskbar: bool,
}

pub fn restore_plan(journal: &TaskbarJournal) -> RestorePlan {
    RestorePlan {
        state: journal.original_state & (ABS_AUTOHIDE | ABS_ALWAYSONTOP),
        show_taskbar: journal.taskbar_was_visible,
    }
}

/// The taskbar state while Glassdock manages it: the original bits plus auto-hide.
/// Both modes need auto-hide, because a hidden taskbar that is not auto-hiding
/// still reserves its strip of the screen.
pub const fn managed_state(original_state: u32) -> u32 {
    original_state | ABS_AUTOHIDE
}

pub fn load_from(path: &Path) -> Option<TaskbarJournal> {
    let text = fs::read_to_string(path).ok()?;
    let journal: TaskbarJournal = serde_json::from_str(&text).ok()?;
    (journal.version == JOURNAL_VERSION).then_some(journal)
}

/// Write via a temporary file and a rename so the journal is never half-written.
pub fn save_to(path: &Path, journal: &TaskbarJournal) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(journal).map_err(io::Error::other)?,
    )?;
    fs::rename(&tmp, path)
}

pub fn clear_at(path: &Path) {
    let _ = fs::remove_file(path);
}

pub fn load() -> Option<TaskbarJournal> {
    load_from(&paths::journal_path())
}

pub fn save(journal: &TaskbarJournal) -> io::Result<()> {
    save_to(&paths::journal_path(), journal)
}

pub fn clear() {
    clear_at(&paths::journal_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("glassdock-journal-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("taskbar-restore.json")
    }

    fn sample(original_state: u32, visible: bool, mode: TaskbarMode) -> TaskbarJournal {
        TaskbarJournal {
            version: JOURNAL_VERSION,
            pid: 1234,
            original_state,
            taskbar_was_visible: visible,
            applied_mode: mode,
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let path = temp_file("roundtrip");
        let journal = sample(ABS_ALWAYSONTOP, true, TaskbarMode::Hidden);
        save_to(&path, &journal).unwrap();
        assert_eq!(load_from(&path), Some(journal));
        clear_at(&path);
        assert_eq!(load_from(&path), None);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_corrupt_or_foreign_version_is_ignored() {
        let path = temp_file("corrupt");
        assert_eq!(load_from(&path), None);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(load_from(&path), None);
        let mut future = sample(0, true, TaskbarMode::AutoHide);
        future.version = JOURNAL_VERSION + 1;
        fs::write(&path, serde_json::to_vec(&future).unwrap()).unwrap();
        assert_eq!(load_from(&path), None);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn restore_returns_the_original_state() {
        // Taskbar was always-visible before: restore clears auto-hide.
        let plan = restore_plan(&sample(ABS_ALWAYSONTOP, true, TaskbarMode::Hidden));
        assert_eq!(
            plan,
            RestorePlan {
                state: ABS_ALWAYSONTOP,
                show_taskbar: true
            }
        );

        // Taskbar was already auto-hiding before: restore keeps auto-hide.
        let plan = restore_plan(&sample(
            ABS_AUTOHIDE | ABS_ALWAYSONTOP,
            true,
            TaskbarMode::AutoHide,
        ));
        assert_eq!(plan.state, ABS_AUTOHIDE | ABS_ALWAYSONTOP);
    }

    #[test]
    fn restore_does_not_unhide_a_taskbar_that_was_hidden_before() {
        let plan = restore_plan(&sample(0, false, TaskbarMode::Hidden));
        assert!(!plan.show_taskbar);
    }

    #[test]
    fn managed_state_adds_autohide_and_keeps_other_bits() {
        assert_eq!(managed_state(0), ABS_AUTOHIDE);
        assert_eq!(
            managed_state(ABS_ALWAYSONTOP),
            ABS_AUTOHIDE | ABS_ALWAYSONTOP
        );
        assert_eq!(managed_state(ABS_AUTOHIDE), ABS_AUTOHIDE);
    }

    #[test]
    fn unknown_state_bits_are_not_written_back() {
        let plan = restore_plan(&sample(0xF0 | ABS_AUTOHIDE, true, TaskbarMode::AutoHide));
        assert_eq!(plan.state, ABS_AUTOHIDE);
    }
}
