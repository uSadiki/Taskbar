# Glassdock progress log

The plan is in [PLAN.md](PLAN.md). This file records what is done, what was decided along the way, and what is known to be open.

## Status

| Phase | State |
|---|---|
| 0. Prerequisites and plan | Done 2026-11-16 |
| 1. Dock shell | Built 2026-11-16; waiting for your full-mode test |
| 2. Liquid glass | Not started |
| 3. Apps widget | Not started |
| 4. Now Playing, Battery Ring, Quick Toggles | Not started |
| 5. Weather, GitHub Activity, Claude Usage | Not started |
| 6. Polish and release | Not started |

## How to run

| What | Command |
|---|---|
| Full mode (registers the AppBar, changes the taskbar) | `npm run tauri dev` |
| Safe mode (no AppBar, taskbar untouched) | `$env:GLASSDOCK_SAFE='1'; npm run tauri dev` |
| Put the taskbar back, whatever state things are in | `src-tauri\target\debug\glassdock.exe --restore` |
| Rust tests and lint | `cargo test --lib` and `cargo clippy --all-targets -- -D warnings` in `src-tauri` |
| Frontend tests and type check | `npm test` and `npm run typecheck` |

Logs are in `%LOCALAPPDATA%\Glassdock\logs` (`glassdock.log` from the app, `shell.log` from the watchdog, `--restore` and crash handlers). Settings are in `%APPDATA%\Glassdock\settings.json`.

## Phase 0: prerequisites (done)

Installed with winget: Visual Studio Build Tools 2022 (C++ workload, MSVC 14.44, Windows SDK 10.0.26100) and rustup with stable MSVC (rustc 1.100.0). Already present: Node 22.19.0, WebView2 157, Git 2.47.0.

## Phase 1: dock shell

### Done

- Tauri 2.12 + React 19 + TypeScript + Vite 8 + Tailwind 4 scaffold; original app icon (`assets/app-icon.svg`).
- Dock window: borderless, transparent, topmost, tool window (no Alt-Tab entry, no taskbar button), does not take focus when clicked.
- AppBar registration with `SHAppBarMessage`; bottom or top edge; pill or full-width layout; three sizes; monitor choice with fallback to the primary.
- Window region: the window spans the monitor width and has 640 logical px of popover headroom, but only the dock and open menus are visible and clickable.
- Taskbar modes (auto-hide, hide completely) with a restore journal written before the first change.
- Restore paths: normal exit, panic on the main thread, unhandled exception, watchdog process for hard kills, leftover journal at next start, session end, tray and dock menu "Restore taskbar", and `glassdock.exe --restore`.
- `TaskbarCreated` handling (Explorer restart) re-registers everything. Explorer is never killed or restarted.
- Fullscreen detection: `ABN_FULLSCREENAPP` plus a foreground-window check; the dock hides while a fullscreen app is in front.
- DPI and display changes reposition the dock.
- Drag the dock to the other edge; it snaps on release.
- Tray menu and dock right-click menu: Settings, Restore taskbar, Reload, Quit.
- Single instance: a second launch opens Settings in the running instance.
- Settings store (`%APPDATA%\Glassdock\settings.json`, versioned, atomic writes, corrupt files set aside) and a Settings window skeleton: General and Appearance work, Widgets can be switched on/off and reordered, quiet hours are stored, About has the controls.
- File logging with rotation.
- Safe mode (`--safe` or `GLASSDOCK_SAFE=1`).

### Verified by me

- 42 Rust unit tests (settings, merge patch, journal, AppBar rect math, placement math, monitor choice, fullscreen rule, CLI modes, panic guard) and 10 frontend tests pass. Clippy with warnings as errors is clean; `tsc` is clean.
- Safe-mode run under `tauri dev`: the dock rendered as a pill with a transparent surround; a point inside the pill hit-tested to the dock, points beside it and in the popover headroom hit-tested to the app behind.
- Settings window opened through a second launch and rendered correctly.
- Crash path without touching the desktop: a journal recording the taskbar's current state plus a stand-in process. The launcher started a detached watchdog; when the stand-in exited, the watchdog restored from the journal, deleted it and exited. With no journal it did nothing. `--restore` ran cleanly. Taskbar state before and after was identical.

### Not verified by me (your test list)

Full mode was not run by me, so everything that needs the real AppBar and taskbar changes is untested: space reservation, both taskbar modes, restore on Quit and on End Task, fullscreen hiding, DPI changes, and "hide completely" on this Insider build.

### Findings

- **`SetWindowRgn` works with a transparent WebView2 window.** The plan's main bet holds, so popovers will live in the dock window and share one glass canvas. The resize fallback is not needed.
- **`WS_EX_NOACTIVATE` on the dock window alone does not stop clicks from activating it.** The click lands on WebView2's `Chrome_RenderWidgetHostHWND`, which answered `WM_MOUSEACTIVATE` with `MA_ACTIVATE`. Chromium returns `MA_NOACTIVATE` when that window's parent has `WS_EX_NOACTIVATE`, so the dock now sets the style on WebView2's child windows too. Confirmed by sending `WM_MOUSEACTIVATE` to the child before and after; a real click still needs your confirmation (see the test list).
- **Pinned apps**: the pin folder holds two `.lnk` files (File Explorer, Visual Studio Code) while the taskbar shows eight apps. Which of the other six are pinned rather than just running is checked in phase 3; pins that are not in the folder come from the `Taskband\Favorites` read approved in the plan.
- The taskbar state from `ABM_GETSTATE` on this PC is `0` (not auto-hide).

### Measurements

Release build, safe mode, dock idle with seven placeholder tiles and the clock, measured over 30 s after a 20 s settle, on this PC (22 logical cores, 2560×1600 at 150%).

| Metric | Value | Target |
|---|---|---|
| CPU, whole process tree | 0.05% of one core | under 1% |
| Memory, private working set (Task Manager's "Memory"), whole tree | 145 MB | under 150 MB |
| Memory, private bytes, whole tree | 206 MB | |
| Executable size | 8.9 MB | |

Per process (private working set): WebView2 GPU process 72 MB, WebView2 browser 29 MB, renderer 19 MB, glassdock 14 MB, network service 7 MB, storage service 3 MB, crashpad 2 MB.

Memory is already at the target before the glass renderer and the widgets exist, so it will go over unless something gives. The largest single item is the GPU process, and the dock window is a 2560×1062 px transparent surface at all times. The first thing to try in the performance pass is keeping the window at strip height while no popover is open and growing it only when one opens; that trades the "never resize" simplicity for a smaller idle surface. This is measured again at the end of every phase.

## Decisions

| Date | Decision |
|---|---|
| 2026-11-16 | Claude Usage uses the status line bridge plus local session logs (plan 5.1, option A + B). |
| 2026-11-16 | Glassdock may read `Taskband\Favorites` (read-only) for Store-app pins and pin order. |
| 2026-11-16 | Focus Assist and Night Light are left out entirely. |
| 2026-11-16 | TypeScript 6.0 instead of the 7.0 named in the plan: it is what the current official Tauri template pins. |
| 2026-11-16 | The full-width layout reserves only the dock's own height (no gap), so it sits flush with the screen edge like a taskbar. The pill reserves its height plus an 8 px gap on each side. |
| 2026-11-16 | Release builds unwind on panic (no `panic = "abort"`), so a panic in a worker or a Windows callback is contained instead of ending the process. |
| 2026-11-16 | "Restore taskbar" restores the taskbar and pauses taskbar management until Reload or until a taskbar mode is picked in Settings. The dock stays registered and sits above the taskbar. |
| 2026-11-16 | The watchdog is started through a short-lived launcher process so it is not part of the dock's process tree and survives a tree kill. |

## Known issues and open points

- Dragging snaps to the new edge without animation. The spring arrives with the glass renderer in phase 2.
- The dock uses a plain translucent fill until phase 2.
- Widget tiles are placeholders until their phases.
- Content Security Policy is off (`csp: null`); to be tightened in phase 6 once all asset sources are known.
- In auto-hide mode with the dock at the bottom, pushing the cursor into the bottom screen edge slides the Windows taskbar up over the dock. This is inherent to auto-hide; "hide completely" avoids it.
- Fullscreen detection relies on `ABN_FULLSCREENAPP` for a window that goes fullscreen without a focus change (F11 in a browser). If that notification turns out to be unreliable on this build, a location-change hook is the fallback.
