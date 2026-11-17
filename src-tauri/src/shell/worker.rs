//! A dedicated thread for shell COM work (shortcuts, icons, launching).
//!
//! Shell objects want a single-threaded apartment, and an apartment that
//! never pumps messages stalls system-wide broadcasts. This thread owns one
//! apartment and runs a real message loop; work arrives as posted jobs. It
//! keeps slow shell calls off the main thread, so the dock never hitches.

use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};

use parking_lot::Mutex;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    TranslateMessage, WM_APP,
};

use super::guard;

type Job = Box<dyn FnOnce() + Send>;

const WM_JOB: u32 = WM_APP + 0x70;

struct Worker {
    sender: Mutex<Sender<Job>>,
    thread_id: u32,
}

static WORKER: OnceLock<Worker> = OnceLock::new();

fn worker() -> &'static Worker {
    WORKER.get_or_init(|| {
        let (job_tx, job_rx) = channel::<Job>();
        let (id_tx, id_rx) = channel::<u32>();
        let spawned = std::thread::Builder::new()
            .name("shell-worker".into())
            .spawn(move || unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let mut msg = MSG::default();
                // Touching the queue creates it, so posts can't be lost.
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                let _ = id_tx.send(GetCurrentThreadId());
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                    if msg.hwnd.is_invalid() && msg.message == WM_JOB {
                        while let Ok(job) = job_rx.try_recv() {
                            guard::run("a shell worker job", job);
                        }
                    } else {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            });
        if let Err(e) = spawned {
            log::error!("shell worker: could not start: {e}");
        }
        Worker {
            sender: Mutex::new(job_tx),
            // 0 if the thread failed to start; posting to it then fails quietly.
            thread_id: id_rx.recv().unwrap_or(0),
        }
    })
}

/// Run `job` on the shell thread, without waiting for it.
pub fn post(job: impl FnOnce() + Send + 'static) {
    let worker = worker();
    if worker.sender.lock().send(Box::new(job)).is_ok() {
        unsafe {
            let _ = PostThreadMessageW(worker.thread_id, WM_JOB, WPARAM(0), LPARAM(0));
        }
    }
}

/// Run `job` on the shell thread and wait for its result. Returns `None` if
/// the job panicked or the worker is gone. Must not be called from the shell
/// thread itself, and should not be called from the main thread.
pub fn call<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = channel();
    post(move || {
        let _ = tx.send(job());
    });
    rx.recv().ok()
}
