mod active_windows;
mod focus_window;
mod window;
mod window_tracker;

use crate::window::window::Window;
use focus_window::focus_window;
use rdev::{Event, EventType, Key, listen};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use window_tracker::WindowTracker;

enum SwitcherEvent {
    AltPressed,
    TabPressed,
    AltReleased,
}

fn main() {
    let mut tracker = WindowTracker::new();

    if tracker.len() == 0 {
        println!("No windows found!");
        return;
    }

    println!("Window Switcher - Press ALT+TAB to cycle through last-used windows");
    println!("Tracking {} windows:\n", tracker.len());

    for (i, window) in tracker.windows().enumerate() {
        println!(
            "[{}] {} - {} (onscreen: {})",
            i, window.app_name, window.window_name, window.onscreen
        );
    }
    println!("\nListening for ALT+TAB... Press Ctrl+C to exit.\n");

    // Channel for communication from keyboard listener to main thread
    let (tx, rx) = channel::<SwitcherEvent>();
    let alt_pressed = Arc::new(Mutex::new(false));
    let alt_pressed_clone = Arc::clone(&alt_pressed);

    // Spawn keyboard listener thread
    thread::spawn(move || {
        if let Err(error) = listen(move |event: Event| match event.event_type {
            EventType::KeyPress(key) => match key {
                Key::Alt | Key::AltGr => {
                    let mut pressed = alt_pressed_clone.lock().unwrap();
                    if !*pressed {
                        *pressed = true;
                        let _ = tx.send(SwitcherEvent::AltPressed);
                    }
                }
                Key::Tab => {
                    if *alt_pressed_clone.lock().unwrap() {
                        let _ = tx.send(SwitcherEvent::TabPressed);
                    }
                }
                _ => {}
            },
            EventType::KeyRelease(key) => match key {
                Key::Alt | Key::AltGr => {
                    let mut pressed = alt_pressed_clone.lock().unwrap();
                    if *pressed {
                        *pressed = false;
                        let _ = tx.send(SwitcherEvent::AltReleased);
                    }
                }
                _ => {}
            },
            _ => {}
        }) {
            eprintln!("Error listening to keyboard events: {:?}", error);
        }
    });

    let mut alt_is_pressed = false;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => match event {
                SwitcherEvent::AltPressed => {
                    alt_is_pressed = true;
                    tracker.begin_switching();
                    println!("ALT pressed - ready to switch windows");
                }
                SwitcherEvent::TabPressed => {
                    if alt_is_pressed {
                        if let Some(window) = tracker.advance_selection() {
                            println!(
                                "[→] Selecting: {} - {}",
                                window.app_name, window.window_name
                            );
                        }
                    }
                }
                SwitcherEvent::AltReleased => {
                    if alt_is_pressed {
                        alt_is_pressed = false;
                        if let Some(window) = tracker.selected_window() {
                            println!(
                                "\n✓ Focusing: {} - {}\n",
                                window.app_name, window.window_name
                            );
                            focus_window(&window.element);
                        }
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
