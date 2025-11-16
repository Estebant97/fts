use accessibility::{AXAttribute, AXUIElement};
use core_foundation::{base::TCFType, boolean::CFBoolean, string::CFString};

pub fn focus_window(window_element: &AXUIElement) {
    // 1. First, get the parent application element
    if let Ok(app_element) = window_element.attribute(&AXAttribute::parent()) {
        // 2. Set the application as frontmost
        let frontmost_attr = AXAttribute::new(&CFString::new("AXFrontmost"));
        let _ = app_element
            .set_attribute(&frontmost_attr, CFBoolean::true_value().as_CFType())
            .map_err(|e| eprintln!("Failed to set AXFrontmost: {:?}", e));
    }

    // 3. Raise the specific window
    let raise = CFString::new("AXRaise");
    let _ = window_element
        .perform_action(&raise)
        .map_err(|e| eprintln!("AXRaise failed: {:?}", e));
}
