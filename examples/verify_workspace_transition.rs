#![allow(dead_code, deprecated)]

//! Run on an unlocked Mac: cargo run --example verify_workspace_transition -- 2 3 1
//! The last argument is the desktop to restore if any request fails.

#[cfg(target_os = "macos")]
#[path = "../src/mission_control.rs"]
mod mission_control;
#[cfg(target_os = "macos")]
#[path = "../src/permissions.rs"]
mod permissions;
#[cfg(target_os = "macos")]
#[path = "../src/space_index.rs"]
mod space_index;
#[cfg(target_os = "macos")]
#[path = "../src/space_switch.rs"]
mod space_switch;
#[cfg(target_os = "macos")]
#[path = "../src/spaces.rs"]
mod spaces;
#[cfg(target_os = "macos")]
#[path = "../src/transition_cover.rs"]
mod transition_cover;

#[cfg(target_os = "macos")]
fn assert_no_cover_windows() {
    use core_foundation::{
        array::CFArray,
        base::{CFType, CFTypeRef, TCFType},
        dictionary::CFDictionary,
        number::CFNumber,
        string::CFString,
    };
    use core_graphics::display::{
        CGWindowListCopyWindowInfo, kCGNullWindowID, kCGWindowListOptionOnScreenOnly,
    };
    fn number(row: &CFDictionary, key: &str) -> Option<i64> {
        let key = CFString::new(key);
        let raw = *row.find(key.as_CFTypeRef())? as CFTypeRef;
        unsafe { CFType::wrap_under_get_rule(raw) }
            .downcast::<CFNumber>()?
            .to_i64()
    }
    for _ in 0..20 {
        let raw =
            unsafe { CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID) };
        assert!(!raw.is_null(), "read visible windows");
        let rows = unsafe { CFArray::<CFDictionary>::wrap_under_create_rule(raw as *const _) };
        let visible = rows.iter().any(|row| {
            number(&row, "kCGWindowOwnerPID") == Some(i64::from(std::process::id()))
                && number(&row, "kCGWindowLayer") == Some(1000)
        });
        if !visible {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("a transition snapshot was left visible");
}

#[cfg(target_os = "macos")]
fn main() {
    let bare = std::env::var_os("FTS_TEST_BARE").is_some();
    if bare {
        use cocoa::{
            appkit::{NSApplication, NSApplicationActivationPolicyAccessory},
            base::nil,
        };
        unsafe {
            let app = NSApplication::sharedApplication(nil);
            app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
            app.finishLaunching();
        }
    }
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> core_foundation::dictionary::CFDictionaryRef;
    }
    use core_foundation::{
        base::{CFTypeRef, TCFType},
        boolean::CFBoolean,
        dictionary::CFDictionary,
        string::CFString,
    };
    let raw = unsafe { CGSessionCopyCurrentDictionary() };
    assert!(!raw.is_null(), "an unlocked desktop session is required");
    let session = unsafe { CFDictionary::<CFTypeRef, CFTypeRef>::wrap_under_create_rule(raw) };
    let key = CFString::from_static_string("CGSSessionScreenIsLocked");
    if let Some(value) = session.find(key.as_CFTypeRef()) {
        let value = unsafe { core_foundation::base::CFType::wrap_under_get_rule(*value) };
        let locked = value
            .downcast::<CFBoolean>()
            .map(bool::from)
            .or_else(|| {
                value
                    .downcast::<core_foundation::number::CFNumber>()
                    .and_then(|n| n.to_i64())
                    .map(|n| n != 0)
            })
            .unwrap_or(true);
        assert!(!locked, "unlock the Mac before running this visual test");
    }
    let indexes: Vec<u8> = std::env::args()
        .skip(1)
        .map(|arg| arg.parse().expect("expected a desktop number"))
        .collect();
    let Some(&original) = indexes.last() else {
        eprintln!(
            "Usage: cargo run --example verify_workspace_transition -- <destination>... <starting desktop>"
        );
        std::process::exit(1);
    };
    struct Restore(u8, bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = spaces::activate_space_impl(self.0, self.1);
        }
    }
    let _restore = Restore(original, !bare);
    // Exercise early error cleanup while the process remains alive, so process
    // exit cannot hide a leaked panel from this check.
    let failed: Result<(), &str> = (|| {
        if bare {
            return Err("skip cover for baseline");
        }
        let _cover =
            transition_cover::TransitionCover::begin().expect("transition snapshot unavailable");
        Err("simulated backend failure")
    })();
    assert!(failed.is_err());
    assert_no_cover_windows();
    for index in indexes {
        println!(
            "Space {index}: {:?}",
            spaces::activate_space_impl(index, !bare).expect("desktop switch failed")
        );
        assert_no_cover_windows();
        if std::env::var_os("FTS_TEST_SCREENSHOTS").is_some() {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let path = format!("/tmp/fts-covered-space-{index}.png");
            assert!(
                std::process::Command::new("/usr/sbin/screencapture")
                    .args(["-x", &path])
                    .status()
                    .expect("capture destination")
                    .success()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This visual check requires macOS.");
}
