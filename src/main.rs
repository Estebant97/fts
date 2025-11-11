mod active_windows;

fn main() {
    let windows: Vec<active_windows::Window> = active_windows::get_active_windows();
    println!("Open Windows:\n-----------------");
    for window in windows.iter() {
        println!("{:?}", window);
    }
}
