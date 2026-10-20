mod mru_list;
mod window_filter;

#[cfg(target_os = "macos")]
mod active_windows;
#[cfg(target_os = "macos")]
mod focus_window;
#[cfg(target_os = "macos")]
mod overlay_window;
#[cfg(target_os = "macos")]
mod permissions;
#[cfg(target_os = "macos")]
mod window;
#[cfg(target_os = "macos")]
mod window_tracker;

#[cfg(target_os = "macos")]
pub(crate) use window::window::Window;
#[cfg(target_os = "macos")]
use focus_window::focus_window;
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
use window_tracker::WindowTracker;

#[cfg(target_os = "macos")]
enum SwitcherEvent {
    SpacePressed { reverse: bool },
    Cancel,
    CtrlReleased,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct ModifierState {
    ctrl_left: bool,
    ctrl_right: bool,
    shift_left: bool,
    shift_right: bool,
}

#[cfg(target_os = "macos")]
impl ModifierState {
    fn ctrl_down(&self) -> bool {
        self.ctrl_left || self.ctrl_right
    }

    fn shift_down(&self) -> bool {
        self.shift_left || self.shift_right
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
    println!("  Tracking {} windows.\n", tracker.len());

    for (i, window) in tracker.windows().enumerate() {
        println!(
            "[{}] {} - {} (onscreen: {}, cg: {:?}, fallback: {})",
            i,
            window.app_name,
            window.window_name,
            window.onscreen,
            window.cg_window_id,
            window.is_app_fallback
        );
    }
    println!();

    let (tx, rx) = channel::<SwitcherEvent>();
    let modifiers = Arc::new(Mutex::new(ModifierState::default()));
    let modifiers_listener = Arc::clone(&modifiers);
    let session_active = Arc::new(AtomicBool::new(false));
    let session_listener = Arc::clone(&session_active);

    thread::spawn(move || {
        let result = grab(move |event: Event| {
            let mut mods = modifiers_listener.lock().unwrap();
            match event.event_type {
                EventType::KeyPress(key) => match key {
                    Key::ControlLeft => {
                        mods.ctrl_left = true;
                        Some(event)
                    }
                    Key::ControlRight => {
                        mods.ctrl_right = true;
                        Some(event)
                    }
                    Key::ShiftLeft => {
                        mods.shift_left = true;
                        Some(event)
                    }
                    Key::ShiftRight => {
                        mods.shift_right = true;
                        Some(event)
                    }
                    Key::Space if mods.ctrl_down() => {
                        let reverse = mods.shift_down();
                        let _ = tx.send(SwitcherEvent::SpacePressed { reverse });
                        None
                    }
                    Key::Escape if session_listener.load(Ordering::SeqCst) => {
                        let _ = tx.send(SwitcherEvent::Cancel);
                        None
                    }
                    _ => Some(event),
                },
                EventType::KeyRelease(key) => match key {
                    Key::ControlLeft => {
                        let was_down = mods.ctrl_down();
                        mods.ctrl_left = false;
                        if was_down && !mods.ctrl_down() {
                            let _ = tx.send(SwitcherEvent::CtrlReleased);
                        }
                        Some(event)
                    }
                    Key::ControlRight => {
                        let was_down = mods.ctrl_down();
                        mods.ctrl_right = false;
                        if was_down && !mods.ctrl_down() {
                            let _ = tx.send(SwitcherEvent::CtrlReleased);
                        }
                        Some(event)
                    }
                    Key::ShiftLeft => {
                        mods.shift_left = false;
                        Some(event)
                    }
                    Key::ShiftRight => {
                        mods.shift_right = false;
                        Some(event)
                    }
                    Key::Space if mods.ctrl_down() || session_listener.load(Ordering::SeqCst) => {
                        None
                    }
                    Key::Escape if session_listener.load(Ordering::SeqCst) => None,
                    _ => Some(event),
                },
                _ => Some(event),
            }
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
                        println!(
                            "\nFocusing: {} - {}\n",
                            window.app_name, window.window_name
                        );
                        if let Err(error) = focus_window(&window) {
                            eprintln!("Focus failed: {:?}", error);
                        }
                        tracker.finish_switching();
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

#[cfg(target_os = "macos")]
fn print_switching_selection(position: usize, total_windows: usize, window: &Window) {
    println!(
        "[switch {position}/{total_windows}] {} - {}",
        window.app_name, window.window_name
    );
}
