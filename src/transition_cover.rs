#![allow(deprecated, unexpected_cfgs)]

//! Keep the source desktop stationary while Dock completes a normal transition.
//! Snapshots stay in memory and are discarded when the switch completes or fails.

use cocoa::{
    appkit::{
        NSApplication, NSApplicationActivationPolicyAccessory, NSBackingStoreBuffered, NSColor,
        NSEvent, NSEventMask, NSEventType, NSImageView, NSScreen, NSView, NSWindow,
        NSWindowCollectionBehavior,
    },
    base::{NO, YES, id, nil},
    foundation::{NSDate, NSDefaultRunLoopMode, NSPoint, NSRect},
};
use core_foundation::{
    base::{CFRelease, TCFType},
    string::CFString,
};
use core_graphics::display::CGDisplayCreateImage;
use objc::{class, msg_send, rc::autoreleasepool, sel, sel_impl};
use std::{thread, time::Duration};

pub struct TransitionCover {
    app: id,
    panels: Vec<id>,
    visible: bool,
}

impl TransitionCover {
    pub fn begin() -> Result<Self, &'static str> {
        autoreleasepool(|| unsafe {
            let main_thread: bool = msg_send![class!(NSThread), isMainThread];
            if !main_thread {
                return Err("transition snapshots require the main thread");
            }
            // Resolve dynamically so older macOS versions without the screen
            // recording privacy API can still use their native capture API.
            let check = libc::dlsym(
                libc::RTLD_DEFAULT,
                c"CGPreflightScreenCaptureAccess".as_ptr(),
            );
            if !check.is_null() {
                let allowed: unsafe extern "C" fn() -> bool = std::mem::transmute(check);
                if !allowed() {
                    return Err("Screen Recording access is needed to hide workspace animations");
                }
            }
            let app = NSApplication::sharedApplication(nil);
            let policy: isize = msg_send![app, activationPolicy];
            if policy != NSApplicationActivationPolicyAccessory as isize {
                app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
                app.finishLaunching();
            }
            let mut cover = Self {
                app,
                panels: Vec::new(),
                visible: false,
            };
            let screens: id = NSScreen::screens(nil);
            let count: usize = msg_send![screens, count];
            if count == 0 {
                return Err("no screens are available for the transition snapshot");
            }
            let number_key = CFString::from_static_string("NSScreenNumber");
            // Prepare every display before showing any panel, including on
            // multi-monitor setups where Mission Control opens on all screens.
            for index in 0..count {
                let screen: id = msg_send![screens, objectAtIndex: index];
                let frame = NSScreen::frame(screen);
                let status_bar: id = msg_send![class!(NSStatusBar), systemStatusBar];
                let menu_height: f64 = msg_send![status_bar, thickness];
                let visible = NSScreen::visibleFrame(screen);
                let top_inset =
                    (frame.origin.y + frame.size.height - visible.origin.y - visible.size.height)
                        .max(menu_height);
                let mut covered_frame = frame;
                covered_frame.size.height = (frame.size.height - top_inset).max(1.0);
                let description: id = msg_send![screen, deviceDescription];
                let number: id =
                    msg_send![description, objectForKey: number_key.as_CFTypeRef() as id];
                if number == nil {
                    return Err("could not identify the display");
                }
                let display_id: u32 = msg_send![number, unsignedIntValue];
                let captured = CGDisplayCreateImage(display_id);
                if captured.is_null() {
                    return Err("could not capture the source desktop");
                }
                let image: id = msg_send![class!(NSImage), alloc];
                let image: id = msg_send![image, initWithCGImage: captured size: frame.size];
                CFRelease(captured as *const _);
                if image == nil {
                    return Err("could not create the transition image");
                }
                let panel: id = msg_send![class!(NSPanel), alloc];
                // Borderless, nonactivating panel: it cannot steal keyboard
                // focus, and absorbs mouse clicks while its snapshot is shown.
                let panel: id = msg_send![panel,
                    initWithContentRect: covered_frame styleMask: 128usize
                    backing: NSBackingStoreBuffered defer: NO];
                if panel == nil {
                    let _: () = msg_send![image, release];
                    return Err("could not create the transition panel");
                }
                cover.panels.push(panel);
                panel.setReleasedWhenClosed_(NO);
                // Leave normal desktop compositing enabled underneath.
                panel.setOpaque_(NO);
                panel.setBackgroundColor_(NSColor::clearColor(nil));
                panel.setHasShadow_(NO);
                panel.setHidesOnDeactivate_(NO);
                panel.setCanHide_(NO);
                panel.setIgnoresMouseEvents_(NO);
                panel.setLevel_(1000); // screen-saver level, above Mission Control
                let _: () = msg_send![panel, setAnimationBehavior: 2isize]; // none
                let _: () = msg_send![panel, setBecomesKeyOnlyIfNeeded: YES];
                panel.setCollectionBehavior_(
                    NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
                        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle
                        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary,
                );
                // Clip the full-size snapshot rather than rescaling it: every
                // desktop pixel stays in place, and the live menu bar is never
                // occluded by our window.
                let image_frame = NSRect::new(NSPoint::new(0.0, 0.0), frame.size);
                let container_frame = NSRect::new(NSPoint::new(0.0, 0.0), covered_frame.size);
                let container = NSView::initWithFrame_(NSView::alloc(nil), container_frame);
                let view = NSImageView::initWithFrame_(NSImageView::alloc(nil), image_frame);
                view.setImage_(image);
                let _: () = msg_send![view, setImageScaling: 1isize]; // scale axes independently
                container.addSubview_(view);
                panel.setContentView_(container);
                let _: () = msg_send![container, release];
                let _: () = msg_send![view, release];
                let _: () = msg_send![image, release];
            }
            for panel in &cover.panels {
                panel.orderFrontRegardless();
                let _: () = msg_send![*panel, display];
            }
            let _: () = msg_send![app, updateWindows];
            cover.visible = true;
            cover.pump_events();
            // Ensure the stationary panels have reached WindowServer before
            // Dock is asked to animate the desktops underneath them.
            thread::sleep(Duration::from_millis(20));
            Ok(cover)
        })
    }

    fn pump_events(&self) {
        unsafe {
            loop {
                let event = self.app.nextEventMatchingMask_untilDate_inMode_dequeue_(
                    NSEventMask::NSAnyEventMask.bits(),
                    NSDate::distantPast(nil),
                    NSDefaultRunLoopMode,
                    YES,
                );
                if event == nil {
                    break;
                }
                match event.eventType() {
                    NSEventType::NSKeyDown | NSEventType::NSKeyUp | NSEventType::NSFlagsChanged => {
                    }
                    _ => self.app.sendEvent_(event),
                }
            }
        }
    }
}

impl Drop for TransitionCover {
    fn drop(&mut self) {
        // The current-Space metadata and AX overview can settle before the
        // final display frame. Keep the last bit of that animation covered too.
        if self.visible && !thread::panicking() {
            thread::sleep(Duration::from_millis(200));
        }
        unsafe {
            for panel in &self.panels {
                panel.orderOut_(nil);
            }
            if self.visible {
                self.pump_events();
            }
            for panel in self.panels.drain(..) {
                panel.close();
                let _: () = msg_send![panel, release];
            }
        }
    }
}
