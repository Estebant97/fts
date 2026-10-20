pub mod window {
    use accessibility::AXUIElement;
    use core_foundation::base::{CFEqual, TCFType};

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct WindowId {
        pub pid: i32,
        pub cg_window_id: Option<u32>,
        pub ax_ref: usize,
    }

    #[derive(Clone, Debug)]
    pub struct Window {
        pub id: WindowId,
        pub pid: i32,
        pub cg_window_id: Option<u32>,
        pub app_name: String,
        pub window_name: String,
        pub onscreen: bool,
        /// True when we only have the application AX element (no real window).
        pub is_app_fallback: bool,
        pub element: AXUIElement,
    }

    impl Window {
        pub fn new(
            pid: i32,
            cg_window_id: Option<u32>,
            app_name: String,
            window_name: String,
            onscreen: bool,
            is_app_fallback: bool,
            element: AXUIElement,
        ) -> Self {
            Self {
                id: WindowId {
                    pid,
                    cg_window_id,
                    ax_ref: element.as_CFTypeRef() as usize,
                },
                pid,
                cg_window_id,
                app_name,
                window_name,
                onscreen,
                is_app_fallback,
                element,
            }
        }

        pub fn same_element(&self, element: &AXUIElement) -> bool {
            self.id.ax_ref == element.as_CFTypeRef() as usize
                || unsafe { CFEqual(self.element.as_CFTypeRef(), element.as_CFTypeRef()) != 0 }
        }

        pub fn same_window(&self, other: &Window) -> bool {
            if self.pid != other.pid {
                return false;
            }

            if let (Some(left), Some(right)) = (self.cg_window_id, other.cg_window_id) {
                return left == right;
            }

            if !self.is_app_fallback && !other.is_app_fallback && self.same_element(&other.element)
            {
                return true;
            }

            !self.window_name.is_empty()
                && self.window_name == other.window_name
                && self.is_app_fallback == other.is_app_fallback
        }
    }
}
