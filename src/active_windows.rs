#![allow(deprecated, unexpected_cfgs)]

use accessibility::{AXAttribute, AXUIElement};
use cocoa::appkit::NSApplicationActivationPolicyRegular;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSArray, NSString, NSUInteger};
use core_foundation::{
    base::{CFGetTypeID, CFTypeRef, TCFType},
    boolean::CFBoolean,
    dictionary::CFDictionary,
    number::CFNumber,
    string::CFString,
};
use core_graphics::display::{CGWindowListCopyWindowInfo, kCGNullWindowID, kCGWindowListOptionAll};
use objc::{class, msg_send, sel, sel_impl};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::browser_profile::{
    BrowserProfile, profile_from_window_title, profile_name_for_directory, profiles_from_local_state,
};
use crate::window_enum::{EnumeratedAxWindow, EnumeratedCgWindow, merge_cg_and_ax};
use crate::window_filter::{is_switchable_window_size, should_include_cg_window};
use crate::Window;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn _AXUIElementGetWindow(element: *const std::ffi::c_void, identifier: *mut u32) -> i32;
}

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

            let cf_num = CFNumber::wrap_under_get_rule(value_ref as *const _);
            cf_num.to_i64().map(|n| n != 0)
        }
    })
}

fn get_cf_bounds_size(dict: &CFDictionary, key: &CFString) -> Option<(f64, f64)> {
    let key_ref = key.as_CFTypeRef();
    let value_ref = *dict.find(key_ref)? as CFTypeRef;
    if value_ref.is_null() {
        return None;
    }

    let bounds: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(value_ref as *const _) };
    let width_key = CFString::from_static_string("Width");
    let height_key = CFString::from_static_string("Height");
    let width = get_cf_f64(&bounds, &width_key)?;
    let height = get_cf_f64(&bounds, &height_key)?;
    Some((width, height))
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
    let bounds_key = CFString::from_static_string("kCGWindowBounds");

    for dict in array.iter() {
        let layer = get_cf_i64(&dict, &layer_key).unwrap_or(0);
        let alpha = get_cf_f64(&dict, &alpha_key).unwrap_or(1.0);
        if !should_include_cg_window(layer, alpha) {
            continue;
        }

        if let Some((width, height)) = get_cf_bounds_size(&dict, &bounds_key) {
            if !is_switchable_window_size(width, height) {
                continue;
            }
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

/// CGWindowID for an AX window via the private `_AXUIElementGetWindow` helper.
pub fn ax_cg_window_id(window_element: &AXUIElement) -> Option<u32> {
    let mut identifier = 0u32;
    let status = unsafe {
        _AXUIElementGetWindow(
            window_element.as_CFTypeRef() as *const std::ffi::c_void,
            &mut identifier,
        )
    };
    if status == 0 && identifier != 0 {
        Some(identifier)
    } else {
        None
    }
}

fn regular_app_pids() -> HashMap<i32, String> {
    let mut apps = HashMap::new();
    unsafe {
        // cocoa 0.26 does not bind NSWorkspace; call through objc directly.
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace == nil {
            return apps;
        }
        let running: id = msg_send![workspace, runningApplications];
        if running == nil {
            return apps;
        }

        let count: NSUInteger = msg_send![running, count];
        for index in 0..count {
            let app: id = running.objectAtIndex(index);
            if app == nil {
                continue;
            }

            let policy: i64 = msg_send![app, activationPolicy];
            if policy != NSApplicationActivationPolicyRegular as i64 {
                continue;
            }

            let pid: i32 = msg_send![app, processIdentifier];
            if pid <= 0 {
                continue;
            }

            let name: id = msg_send![app, localizedName];
            let owner = if name != nil {
                let utf8 = name.UTF8String();
                if utf8.is_null() {
                    format!("PID {pid}")
                } else {
                    std::ffi::CStr::from_ptr(utf8)
                        .to_string_lossy()
                        .into_owned()
                }
            } else {
                format!("PID {pid}")
            };

            apps.insert(pid, owner);
        }
    }
    apps
}

fn chromium_local_state_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Some(home) = std::env::var_os("HOME") else {
        return paths;
    };
    let support = PathBuf::from(home).join("Library/Application Support");
    for relative in [
        "Google/Chrome/Local State",
        "Google/Chrome Beta/Local State",
        "Google/Chrome Canary/Local State",
        "Google/Chrome Dev/Local State",
        "Chromium/Local State",
        "BraveSoftware/Brave-Browser/Local State",
        "Microsoft Edge/Local State",
        "Microsoft Edge Beta/Local State",
        "Microsoft Edge Canary/Local State",
        "Vivaldi/Local State",
    ] {
        let path = support.join(relative);
        if path.is_file() {
            paths.push(path);
        }
    }
    paths
}

fn load_browser_profiles() -> Vec<BrowserProfile> {
    let mut profiles = Vec::new();
    let mut seen = HashSet::new();
    for path in chromium_local_state_paths() {
        let Ok(json) = std::fs::read_to_string(&path) else {
            continue;
        };
        for profile in profiles_from_local_state(&json) {
            let key = (profile.directory.clone(), profile.name.clone());
            if seen.insert(key) {
                profiles.push(profile);
            }
        }
    }
    profiles
}

fn profile_directory_from_pid(pid: i32) -> Option<String> {
    let args = process_arguments(pid)?;
    for (index, arg) in args.iter().enumerate() {
        if let Some(value) = arg.strip_prefix("--profile-directory=") {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
        if arg == "--profile-directory" {
            if let Some(value) = args.get(index + 1) {
                if !value.is_empty() {
                    return Some(value.clone());
                }
            }
        }
    }
    None
}

fn process_arguments(pid: i32) -> Option<Vec<String>> {
    // sysctl KERN_PROCARGS2 — macOS-only process argument vector.
    let mut size: libc::size_t = 0;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let status = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size];
    let status = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buffer.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || size < 4 {
        return None;
    }

    let argc = i32::from_ne_bytes(buffer[0..4].try_into().ok()?);
    if argc <= 0 {
        return None;
    }

    let mut args = Vec::new();
    let mut index = 4usize;
    // Skip the executable path, then read argc argument strings.
    while index < size && buffer[index] != 0 {
        index += 1;
    }
    while index < size && buffer[index] == 0 {
        index += 1;
    }

    for _ in 0..argc {
        if index >= size {
            break;
        }
        let start = index;
        while index < size && buffer[index] != 0 {
            index += 1;
        }
        if start < index {
            let arg = String::from_utf8_lossy(&buffer[start..index]).into_owned();
            args.push(arg);
        }
        index += 1;
        while index < size && buffer[index] == 0 {
            index += 1;
        }
    }

    Some(args)
}

fn resolve_profile_name(
    app_name: &str,
    window_title: &str,
    pid: i32,
    profiles: &[BrowserProfile],
    pid_profile_dirs: &HashMap<i32, Option<String>>,
) -> Option<String> {
    if !is_chromium_family(app_name) || profiles.is_empty() {
        return None;
    }

    if let Some(from_title) = profile_from_window_title(window_title, profiles) {
        return Some(from_title);
    }

    let directory = pid_profile_dirs.get(&pid).cloned().unwrap_or(None)?;
    profile_name_for_directory(profiles, &directory)
}

fn is_chromium_family(app_name: &str) -> bool {
    let lower = app_name.to_ascii_lowercase();
    lower.contains("chrome")
        || lower.contains("chromium")
        || lower.contains("brave")
        || lower.contains("edge")
        || lower.contains("vivaldi")
}

/// Enumerate windows via CGWindowList + NSWorkspace + AXWindows.
///
/// Empty CG titles no longer drop the pid; AX windows for that app are still
/// collected. CG window numbers (via `_AXUIElementGetWindow`) stabilize
/// identity across refreshes and keep Chrome profiles distinct.
pub fn get_active_windows() -> Vec<Window> {
    let cg_windows = collect_cg_windows();
    let mut by_pid: HashMap<i32, Vec<CgWindow>> = HashMap::new();
    for window in cg_windows {
        by_pid.entry(window.pid).or_default().push(window);
    }

    let workspace_apps = regular_app_pids();
    for (pid, owner) in &workspace_apps {
        by_pid.entry(*pid).or_insert_with(Vec::new);
        let _ = owner;
    }

    let profiles = load_browser_profiles();
    let mut pid_profile_dirs: HashMap<i32, Option<String>> = HashMap::new();
    let mut window_list: Vec<Window> = Vec::new();

    for (pid, cg_windows) in by_pid.iter() {
        let app_element = AXUIElement::application(*pid);
        let app_name = cg_windows
            .first()
            .map(|window| window.owner.clone())
            .or_else(|| workspace_apps.get(pid).cloned())
            .unwrap_or_else(|| format!("PID {pid}"));

        let enumerated_cg: Vec<EnumeratedCgWindow> = cg_windows
            .iter()
            .map(|cg| EnumeratedCgWindow {
                pid: cg.pid,
                owner: cg.owner.clone(),
                name: cg.name.clone(),
                onscreen: cg.onscreen,
                window_number: cg.window_number,
            })
            .collect();

        let mut ax_elements: Vec<AXUIElement> = Vec::new();
        let mut enumerated_ax: Vec<EnumeratedAxWindow> = Vec::new();

        if let Ok(ax_windows) = app_element.attribute(&AXAttribute::windows()) {
            for i in 0..ax_windows.len() {
                let Some(window_element) = ax_windows.get(i) else {
                    continue;
                };
                let title = ax_window_title(&window_element).unwrap_or_default();
                let cg_window_id = ax_cg_window_id(&window_element);
                let ax_index = ax_elements.len();
                ax_elements.push(window_element.clone());
                enumerated_ax.push(EnumeratedAxWindow {
                    title,
                    cg_window_id,
                    ax_index,
                });
            }
        }

        if is_chromium_family(&app_name) && !pid_profile_dirs.contains_key(pid) {
            pid_profile_dirs.insert(*pid, profile_directory_from_pid(*pid));
        }

        let merged = merge_cg_and_ax(*pid, &app_name, &enumerated_cg, &enumerated_ax);
        for row in merged {
            let element = match row.ax_index {
                Some(index) => ax_elements[index].clone(),
                None => app_element.clone(),
            };
            let profile_name = resolve_profile_name(
                &row.app_name,
                &row.window_name,
                row.pid,
                &profiles,
                &pid_profile_dirs,
            );

            window_list.push(Window::new(
                row.pid,
                row.cg_window_id,
                row.app_name,
                row.window_name,
                profile_name,
                row.onscreen,
                row.is_app_fallback,
                element,
            ));
        }
    }

    window_list
}
