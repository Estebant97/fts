pub mod window {
    use accessibility::AXUIElement;
    use core_foundation::base::{CFEqual, TCFType};

    use crate::window_identity::{WindowFields, same_window_by_fields};

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
            same_window_by_fields(
                &WindowFields {
                    pid: self.pid,
                    cg_window_id: self.cg_window_id,
                    window_name: &self.window_name,
                    is_app_fallback: self.is_app_fallback,
                },
                &WindowFields {
                    pid: other.pid,
                    cg_window_id: other.cg_window_id,
                    window_name: &other.window_name,
                    is_app_fallback: other.is_app_fallback,
                },
                self.same_element(&other.element),
            )
        }
    }
}
