#![allow(deprecated, unexpected_cfgs)]

use cocoa::{
    appkit::{
        NSApplication, NSApplicationActivationPolicyRegular, NSBackingStoreBuffered, NSMenu,
        NSMenuItem, NSTextField, NSView, NSWindow, NSWindowStyleMask,
    },
    base::{BOOL, NO, YES, id, nil},
    foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString},
};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Object, Sel},
    sel, sel_impl,
};
use std::cell::RefCell;

struct TickContext<'a>(RefCell<Box<dyn FnMut() + 'a>>);

/// Owns AppKit independently of the optional switcher overlay.
pub struct App {
    app: id,
    delegate: id,
    status_label: id,
    _pool: id,
}

/// Drain temporary AppKit objects after each pass through the long-running loop.
pub struct EventPool(id);

impl EventPool {
    pub fn new() -> Self {
        Self(unsafe { NSAutoreleasePool::new(nil) })
    }
}

impl Drop for EventPool {
    fn drop(&mut self) {
        unsafe { self.0.drain() }
    }
}

impl App {
    pub fn new(background: bool) -> Self {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let app = NSApplication::sharedApplication(nil);
            app.setActivationPolicy_(NSApplicationActivationPolicyRegular);

            let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(520.0, 300.0)),
                NSWindowStyleMask::NSTitledWindowMask
                    | NSWindowStyleMask::NSClosableWindowMask
                    | NSWindowStyleMask::NSMiniaturizableWindowMask,
                NSBackingStoreBuffered,
                NO,
            );
            window.setTitle_(string("Fast True Switcher"));
            window.setReleasedWhenClosed_(NO);
            window.center();

            let content = window.contentView();
            let status_label = text(content, 248.0, 28.0, "Starting keyboard listener…", 18.0);
            text(
                content,
                166.0,
                66.0,
                "Control + Space: cycle windows\nShift + Space: cycle backward · Escape: cancel\nOption + 1–9: switch desktop",
                14.0,
            );
            text(
                content,
                62.0,
                88.0,
                "Allow Fast True Switcher in System Settings → Privacy & Security:\nAccessibility, Input Monitoring, and Screen Recording.\nAfter granting permissions, quit and reopen the app.",
                13.0,
            );
            text(
                content,
                20.0,
                28.0,
                "Close this window to keep running. Use ⌘Q to quit.",
                13.0,
            );

            let mut declaration = ClassDecl::new("FTSAppDelegate", class!(NSObject)).unwrap();
            declaration.add_ivar::<id>("statusWindow");
            declaration.add_ivar::<BOOL>("showOnLaunch");
            declaration.add_ivar::<*mut std::ffi::c_void>("tickContext");
            declaration.add_method(sel!(tick:), tick as extern "C" fn(&Object, Sel, id));
            declaration.add_method(
                sel!(applicationShouldHandleReopen:hasVisibleWindows:),
                reopen as extern "C" fn(&Object, Sel, id, BOOL) -> BOOL,
            );
            declaration.add_method(
                sel!(applicationShouldTerminateAfterLastWindowClosed:),
                keep_running as extern "C" fn(&Object, Sel, id) -> BOOL,
            );
            declaration.add_method(
                sel!(applicationShouldOpenUntitledFile:),
                open_untitled as extern "C" fn(&Object, Sel, id) -> BOOL,
            );
            declaration.add_method(
                sel!(showStatus:),
                show_status as extern "C" fn(&Object, Sel, id),
            );
            let delegate: id = msg_send![declaration.register(), new];
            (*delegate).set_ivar("statusWindow", window);
            (*delegate).set_ivar("showOnLaunch", if background { NO } else { YES });
            let _: () = msg_send![app, setDelegate: delegate];

            let menu = NSMenu::new(nil);
            let root = NSMenuItem::new(nil);
            menu.addItem_(root);
            let app_menu = NSMenu::new(nil);
            root.setSubmenu_(app_menu);
            let status = app_menu.addItemWithTitle_action_keyEquivalent(
                string("Show Status"),
                sel!(showStatus:),
                string(","),
            );
            status.setTarget_(delegate);
            app_menu.addItem_(NSMenuItem::separatorItem(nil));
            app_menu.addItemWithTitle_action_keyEquivalent(
                string("Hide Fast True Switcher"),
                sel!(hide:),
                string("h"),
            );
            app_menu.addItem_(NSMenuItem::separatorItem(nil));
            let quit = app_menu.addItemWithTitle_action_keyEquivalent(
                string("Quit Fast True Switcher"),
                sel!(terminate:),
                string("q"),
            );
            quit.setTarget_(app);
            app.setMainMenu_(menu);
            app.finishLaunching();

            Self {
                app,
                delegate,
                status_label,
                _pool: pool,
            }
        }
    }

    pub fn set_status(&self, status: &str) {
        unsafe {
            self.status_label.setStringValue_(string(status));
        }
    }

    /// Let AppKit dispatch Dock AppleEvents, menus, and termination normally.
    /// The callback stays on the main thread and lives until `run` returns.
    pub fn run(&self, callback: impl FnMut()) {
        let mut context = TickContext(RefCell::new(Box::new(callback)));
        unsafe {
            (*self.delegate).set_ivar(
                "tickContext",
                (&mut context as *mut TickContext<'_>).cast::<std::ffi::c_void>(),
            );
            let timer: id = msg_send![class!(NSTimer),
                scheduledTimerWithTimeInterval: (1.0 / 60.0)
                target: self.delegate
                selector: sel!(tick:)
                userInfo: nil
                repeats: YES
            ];
            self.app.run();
            let _: () = msg_send![timer, invalidate];
            (*self.delegate).set_ivar("tickContext", std::ptr::null_mut::<std::ffi::c_void>());
        }
    }
}

extern "C" fn tick(this: &Object, _selector: Sel, _timer: id) {
    unsafe {
        let context = *this.get_ivar::<*mut std::ffi::c_void>("tickContext");
        if context.is_null() {
            return;
        }
        let context = &*context.cast::<TickContext<'_>>();
        // Some native calls enter nested run loops. Never reenter the Rust callback.
        let Ok(mut callback) = context.0.try_borrow_mut() else {
            return;
        };
        let _pool = EventPool::new();
        callback();
    }
}

extern "C" fn reopen(this: &Object, _selector: Sel, app: id, _visible: BOOL) -> BOOL {
    show_status(this, sel!(showStatus:), app);
    NO
}

extern "C" fn keep_running(_this: &Object, _selector: Sel, _app: id) -> BOOL {
    NO
}

extern "C" fn open_untitled(this: &Object, _selector: Sel, app: id) -> BOOL {
    if unsafe { *this.get_ivar::<BOOL>("showOnLaunch") } != NO {
        show_status(this, sel!(showStatus:), app);
    }
    NO
}

extern "C" fn show_status(this: &Object, _selector: Sel, _sender: id) {
    unsafe {
        let window: id = *this.get_ivar("statusWindow");
        let app = NSApplication::sharedApplication(nil);
        app.activateIgnoringOtherApps_(YES);
        window.makeKeyAndOrderFront_(nil);
    }
}

fn string(value: &str) -> id {
    unsafe { NSString::alloc(nil).init_str(value).autorelease() }
}

fn text(content: id, y: f64, height: f64, value: &str, size: f64) -> id {
    unsafe {
        let label = NSTextField::initWithFrame_(
            NSTextField::alloc(nil),
            NSRect::new(NSPoint::new(24.0, y), NSSize::new(472.0, height)),
        );
        label.setEditable_(NO);
        let _: () = msg_send![label, setBezeled: NO];
        let _: () = msg_send![label, setDrawsBackground: NO];
        let font: id = msg_send![class!(NSFont), systemFontOfSize: size];
        let _: () = msg_send![label, setFont: font];
        label.setStringValue_(string(value));
        content.addSubview_(label);
        label
    }
}
