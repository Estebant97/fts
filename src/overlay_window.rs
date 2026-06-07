#![allow(deprecated, unexpected_cfgs)]

use cocoa::{
    appkit::{
        NSApplication, NSApplicationActivationPolicyAccessory, NSBackingStoreBuffered, NSColor,
        NSEvent, NSEventMask, NSEventType, NSMainMenuWindowLevel, NSTextField, NSView, NSWindow,
        NSWindowCollectionBehavior, NSWindowStyleMask,
    },
    base::{NO, YES, id, nil},
    foundation::{
        NSAutoreleasePool, NSDate, NSDefaultRunLoopMode, NSPoint, NSRect, NSSize, NSString,
    },
};
use objc::{class, msg_send, sel, sel_impl};

use crate::Window;

const PANEL_WIDTH: f64 = 560.0;
const PANEL_HEIGHT: f64 = 148.0;
const LABEL_MARGIN: f64 = 24.0;

pub struct OverlayWindow {
    app: id,
    panel: id,
    content_view: id,
    title_label: id,
    detail_label: id,
}

impl OverlayWindow {
    pub fn new() -> Self {
        unsafe {
            let app = NSApplication::sharedApplication(nil);
            app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
            app.finishLaunching();

            let frame = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(PANEL_WIDTH, PANEL_HEIGHT),
            );
            let style = NSWindowStyleMask::NSBorderlessWindowMask;
            let panel = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
                frame,
                style,
                NSBackingStoreBuffered,
                NO,
            );

            panel.setReleasedWhenClosed_(NO);
            panel.setCanHide_(NO);
            panel.setHidesOnDeactivate_(NO);
            panel.setOpaque_(NO);
            panel.setHasShadow_(YES);
            panel.setBackgroundColor_(NSColor::colorWithSRGBRed_green_blue_alpha_(
                nil, 0.08, 0.09, 0.11, 0.92,
            ));
            panel.setLevel_((NSMainMenuWindowLevel + 2).into());
            panel.setIgnoresMouseEvents_(YES);
            panel.setCollectionBehavior_(
                NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                    | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle
                    | NSWindowCollectionBehavior::NSWindowCollectionBehaviorTransient,
            );

            let content_view = NSView::initWithFrame_(NSView::alloc(nil), frame);
            panel.setContentView_(content_view);

            let title_label = label(
                NSRect::new(
                    NSPoint::new(LABEL_MARGIN, 72.0),
                    NSSize::new(PANEL_WIDTH - (LABEL_MARGIN * 2.0), 42.0),
                ),
                28.0,
                true,
            );
            let detail_label = label(
                NSRect::new(
                    NSPoint::new(LABEL_MARGIN, 34.0),
                    NSSize::new(PANEL_WIDTH - (LABEL_MARGIN * 2.0), 28.0),
                ),
                16.0,
                false,
            );

            content_view.addSubview_(title_label);
            content_view.addSubview_(detail_label);
            panel.center();

            Self {
                app,
                panel,
                content_view,
                title_label,
                detail_label,
            }
        }
    }

    pub fn show_waiting(&self, total_windows: usize) {
        self.set_text(
            "Switching Windows",
            &format!("Ctrl is held. Press Space to cycle through {total_windows} windows."),
        );
        self.show();
    }

    pub fn show_selection(&self, position: usize, total_windows: usize, window: &Window) {
        self.set_text(
            &window.app_name,
            &format!("{position}/{total_windows}  {}", window.window_name),
        );
        self.show();
    }

    pub fn hide(&self) {
        unsafe {
            self.panel.orderOut_(nil);
            self.pump_events();
        }
    }

    pub fn pump_events(&self) {
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

    fn show(&self) {
        unsafe {
            self.app.activateIgnoringOtherApps_(YES);
            self.panel.center();
            self.panel.orderFrontRegardless();
            self.refresh();
            self.pump_events();
        }
    }

    fn set_text(&self, title: &str, detail: &str) {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            self.title_label
                .setStringValue_(NSString::alloc(nil).init_str(title));
            self.detail_label
                .setStringValue_(NSString::alloc(nil).init_str(detail));
            pool.drain();
        }
    }

    fn refresh(&self) {
        unsafe {
            self.title_label.display_();
            self.detail_label.display_();
            self.content_view.display_();
            let _: () = msg_send![self.panel, display];
            let _: () = msg_send![self.panel, update];
        }
    }
}

fn label(frame: NSRect, font_size: f64, emphasized: bool) -> id {
    unsafe {
        let label = NSTextField::initWithFrame_(NSTextField::alloc(nil), frame);
        label.setEditable_(NO);
        let _: () = msg_send![label, setBezeled: NO];
        let _: () = msg_send![label, setBordered: NO];
        let _: () = msg_send![label, setSelectable: NO];
        let _: () = msg_send![label, setDrawsBackground: NO];
        let _: () = msg_send![label, setAlignment: 1_i64];

        let font: id = if emphasized {
            msg_send![class!(NSFont), boldSystemFontOfSize: font_size]
        } else {
            msg_send![class!(NSFont), systemFontOfSize: font_size]
        };
        let color = if emphasized {
            NSColor::colorWithSRGBRed_green_blue_alpha_(nil, 0.95, 0.97, 1.0, 1.0)
        } else {
            NSColor::colorWithSRGBRed_green_blue_alpha_(nil, 0.68, 0.74, 0.82, 1.0)
        };

        let _: () = msg_send![label, setFont: font];
        let _: () = msg_send![label, setTextColor: color];

        label
    }
}
