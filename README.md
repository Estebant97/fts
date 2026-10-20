# Fast True Switcher (fts)

A macOS most-recently-used (MRU) window switcher.

Hold **Control**, tap **Space** to cycle through windows, **Shift+Space** to move backward, **Escape** to cancel, and release **Control** to focus the selected window.

## Requirements

- macOS
- Rust 1.85+ (edition 2024)
- Privacy grants:
  - **Accessibility** — raise and focus windows, and (with `rdev` grab) intercept the hotkey
  - **Input Monitoring** — required on newer macOS builds for global key grab
  - **Screen Recording** — other apps’ window titles are blank in `CGWindowList` without it

Grant these to the `fts` binary, or to the Terminal / IDE that launches it.

## Build and run

```bash
cargo run --release
```

Optional:

```bash
FTS_DISABLE_OVERLAY=1 cargo run --release
```

## Behavior

- The switcher arms on the first **Ctrl+Space**, not on Control alone
- Space (and Escape while switching) is consumed so it does not type into the front app
- Left and right Control are tracked independently
- The window list refreshes about once per second and again when a session starts
- After a successful focus, MRU updates pause briefly so the previous app cannot steal front again
- The overlay lists nearby MRU entries on the display under the cursor

## Develop

Pure logic (`mru_list`, `window_filter`, `window_enum`, `window_identity`,
`browser_profile`, `focus_resolve`) is platform-free:

```bash
cargo test
```

macOS-only modules compile only on `target_os = "macos"`.

Ghostty windows (including Float on Top) are included via floating-layer CG
rows and NSWorkspace pid discovery. Chrome work/personal profiles stay distinct
by CG window id and show as `Google Chrome · Work — …` in the overlay.

Only regular apps are switchable. Denied owners (AutoFill, loginwindow, Wi-Fi,
…) are filtered at collect and again at merge. Unmatched CG rows beside a
partial AX list are kept when their titles are distinct (Ghostty terminals);
generic `App — App` copies and known helper titles (share pickers, healthcheck)
are dropped. Activate-only focus still fails closed so MRU is not promoted.
