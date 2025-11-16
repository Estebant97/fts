pub mod window {
    use accessibility::AXUIElement;

    #[derive(Debug)]
    pub struct Window {
        pub(crate) pid: i32,
        pub app_name: String,
        pub window_name: String,
        pub onscreen: u32,
        pub element: AXUIElement,
    }
}
