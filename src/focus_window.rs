#![allow(deprecated)]

use accessibility::{AXAttribute, AXUIElement};
use cocoa::appkit::{NSApplicationActivateIgnoringOtherApps, NSRunningApplication};
use cocoa::base::nil;
use core_foundation::{base::TCFType, boolean::CFBoolean, string::CFString};

use crate::active_windows::ax_cg_window_id;
use crate::focus_resolve::{FocusCandidate, allow_focused_fallback_after_cg_miss, focus_candidate};
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
        eprintln!(
            "warning: no AX window for {} — {} (activated app only)",
            window.app_name, window.window_name
        );
        return Err(FocusError::AppFallback);
    }

    // Prefer re-resolving by CG window id so we raise the intended Chrome
    // profile window even if the cached AXUIElement went stale.
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

/// Find the live AX window for `window`, preferring CG window id equality.
///
/// Does **not** fall back to `AXFocusedWindow` when a CG id is known but
/// unmatched — that fallback is what left the other Chrome profile in front.
fn resolve_window_element(window: &Window) -> Option<AXUIElement> {
    let app_element = AXUIElement::application(window.pid);
    let Ok(ax_windows) = app_element.attribute(&AXAttribute::windows()) else {
        if allow_focused_fallback_after_cg_miss(window.cg_window_id) {
            return focused_window_of(&app_element);
        }
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
            None
        }
        FocusCandidate::ByTitle => {
            for i in 0..ax_windows.len() {
                let Some(window_element) = ax_windows.get(i) else {
                    continue;
                };
                let Ok(title_cftype) = window_element.attribute(&title_attr) else {
                    continue;
                };
                let title: CFString =
                    unsafe { CFString::wrap_under_get_rule(title_cftype.as_CFTypeRef() as *mut _) };
                if title.to_string() == window.window_name {
                    return Some(window_element.clone());
                }
            }
            focused_window_of(&app_element)
        }
    }
}

fn focused_window_of(app_element: &AXUIElement) -> Option<AXUIElement> {
    let focused_attr = AXAttribute::new(&CFString::new("AXFocusedWindow"));
    let focused_cftype = app_element.attribute(&focused_attr).ok()?;
    Some(unsafe { AXUIElement::wrap_under_get_rule(focused_cftype.as_CFTypeRef() as *mut _) })
}
