use accessibility::{AXAttribute, AXUIElement};
use core_foundation::{
    base::{CFTypeRef, TCFType},
    dictionary::CFDictionary,
    number::CFNumber,
    string::CFString,
};
use core_graphics::display::{CGWindowListCopyWindowInfo, kCGNullWindowID, kCGWindowListOptionAll};
use std::collections::HashMap;

use crate::Window;
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

pub fn get_active_windows() -> Vec<Window> {
    let options = kCGWindowListOptionAll;
    let mut window_list: Vec<Window> = vec![];
    let window_info = unsafe { CGWindowListCopyWindowInfo(options, kCGNullWindowID) };

    if window_info.is_null() {
        eprintln!("Failed to get window info");
        return window_list;
    }

    let array: core_foundation::array::CFArray<CFDictionary> =
        unsafe { core_foundation::array::CFArray::wrap_under_get_rule(window_info) };

    let name_key = CFString::from_static_string("kCGWindowName");
    let owner_key = CFString::from_static_string("kCGWindowOwnerName");
    let layer_key = CFString::from_static_string("kCGWindowLayer");
    let alpha_key = CFString::from_static_string("kCGWindowAlpha");
    let pid_key = CFString::from_static_string("kCGWindowOwnerPID");
    let onscreen_key = CFString::from_static_string("kCGWindowIsOnscreen");

    // First pass: collect all windows from CGWindowList by PID
    let mut cg_windows_by_pid: HashMap<i32, Vec<(String, String, u32)>> = HashMap::new();

    for dict in array.iter() {
        let layer = get_cf_u32(&dict, &layer_key).unwrap_or(0);
        let alpha = get_cf_u32(&dict, &alpha_key).unwrap_or(0);
        let onscreen = get_cf_u32(&dict, &onscreen_key).unwrap_or(0);

        // Allow normal windows (layer 0) and full-screen windows (layer can be different)
        // Skip only if alpha is 0 (fully transparent/invisible)
        if alpha == 0 {
            continue;
        }

        let name = get_cf_string(&dict, &name_key);
        let owner = get_cf_string(&dict, &owner_key);
        let pid = get_cf_pid(&dict, &pid_key);

        eprintln!(
            "[DEBUG] CGWindow: app='{}', window='{}', layer={}, alpha={}, onscreen={}",
            owner, name, layer, alpha, onscreen
        );

        if pid.is_some() && !owner.is_empty() && !name.is_empty() {
            cg_windows_by_pid
                .entry(pid.unwrap())
                .or_insert_with(Vec::new)
                .push((owner, name, onscreen));
        }
    }

    // Second pass: for each unique PID, get all AX windows
    for (pid, cg_windows) in cg_windows_by_pid.iter() {
        let app_element = AXUIElement::application(*pid);
        let app_name = &cg_windows[0].0;

        eprintln!(
            "[DEBUG] Processing app: {} (PID: {}), CGWindows count: {}",
            app_name,
            pid,
            cg_windows.len()
        );

        // Try to get all windows from the application
        if let Ok(ax_windows) = app_element.attribute(&AXAttribute::windows()) {
            eprintln!("[DEBUG]   AXWindows count: {}", ax_windows.len());

            // Add all windows from AXWindows
            for i in 0..ax_windows.len() {
                if let Some(window_element) = ax_windows.get(i) {
                    let title_attr = AXAttribute::new(&CFString::new("AXTitle"));
                    if let Ok(title_cftype) = window_element.attribute(&title_attr) {
                        let title_cfstring: CFString = unsafe {
                            CFString::wrap_under_get_rule(title_cftype.as_CFTypeRef() as *mut _)
                        };
                        let title_str = title_cfstring.to_string();
                        eprintln!("[DEBUG]   ✓ Adding AX window: '{}'", title_str);

                        // Find matching onscreen status from CGWindows
                        let onscreen = cg_windows
                            .iter()
                            .find(|(_, name, _)| name == &title_str)
                            .map(|(_, _, os)| *os)
                            .unwrap_or(1);

                        window_list.push(Window {
                            pid: *pid,
                            app_name: app_name.clone(),
                            window_name: title_str,
                            onscreen,
                            element: window_element.clone(),
                        });
                    }
                }
            }

            // If we didn't get all windows from AXWindows, try system-wide search
            let windows_found = window_list.iter().filter(|w| w.pid == *pid).count();
            if windows_found < cg_windows.len() {
                eprintln!(
                    "[DEBUG]   Missing windows! Found {} but expected {}. Trying system-wide search...",
                    windows_found,
                    cg_windows.len()
                );
                let system_wide = AXUIElement::system_wide();
                // Try to get all windows from the system
                if let Ok(all_windows_cftype) =
                    system_wide.attribute(&AXAttribute::new(&CFString::new("AXWindows")))
                {
                    let all_windows: core_foundation::array::CFArray<AXUIElement> = unsafe {
                        core_foundation::array::CFArray::wrap_under_get_rule(
                            all_windows_cftype.as_CFTypeRef() as *const _,
                        )
                    };

                    eprintln!("[DEBUG]   System-wide windows count: {}", all_windows.len());

                    for i in 0..all_windows.len() {
                        if let Some(window) = all_windows.get(i) {
                            // Check if this window belongs to our PID
                            let pid_attr = AXAttribute::new(&CFString::new("AXPid"));
                            if let Ok(window_pid_cftype) = window.attribute(&pid_attr) {
                                let window_pid_cfnum: CFNumber = unsafe {
                                    CFNumber::wrap_under_get_rule(
                                        window_pid_cftype.as_CFTypeRef() as *mut _
                                    )
                                };
                                if let Some(window_pid) = window_pid_cfnum.to_i32() {
                                    if window_pid == *pid {
                                        let title_attr =
                                            AXAttribute::new(&CFString::new("AXTitle"));
                                        if let Ok(title_cftype) = window.attribute(&title_attr) {
                                            let title_cfstring: CFString = unsafe {
                                                CFString::wrap_under_get_rule(
                                                    title_cftype.as_CFTypeRef() as *mut _,
                                                )
                                            };
                                            let title_str = title_cfstring.to_string();

                                            // Check if this matches one of our CGWindows and isn't already in the list
                                            if cg_windows
                                                .iter()
                                                .any(|(_, name, _)| name == &title_str)
                                                && !window_list.iter().any(|w| {
                                                    w.pid == *pid && w.window_name == title_str
                                                })
                                            {
                                                eprintln!(
                                                    "[DEBUG]   ✓ Found via system-wide search: '{}'",
                                                    title_str
                                                );

                                                let onscreen = cg_windows
                                                    .iter()
                                                    .find(|(_, name, _)| name == &title_str)
                                                    .map(|(_, _, os)| *os)
                                                    .unwrap_or(1);

                                                window_list.push(Window {
                                                    pid: *pid,
                                                    app_name: app_name.clone(),
                                                    window_name: title_str,
                                                    onscreen,
                                                    element: window.clone(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Final fallback: if we still have no windows for this app, try focused window
            if !window_list.iter().any(|w| w.pid == *pid) {
                let focused_attr = AXAttribute::new(&CFString::new("AXFocusedWindow"));
                if let Ok(focused_cftype) = app_element.attribute(&focused_attr) {
                    let focused_window: AXUIElement = unsafe {
                        AXUIElement::wrap_under_get_rule(focused_cftype.as_CFTypeRef() as *mut _)
                    };

                    let title_attr = AXAttribute::new(&CFString::new("AXTitle"));
                    if let Ok(title_cftype) = focused_window.attribute(&title_attr) {
                        let title_cfstring: CFString = unsafe {
                            CFString::wrap_under_get_rule(title_cftype.as_CFTypeRef() as *mut _)
                        };
                        let title_str = title_cfstring.to_string();
                        eprintln!("[DEBUG]   ✓ Adding focused window: '{}'", title_str);

                        let onscreen = cg_windows
                            .iter()
                            .find(|(_, name, _)| name == &title_str)
                            .map(|(_, _, os)| *os)
                            .unwrap_or(1);

                        window_list.push(Window {
                            pid: *pid,
                            app_name: app_name.clone(),
                            window_name: title_str,
                            onscreen,
                            element: focused_window,
                        });
                    }
                } else {
                    eprintln!("[DEBUG]   ✗ No windows available via any method");
                }
            }
        } else {
            eprintln!("[DEBUG]   ✗ Failed to get AXWindows attribute");
        }
    }

    return window_list;
}
