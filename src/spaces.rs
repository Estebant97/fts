//! Read Space metadata through SkyLight; let Dock perform the actual transition.
//!
//! Apple does not expose a public “go to Space N” API. These FFI bindings mirror
//! the metadata queried by Hammerspoon’s spaces module. They may break
//! across macOS updates; all failures are returned as `Err` (never panic).

use accessibility::{AXAttribute, AXUIElement, AXUIElementActions};
use core_foundation::array::CFArray;
use core_foundation::base::{CFGetTypeID, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation::uuid::{CFUUID, CFUUIDCreateFromString, CFUUIDRef};
use core_graphics::display::CGDisplay;
use std::fmt;
use std::{thread, time::Duration};

use crate::mission_control::{MissionControlNode, space_buttons};
use crate::space_index::{
    DisplaySpaces, mission_control_index, select_display_spaces, space_id_at_index,
};
pub use crate::space_switch::SpaceSwitch;
use crate::space_switch::{SpaceTransition, switch_space};

type CGSConnectionID = i32;
type CGSSpaceID = u64;

#[link(name = "SkyLight", kind = "framework")]
unsafe extern "C" {
    fn CGSMainConnectionID() -> CGSConnectionID;
    fn CGSGetActiveSpace(cid: CGSConnectionID) -> CGSSpaceID;
    fn CGSCopyManagedDisplaySpaces(cid: CGSConnectionID) -> CFTypeRef;
    fn CGSCopyManagedDisplayForSpace(cid: CGSConnectionID, space: CGSSpaceID) -> CFTypeRef;
    fn CGSManagedDisplayGetCurrentSpace(cid: CGSConnectionID, display: CFTypeRef) -> CGSSpaceID;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayGetDisplayIDFromUUID(uuid: CFUUIDRef) -> u32;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpaceError {
    CopyManagedDisplaySpacesFailed,
    NoSpacesFound,
    IndexOutOfRange { index: u8, count: usize },
    CopyDisplayForSpaceFailed { space_id: u64 },
    MissionControl(String),
    TransitionTimeout { space_id: u64 },
}

impl fmt::Display for SpaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CopyManagedDisplaySpacesFailed => {
                write!(f, "CGSCopyManagedDisplaySpaces returned null")
            }
            Self::NoSpacesFound => write!(f, "no Mission Control Spaces found"),
            Self::IndexOutOfRange { index, count } => write!(
                f,
                "workspace {index} out of range ({count} Space(s) on focused display)"
            ),
            Self::CopyDisplayForSpaceFailed { space_id } => {
                write!(f, "could not resolve display for Space {space_id}")
            }
            Self::MissionControl(message) => write!(f, "Mission Control: {message}"),
            Self::TransitionTimeout { space_id } => {
                write!(f, "Dock did not finish switching to Space {space_id}")
            }
        }
    }
}
impl std::error::Error for SpaceError {}

/// Activate a user desktop through Dock, which owns window visibility, focus,
/// and menu-bar state. Setting CGSManagedDisplaySetCurrentSpace alone bypasses
/// that transition and can leave windows and menu bars from multiple Spaces.
pub fn activate_space(index: u8) -> Result<SpaceSwitch, SpaceError> {
    let cid = unsafe { CGSMainConnectionID() };
    let active = unsafe { CGSGetActiveSpace(cid) };
    let displays = copy_user_spaces_per_display(cid)?;
    let spaces = select_display_spaces(&displays, active).ok_or(SpaceError::NoSpacesFound)?;
    let space_id = space_id_at_index(spaces, index).ok_or(SpaceError::IndexOutOfRange {
        index,
        count: spaces.len(),
    })?;
    let display_spaces = displays
        .iter()
        .find(|d| d.user_ids.contains(&space_id))
        .ok_or(SpaceError::NoSpacesFound)?;
    let thumbnail =
        mission_control_index(display_spaces, index).ok_or(SpaceError::NoSpacesFound)?;
    let display_ref = unsafe { CGSCopyManagedDisplayForSpace(cid, space_id) };
    if display_ref.is_null() {
        return Err(SpaceError::CopyDisplayForSpaceFailed { space_id });
    }
    let display = unsafe { CFString::wrap_under_create_rule(display_ref as *const _) };
    let mut transition = DockTransition {
        cid,
        display,
        space_id,
        thumbnail,
        dock: None,
        opened_overview: false,
    };
    let result = switch_space(&mut transition);
    if result.is_err() {
        transition.close_owned_overview();
    }
    result
}

struct DockTransition {
    cid: CGSConnectionID,
    display: CFString,
    space_id: u64,
    thumbnail: usize,
    dock: Option<AXUIElement>,
    opened_overview: bool,
}

impl DockTransition {
    fn close_owned_overview(&self) {
        if self.opened_overview
            && self
                .dock
                .as_ref()
                .is_some_and(|dock| child_with_id(dock, "mc").is_some())
        {
            let _ = toggle_mission_control();
        }
    }
}

impl SpaceTransition for DockTransition {
    type Error = SpaceError;
    fn is_current(&mut self) -> Result<bool, SpaceError> {
        // Check the destination display rather than global focus, which may
        // still belong to another monitor during the transition.
        let current =
            unsafe { CGSManagedDisplayGetCurrentSpace(self.cid, self.display.as_CFTypeRef()) };
        if current == 0 {
            return Err(SpaceError::NoSpacesFound);
        }
        // A metadata update alone is insufficient: Dock must also have exited
        // its overview before another queued Option+N request can run.
        let overview_open = self
            .dock
            .as_ref()
            .is_some_and(|dock| child_with_id(dock, "mc").is_some());
        Ok(current == self.space_id && !overview_open)
    }
    fn press_destination(&mut self) -> Result<(), SpaceError> {
        if !crate::permissions::ensure_accessibility_trust(false) {
            return Err(SpaceError::MissionControl(
                "Accessibility permission is required".into(),
            ));
        }
        let dock = AXUIElement::application_with_bundle("com.apple.dock")
            .map_err(|e| SpaceError::MissionControl(e.to_string()))?;
        let display_id = display_id(&self.display)?;
        let opened = child_with_id(&dock, "mc").is_none();
        self.dock = Some(dock.clone());
        if opened {
            toggle_mission_control()?;
            self.opened_overview = true;
        }
        // Wait before looking up thumbnails: holding one across the animation
        // leaves a stale AX element when WindowManager rebuilds the spaces bar.
        thread::sleep(Duration::from_millis(300));
        // Retry the whole lookup: displays and their spaces bars can appear
        // separately while Mission Control animates, especially on reopening.
        for _ in 0..40 {
            thread::sleep(Duration::from_millis(50));
            if let Some(mc) = child_with_id(&dock, "mc") {
                let mut roots = vec![mc];
                if let Ok(wm) = AXUIElement::application_with_bundle("com.apple.WindowManager") {
                    roots.push(wm);
                }
                if let Some(buttons) = space_buttons(&roots, display_id) {
                    if let Some(button) = buttons.get(self.thumbnail) {
                        match button.press() {
                            Ok(()) => return Ok(()),
                            // A stale element means no action was performed;
                            // retry the lookup without toggling the overview.
                            Err(accessibility::Error::Ax(
                                accessibility_sys::kAXErrorInvalidUIElement,
                            )) => continue,
                            Err(error) => {
                                return Err(SpaceError::MissionControl(error.to_string()));
                            }
                        }
                    }
                }
            }
        }
        Err(SpaceError::MissionControl(
            "destination thumbnail unavailable".into(),
        ))
    }

    fn wait(&mut self) {
        thread::sleep(Duration::from_millis(50));
    }
    fn timeout(&self) -> SpaceError {
        SpaceError::TransitionTimeout {
            space_id: self.space_id,
        }
    }
}

impl MissionControlNode for AXUIElement {
    fn identifier(&self) -> Option<String> {
        identifier(self)
    }
    fn children(&self) -> Vec<Self> {
        children(self)
    }
    fn display_id(&self) -> Option<u32> {
        ax_display_id(self)
    }
}

fn children(element: &AXUIElement) -> Vec<AXUIElement> {
    element
        .attribute(&AXAttribute::children())
        .map(|a| a.iter().map(|e| (*e).clone()).collect())
        .unwrap_or_default()
}
fn identifier(element: &AXUIElement) -> Option<String> {
    element
        .attribute(&AXAttribute::identifier())
        .ok()
        .map(|s| s.to_string())
}
fn child_with_id(element: &AXUIElement, id: &str) -> Option<AXUIElement> {
    children(element)
        .into_iter()
        .find(|child| identifier(child).as_deref() == Some(id))
}
fn ax_display_id(element: &AXUIElement) -> Option<u32> {
    let value = element
        .attribute(&AXAttribute::new(&CFString::from_static_string(
            "AXDisplayID",
        )))
        .ok()?;
    value
        .downcast::<CFNumber>()?
        .to_i64()
        .and_then(|id| u32::try_from(id).ok())
}
fn display_id(display: &CFString) -> Result<u32, SpaceError> {
    // With separate Spaces disabled CGS uses "Main" instead of a UUID.
    if display.to_string() == "Main" {
        return Ok(CGDisplay::main().id);
    }
    let raw = unsafe { CFUUIDCreateFromString(std::ptr::null(), display.as_concrete_TypeRef()) };
    if raw.is_null() {
        return Err(SpaceError::MissionControl("invalid display UUID".into()));
    }
    let uuid = unsafe { CFUUID::wrap_under_create_rule(raw) };
    let id = unsafe { CGDisplayGetDisplayIDFromUUID(uuid.as_concrete_TypeRef()) };
    if id == 0 {
        return Err(SpaceError::MissionControl("display disconnected".into()));
    }
    Ok(id)
}
fn toggle_mission_control() -> Result<(), SpaceError> {
    // Launch Services supplies the application launch context required by the
    // system app. Executing its Mach-O directly is killed on newer macOS.
    let status = std::process::Command::new("/usr/bin/open")
        .args(["-b", "com.apple.exposelauncher"])
        .status()
        .map_err(|e| SpaceError::MissionControl(e.to_string()))?;
    if !status.success() {
        return Err(SpaceError::MissionControl(format!(
            "launcher exited with {status}"
        )));
    }
    Ok(())
}

/// Returns per-display membership + user-Space lists in Mission Control order.
///
/// `membership_ids` keeps fullscreen/system Spaces so we can resolve which
/// display owns the active Space; `user_ids` is what Option+N indexes.
fn copy_user_spaces_per_display(cid: CGSConnectionID) -> Result<Vec<DisplaySpaces>, SpaceError> {
    let raw = unsafe { CGSCopyManagedDisplaySpaces(cid) };
    if raw.is_null() {
        return Err(SpaceError::CopyManagedDisplaySpacesFailed);
    }

    let displays: CFArray<CFDictionary> =
        unsafe { CFArray::wrap_under_create_rule(raw as *const _) };
    let mut result = Vec::with_capacity(displays.len() as usize);
    let spaces_key = CFString::from_static_string("Spaces");

    for dict in displays.iter() {
        let Some(spaces_array) = get_cf_array_of_dicts(&dict, &spaces_key) else {
            result.push(DisplaySpaces {
                membership_ids: Vec::new(),
                user_ids: Vec::new(),
            });
            continue;
        };

        let mut membership_ids = Vec::new();
        let mut user_ids = Vec::new();
        for space_dict in spaces_array.iter() {
            let Some(id) = space_id_from_dict(&space_dict) else {
                continue;
            };
            membership_ids.push(id);
            if is_user_space(&space_dict) {
                user_ids.push(id);
            }
        }
        result.push(DisplaySpaces {
            membership_ids,
            user_ids,
        });
    }

    if result.iter().all(|display| display.user_ids.is_empty()) {
        return Err(SpaceError::NoSpacesFound);
    }

    Ok(result)
}

fn space_id_from_dict(dict: &CFDictionary) -> Option<u64> {
    get_cf_i64(dict, &CFString::from_static_string("ManagedSpaceID"))
        .or_else(|| get_cf_i64(dict, &CFString::from_static_string("id64")))
        .map(|n| n as u64)
}

/// User desktops are type `0`. Missing type is treated as user (older payloads).
fn is_user_space(dict: &CFDictionary) -> bool {
    match get_cf_i64(dict, &CFString::from_static_string("type")) {
        Some(0) | None => true,
        Some(_) => false,
    }
}

fn get_cf_array_of_dicts(dict: &CFDictionary, key: &CFString) -> Option<CFArray<CFDictionary>> {
    let key_ref = key.as_CFTypeRef();
    let value_ref = *dict.find(key_ref)? as CFTypeRef;
    if value_ref.is_null() {
        return None;
    }
    if unsafe { CFGetTypeID(value_ref) } != CFArray::<CFDictionary>::type_id() {
        return None;
    }
    Some(unsafe { CFArray::wrap_under_get_rule(value_ref as *const _) })
}

fn get_cf_i64(dict: &CFDictionary, key: &CFString) -> Option<i64> {
    let key_ref: CFTypeRef = key.as_CFTypeRef();
    dict.find(key_ref).and_then(|v| unsafe {
        let value_ref = *v as CFTypeRef;
        if value_ref.is_null() {
            return None;
        }
        let type_id = CFGetTypeID(value_ref);
        if type_id != CFNumber::type_id() {
            return None;
        }
        let cf_num = CFNumber::wrap_under_get_rule(value_ref as *const _);
        cf_num.to_i64()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[link(name = "SkyLight", kind = "framework")]
    unsafe extern "C" {
        fn SLSCopySpacesForWindows(
            cid: CGSConnectionID,
            mask: i32,
            windows: CFTypeRef,
        ) -> CFTypeRef;
    }

    fn onscreen_windows() -> HashSet<u32> {
        use core_graphics::display::{
            CGWindowListCopyWindowInfo, kCGNullWindowID, kCGWindowListOptionOnScreenOnly,
        };
        let raw =
            unsafe { CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID) };
        assert!(!raw.is_null(), "read onscreen window inventory");
        let rows = unsafe { CFArray::<CFDictionary>::wrap_under_create_rule(raw as *const _) };
        rows.iter()
            .filter(|row| {
                get_cf_i64(row, &CFString::from_static_string("kCGWindowLayer")) == Some(0)
            })
            .filter_map(|row| get_cf_i64(&row, &CFString::from_static_string("kCGWindowNumber")))
            .filter_map(|id| u32::try_from(id).ok())
            .collect()
    }

    fn windows_exclusive_to_source(cid: CGSConnectionID, source: u64, target: u64) -> HashSet<u32> {
        onscreen_windows()
            .into_iter()
            .filter(|id| {
                let windows = CFArray::from_CFTypes(&[CFNumber::from(*id as i64)]);
                let raw = unsafe { SLSCopySpacesForWindows(cid, 7, windows.as_CFTypeRef()) };
                assert!(!raw.is_null(), "read window Space membership");
                let spaces =
                    unsafe { CFArray::<CFNumber>::wrap_under_create_rule(raw as *const _) };
                let ids: Vec<u64> = spaces
                    .iter()
                    .filter_map(|n| n.to_i64())
                    .map(|n| n as u64)
                    .collect();
                // Sticky/all-Spaces windows and other displays are allowed to remain.
                ids.contains(&source) && !ids.contains(&target)
            })
            .collect()
    }

    fn assert_source_windows_hidden(source_only: &HashSet<u32>) {
        for _ in 0..20 {
            if source_only.is_disjoint(&onscreen_windows()) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let still_visible: Vec<_> = source_only
            .intersection(&onscreen_windows())
            .copied()
            .collect();
        assert!(
            still_visible.is_empty(),
            "source desktop windows remained onscreen: {still_visible:?}"
        );
    }

    #[test]
    #[ignore = "changes real desktops; requires an unlocked Mac with Accessibility permission"]
    fn live_desktop_round_trip() {
        assert!(
            crate::permissions::ensure_accessibility_trust(false),
            "Accessibility permission is required for the live desktop test"
        );
        assert!(
            !CGDisplay::active_displays()
                .expect("read active displays")
                .is_empty(),
            "an active display is required"
        );
        let cid = unsafe { CGSMainConnectionID() };
        let original = unsafe { CGSGetActiveSpace(cid) };
        let displays = copy_user_spaces_per_display(cid).expect("read Spaces");
        let spaces = select_display_spaces(&displays, original).expect("focused display");
        let original_index = spaces
            .iter()
            .position(|id| *id == original)
            .expect("start the test on a user desktop") as u8
            + 1;
        assert!(
            spaces.len() >= 2,
            "the live test requires at least two desktops"
        );
        struct Restore(u8);
        impl Drop for Restore {
            fn drop(&mut self) {
                if let Err(error) = activate_space(self.0) {
                    eprintln!("Could not restore initial desktop: {error}");
                }
            }
        }
        let _restore = Restore(original_index);
        for round in 0..3 {
            for destination in 1..=spaces.len().min(9) as u8 {
                if destination == original_index {
                    continue;
                }
                let destination_id = spaces[usize::from(destination - 1)];
                let source_windows = windows_exclusive_to_source(cid, original, destination_id);
                assert_eq!(activate_space(destination), Ok(SpaceSwitch::Switched));
                assert_source_windows_hidden(&source_windows);
                if round == 0 && std::env::var_os("FTS_TEST_SCREENSHOTS").is_some() {
                    // Capture a settled frame, rather than the last part of
                    // Mission Control's visual transition.
                    thread::sleep(Duration::from_millis(500));
                    let path = format!("/tmp/fts-space-{destination}.png");
                    assert!(
                        std::process::Command::new("/usr/sbin/screencapture")
                            .args(["-x", &path])
                            .status()
                            .expect("capture desktop")
                            .success(),
                        "Screen Recording permission is required for screenshots"
                    );
                }
                assert_eq!(activate_space(destination), Ok(SpaceSwitch::AlreadyActive));
                let destination_windows =
                    windows_exclusive_to_source(cid, destination_id, original);
                assert_eq!(activate_space(original_index), Ok(SpaceSwitch::Switched));
                assert_source_windows_hidden(&destination_windows);
                eprintln!(
                    "Space {original_index} → {destination} → {original_index}: verified {} source and {} destination windows hidden",
                    source_windows.len(),
                    destination_windows.len()
                );
            }
        }
    }
}
