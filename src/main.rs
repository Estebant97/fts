mod active_windows;

use active_windows::Window;
use active_windows::get_active_windows;

fn main() {
    let windows: Vec<Window> = get_active_windows();
    println!("Open Windows:\n-----------------");
    for window in windows.iter() {
        println!("{:?}", window);
    }
}
