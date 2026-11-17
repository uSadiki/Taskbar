# Glassdock

A liquid glass dock for Windows 11 that replaces the taskbar: your apps, now playing, weather, GitHub activity, Claude usage, battery, quick settings and a clock, on glass that bends your wallpaper.

## Install

Run `Glassdock_0.1.0_x64-setup.exe` (in `src-tauri\target\release\bundle\nsis\`). It installs for your user only, needs no administrator rights, and adds two Start menu entries:

- **Glassdock** starts the dock.
- **Glassdock - Restore taskbar** puts the Windows taskbar back, whatever state things are in. Use it if the taskbar is ever missing.

To remove Glassdock, use Settings > Apps > Installed apps. Uninstalling restores the taskbar, removes Glassdock from Claude Code's settings and from start-at-login.

## Using the dock

| To | Do |
|---|---|
| Open an app, or switch to it | Click its icon. Click the app in front to minimize it; click an app with several windows to step through them. |
| Open another window of an app | Middle-click its icon. |
| See an app's windows, close them | Right-click its icon. |
| See details for a widget | Click it. Click again, press Esc or click anywhere else to close. |
| Open Settings, restore the taskbar, reload or quit | Right-click an empty part of the dock (or use the tray icon, when the taskbar is visible). |
| Move the dock to the top or bottom | Drag it to the other edge. |
| Bring the dock back while a window covers it | Push the pointer against the screen edge, below the dock. It slides away again when the pointer leaves. |

The dock takes no screen space of its own: maximized windows fill the whole screen, and the dock slides away while a window overlaps it. To have it keep its own strip instead (maximized windows then stop above it), turn off Hide for windows in Settings > General.

The Windows taskbar is set to auto-hide while Glassdock runs (or hidden completely, if you choose that in Settings). With the dock at the bottom, Hide completely works best, because the auto-hidden taskbar also comes up at that edge. Quitting Glassdock, or Glassdock crashing, puts it back the way it was.

## Setting up the widgets

All in Settings > Widgets, where you can also turn widgets on or off and change their order.

- **Weather**: search for your city (or click Locate). Celsius by default.
- **GitHub Activity**: paste a fine-grained personal access token. Read-only access is enough. It is kept in Windows Credential Manager.
- **Claude Usage**: works on its own. Glassdock adds itself as Claude Code's status line command, which is how it learns your real 5-hour and weekly limits; they appear after Claude Code's next response. If you already have a status line, Glassdock leaves it alone and estimates usage from Claude Code's logs instead.
- **Now Playing**, **Battery Ring** and **Quick Toggles** need no setup. Now Playing appears while something is playing.

Alerts at 80% and 95% of the Claude limits appear above the dock. Quiet (in Quick Toggles) or the quiet-hours schedule (Settings > Notifications) holds them.

## Appearance

Settings > Appearance: six themes, light or dark, accent colour, dock size, and sliders for the glass (refraction, edge width, frost, colour fringing, highlight).

## Files

| What | Where |
|---|---|
| Settings | `%APPDATA%\Glassdock\settings.json` |
| Logs | `%LOCALAPPDATA%\Glassdock\logs` |
| GitHub token | Windows Credential Manager, "Glassdock" |

## Development

See [PLAN.md](PLAN.md) for the design and [PROGRESS.md](PROGRESS.md) for what was built, decided and verified.

```
npm install
npm run tauri dev          # run the dock (changes the taskbar while it runs)
npm run tauri build        # build the installer
npm test                   # frontend tests
cd src-tauri && cargo test # backend tests
```

Safe mode (`$env:GLASSDOCK_SAFE='1'` before `npm run tauri dev`) runs the dock without reserving screen space or touching the taskbar.
