mod active_windows;
mod focus_window;
mod window;

use active_windows::get_active_windows;
use focus_window::focus_window;

use crate::window::window::Window;

fn main() {
    let windows: Vec<Window> = get_active_windows();
    println!("Open Windows:\n-----------------");
    for window in windows.iter() {
        println!("{:?}", window);
        focus_window(&window.element);
    }
}
