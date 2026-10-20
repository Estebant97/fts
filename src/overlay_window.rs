#![allow(deprecated, unexpected_cfgs)]

use cocoa::{
    appkit::{
        NSApplication, NSApplicationActivationPolicyAccessory, NSBackingStoreBuffered, NSColor,
        NSEvent, NSEventMask, NSEventType, NSMainMenuWindowLevel, NSScreen, NSTextField, NSView,
        NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
    },
    base::{NO, YES, id, nil},
    foundation::{
        NSAutoreleasePool, NSDate, NSDefaultRunLoopMode, NSPoint, NSRect, NSSize, NSString,
    },
};
use objc::{class, msg_send, sel, sel_impl};

use crate::Window;

pub enum OverlayWindow {
    Enabled(OverlayState),
    Disabled,
}

const PANEL_WIDTH: f64 = 420.0;
const ROW_HEIGHT: f64 = 28.0;
const HEADER_HEIGHT: f64 = 36.0;
const LABEL_MARGIN: f64 = 18.0;
const MAX_ROWS: usize = 7;

pub struct OverlayState {
    app: id,
    panel: id,
    content_view: id,
    header_label: id,
    row_labels: Vec<id>,
}

impl OverlayWindow {
    pub fn new() -> Self {
        if std::env::var("FTS_DISABLE_OVERLAY").ok().as_deref() == Some("1") {
            return Self::Disabled;
        }

        Self::Enabled(OverlayState::new())
    }

    pub fn show_selection(&self, position: usize, total_windows: usize, entries: &[(usize, Window, bool)]) {
        if let Self::Enabled(overlay) = self {
            overlay.show_selection(position, total_windows, entries);
        }
    }

    pub fn hide(&self) {
        if let Self::Enabled(overlay) = self {
            overlay.hide();
        }
    }

    pub fn pump_events(&self) {
        if let Self::Enabled(overlay) = self {
            overlay.pump_events();
        }
    }
}

impl OverlayState {
    fn new() -> Self {
        unsafe {
            let app = NSApplication::sharedApplication(nil);
            app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
            app.finishLaunching();

            let height = HEADER_HEIGHT + (MAX_ROWS as f64) * ROW_HEIGHT + LABEL_MARGIN;
            let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, height));
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
                nil, 0.08, 0.09, 0.11, 0.94,
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

            let header_label = label(
                NSRect::new(
                    NSPoint::new(LABEL_MARGIN, height - LABEL_MARGIN - 22.0),
                    NSSize::new(PANEL_WIDTH - (LABEL_MARGIN * 2.0), 22.0),
                ),
                13.0,
                true,
                false,
            );
            content_view.addSubview_(header_label);

            let mut row_labels = Vec::with_capacity(MAX_ROWS);
            for index in 0..MAX_ROWS {
                let y = height - LABEL_MARGIN - HEADER_HEIGHT - ((index as f64 + 1.0) * ROW_HEIGHT);
                let row = label(
                    NSRect::new(
                        NSPoint::new(LABEL_MARGIN, y),
                        NSSize::new(PANEL_WIDTH - (LABEL_MARGIN * 2.0), ROW_HEIGHT - 4.0),
                    ),
                    14.0,
                    false,
                    true,
                );
                content_view.addSubview_(row);
                row_labels.push(row);
            }

            Self {
                app,
                panel,
                content_view,
                header_label,
                row_labels,
            }
        }
    }

    fn show_selection(
        &self,
        position: usize,
        total_windows: usize,
        entries: &[(usize, Window, bool)],
    ) {
        self.set_header(&format!("Switching  {position}/{total_windows}"));

        for (slot, row) in self.row_labels.iter().enumerate() {
            if let Some((_, window, selected)) = entries.get(slot) {
                let prefix = if *selected { "▸ " } else { "  " };
                let text = format!(
                    "{prefix}{} — {}",
                    window.app_name,
                    truncate(&window.window_name, 48)
                );
                set_label_text(*row, &text);
                set_label_emphasis(*row, *selected);
            } else {
                set_label_text(*row, "");
            }
        }

        self.show();
    }

    fn hide(&self) {
        unsafe {
            self.panel.orderOut_(nil);
            self.pump_events();
        }
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
                    NSEventType::NSKeyDown | NSEventType::NSKeyUp | NSEventType::NSFlagsChanged => {}
                    _ => self.app.sendEvent_(event),
                }
            }
        }
    }

    fn show(&self) {
        unsafe {
            position_on_active_screen(self.panel);
            self.panel.orderFrontRegardless();
            self.refresh();
            self.pump_events();
        }
    }

    fn set_header(&self, text: &str) {
        set_label_text(self.header_label, text);
    }

    fn refresh(&self) {
        unsafe {
            self.header_label.display_();
            for row in &self.row_labels {
                row.display_();
            }
            self.content_view.display_();
            let _: () = msg_send![self.panel, display];
            let _: () = msg_send![self.panel, update];
        }
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let trimmed: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{trimmed}…")
}

fn set_label_text(label: id, text: &str) {
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        label.setStringValue_(NSString::alloc(nil).init_str(text));
        pool.drain();
    }
}

fn set_label_emphasis(label: id, emphasized: bool) {
    unsafe {
        let font: id = if emphasized {
            msg_send![class!(NSFont), boldSystemFontOfSize: 14.0]
        } else {
            msg_send![class!(NSFont), systemFontOfSize: 14.0]
        };
        let color = if emphasized {
            NSColor::colorWithSRGBRed_green_blue_alpha_(nil, 0.95, 0.97, 1.0, 1.0)
        } else {
            NSColor::colorWithSRGBRed_green_blue_alpha_(nil, 0.68, 0.74, 0.82, 1.0)
        };
        let _: () = msg_send![label, setFont: font];
        let _: () = msg_send![label, setTextColor: color];
    }
}

fn position_on_active_screen(panel: id) {
    unsafe {
        let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        let screens: id = NSScreen::screens(nil);
        let count: usize = msg_send![screens, count];
        let mut target: id = NSScreen::mainScreen(nil);

        for index in 0..count {
            let screen: id = msg_send![screens, objectAtIndex: index];
            let frame = screen.frame();
            let contains = mouse.x >= frame.origin.x
                && mouse.x <= frame.origin.x + frame.size.width
                && mouse.y >= frame.origin.y
                && mouse.y <= frame.origin.y + frame.size.height;
            if contains {
                target = screen;
                break;
            }
        }

        if target == nil {
            panel.center();
            return;
        }

        let visible = target.visibleFrame();
        let panel_frame = panel.frame();
        let x = visible.origin.x + (visible.size.width - panel_frame.size.width) / 2.0;
        let y = visible.origin.y + (visible.size.height - panel_frame.size.height) * 0.62;
        panel.setFrameOrigin_(NSPoint::new(x, y));
    }
}

fn label(frame: NSRect, font_size: f64, emphasized: bool, left_align: bool) -> id {
    unsafe {
        let label = NSTextField::initWithFrame_(NSTextField::alloc(nil), frame);
        label.setEditable_(NO);
        let _: () = msg_send![label, setBezeled: NO];
        let _: () = msg_send![label, setBordered: NO];
        let _: () = msg_send![label, setSelectable: NO];
        let _: () = msg_send![label, setDrawsBackground: NO];
        let alignment: i64 = if left_align { 0 } else { 1 };
        let _: () = msg_send![label, setAlignment: alignment];

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
