mod active_windows;

use accessibility::AXUIElement;
use accessibility::AXUIElementAttributes;
use active_windows::Window;
use active_windows::get_active_windows;
use core_foundation::string::CFString;

fn main() {
    let windows: Vec<Window> = get_active_windows();
    println!("Open Windows:\n-----------------");
    for window in windows.iter() {
        println!("{:?}", window);
        let app = AXUIElement::application(window.pid);
        let window_ax = app.attribute_names();
        println!("{:?}", window_ax);
        let ax_window_name = CFString::new("AXTitle");
        let raise = CFString::new("AXRaise");
        // println!("{:?}", ax_window_name);
        if let Err(e) = app.perform_action(&raise) {
            eprintln!("Failed to raise window: {:?}", e);
        }
        // let ax_window = app.perform_action(&window.window_name).unwrap();
    }
}
