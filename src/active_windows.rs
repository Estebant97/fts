use accessibility::{AXAttribute, AXUIElement};
use core_foundation::{
    base::{CFGetTypeID, CFTypeRef, TCFType},
    boolean::CFBoolean,
    dictionary::CFDictionary,
    number::CFNumber,
    string::CFString,
};
use core_graphics::display::{CGWindowListCopyWindowInfo, kCGNullWindowID, kCGWindowListOptionAll};
use std::collections::{HashMap, HashSet};

use crate::window_filter::should_include_cg_window;
use crate::Window;

#[derive(Clone, Debug)]
struct CgWindow {
    pid: i32,
    owner: String,
    name: String,
    onscreen: bool,
    window_number: u32,
}

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

fn get_cf_i64(dict: &CFDictionary, key: &CFString) -> Option<i64> {
    let key_ref: CFTypeRef = key.as_CFTypeRef();
    dict.find(key_ref).and_then(|v| unsafe {
        let cf_num = CFNumber::wrap_under_get_rule(*v as *const _);
        cf_num.to_i64()
    })
}

fn get_cf_f64(dict: &CFDictionary, key: &CFString) -> Option<f64> {
    let key_ref: CFTypeRef = key.as_CFTypeRef();
    dict.find(key_ref).and_then(|v| unsafe {
        let cf_num = CFNumber::wrap_under_get_rule(*v as *const _);
        cf_num.to_f64()
    })
}

fn get_cf_pid(dict: &CFDictionary, key: &CFString) -> Option<i32> {
    get_cf_i64(dict, key).map(|n| n as i32)
}

fn get_cf_bool(dict: &CFDictionary, key: &CFString) -> Option<bool> {
    let key_ref = key.as_CFTypeRef();
    dict.find(key_ref).and_then(|v| {
        let value_ref = *v as CFTypeRef;
        if value_ref.is_null() {
            return None;
        }

        unsafe {
            let type_id = CFGetTypeID(value_ref);
            if type_id == CFBoolean::type_id() {
                let as_bool = CFBoolean::wrap_under_get_rule(value_ref as *const _);
                return Some(bool::from(as_bool));
            }

            // Fallback: some encodings use a number.
            let cf_num = CFNumber::wrap_under_get_rule(value_ref as *const _);
            cf_num.to_i64().map(|n| n != 0)
        }
    })
}

fn collect_cg_windows() -> Vec<CgWindow> {
    let mut windows = Vec::new();
    let window_info = unsafe { CGWindowListCopyWindowInfo(kCGWindowListOptionAll, kCGNullWindowID) };

    if window_info.is_null() {
        eprintln!("Failed to get window info");
        return windows;
    }

    let array: core_foundation::array::CFArray<CFDictionary> =
        unsafe { core_foundation::array::CFArray::wrap_under_get_rule(window_info) };

    let name_key = CFString::from_static_string("kCGWindowName");
    let owner_key = CFString::from_static_string("kCGWindowOwnerName");
    let layer_key = CFString::from_static_string("kCGWindowLayer");
    let alpha_key = CFString::from_static_string("kCGWindowAlpha");
    let pid_key = CFString::from_static_string("kCGWindowOwnerPID");
    let onscreen_key = CFString::from_static_string("kCGWindowIsOnscreen");
    let number_key = CFString::from_static_string("kCGWindowNumber");

    for dict in array.iter() {
        let layer = get_cf_i64(&dict, &layer_key).unwrap_or(0);
        let alpha = get_cf_f64(&dict, &alpha_key).unwrap_or(1.0);
        if !should_include_cg_window(layer, alpha) {
            continue;
        }

        let Some(pid) = get_cf_pid(&dict, &pid_key) else {
            continue;
        };
        let owner = get_cf_string(&dict, &owner_key);
        if owner.is_empty() {
            continue;
        }

        let name = get_cf_string(&dict, &name_key);
        let onscreen = get_cf_bool(&dict, &onscreen_key).unwrap_or(true);
        let window_number = get_cf_i64(&dict, &number_key).unwrap_or(0) as u32;

        windows.push(CgWindow {
            pid,
            owner,
            name,
            onscreen,
            window_number,
        });
    }

    windows
}

fn ax_window_title(window_element: &AXUIElement) -> Option<String> {
    let title_attr = AXAttribute::new(&CFString::new("AXTitle"));
    let title_cftype = window_element.attribute(&title_attr).ok()?;
    let title_cfstring: CFString =
        unsafe { CFString::wrap_under_get_rule(title_cftype.as_CFTypeRef() as *mut _) };
    let title = title_cfstring.to_string();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// Enumerate windows via CGWindowList + AXWindows.
///
/// Empty CG titles (common without Screen Recording) no longer drop the pid;
/// AX windows for that app are still collected. CG window numbers are kept for
/// stable identity across refreshes.
pub fn get_active_windows() -> Vec<Window> {
    let cg_windows = collect_cg_windows();
    let mut by_pid: HashMap<i32, Vec<CgWindow>> = HashMap::new();
    for window in cg_windows {
        by_pid.entry(window.pid).or_default().push(window);
    }

    // Also discover apps that AX can see from the currently focused app path:
    // every pid we saw in CG is queried for AXWindows, including untitled CG rows.
    let mut window_list: Vec<Window> = Vec::new();
    let mut seen_keys: HashSet<(i32, Option<u32>, String)> = HashSet::new();

    for (pid, cg_windows) in by_pid.iter() {
        let app_element = AXUIElement::application(*pid);
        let app_name = cg_windows
            .first()
            .map(|window| window.owner.clone())
            .unwrap_or_else(|| format!("PID {pid}"));

        let mut matched_cg_numbers = HashSet::new();

        if let Ok(ax_windows) = app_element.attribute(&AXAttribute::windows()) {
            for i in 0..ax_windows.len() {
                let Some(window_element) = ax_windows.get(i) else {
                    continue;
                };
                let title = ax_window_title(&window_element)
                    .unwrap_or_else(|| "Untitled Window".to_string());

                let matched = cg_windows.iter().find(|cg| {
                    !matched_cg_numbers.contains(&cg.window_number)
                        && !cg.name.is_empty()
                        && cg.name == title
                });

                let (cg_window_id, onscreen) = if let Some(cg) = matched {
                    matched_cg_numbers.insert(cg.window_number);
                    (Some(cg.window_number), cg.onscreen)
                } else if let Some(cg) = cg_windows.iter().find(|cg| {
                    !matched_cg_numbers.contains(&cg.window_number) && cg.name.is_empty()
                }) {
                    // Untitled CG rows often correspond to AX windows when Screen
                    // Recording has not been granted.
                    matched_cg_numbers.insert(cg.window_number);
                    (Some(cg.window_number), cg.onscreen)
                } else {
                    (None, true)
                };

                let key = (*pid, cg_window_id, title.clone());
                if !seen_keys.insert(key) {
                    continue;
                }

                window_list.push(Window::new(
                    *pid,
                    cg_window_id,
                    app_name.clone(),
                    title,
                    onscreen,
                    false,
                    window_element.clone(),
                ));
            }
        }

        // CG windows that never appeared in AX (other Spaces / limited trust).
        for cg in cg_windows {
            if matched_cg_numbers.contains(&cg.window_number) {
                continue;
            }
            if cg.name.is_empty() {
                continue;
            }

            let key = (*pid, Some(cg.window_number), cg.name.clone());
            if !seen_keys.insert(key) {
                continue;
            }

            window_list.push(Window::new(
                *pid,
                Some(cg.window_number),
                cg.owner.clone(),
                cg.name.clone(),
                cg.onscreen,
                true,
                app_element.clone(),
            ));
        }
    }

    window_list
}
