# Glassdock — implementation plan

## Context

Glassdock is a personal Windows 10/11 taskbar replacement: a floating "smart dock" registered as an AppBar, with seven live widgets plus a clock, and a liquid-glass look that refracts the real wallpaper instead of blurring it. It is inspired by lumabar's behaviour (pill or full width, top or bottom, taskbar auto-hide or hidden, six styles) but uses its own name, icons and assets. It runs on a daily-driver PC, so the overriding constraint is that nothing it changes in Windows may outlive the process.

The project folder is empty (no git repo, no `./reference/`), so this is a greenfield build. On approval, this plan is saved as `PLAN.md` in the repo and `PROGRESS.md` is started.

## Phase 0 status: prerequisites (done)

| Tool | Version | Note |
|---|---|---|
| Rust | rustc/cargo 1.100.0, `stable-x86_64-pc-windows-msvc` default | installed via winget (rustup 1.29.3) |
| VS Build Tools 2022 | 17.14, MSVC 14.44, Windows SDK 10.0.26100 | installed via winget, C++ workload |
| Node.js | 22.19.0 (npm 10.9.3) | already present |
| WebView2 runtime | 157.0.4341.46 | already present |
| Git | 2.47.0 | already present |

Machine facts that shape the plan: Windows 11 build 26300 (Insider), one 2560×1600 display at 150% scaling, a battery, Claude Code 2.1.325 with no `statusLine` configured, and a pinned-taskbar folder that holds only `File Explorer.lnk` and `Visual Studio Code.lnk`.

## 1. Architecture

### 1.1 Processes and windows

- **One Tauri 2 process** (`glassdock.exe`): Rust backend plus one WebView2 webview at idle.
- **Watchdog**: the same exe started with `--watchdog <pid>`. It has no window, waits on the main process handle, and restores the taskbar if the main process dies without cleaning up. See 1.5.
- **Windows**:
  - `dock`: always present. Borderless, transparent, topmost, `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` (not in Alt-Tab, no taskbar button, clicks don't steal focus so "click the active app to minimize" works). It explicitly takes focus only while a popover is open, so deactivation and Esc dismiss it.
  - `settings`: created on demand, destroyed on close, so it costs no memory at idle. A normal window that does appear in Alt-Tab.
  - `glasslab`: dev builds only, created on demand.
- **Popovers live inside the dock window**, not in separate windows. The dock window is as wide as the monitor and as tall as the dock band plus popover headroom. A window region (`SetWindowRgn`) limits what is visible and clickable to the dock pill, any open popover and any toast. This keeps one WebGL canvas for all glass, which is what makes the droplet morph between dock and popover seamless, and it avoids resizing a transparent webview during animations. This is the main technical bet; Phase 1 starts with a spike to prove it, and the fallback is resizing the window before a popover opens and after it closes.

### 1.2 Rust backend (`src-tauri/src/`)

| Module | Responsibility |
|---|---|
| `main.rs`, `lib.rs` | Argument dispatch (`--watchdog`, `--restore`, `--statusline`, `--safe`), Tauri builder, plugins, command registration |
| `error.rs` | `AppError` (thiserror), serializable for IPC |
| `settings/` | Versioned schema, atomic load/save, migration, change events |
| `shell/` | `subclass.rs` (window proc hook), `appbar.rs`, `taskbar.rs`, `journal.rs`, `watchdog.rs`, `monitors.rs`, `fullscreen.rs`, `tray.rs` |
| `glass/` | `wallpaper.rs` (IDesktopWallpaper), `slice.rs` (pure fit-mode math), `decode.rs` (WIC), `backdrop.rs`, `luma.rs` |
| `widgets/` | `apps/`, `media.rs`, `battery.rs`, `toggles/`, `weather.rs`, `github.rs`, `claude/` |
| `alerts/` | Toast queue, thresholds, quiet hours |

Threading: the main thread runs the Tauri event loop and the dock window's subclass proc. One COM worker thread owns Core Audio, WMI and shell COM objects. WinRT events arrive on thread-pool threads and are forwarded with `AppHandle::emit`. HTTP runs on Tauri's tokio runtime. All network I/O is done in Rust, so tokens never enter the webview and the CSP can block outbound requests from it.

### 1.3 Frontend (`src/`)

React 19 + TypeScript + Vite, Tailwind 4 for layout and theme tokens, Motion (the current Framer Motion package) for springs and drag-reorder, zustand stores fed by backend events, and a raw WebGL2 renderer for the glass (one full-window quad, about 200 lines, no library).

```
src/dock/      Dock.tsx, widgets/*, popovers/*, ContextMenu
src/glass/     renderer.ts, glass.frag, shapes.ts, contrast.ts, GlassLab.tsx
src/settings/  Settings window
src/themes/    six theme definitions (CSS tokens + glass parameters)
src/ipc/       typed invoke/listen wrappers and shared types
src/stores/    one store per widget + settings
```

### 1.4 IPC

Commands return `Result<T, AppError>`. The backend pushes state with events named `<domain>://changed` (for example `media://changed`); the frontend never polls. Binary payloads (backdrop pixels, icons, album art) use `tauri::ipc::Response` raw bytes.

### 1.5 Never leaving Windows broken

What the app changes: the taskbar's auto-hide state, the taskbar window's visibility (hide mode only), and its own AppBar registration.

1. **Journal first.** Before the first change, the original taskbar state (`ABM_GETSTATE`, visibility of `Shell_TrayWnd` and each `Shell_SecondaryTrayWnd`) is written to `%LOCALAPPDATA%\Glassdock\taskbar-restore.json`. It is deleted only after a successful restore.
2. **Normal exit** (Quit, Reload, session end, installer upgrade): restore, `ABM_REMOVE`, delete the journal.
3. **Panic or unhandled exception**: a panic hook and `SetUnhandledExceptionFilter` both run the restore. Callbacks from Windows (window proc, WinEvent, COM) are wrapped in `catch_unwind`.
4. **Hard kill** (Task Manager, abort): the watchdog sees the process handle signal and restores from the journal. Windows itself drops the AppBar reservation when the window dies.
5. **Power loss**: on next start, a leftover journal is restored before anything else.
6. **Manual**: "Restore taskbar" in the tray menu and in the dock's right-click menu (the tray is unreachable when the taskbar is fully hidden), plus `glassdock.exe --restore`, which the installer also exposes as a Start menu shortcut and runs on uninstall.
7. **Explorer restarts**: the `TaskbarCreated` message re-applies the taskbar mode and re-registers the AppBar. The app never kills or restarts explorer.exe.

Registry writes are limited to autostart (`HKCU\...\Run`, through tauri-plugin-autostart) and the light/dark toggle. The only registry read outside those is the taskbar pin list (`Taskband\Favorites`, read-only, approved in section 11). Settings live in a JSON file, not the registry.

**Safe mode.** `--safe` (or `GLASSDOCK_SAFE=1`) skips AppBar registration and all taskbar changes. I use it for my own smoke runs so your desktop doesn't move while you work; you run the full mode when you test.

## 2. Dock window and AppBar

| Concern | API |
|---|---|
| Window proc hook | `SetWindowSubclass` on the dock HWND |
| Reserve screen space | `SHAppBarMessage`: `ABM_NEW`, `ABM_QUERYPOS`, `ABM_SETPOS`, `ABM_REMOVE`; forward `WM_ACTIVATE` → `ABM_ACTIVATE` and `WM_WINDOWPOSCHANGED` → `ABM_WINDOWPOSCHANGED` |
| Notifications | `ABN_POSCHANGED` (re-query and move), `ABN_FULLSCREENAPP` (hide/show), `ABN_STATECHANGE` |
| Fullscreen apps and games | `ABN_FULLSCREENAPP` plus `SHQueryUserNotificationState` on foreground changes; the dock hides until the fullscreen app leaves the foreground |
| DPI and display changes | `WM_DPICHANGED`, `WM_DISPLAYCHANGE`, `EnumDisplayMonitors`, `GetMonitorInfoW`, `GetDpiForMonitor`; per-monitor-v2 aware. If the chosen monitor disappears, fall back to the primary and move back when it returns |
| Taskbar auto-hide | `ABM_GETSTATE` / `ABM_SETSTATE` with `ABS_AUTOHIDE` |
| Taskbar hidden | auto-hide (so it reserves no space) plus `ShowWindow(SW_HIDE)` on `Shell_TrayWnd` and secondaries; a WinEvent hook re-hides it when Explorer shows it again |
| Click-through outside the glass | `SetWindowRgn`, updated at animation start/end, not per frame |
| Tray | Tauri `tray-icon` feature: Settings, Restore taskbar, Reload, Quit |
| One instance | tauri-plugin-single-instance |

- **Reserved band** = dock height + gap above and below. Maximized windows stop at it.
- **Placement**: bottom or top (`ABE_BOTTOM` / `ABE_TOP`). Dragging the dock background past a threshold moves the window with the cursor; releasing snaps to the nearer edge with a spring and re-registers. At the top, popovers open downward.
- **Layout**: pill (centred, sized to content, width animated by a spring) or full width.
- **Known conflict**: in auto-hide mode with the dock at the bottom, pushing the cursor into the very bottom screen edge still slides the Windows taskbar up over the dock. That is how auto-hide works; "hide completely" avoids it. In hide mode the system tray and notification area are unreachable until you restore.

## 3. Liquid glass

### 3.1 Backend: the backdrop

1. `IDesktopWallpaper`: `GetMonitorDevicePathAt`, `GetMonitorRECT`, `GetWallpaper(monitorId)`, `GetPosition`, `GetBackgroundColor`. An empty path means a solid colour. Slideshows return the current image per monitor.
2. `slice.rs` is pure, unit-tested math: given image size, monitor rect, fit mode (fill, fit, stretch, tile, center, span) and a target rect, it returns source and destination rects, including tiling and the background-colour bars.
3. `decode.rs` uses WIC (`IWICImagingFactory`, format converter, scaler, clipper), so every format Windows accepts as a wallpaper decodes, with EXIF orientation applied.
4. `backdrop.rs` composes the wallpaper as it appears on that monitor and crops the band behind the dock window at physical pixels. The frontend gets an RGBA buffer that maps 1:1 to its canvas, so any shape in the window samples the right pixels with no per-popover cropping.
5. `luma.rs` produces a coarse luminance grid of the band for adaptive text colour.
6. Change detection: `WM_SETTINGCHANGE` (`SPI_SETDESKWALLPAPER`) in the subclass proc, backed by a `notify` watcher on `%APPDATA%\Microsoft\Windows\Themes\`. Both are debounced and compared against the last (path, mtime, fit, colour).
7. If the wallpaper can't be read, try `TranscodedWallpaper`; if that fails too, switch to the fallback: DWM acrylic through window-vibrancy with a rounded window region and a CSS approximation.

### 3.2 Frontend: the shader

One fragment shader draws every glass surface. Shapes are rounded-rect SDFs combined with a smooth minimum, so a popover grows out of the dock as a droplet and merges or separates naturally.

- **Refraction**: a lens height profile over distance-to-edge; displacement follows the SDF gradient and grows toward the rim, leaving the centre nearly undistorted.
- **Chromatic aberration**: R, G and B sampled with slightly different displacement at the rim.
- **Frost**: mix between the sharp backdrop and a blurred copy (mipmaps plus a cached two-pass blur), low by default.
- **Light**: specular highlight on the top rim, inner rim light, fresnel-like edge glow, soft outer shadow.
- **Legibility**: small saturation and brightness lift, plus the theme tint.

Shape rects come from DOM layout and are animated by Motion springs. The renderer draws only when a spring value, the layout, the theme or the backdrop changes; there is no animation loop at idle. Adaptive contrast compares the luminance under each surface, with hysteresis, and sets light or dark content per surface. Reduced motion (`prefers-reduced-motion`, which follows the Windows setting) swaps springs for short fades.

**Glass lab** (dev builds only): sliders for refraction strength, edge width, blur, chromatic aberration and highlight intensity, plus rim, fresnel, saturation, brightness and tint, with a "copy as theme JSON" button.

## 4. Themes

Each theme is a set of CSS tokens plus glass parameters, and styles the dock, popovers, menus and toasts.

| Theme | Character |
|---|---|
| Glass (default) | clear liquid glass, low frost |
| iOS Glass | lighter, more frost, brighter highlights |
| White | opaque light |
| Black | opaque dark |
| Midnight | deep blue-black tinted glass |
| Graphite | neutral grey smoked glass |

Light/dark follows Windows through `prefers-color-scheme`, with an override in Settings. The accent colour is user-chosen or read from `UISettings.GetColorValue`.

## 5. Widgets

| Widget | Data source and APIs | Updates |
|---|---|---|
| **Apps** | Pins: `.lnk` files in `%APPDATA%\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar` (confirmed present on this PC), resolved with `IShellLinkW` and `PKEY_AppUserModel_ID`; Store-app pins and the pin order come from the `Taskband\Favorites` value, whose entries are shell item ID lists resolved with `SHCreateItemFromIDList` (read-only, watched with `RegNotifyChangeKeyValue`). Windows: `EnumWindows` with the Alt-Tab filter (visible, not cloaked via `DWMWA_CLOAKED`, no tool windows, root owner). Grouping: window AUMID (`SHGetPropertyStoreForWindow`) → process AUMID → exe path. Icons: `IShellItemImageFactory::GetImage` on the exe, the `.lnk` or `shell:AppsFolder\<AUMID>` (covers Store apps), cached as PNG on disk. Actions: `ShellExecuteExW`, `IApplicationActivationManager`, `SetForegroundWindow`, `ShowWindow`, `WM_SYSCOMMAND` | `RegisterShellHookWindow` and `SetWinEventHook` (create, destroy, foreground, title, cloak, minimize); `notify` on the pin folder |
| **Now Playing** | `GlobalSystemMediaTransportControlsSessionManager`: media properties, playback info, timeline, thumbnail; `TryTogglePlayPauseAsync`, `TrySkipNextAsync`, `TrySkipPreviousAsync`, `TryChangePlaybackPositionAsync`, shuffle and repeat. Prefers the session whose `SourceAppUserModelId` is Spotify | `SessionsChanged`, `MediaPropertiesChanged`, `PlaybackInfoChanged`, `TimelinePropertiesChanged`; progress is extrapolated locally, no polling |
| **Battery Ring** | `GetSystemPowerStatus`; `Windows.Devices.Power.Battery.AggregateBattery` for charge rate and time to full. Hidden when there is no battery | `RegisterPowerSettingNotification` → `WM_POWERBROADCAST` |
| **Quick Toggles** | Wi-Fi/Bluetooth: `Windows.Devices.Radios`. Volume/mute: `IAudioEndpointVolume`. Devices: `IMMDeviceEnumerator`. Brightness: WMI `WmiMonitorBrightness` and `WmiMonitorBrightnessMethods`. Light/dark: the two `Personalize` registry values plus a `WM_SETTINGCHANGE` broadcast. Quiet hours: internal | `Radio.StateChanged`, `IAudioEndpointVolumeCallback`, `IMMNotificationClient`, `WmiMonitorBrightnessEvent` |
| **Weather** | Open-Meteo `v1/forecast` (`current`, `hourly`, `daily`, `timezone=auto`, unit parameters) and its geocoding search; all requests from Rust | every 30 min and on wake |
| **GitHub Activity** | GraphQL `viewer.contributionsCollection.contributionCalendar`; GraphQL search for open PRs and review requests; REST `/users/{login}/events`. Token in Credential Manager | every 15 min, manual refresh, disk cache |
| **Claude Usage** | See 5.1 | file watcher |
| **Clock** | Local time and locale from the webview; month-grid calendar popover | one timer aligned to the minute |

Notes per widget:

- **Apps**: one dot per window, highlight on the active window, click to launch/focus/minimize (cycling when an app has several windows), middle click for a new instance, right click for a glass context menu (close window, close all, new window), spring magnification on hover. Windows has no supported API for pinning or unpinning, so pin management stays in Windows. Stretch: live thumbnails with `DwmRegisterThumbnail`.
- **Output device switching** has no public API. It needs the undocumented `IPolicyConfig::SetDefaultEndpoint` COM interface, which is what every audio switcher uses. I'll implement it behind that one call and label it in the code and in PROGRESS.md.
- **Brightness** for external monitors over DDC/CI (`SetVCPFeature`) is a stretch goal.
- **Focus Assist and Night Light** are not part of Quick Toggles (no public API; decided in section 11).
- **Bluetooth device batteries** are a stretch goal (GATT battery service for LE devices, the Bluetooth battery device property for classic ones).
- **GitHub**: streak and "today" are computed in Rust and unit-tested. Rate limits are read from response headers and `rateLimit`; offline keeps the cached data with a stale marker; a 401 shows an "invalid token" state linking to Settings. Whether a fine-grained token returns private contribution counts depends on the account's profile setting; I'll document the exact token permissions after testing with yours.
- **Weather**: Celsius and metric by default, Fahrenheit optional. Animated icons are original SVGs mapped from WMO weather codes. IP auto-detect is off by default and only runs when you click it, because it sends your IP to a third-party geolocation service.

### 5.1 Claude Usage: data source options

| Option | What it gives | Catch |
|---|---|---|
| **A. Status line bridge** (official, documented) | Claude Code passes `rate_limits.five_hour` and `rate_limits.seven_day` (`used_percentage`, `resets_at`) to a status line command. Glassdock registers itself (`glassdock.exe --statusline`) as that command, writes the values to a file and watches it. These are the real numbers `/usage` shows | Adds a `statusLine` entry to `~/.claude/settings.json` (none is configured today). Only refreshes while a Claude Code session is getting responses, so usage from claude.ai chat shows up on the next Claude Code response |
| **B. Local session logs** (the ccusage approach) | Parses `~/.claude/projects/**/*.jsonl`: per-request `message.usage` (input, output, cache write 5m/1h, cache read), model and timestamp, deduplicated by message and request id. Gives per-day and per-model breakdown, estimated cost, 5-hour blocks and burn rate. I confirmed the format on your 2.1.325 logs | Cannot know the real limit, so a percentage would only be an estimate |
| **C. OAuth usage endpoint** | Exact numbers across all surfaces | Undocumented, and needs the OAuth token from `~/.claude/.credentials.json`. Not used unless you explicitly approve it |
| D. Admin/Analytics API | Organisation API usage | Does not cover personal subscriptions |

**Recommended: A + B.** A drives the ring, the reset countdown, the weekly bar and the 80%/95% toasts (once per window, keyed by `resets_at`). B drives the per-day and per-model breakdown, the cost estimate from a bundled, editable price table, and the burn-rate projection, and stands in as an estimate when A's data is stale.

## 6. Settings, storage, logging

- Settings: `%APPDATA%\Glassdock\settings.json`, versioned and written atomically. The Rust store is the source of truth and broadcasts changes to every window, so changes apply live.
- Secrets: `keyring` 4 (Windows Credential Manager).
- Logs: `%LOCALAPPDATA%\Glassdock\logs\`, rotated, through tauri-plugin-log. Caches (icons, last API responses): `%LOCALAPPDATA%\Glassdock\cache\`.
- Settings window sections: General, Appearance, Widgets, Notifications, About, as in the brief.
- Installer: NSIS through the Tauri bundler, per-user install (no admin), with autostart handled by tauri-plugin-autostart.

## 7. Dependencies (versions checked against the registries on 2026-11-16)

**Rust**: `tauri` 2.12 (features `tray-icon`, `image-png`), `tauri-build` 2.7, `windows` 0.62 (the version Tauri, tao and wry use, so there is one copy), `window-vibrancy` 0.8, `keyring` 4.2, `tauri-plugin-autostart` 2.7, `tauri-plugin-single-instance` 2.5, `tauri-plugin-log` 2.10, `tauri-plugin-opener` 2.7, `reqwest` 0.13 (rustls), `notify` 8, `serde`/`serde_json`, `thiserror` 2, `chrono`, `png`, `parking_lot`.

**npm**: `@tauri-apps/cli` and `@tauri-apps/api` 2.12, `react` 19.3, `vite` 8.3, `typescript` 7.0, `tailwindcss` 4.3 with `@tailwindcss/vite`, `motion` 14, `zustand` 5, `lucide-react` for UI glyphs, `vitest` 5.

Not used, with reasons: `ogl` (raw WebGL2 is enough for one quad), `@dnd-kit` (Motion's Reorder covers widget reordering), `image` (WIC decodes, `png` encodes), `tauri-plugin-store` (own settings file is simpler to version and test).

## 8. Quality

- **Performance**: no timers at idle beyond the minute tick and the backend refresh timers; no render loop; one webview. I'll measure CPU and the private working set of the whole process tree (including the WebView2 processes) on a release build at the end of each phase and report the real numbers. The 150 MB target is tight for WebView2 and is the figure most at risk.
- **Errors**: every fallible call returns `Result`; no `unwrap` outside tests; a React error boundary per widget so one failing widget can't blank the dock.
- **Tests**: Rust unit tests for wallpaper-slice math, Claude usage (dedupe, blocks, burn rate, cost), contribution parsing and streaks, settings migration, quiet-hours schedule, the restore journal and AppBar rect math. Vitest for shape packing, contrast decisions and weather-code mapping. `cargo clippy -D warnings` and `tsc --noEmit` stay clean.
- **No console windows**: `windows_subsystem = "windows"`; nothing shells out; the watchdog is windowless.

## 9. Phases

Each phase ends with: it builds, unit tests pass, a safe-mode smoke run, a commit, a PROGRESS.md entry, a list of what to test, and a stop until you confirm.

**Phase 1: dock shell**
- [ ] Scaffold Tauri 2 + React + TS + Vite + Tailwind, `git init`, `.gitignore` (including `reference/`), PLAN.md, PROGRESS.md
- [ ] Spike: transparent WebView2 + `SetWindowRgn` click-through; choose region or resize strategy and record it
- [ ] Dock window styles, subclass proc, AppBar registration, top/bottom, pill/full width, monitor choice
- [ ] Taskbar modes with journal, watchdog, `--restore`, panic/exception hooks, `TaskbarCreated` handling
- [ ] Fullscreen hiding, DPI/display/monitor changes
- [ ] Tray menu, dock right-click menu, single instance, logging, settings store and a skeleton Settings window
- You test: maximized windows stop at the dock; top/bottom; both taskbar modes; Quit restores; End Task in Task Manager restores; tray "Restore taskbar"; fullscreen video or a game hides the dock; 100/125/150% scaling

**Phase 2: liquid glass**
- [ ] Wallpaper reader, slice math + tests, WIC decode, backdrop band, luminance grid, change detection
- [ ] WebGL2 renderer and shader, spring-driven shapes, droplet popover (with a placeholder popover)
- [ ] Six themes, light/dark, adaptive contrast, reduced motion, acrylic fallback, glass lab
- You test: each fit mode; change wallpaper, slideshow, solid colour; bright and dark wallpapers; each theme; idle CPU

**Phase 3: Apps widget**
- [ ] Pins, window tracking, grouping, icon cache, actions, context menu, hover magnification, drag-to-reorder widgets
- [ ] Stretch: DWM thumbnails
- You test: pin/unpin in Windows; Store apps; multi-window apps; click, middle click, right click; minimized and cloaked windows

**Phase 4: Now Playing, Battery Ring, Quick Toggles**
- You test: Spotify and a browser playing together; seek, shuffle, repeat; plug and unplug power; Wi-Fi, Bluetooth, volume, device switch, brightness, light/dark

**Phase 5: Weather, GitHub Activity, Claude Usage**
- You test: city search and units; a valid, an invalid and no GitHub token; offline; Claude ring against `/usage`; 80%/95% toasts

**Phase 6: polish and release**
- [ ] Settings polish, notification rules and quiet-hours schedule, autostart, NSIS installer with restore-on-uninstall, performance pass
- You test: install, autostart at login, uninstall leaves the taskbar as it was

## 10. Risks to settle early

| Risk | Where it is settled |
|---|---|
| `SetWindowRgn` with a transparent WebView2 window | Phase 1 spike; fallback is resize-on-popover |
| "Hide completely" behaviour on Insider build 26300 | Phase 1, on your machine |
| WebView2 memory against the 150 MB target | measured from Phase 1 on |
| Wallpaper path for Windows Spotlight and slideshow change events | Phase 2; fallback is `TranscodedWallpaper` |
| `Windows.Devices.Radios` from an unpackaged app (needs a native-architecture build and `RequestAccessAsync`) | Phase 4; if denied, the toggle opens the Windows settings page instead |
| `keyring` 4 API (restructured since v3) | checked against the crate source when first used |

## 11. Decisions made (2026-11-16)

1. **Claude Usage source**: A + B (5.1). Glassdock adds a `statusLine` entry to `~/.claude/settings.json` in Phase 5, with a Settings toggle that removes it again. Option C is not used.
2. **Store-app pins and pin order**: Glassdock may read the binary `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Taskband` `Favorites` value, read-only. It never writes there. If the undocumented format fails to parse, it falls back to the `.lnk` folder ordered by name.
3. **Focus Assist and Night Light**: left out entirely. No toggles and no tiles.

## Verification

- `cargo test` and `npm test` for the unit-tested logic; `cargo clippy -D warnings`; `tsc --noEmit`.
- `npm run tauri dev` in safe mode for my smoke run each phase (window created, no errors in the log).
- Full-mode behaviour is verified by you on real hardware using the "You test" list of each phase.
- Restore safety is verified in Phase 1 by killing the process and checking that the taskbar state matches the journal's original values.
