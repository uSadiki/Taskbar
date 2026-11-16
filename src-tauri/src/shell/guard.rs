//! Panic containment for callbacks that Windows calls into (window procs,
//! WinEvent hooks). Unwinding across that boundary is undefined behaviour, and
//! one bad callback must not take the dock down.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// True while the current thread is inside `run`. The panic hook uses this to
/// tell a contained panic from one that will end the process.
pub fn in_guarded_callback() -> bool {
    DEPTH.with(|depth| depth.get() > 0)
}

/// Run `f`, returning `None` if it panicked.
pub fn run<T>(name: &str, f: impl FnOnce() -> T) -> Option<T> {
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    let result = catch_unwind(AssertUnwindSafe(f));
    DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    match result {
        Ok(value) => Some(value),
        Err(_) => {
            log::error!("a panic was contained in {name}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_value_and_tracks_depth() {
        assert!(!in_guarded_callback());
        let inside = run("test", in_guarded_callback);
        assert_eq!(inside, Some(true));
        assert!(!in_guarded_callback());
    }

    #[test]
    fn contains_a_panic_and_resets_depth() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result: Option<()> = run("test", || panic!("boom"));
        std::panic::set_hook(previous);
        assert_eq!(result, None);
        assert!(!in_guarded_callback());
    }
}
