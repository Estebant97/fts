//! Mission Control Space switching via private CGS / SkyLight symbols.
//!
//! Apple does not expose a public “go to Space N” API. These FFI bindings mirror
//! what tools like Hammerspoon’s undocumented spaces module use. They may break
//! across macOS updates; all failures are returned as `Err` (never panic).

use core_foundation::array::CFArray;
use core_foundation::base::{CFGetTypeID, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use std::fmt;

use crate::space_index::{select_display_spaces, space_id_at_index};

type CGSConnectionID = i32;
type CGSSpaceID = u64;

#[link(name = "SkyLight", kind = "framework")]
unsafe extern "C" {
    fn CGSMainConnectionID() -> CGSConnectionID;
    fn CGSGetActiveSpace(cid: CGSConnectionID) -> CGSSpaceID;
    fn CGSCopyManagedDisplaySpaces(cid: CGSConnectionID) -> CFTypeRef;
    fn CGSCopyManagedDisplayForSpace(cid: CGSConnectionID, space: CGSSpaceID) -> CFTypeRef;
    fn CGSManagedDisplaySetCurrentSpace(
        cid: CGSConnectionID,
        display: CFTypeRef,
        space: CGSSpaceID,
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpaceError {
    CopyManagedDisplaySpacesFailed,
    NoSpacesFound,
    IndexOutOfRange { index: u8, count: usize },
    CopyDisplayForSpaceFailed { space_id: u64 },
}

impl fmt::Display for SpaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpaceError::CopyManagedDisplaySpacesFailed => {
                write!(f, "CGSCopyManagedDisplaySpaces returned null")
            }
            SpaceError::NoSpacesFound => write!(f, "no Mission Control Spaces found"),
            SpaceError::IndexOutOfRange { index, count } => {
                write!(
                    f,
                    "workspace {index} out of range ({count} Space(s) on focused display)"
                )
            }
            SpaceError::CopyDisplayForSpaceFailed { space_id } => {
                write!(
                    f,
                    "CGSCopyManagedDisplayForSpace failed for space {space_id}"
                )
            }
        }
    }
}

impl std::error::Error for SpaceError {}

/// Activate the Mission Control Space at 1-based `index` on the focused display.
pub fn activate_space(index: u8) -> Result<(), SpaceError> {
    let cid = unsafe { CGSMainConnectionID() };
    let active = unsafe { CGSGetActiveSpace(cid) };
    let displays = copy_user_spaces_per_display(cid)?;
    let spaces = select_display_spaces(&displays, active).ok_or(SpaceError::NoSpacesFound)?;
    let space_id = space_id_at_index(spaces, index).ok_or(SpaceError::IndexOutOfRange {
        index,
        count: spaces.len(),
    })?;

    if space_id == active {
        return Ok(());
    }

    set_current_space(cid, space_id)
}

fn set_current_space(cid: CGSConnectionID, space_id: CGSSpaceID) -> Result<(), SpaceError> {
    let display_ref = unsafe { CGSCopyManagedDisplayForSpace(cid, space_id) };
    if display_ref.is_null() {
        return Err(SpaceError::CopyDisplayForSpaceFailed { space_id });
    }

    // Owned CFString from Copy*.
    let display = unsafe { CFString::wrap_under_create_rule(display_ref as *const _) };
    unsafe {
        CGSManagedDisplaySetCurrentSpace(cid, display.as_CFTypeRef(), space_id);
    }
    Ok(())
}

/// Returns one `Vec<space_id>` per managed display, in Mission Control order.
/// Only user desktop Spaces are included (fullscreen/system Spaces skipped when typed).
fn copy_user_spaces_per_display(cid: CGSConnectionID) -> Result<Vec<Vec<u64>>, SpaceError> {
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
            result.push(Vec::new());
            continue;
        };

        let mut space_ids = Vec::new();
        for space_dict in spaces_array.iter() {
            if !is_user_space(&space_dict) {
                continue;
            }
            if let Some(id) = space_id_from_dict(&space_dict) {
                space_ids.push(id);
            }
        }
        result.push(space_ids);
    }

    if result.iter().all(|spaces| spaces.is_empty()) {
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
        if type_id == CFBoolean::type_id() {
            return None;
        }
        let cf_num = CFNumber::wrap_under_get_rule(value_ref as *const _);
        cf_num.to_i64()
    })
}
