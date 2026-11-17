# Glassdock progress log

The plan is in [PLAN.md](PLAN.md). This file records what is done, what was decided along the way, and what is known to be open.

## Status

| Phase | State |
|---|---|
| 0. Prerequisites and plan | Done 2026-11-16 |
| 1. Dock shell | Done 2026-11-16, verified in full mode |
| 2. Liquid glass | Done 2026-11-17 |
| 3. Apps widget | Done 2026-11-17 |
| 4. Now Playing, Battery Ring, Quick Toggles | Done 2026-11-17 |
| 5. Weather, GitHub Activity, Claude Usage | Done 2026-11-17 |
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

## Working agreement (changed 2026-11-16)

The plan had a stop after every phase for you to test. You asked for that to go: I verify each phase myself, including full-mode runs that really change the taskbar, and hand over a finished app with usage instructions. The "what to test" lists in PLAN.md are now my checklist, not yours.

How I verify without touching your mouse or keyboard: `scripts/cdp.mjs` drives the page over the DevTools protocol (direct element clicks and in-page screenshots), `scripts/shellstate.ps1` reads taskbar, work area and journal state, `scripts/fullscreen-test.ps1` and `scripts/focus-test.ps1` exercise the fullscreen and popover-dismissal rules by activating windows, and `scripts/capture.ps1` grabs the composited screen.

## Phase 1 follow-up: verified in full mode

- Space reservation, top and bottom edge, pill and full width, both taskbar modes, "Restore taskbar" and resuming, and a clean `--quit` all behaved as designed; the taskbar state and work area were back to their originals afterwards.
- Fixed: the dock did not hide for fullscreen windows. `GetForegroundWindow` can be null mid-switch; the check now uses the window named by the foreground event and retries when inconclusive. It also runs at startup and after every reposition, and `show` no longer un-hides a dock that is hidden for a fullscreen app.
- Under `tauri dev` the watchdog is killed together with the dock (the dev tool and this harness run everything in one job object), so there the next start restores the taskbar from the leftover journal. Outside a job the watchdog survives; that case is still to be confirmed with an installed build.

## Phase 2: liquid glass

### Done

- Backend: `IDesktopWallpaper` query (image, fit mode, background colour, per-monitor), fit-mode placement math for fill, fit, stretch, tile, center and span, WIC decoding and scaling, and compositing over the background colour. The backdrop is the whole monitor as Windows paints it, so the glass needs no re-read when the dock moves.
- Change detection: `WM_SETTINGCHANGE` plus a watcher on the Windows themes folder, debounced, skipped when nothing relevant changed. Seen working live: this PC's ASUS OLED shifter swaps the wallpaper file every few minutes and each swap produced a new backdrop.
- Frontend: a WebGL2 renderer with one fragment shader for all glass (rounded-rect distance fields merged with a smooth minimum). Edge refraction, chromatic fringing, frost, specular rim, inner rim line, edge glow, saturation and brightness lift, tint, soft shadow. Draws only on change; no loop at idle.
- Spring-driven shapes: the dock's glass follows its size; popovers grow out of the tile they were opened from and merge with the dock on the way.
- Popovers live in the dock window. The calendar (from the clock) and the right-click menu are the first two. They take focus while open; Esc, clicking the dock background, or activating another window closes them.
- Six themes (Glass, iOS Glass, White, Black, Midnight, Graphite), light/dark following Windows with an override, user accent colour, five glass sliders in Settings with reset, and adaptive light or dark content chosen per surface from the wallpaper behind it.
- Glass lab (dev builds, from the dock menu): twelve sliders and a tint picker, applied live, with "Copy as theme JSON".
- Reduced motion: springs become jumps.

### Verified

- 60 Rust tests (17 new: placement math, compositing, wallpaper mapping) and 38 frontend tests (28 new: luminance grid, tone choice, themes, calendar, colour).
- On screen in full mode: the glass refracts the real wallpaper at the bottom and the top edge, all six themes render with readable content, the calendar and menu open and dismiss correctly.

### Bugs found and fixed while verifying

- Red and blue were swapped in the backdrop: WIC's scaler returns BGRA even when RGBA is requested. It was invisible on near-neutral sand at the bottom of the wallpaper and obvious against blue sky at the top.
- Popover text was hard to read on a busy wallpaper; popover glass now gets extra frost on top of the theme's.

### Not done

- The acrylic fallback from the plan is not implemented. When the wallpaper cannot be read, the glass falls back to a flat theme-coloured fill with the same highlights. The fallback path itself has not been exercised, because the wallpaper has always been readable here.
- EXIF orientation of wallpapers is not applied.
- Only the Fill fit mode has been seen on screen; the others are covered by unit tests only.
- The drag between edges still snaps without a spring.

## Phase 3: Apps widget

### Done

- Pinned apps from the `Taskband\Favorites` registry value (read-only), in taskbar order, including Store apps. The layout matched what the plan assumed: `[0x00][u32 length][item id list]` records ended by `0xFF`. Falls back to the pin folder's shortcuts. Pinning or unpinning in Windows updates the dock (pin folder watcher plus `RegNotifyChangeKeyValue`).
- Open windows by the Alt-Tab rule (visible, not a tool window, not owned, not cloaked, titled), grouped by AppUserModelID, then executable.
- High-resolution icons from `IShellItemImageFactory`, kept in memory for the session.
- Left click launches, focuses, minimizes the app in front, or steps through an app's windows. Middle click opens a new instance. Right click opens a menu with the app's windows (focus or close each), New window, and Close all.
- A dot per window, an accent bar under the app in front, spring magnification on hover.
- Shell work runs on its own thread (a single-threaded apartment with a message pump), so the dock never waits on the shell.

### Verified

- All eight of this PC's pins resolved with name, identity and icon: Discord, Spotify, Chrome, Terminal, File Explorer, Visual Studio Code, Armoury Crate, Claude.
- With a File Explorer window opened for the test: launch from the pin, focus, minimize by clicking the app in front, restore, and close all each behaved correctly.
- 22 new unit tests for grouping, matching and click behaviour.

### Bugs found and fixed while verifying

- Discord showed twice. Its pin points at `Discord\Discord.exe` while it runs from `Discord\app-<version>\Discord.exe`; matching now ignores version folders.
- File Explorer's pin has no shortcut target and its windows declare no identity; they are now matched by executable.

### Not done

- Live window thumbnails on hover (the stretch goal).
- Reordering widgets by dragging in the dock. Settings has up/down buttons.
- Icons are not cached on disk; each is rendered once per run.
- Only left click, the menu and its actions were exercised. Middle click and the hover magnification were not exercised with a real mouse.

## Phase 4: Now Playing, Battery Ring, Quick Toggles

### Done

- **Now Playing**: Windows' media session manager, event-driven with no polling. Prefers Spotify when several apps are playing. Tile with artwork, scrolling title and artist, play/pause. Popover with large artwork, seek bar, previous/play/next, shuffle and repeat when the player supports them, tinted from the artwork's average colour. Hidden when nothing is playing.
- **Battery Ring**: level, charging state, time remaining, and time to full from the charge rate. Updated by power notifications, no timer. Hidden on a PC without a battery.
- **Quick Toggles**: Wi-Fi and Bluetooth (`Windows.Devices.Radios`), volume, mute and output device (Core Audio), brightness (WMI), Windows light/dark mode, and Quiet for the dock's own alerts.
- Output device switching uses the undocumented `IPolicyConfig` interface, as the plan said it would have to; it is confined to one call in `toggles/audio.rs`.
- Focus Assist and Night Light are not included, as decided.

### Verified on this PC

- Radios: both found without a package identity; Wi-Fi reported on, Bluetooth off.
- Volume +1 and back, mute and unmute, brightness -1 and back all took effect and returned to the starting values. Dark mode, Bluetooth and the output device were re-applied at their current values, which exercises the calls without changing anything. No warnings were logged.
- Now Playing, with a silent test clip in an Edge window: title, artist and artwork arrived; pause, play and seek worked; the tile disappeared when the player closed.
- 12 new backend tests (battery state, media commands, brightness path) and 6 new frontend tests.

### Not verified

- Switching Wi-Fi or Bluetooth to the other state, switching to the other output device, and flipping light/dark mode: each would have disturbed the PC in use, so only the no-change calls were made.
- Spotify itself, and shuffle/repeat (the test clip does not offer them).
- Charging: the PC was on battery throughout.

### Not done

- Bluetooth device batteries and external-monitor brightness over DDC/CI (both stretch goals).
- Brightness changed outside the dock (hotkeys) shows the next time the popover opens, not live.

## Phase 5: Weather, GitHub Activity, Claude Usage

### Done

- **Weather**: Open-Meteo forecast (current conditions, 24 hours, 7 days) and city search, Celsius or Fahrenheit, refreshed every 30 minutes and on demand, cached for startup and offline. Optional "Locate" uses ipwho.is and only runs when clicked. Original animated SVG icons for every WMO code, still under reduced motion.
- **GitHub Activity**: one GraphQL request for the contribution calendar, open pull requests and review requests, plus the REST events feed. Streaks computed in Rust. The token is checked against GitHub before it is saved, and kept in Windows Credential Manager. States for no token, rejected token, rate limit (with reset time), offline (last data kept) and other errors. Refreshed every 15 minutes and on demand. Links open in the default browser (https only).
- **Claude Usage**: as decided, the status line bridge plus the local logs. `glassdock.exe --statusline` reads Claude Code's JSON, saves `rate_limits.five_hour` / `seven_day` to `%LOCALAPPDATA%\Glassdock\claude-limits.json`, and prints a status line (model, context, 5h %, week %). The app adds itself to `~/.claude/settings.json` only if no other status line is set, never replaces someone else's, removes only its own entry when turned off, and keeps every other key and the key order. Development builds don't register themselves. Logs are read incrementally (only changed files), deduplicated per response, and summarised into the current 5-hour block, burn rate, projected cost, per-day and per-model totals with an API-price estimate.
- **Alerts**: toasts above the dock at 80% and 95% of either limit, once per window, remembered across restarts. Held during Quiet or the quiet-hours schedule (overnight schedules work) and shown when quiet time ends.

### Verified

- 35 new backend tests (forecast and geocoding parsing, streaks, GraphQL and events parsing, log parsing against Claude Code 2.1.325's line format, dedupe, pricing including fast mode, 5-hour blocks, projection, status line merge and install/uninstall rules, quiet hours) and 13 new frontend tests.
- Live: city search returned Oslo, Norway first; the forecast loaded (24 hours, 7 days, sunrise). An invalid GitHub token was rejected with a clear message and nothing was stored. Claude Usage read this PC's real logs (block, 7 days, Fable 5.2 and Opus 5.5 split). The status line mode, run through Git Bash with sample input, printed `Opus 5.5 | context 12% | 5h 24% | week 41%` and saved the limits; about 160 ms per call.
- On screen: Weather, Claude, GitHub (with sample data, as there is no token here) popovers and a sample toast.

### Not verified

- GitHub with a real token: the parsing is tested, the live request isn't.
- The bridge registering itself: only the installed (release) app does that, see Phase 6.
- A real 80% alert from real limits.
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
