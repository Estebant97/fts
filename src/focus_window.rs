use accessibility::{AXAttribute, AXUIElement};
use core_foundation::{base::TCFType, boolean::CFBoolean, string::CFString};

pub fn focus_window(window_element: &AXUIElement) {
    // First, check if this is actually a window or an application element
    let role_attr = AXAttribute::new(&CFString::new("AXRole"));
    let is_app_element = if let Ok(role_cftype) = window_element.attribute(&role_attr) {
        let role_cfstring: CFString =
            unsafe { CFString::wrap_under_get_rule(role_cftype.as_CFTypeRef() as *mut _) };
        role_cfstring.to_string() == "AXApplication"
    } else {
        false
    };

    if is_app_element {
        // This is an application element (fallback), not a window
        // Try to bring the application to the front

        let frontmost_attr = AXAttribute::new(&CFString::new("AXFrontmost"));
        let _ = window_element
            .set_attribute(&frontmost_attr, CFBoolean::true_value().as_CFType())
            .map_err(|_e| {});

        // Optionally, try to get the focused window and raise it
        let focused_attr = AXAttribute::new(&CFString::new("AXFocusedWindow"));
        if let Ok(focused_cftype) = window_element.attribute(&focused_attr) {
            let focused_window: AXUIElement = unsafe {
                AXUIElement::wrap_under_get_rule(focused_cftype.as_CFTypeRef() as *mut _)
            };

            let raise = CFString::new("AXRaise");
            let _ = focused_window.perform_action(&raise).map_err(|_e| {});
        }
    } else {
        // This is a proper window element

        // 1. First, get the parent application element
        if let Ok(app_element) = window_element.attribute(&AXAttribute::parent()) {
            // 2. Set the application as frontmost
            let frontmost_attr = AXAttribute::new(&CFString::new("AXFrontmost"));
            let _ = app_element
                .set_attribute(&frontmost_attr, CFBoolean::true_value().as_CFType())
                .map_err(|_e| {});
        }

        // 3. Raise the specific window
        let raise = CFString::new("AXRaise");
        let _ = window_element.perform_action(&raise).map_err(|_e| {});

        let main_attr = AXAttribute::new(&CFString::new("AXMain"));
        let _ = window_element
            .set_attribute(&main_attr, CFBoolean::true_value().as_CFType())
            .map_err(|_e| {});

        let focused_attr = AXAttribute::new(&CFString::new("AXFocused"));
        let _ = window_element
            .set_attribute(&focused_attr, CFBoolean::true_value().as_CFType())
            .map_err(|_e| {});
    }
}
