mod browser_profile;
mod focus_resolve;
mod gesture;
mod mru_list;
mod space_index;
mod switch_commit;
mod window_enum;
mod window_filter;
mod window_identity;

#[cfg(target_os = "macos")]
mod active_windows;
#[cfg(target_os = "macos")]
mod focus_window;
#[cfg(target_os = "macos")]
mod overlay_window;
#[cfg(target_os = "macos")]
mod permissions;
#[cfg(target_os = "macos")]
mod spaces;
#[cfg(target_os = "macos")]
mod window;
#[cfg(target_os = "macos")]
mod window_tracker;

#[cfg(target_os = "macos")]
pub(crate) use window::window::Window;
#[cfg(target_os = "macos")]
use focus_window::focus_window;
#[cfg(target_os = "macos")]
use gesture::{GestureAction, GestureInput, GestureKey, GestureState};
#[cfg(target_os = "macos")]
use overlay_window::OverlayWindow;
#[cfg(target_os = "macos")]
use permissions::{ensure_accessibility_trust, print_permission_guidance};
#[cfg(target_os = "macos")]
use rdev::{Event, EventType, Key, grab};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::mpsc::channel;
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::thread;
#[cfg(target_os = "macos")]
use std::time::Duration;
#[cfg(target_os = "macos")]
use switch_commit::switch_commit_after_focus;
#[cfg(target_os = "macos")]
use window_tracker::WindowTracker;

#[cfg(target_os = "macos")]
enum SwitcherEvent {
    SpacePressed { reverse: bool },
    GoToWorkspace { index: u8 },
    Cancel,
    CtrlReleased,
}

#[cfg(target_os = "macos")]
fn map_key(key: Key) -> GestureKey {
    match key {
        Key::ControlLeft => GestureKey::ControlLeft,
        Key::ControlRight => GestureKey::ControlRight,
        Key::ShiftLeft => GestureKey::ShiftLeft,
        Key::ShiftRight => GestureKey::ShiftRight,
        Key::Alt => GestureKey::Alt,
        Key::AltGr => GestureKey::AltGr,
        Key::Space => GestureKey::Space,
        Key::Escape => GestureKey::Escape,
        Key::Num1 | Key::Kp1 => GestureKey::Digit(1),
        Key::Num2 | Key::Kp2 => GestureKey::Digit(2),
        Key::Num3 | Key::Kp3 => GestureKey::Digit(3),
        Key::Num4 | Key::Kp4 => GestureKey::Digit(4),
        Key::Num5 | Key::Kp5 => GestureKey::Digit(5),
        Key::Num6 | Key::Kp6 => GestureKey::Digit(6),
        Key::Num7 | Key::Kp7 => GestureKey::Digit(7),
        Key::Num8 | Key::Kp8 => GestureKey::Digit(8),
        Key::Num9 | Key::Kp9 => GestureKey::Digit(9),
        Key::Num0 | Key::Kp0 => GestureKey::Digit(0),
        _ => GestureKey::Other,
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("fts is a macOS window switcher. Build and run it on macOS.");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    let trusted = ensure_accessibility_trust(true);
    print_permission_guidance(trusted);
    if !trusted {
        eprintln!("Continuing, but focus and window enumeration may fail until Accessibility is granted.\n");
    }

    let mut tracker = WindowTracker::new();
    let overlay = OverlayWindow::new();

    println!("Fast True Switcher");
    println!("  Hold Control, tap Space to cycle (Shift+Space reverse, Escape cancel).");
    println!("  Release Control to focus the selected window.");
    println!("  Option+1…9 switches Mission Control Spaces on the focused display.");
    println!("  Tracking {} windows.\n", tracker.len());

    for (i, window) in tracker.windows().enumerate() {
        println!(
            "[{}] {} (onscreen: {}, cg: {:?}, fallback: {}, profile: {:?})",
            i,
            window.display_label(),
            window.onscreen,
            window.cg_window_id,
            window.is_app_fallback,
            window.profile_name
        );
    }
    println!();

    let (tx, rx) = channel::<SwitcherEvent>();
    let gesture = Arc::new(Mutex::new(GestureState::default()));
    let gesture_listener = Arc::clone(&gesture);
    let session_active = Arc::new(AtomicBool::new(false));
    let session_listener = Arc::clone(&session_active);

    thread::spawn(move || {
        let result = grab(move |event: Event| {
            let mut state = gesture_listener.lock().unwrap();
            let session = session_listener.load(Ordering::SeqCst);
            let input = match event.event_type {
                EventType::KeyPress(key) => GestureInput::KeyPress(map_key(key)),
                EventType::KeyRelease(key) => GestureInput::KeyRelease(map_key(key)),
                _ => return Some(event),
            };

            let (action, swallow) = state.handle(input, session);
            match action {
                Some(GestureAction::SpacePressed { reverse }) => {
                    let _ = tx.send(SwitcherEvent::SpacePressed { reverse });
                }
                Some(GestureAction::GoToWorkspace { index }) => {
                    let _ = tx.send(SwitcherEvent::GoToWorkspace { index });
                }
                Some(GestureAction::Cancel) => {
                    let _ = tx.send(SwitcherEvent::Cancel);
                }
                Some(GestureAction::CtrlReleased) => {
                    let _ = tx.send(SwitcherEvent::CtrlReleased);
                }
                None => {}
            }

            if swallow { None } else { Some(event) }
        });

        if let Err(error) = result {
            eprintln!("Error grabbing keyboard events: {:?}", error);
            eprintln!("Grant Accessibility / Input Monitoring and restart fts.");
        }
    });

    loop {
        overlay.pump_events();

        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(event) => match event {
                SwitcherEvent::SpacePressed { reverse } => {
                    if !tracker.is_switching() {
                        tracker.begin_switching();
                        session_active.store(true, Ordering::SeqCst);
                    }

                    let selected = if reverse {
                        tracker.reverse_selection()
                    } else {
                        tracker.advance_selection()
                    };

                    if selected.is_some() {
                        let position = tracker.selected_position().unwrap_or(0) + 1;
                        let total = tracker.len();
                        let entries = tracker.overlay_entries(3);
                        if let Some(window) = tracker.selected_window() {
                            overlay.show_selection(position, total, &entries);
                            print_switching_selection(position, total, &window);
                        }
                    } else if tracker.len() < 2 {
                        println!("[switch] Need at least two windows to cycle");
                    }
                }
                SwitcherEvent::GoToWorkspace { index } => {
                    if tracker.is_switching() {
                        println!("\n[switch] Cancelled (workspace change)\n");
                        overlay.hide();
                        tracker.cancel_switching();
                        session_active.store(false, Ordering::SeqCst);
                    }

                    match spaces::activate_space(index) {
                        Ok(()) => {
                            println!("[workspace] Switched to Space {index}");
                            tracker.refresh_windows();
                        }
                        Err(error) => {
                            eprintln!("[workspace] Could not switch to Space {index}: {error}");
                        }
                    }
                }
                SwitcherEvent::Cancel => {
                    if tracker.is_switching() {
                        println!("\n[switch] Cancelled\n");
                        overlay.hide();
                        tracker.cancel_switching();
                        session_active.store(false, Ordering::SeqCst);
                    }
                }
                SwitcherEvent::CtrlReleased => {
                    if !tracker.is_switching() {
                        continue;
                    }

                    if let Some(window) = tracker.selected_window() {
                        println!("\nFocusing: {}\n", window.display_label());
                        let focus_ok = match focus_window(&window) {
                            Ok(()) => true,
                            Err(error) => {
                                eprintln!("Focus failed: {:?}", error);
                                false
                            }
                        };
                        apply_focus_commit(&mut tracker, focus_ok);
                    } else {
                        tracker.cancel_switching();
                    }

                    overlay.hide();
                    session_active.store(false, Ordering::SeqCst);
                }
            },
            Err(_) => {
                tracker.observe_focused_window();
                tracker.maybe_refresh_windows();
            }
        }
    }
}

/// Commit or abandon the switcher session based on whether focus succeeded.
#[cfg(target_os = "macos")]
fn apply_focus_commit(tracker: &mut WindowTracker, focus_ok: bool) {
    match switch_commit_after_focus(focus_ok) {
        switch_commit::SwitchCommit::Finish => tracker.finish_switching(),
        switch_commit::SwitchCommit::Cancel => tracker.cancel_switching(),
    }
}

#[cfg(target_os = "macos")]
fn print_switching_selection(position: usize, total_windows: usize, window: &Window) {
    println!(
        "[switch {position}/{total_windows}] {}",
        window.display_label()
    );
}
