use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::display::*;

/// Extract a string value from a CFDictionary by key
fn get_cf_string(dict: &CFDictionary, key_cf: &CFString) -> String {
    let key_ref = key_cf.as_CFTypeRef();

    if let Some(value_ref) = dict.find(key_ref) {
        unsafe {
            let cf_string =
                core_foundation::string::CFString::wrap_under_get_rule(*value_ref as *const _);
            return cf_string.to_string();
        }
    }

    String::new()
}
/// Extract a number value from a CFDictionary by key
fn get_cf_u32(dict: &CFDictionary, key: &CFString) -> Option<u32> {
    let key_ref: CFTypeRef = key.as_CFTypeRef();
    dict.find(key_ref).and_then(|v| unsafe {
        let cf_num = CFNumber::wrap_under_get_rule(*v as *const _);
        cf_num.to_i32().map(|n| n as u32)
    })
}
/// Extract a pid value from a CFDictionary by key
fn get_cf_pid(dict: &CFDictionary, key: &CFString) -> Option<i32> {
    let key_ref = key.as_CFTypeRef();
    dict.find(key_ref).and_then(|v| unsafe {
        let cf_num = CFNumber::wrap_under_get_rule(*v as *const _);
        cf_num.to_i32()
    })
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Window {
    pid: i32,
    app_name: String,
    window_name: String,
}

pub fn get_active_windows() -> Vec<Window> {
    let options = kCGWindowListOptionAll;
    let mut window_list: Vec<Window> = vec![];
    let window_info = unsafe { CGWindowListCopyWindowInfo(options, kCGNullWindowID) };

    if window_info.is_null() {
        eprintln!("Failed to get window info");
        return vec![];
    }

    let array: core_foundation::array::CFArray<CFDictionary> =
        unsafe { core_foundation::array::CFArray::wrap_under_get_rule(window_info) };

    let name_key = CFString::from_static_string("kCGWindowName");
    let owner_key = CFString::from_static_string("kCGWindowOwnerName");
    let layer_key = CFString::from_static_string("kCGWindowLayer");
    let alpha_key = CFString::from_static_string("kCGWindowAlpha");
    let pid_key = CFString::from_static_string("kCGWindowOwnerPID");
    // let onscreen_key = CFString::from_static_string("kCGWindowIsOnscreen");

    for dict in array.iter() {
        // Filter only normal layer (0), visible (onscreen), and non-zero alpha
        let layer = get_cf_u32(&dict, &layer_key).unwrap_or(0);
        let alpha = get_cf_u32(&dict, &alpha_key).unwrap_or(0);
        // let onscreen = get_cf_u32(&dict, &onscreen_key).unwrap_or(0);

        if layer != 0 || alpha == 0 {
            continue; // skip system windows or invisible windows
        }
        let name = get_cf_string(&dict, &name_key);
        let owner = get_cf_string(&dict, &owner_key);
        let pid = get_cf_pid(&dict, &pid_key);
        if pid.is_some() && !owner.is_empty() && !name.is_empty() {
            let window = Window {
                pid: pid.unwrap(),
                app_name: name,
                window_name: owner,
            };
            window_list.push(window);
        }
    }
    return window_list;
}
