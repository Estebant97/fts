mod active_windows;
mod focus_window;
mod window;

use crate::window::window::Window;
use active_windows::get_active_windows;
use focus_window::focus_window;
use rdev::{Event, EventType, Key, listen};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

enum SwitcherEvent {
    CmdPressed,
    TabPressed,
    CmdReleased,
}

fn main() {
    let windows: Vec<Window> = get_active_windows();

    if windows.is_empty() {
        println!("No windows found!");
        return;
    }

    println!("Window Switcher - Press CMD+TAB to cycle through windows");
    println!("Found {} windows:\n", windows.len());

    for (i, window) in windows.iter().enumerate() {
        println!("[{}] {} - {}", i, window.app_name, window.window_name);
    }
    println!("\nListening for CMD+TAB... Press Ctrl+C to exit.\n");

    // Channel for communication from keyboard listener to main thread
    let (tx, rx) = channel::<SwitcherEvent>();
    let cmd_pressed = Arc::new(Mutex::new(false));
    let cmd_pressed_clone = Arc::clone(&cmd_pressed);

    // Spawn keyboard listener thread
    thread::spawn(move || {
        if let Err(error) = listen(move |event: Event| match event.event_type {
            EventType::KeyPress(key) => match key {
                Key::MetaLeft | Key::MetaRight => {
                    let mut pressed = cmd_pressed_clone.lock().unwrap();
                    if !*pressed {
                        *pressed = true;
                        let _ = tx.send(SwitcherEvent::CmdPressed);
                    }
                }
                Key::Tab => {
                    if *cmd_pressed_clone.lock().unwrap() {
                        let _ = tx.send(SwitcherEvent::TabPressed);
                    }
                }
                _ => {}
            },
            EventType::KeyRelease(key) => match key {
                Key::MetaLeft | Key::MetaRight => {
                    let mut pressed = cmd_pressed_clone.lock().unwrap();
                    if *pressed {
                        *pressed = false;
                        let _ = tx.send(SwitcherEvent::CmdReleased);
                    }
                }
                _ => {}
            },
            _ => {}
        }) {
            eprintln!("Error listening to keyboard events: {:?}", error);
        }
    });

    // Main thread handles window switching
    let mut current_index = 0usize;
    let mut cmd_is_pressed = false;

    loop {
        // Non-blocking receive with timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => match event {
                SwitcherEvent::CmdPressed => {
                    cmd_is_pressed = true;
                    println!("CMD pressed - ready to switch windows");
                }
                SwitcherEvent::TabPressed => {
                    if cmd_is_pressed {
                        current_index = (current_index + 1) % windows.len();
                        let window = &windows[current_index];
                        println!(
                            "[→] Selecting [{}]: {} - {}",
                            current_index, window.app_name, window.window_name
                        );
                    }
                }
                SwitcherEvent::CmdReleased => {
                    if cmd_is_pressed {
                        cmd_is_pressed = false;
                        let window = &windows[current_index];
                        println!(
                            "\n✓ Focusing: {} - {}\n",
                            window.app_name, window.window_name
                        );
                        focus_window(&window.element);
                    }
                }
            },
            Err(_) => {
                // Timeout - continue loop
            }
        }
    }
}
