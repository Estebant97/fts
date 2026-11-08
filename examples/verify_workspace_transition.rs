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
fn main() {
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
    use cocoa::{
        appkit::{NSApplication, NSApplicationActivationPolicyAccessory},
        base::nil,
    };
    unsafe {
        let app = NSApplication::sharedApplication(nil);
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
        app.finishLaunching();
    }
    struct Restore(u8);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Err(error) = spaces::activate_space(self.0) {
                eprintln!("Could not restore initial desktop: {error}");
            }
        }
    }
    let _restore = Restore(original);
    for index in indexes {
        println!(
            "Space {index}: {:?}",
            spaces::activate_space(index).expect("desktop switch failed")
        );
        if std::env::var_os("FTS_TEST_SCREENSHOTS").is_some() {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let path = format!("/tmp/fts-space-{index}.png");
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
