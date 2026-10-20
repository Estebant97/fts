#![allow(deprecated)]

use accessibility::{AXAttribute, AXUIElement};
use cocoa::appkit::{NSApplicationActivateIgnoringOtherApps, NSRunningApplication};
use cocoa::base::nil;
use core_foundation::{base::TCFType, boolean::CFBoolean, string::CFString};

use crate::active_windows::ax_cg_window_id;
use crate::focus_resolve::{
    FocusCandidate, ax_title_matches_stored, focus_candidate, take_unique_match,
};
use crate::Window;

#[derive(Debug)]
pub enum FocusError {
    ActivateApp,
    RaiseWindow,
    AppFallback,
}

pub fn focus_window(window: &Window) -> Result<(), FocusError> {
    if window.is_app_fallback {
        activate_app(window.pid)?;
        if let Some(resolved) = resolve_window_element(window) {
            unminimize(&resolved);
            return raise_window(&resolved);
        }
        // App came forward, but the selected CG surface was never raised.
        // Fail so MRU does not promote a window that is not actually focused.
        eprintln!(
            "warning: no AX window for {} — {} (activated app only)",
            window.app_name, window.window_name
        );
        return Err(FocusError::AppFallback);
    }

    // CG id → cached element → unique title match. Never guess AXFocusedWindow.
    let target = resolve_window_element(window).unwrap_or_else(|| window.element.clone());
    unminimize(&target);
    activate_app(window.pid)?;
    raise_window(&target)
}

fn activate_app(pid: i32) -> Result<(), FocusError> {
    unsafe {
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(nil, pid);
        if app == nil {
            return Err(FocusError::ActivateApp);
        }
        let _ = app.activateWithOptions_(NSApplicationActivateIgnoringOtherApps);
    }

    let app_element = AXUIElement::application(pid);
    let frontmost_attr = AXAttribute::new(&CFString::new("AXFrontmost"));
    let _ = app_element.set_attribute(&frontmost_attr, CFBoolean::true_value().as_CFType());
    Ok(())
}

fn unminimize(window_element: &AXUIElement) {
    let minimized = AXAttribute::new(&CFString::new("AXMinimized"));
    let _ = window_element.set_attribute(&minimized, CFBoolean::false_value().as_CFType());
}

fn raise_window(window_element: &AXUIElement) -> Result<(), FocusError> {
    if let Ok(app_element) = window_element.attribute(&AXAttribute::parent()) {
        let frontmost_attr = AXAttribute::new(&CFString::new("AXFrontmost"));
        let _ = app_element.set_attribute(&frontmost_attr, CFBoolean::true_value().as_CFType());
    }

    let raise = CFString::new("AXRaise");
    window_element
        .perform_action(&raise)
        .map_err(|_| FocusError::RaiseWindow)?;

    let main_attr = AXAttribute::new(&CFString::new("AXMain"));
    let _ = window_element.set_attribute(&main_attr, CFBoolean::true_value().as_CFType());

    let focused_attr = AXAttribute::new(&CFString::new("AXFocused"));
    let _ = window_element.set_attribute(&focused_attr, CFBoolean::true_value().as_CFType());

    Ok(())
}

/// Find the live AX window for `window`.
///
/// Order: CG window id → cached element (still in the live list) → unique
/// title match. Returns `None` so the caller can keep `window.element` when
/// re-resolve is inconclusive. Does **not** use `AXFocusedWindow`.
fn resolve_window_element(window: &Window) -> Option<AXUIElement> {
    let app_element = AXUIElement::application(window.pid);
    let Ok(ax_windows) = app_element.attribute(&AXAttribute::windows()) else {
        return None;
    };

    let title_attr = AXAttribute::new(&CFString::new("AXTitle"));

    match focus_candidate(window.cg_window_id) {
        FocusCandidate::ByCgWindowId(target_id) => {
            for i in 0..ax_windows.len() {
                let Some(window_element) = ax_windows.get(i) else {
                    continue;
                };
                if ax_cg_window_id(&window_element) == Some(target_id) {
                    return Some(window_element.clone());
                }
            }
            // Known id but not listed — let the caller raise the cached element.
            None
        }
        FocusCandidate::CachedThenTitle => {
            // Prefer the element the switcher already selected when it is still live.
            if !window.is_app_fallback {
                for i in 0..ax_windows.len() {
                    let Some(window_element) = ax_windows.get(i) else {
                        continue;
                    };
                    if window.same_element(&window_element) {
                        return Some(window_element.clone());
                    }
                }
            }

            let mut matches = Vec::new();
            for i in 0..ax_windows.len() {
                let Some(window_element) = ax_windows.get(i) else {
                    continue;
                };
                let live_title = window_element
                    .attribute(&title_attr)
                    .ok()
                    .map(|title_cftype| {
                        let title: CFString = unsafe {
                            CFString::wrap_under_get_rule(title_cftype.as_CFTypeRef() as *mut _)
                        };
                        title.to_string()
                    })
                    .unwrap_or_default();
                if ax_title_matches_stored(&window.window_name, &live_title) {
                    matches.push(window_element.clone());
                }
            }

            take_unique_match(matches)
        }
    }
}
