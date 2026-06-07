pub mod window {
    use accessibility::AXUIElement;
    use core_foundation::base::{CFEqual, TCFType};

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct WindowId {
        pub pid: i32,
        pub ax_ref: usize,
    }

    #[derive(Clone, Debug)]
    pub struct Window {
        pub id: WindowId,
        pub(crate) pid: i32,
        pub app_name: String,
        pub window_name: String,
        pub onscreen: u32,
        pub element: AXUIElement,
    }

    impl Window {
        pub fn new(
            pid: i32,
            app_name: String,
            window_name: String,
            onscreen: u32,
            element: AXUIElement,
        ) -> Self {
            Self {
                id: WindowId {
                    pid,
                    ax_ref: element.as_CFTypeRef() as usize,
                },
                pid,
                app_name,
                window_name,
                onscreen,
                element,
            }
        }

        pub fn same_element(&self, element: &AXUIElement) -> bool {
            self.id.ax_ref == element.as_CFTypeRef() as usize
                || unsafe { CFEqual(self.element.as_CFTypeRef(), element.as_CFTypeRef()) != 0 }
        }
    }
}
