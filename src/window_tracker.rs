use accessibility::{AXAttribute, AXUIElement};
use accessibility_sys::AXUIElementGetPid;
use core_foundation::{base::TCFType, string::CFString};

use crate::{Window, active_windows::get_active_windows, mru_list::MruList};

pub struct WindowTracker {
    mru_windows: MruList<Window>,
}

impl WindowTracker {
    pub fn new() -> Self {
        let mut tracker = Self {
            mru_windows: MruList::new(),
        };

        for window in get_active_windows() {
            tracker.push_back_unique(window);
        }

        tracker.observe_focused_window();
        tracker
    }

    pub fn len(&self) -> usize {
        self.mru_windows.len()
    }

    pub fn windows(&self) -> impl Iterator<Item = &Window> {
        self.mru_windows.iter()
    }

    pub fn begin_switching(&mut self) {
        self.mru_windows.begin_switching();
    }

    pub fn advance_selection(&mut self) -> Option<&Window> {
        self.mru_windows.advance_selection()
    }

    pub fn selected_window(&self) -> Option<Window> {
        self.mru_windows.selected_item().cloned()
    }

    pub fn selected_position(&self) -> Option<usize> {
        self.mru_windows.selected_position()
    }

    pub fn finish_switching(&mut self) {
        self.mru_windows.finish_switching();
    }

    pub fn observe_focused_window(&mut self) {
        let Some(focused_window) = focused_window() else {
            return;
        };

        self.move_focused_to_front(focused_window);
    }

    fn move_focused_to_front(&mut self, focused_window: AXUIElement) {
        let pid = element_pid(&focused_window).unwrap_or_default();
        let app_name = self
            .mru_windows
            .iter()
            .find(|window| window.pid == pid)
            .map(|window| window.app_name.clone())
            .unwrap_or_else(|| format!("PID {}", pid));
        let title = window_title(&focused_window).unwrap_or_else(|| "Untitled Window".to_string());
        let focused_window = Window::new(pid, app_name, title, 1, focused_window);

        self.mru_windows
            .observe_front_by(focused_window, |known_window, focused_window| {
                known_window.same_element(&focused_window.element)
            });
    }

    fn push_back_unique(&mut self, window: Window) {
        self.mru_windows
            .push_back_unique_by(window, |known_window, window| {
                known_window.same_element(&window.element)
            });
    }
}

fn focused_window() -> Option<AXUIElement> {
    AXUIElement::system_wide()
        .attribute(&AXAttribute::focused_window())
        .ok()
}

fn element_pid(element: &AXUIElement) -> Option<i32> {
    let mut pid = 0;
    let result = unsafe { AXUIElementGetPid(element.as_concrete_TypeRef(), &mut pid) };

    if result == 0 { Some(pid) } else { None }
}

fn window_title(element: &AXUIElement) -> Option<String> {
    element
        .attribute(&AXAttribute::title())
        .ok()
        .map(|title: CFString| title.to_string())
        .filter(|title| !title.is_empty())
}
