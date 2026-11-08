# Fast True Switcher (fts)

A macOS most-recently-used (MRU) window switcher.

Hold **Control**, tap **Space** to cycle through windows, **Shift+Space** to move backward, **Escape** to cancel, and release **Control** to focus the selected window.

Hold **Option** and tap **1**–**9** to switch to that Mission Control Space on the focused display (similar to i3 / Hyprland / OmniWM workspace chords).

## Requirements

- macOS
- Rust 1.85+ (edition 2024)
- Privacy grants:
  - **Accessibility** — raise and focus windows, and (with `rdev` grab) intercept the hotkey
  - **Input Monitoring** — required on newer macOS builds for global key grab
  - **Screen Recording** — other apps’ window titles are blank in `CGWindowList` without it

Grant these to **Fast True Switcher** when using the installed app, or to the
Terminal / IDE when running with Cargo.

## Install as a Dock app

```bash
python3 scripts/macos.py install
```

This builds a locally signed **Fast True Switcher.app**, installs it in
`~/Applications`, and registers a per-user launch agent. The app starts right
away and at login, runs without a terminal, and restarts after a crash. It runs
in your logged-in desktop session so it can intercept hotkeys and focus windows.

Click its Dock icon to open the status and permission guidance window. Closing
that window keeps the switcher running. **⌘Q** or **Quit** from the Dock stops
it until you open the app again or log in. To retain the icon when stopped,
right-click it in the Dock and choose **Options → Keep in Dock**.

After granting permissions, quit and reopen the app. The installed app needs its
own grants even if Terminal was already allowed. Local ad-hoc signing is for
development; macOS may request permissions again after rebuilding the app.

```bash
python3 scripts/macos.py status         # Service details
python3 scripts/macos.py stop           # Stop until the next start or login
python3 scripts/macos.py start          # Start again
python3 scripts/macos.py disable-login # Stop and remove launch at login; keep the app
```

Logs are in `~/Library/Logs/Fast True Switcher/fts.log` and `fts-error.log`.
To build the bundle without installing or enabling launch at login:

```bash
python3 scripts/macos.py build
open "target/release/Fast True Switcher.app"
```

## Build and run

```bash
cargo run --release
```

Cargo runs also show a Dock icon. Only one copy of the switcher can run at a time;
quit an existing copy before switching between Cargo and the installed app.

Optional:

```bash
FTS_DISABLE_OVERLAY=1 cargo run --release
```

## Behavior

- The switcher arms on the first **Ctrl+Space**, not on Control alone
- Space (and Escape while switching) is consumed so it does not type into the front app
- Left and right Control are tracked independently
- **Option+1…9** switches Mission Control Spaces; the digit is consumed while Option is held
- Left and right Option both work; keypad digits map the same as the top row
- If a Ctrl+Space session is open, Option+N cancels it before changing Space
- Requesting a Space that does not exist is a no-op (logged)
- Space switches use Mission Control’s desktop buttons so Dock updates windows, focus, and the menu bar together; the overview briefly appears during a switch
- A switch is reported successful only after the destination display confirms its current Space
- Space discovery uses private SkyLight / CGS APIs and switching uses Mission Control Accessibility elements (Dock on older macOS, WindowManager on macOS 27+); these may change on macOS updates
- The window list refreshes about once per second, when a session starts, and after a Space change
- After a successful focus, MRU updates pause briefly so the previous app cannot steal front again
- The overlay lists nearby MRU entries on the display under the cursor

## Develop

Pure logic (`mru_list`, `window_filter`, `window_enum`, `window_identity`,
`browser_profile`, `focus_resolve`, `gesture`, `space_index`, `space_switch`,
`mission_control`) is platform-free:

```bash
cargo test
```

macOS-only modules compile only on `target_os = "macos"`.

Ghostty windows (including Float on Top) are included via floating-layer CG
rows and NSWorkspace pid discovery. Chrome work/personal profiles stay distinct
by CG window id and show as `Google Chrome · Work — …` in the overlay.

Only regular apps are switchable. Denied owners (AutoFill, loginwindow, Wi-Fi,
…) are filtered at collect and again at merge. When Accessibility already lists
windows for an app, unmatched CG rows are dropped — Ghostty tabs and helper
chrome share CG titles but are not separate OS windows. CG fallbacks are only
emitted when AX is empty (so Ghostty still appears without Screen Recording).
Known helper titles (share pickers, healthcheck) are dropped either way.
Activate-only focus still fails closed so MRU is not promoted.

To exercise real desktop transitions on an unlocked Mac with Accessibility
permission and at least two user desktops on the focused display:

```bash
cargo test live_desktop_round_trip -- --ignored --nocapture
```

This opt-in test visits each other desktop reachable with Option+1–9 and returns
three times, checks repeated requests and source-window visibility, and attempts
to restore the initial desktop even on failure. Also check visually
that each desktop shows only its own windows and the menu bar belongs to its
focused app. Ordinary unit tests do not manipulate the desktop.

For optional screenshots of each destination, grant Screen Recording access and
run `FTS_TEST_SCREENSHOTS=1 cargo test live_desktop_round_trip -- --ignored --nocapture`.
Images are written to `/tmp/fts-space-N.png`.

To check the production switch path on the main thread, pass destination desktop
numbers and end with the starting desktop (repeat a number to check a no-op):

```bash
cargo run --example verify_workspace_transition -- 2 2 3 1
```

The example attempts to return to the last desktop number if a request fails.
Run it on an unlocked Mac with Accessibility access. Screen Recording is needed
only for optional captures: set `FTS_TEST_SCREENSHOTS=1` to save them in
`/tmp/fts-space-N.png`. Check the windows and menu bar visually as well as the
reported result; destination confirmation alone does not verify rendering.
