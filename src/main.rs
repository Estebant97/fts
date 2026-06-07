mod active_windows;
mod focus_window;
mod mru_list;
mod overlay_window;
mod window;
mod window_tracker;

use crate::window::window::Window;
use focus_window::focus_window;
use overlay_window::OverlayWindow;
use rdev::{Event, EventType, Key, listen};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use window_tracker::WindowTracker;

enum SwitcherEvent {
    CtrlPressed,
    SpacePressed,
    CtrlReleased,
}

fn main() {
    let mut tracker = WindowTracker::new();
    let overlay = OverlayWindow::new();

    if tracker.len() == 0 {
        println!("No windows found!");
        return;
    }

    println!("Window Switcher - Press CTRL+SPACE to cycle through last-used windows");
    println!("Tracking {} windows:\n", tracker.len());

    for (i, window) in tracker.windows().enumerate() {
        println!(
            "[{}] {} - {} (onscreen: {})",
            i, window.app_name, window.window_name, window.onscreen
        );
    }
    println!("\nListening for CTRL+SPACE... Press Ctrl+C to exit.\n");

    // Channel for communication from keyboard listener to main thread
    let (tx, rx) = channel::<SwitcherEvent>();
    let ctrl_pressed = Arc::new(Mutex::new(false));
    let ctrl_pressed_clone = Arc::clone(&ctrl_pressed);

    // Spawn keyboard listener thread
    thread::spawn(move || {
        if let Err(error) = listen(move |event: Event| match event.event_type {
            EventType::KeyPress(key) => match key {
                Key::ControlLeft | Key::ControlRight => {
                    let mut pressed = ctrl_pressed_clone.lock().unwrap();
                    if !*pressed {
                        *pressed = true;
                        let _ = tx.send(SwitcherEvent::CtrlPressed);
                    }
                }
                Key::Space => {
                    if *ctrl_pressed_clone.lock().unwrap() {
                        let _ = tx.send(SwitcherEvent::SpacePressed);
                    }
                }
                _ => {}
            },
            EventType::KeyRelease(key) => match key {
                Key::ControlLeft | Key::ControlRight => {
                    let mut pressed = ctrl_pressed_clone.lock().unwrap();
                    if *pressed {
                        *pressed = false;
                        let _ = tx.send(SwitcherEvent::CtrlReleased);
                    }
                }
                _ => {}
            },
            _ => {}
        }) {
            eprintln!("Error listening to keyboard events: {:?}", error);
        }
    });

    let mut ctrl_is_pressed = false;

    loop {
        overlay.pump_events();

        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(event) => match event {
                SwitcherEvent::CtrlPressed => {
                    ctrl_is_pressed = true;
                    tracker.begin_switching();
                    overlay.show_waiting(tracker.len());
                    print_switching_started(tracker.len());
                }
                SwitcherEvent::SpacePressed => {
                    if ctrl_is_pressed {
                        if tracker.advance_selection().is_some() {
                            let position = tracker.selected_position().unwrap_or(0) + 1;
                            let total = tracker.len();
                            if let Some(window) = tracker.selected_window() {
                                overlay.show_selection(position, total, &window);
                                print_switching_selection(position, total, &window);
                            }
                        }
                    }
                }
                SwitcherEvent::CtrlReleased => {
                    if ctrl_is_pressed {
                        ctrl_is_pressed = false;
                        if let Some(window) = tracker.selected_window() {
                            println!(
                                "\n✓ Focusing: {} - {}\n",
                                window.app_name, window.window_name
                            );
                            focus_window(&window.element);
                        } else {
                            println!("[switch] No window selected");
                        }
                        overlay.hide();
                        tracker.finish_switching();
                    }
                }
            },
            Err(_) => {
                tracker.observe_focused_window();
            }
        }
    }
}

fn print_switching_started(total_windows: usize) {
    println!();
    println!("================ SWITCHING WINDOWS ================");
    println!("CTRL is held. Press SPACE to cycle through {total_windows} windows.");
}

fn print_switching_selection(position: usize, total_windows: usize, window: &Window) {
    println!(
        "[switch {position}/{total_windows}] {} - {}",
        window.app_name, window.window_name
    );
}
