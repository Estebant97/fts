use accessibility::{AXAttribute, AXUIElement};
use accessibility_sys::AXUIElementGetPid;
use core_foundation::{base::TCFType, string::CFString};
use std::collections::VecDeque;

use crate::{Window, active_windows::get_active_windows};

pub struct WindowTracker {
    mru_windows: VecDeque<Window>,
    selected_index: usize,
    is_switching: bool,
    did_advance_selection: bool,
}

impl WindowTracker {
    pub fn new() -> Self {
        let mut tracker = Self {
            mru_windows: VecDeque::new(),
            selected_index: 0,
            is_switching: false,
            did_advance_selection: false,
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
        self.is_switching = true;
        self.selected_index = 0;
        self.did_advance_selection = false;
    }

    pub fn advance_selection(&mut self) -> Option<&Window> {
        if self.mru_windows.len() < 2 {
            return None;
        }

        self.selected_index = (self.selected_index + 1) % self.mru_windows.len();
        self.did_advance_selection = true;
        self.mru_windows.get(self.selected_index)
    }

    pub fn selected_window(&self) -> Option<Window> {
        if !self.did_advance_selection {
            return None;
        }

        self.mru_windows.get(self.selected_index).cloned()
    }

    pub fn finish_switching(&mut self) {
        self.is_switching = false;
        self.did_advance_selection = false;
        if self.selected_index == 0 {
            return;
        }

        if let Some(window) = self.mru_windows.remove(self.selected_index) {
            self.mru_windows.push_front(window);
        }

        self.selected_index = 0;
    }

    pub fn observe_focused_window(&mut self) {
        if self.is_switching {
            return;
        }

        let Some(focused_window) = focused_window() else {
            return;
        };

        self.move_focused_to_front(focused_window);
    }

    fn move_focused_to_front(&mut self, focused_window: AXUIElement) {
        if self
            .mru_windows
            .front()
            .is_some_and(|window| window.same_element(&focused_window))
        {
            return;
        }

        if let Some(index) = self
            .mru_windows
            .iter()
            .position(|window| window.same_element(&focused_window))
        {
            if let Some(window) = self.mru_windows.remove(index) {
                self.mru_windows.push_front(window);
            }
            return;
        }

        let pid = element_pid(&focused_window).unwrap_or_default();
        let title = window_title(&focused_window).unwrap_or_else(|| "Untitled Window".to_string());
        let app_name = self
            .mru_windows
            .iter()
            .find(|window| window.pid == pid)
            .map(|window| window.app_name.clone())
            .unwrap_or_else(|| format!("PID {}", pid));

        self.mru_windows
            .push_front(Window::new(pid, app_name, title, 1, focused_window));
    }

    fn push_back_unique(&mut self, window: Window) {
        if self
            .mru_windows
            .iter()
            .any(|known_window| known_window.same_element(&window.element))
        {
            return;
        }

        self.mru_windows.push_back(window);
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
