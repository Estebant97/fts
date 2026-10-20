use accessibility::{AXAttribute, AXUIElement};
use accessibility_sys::AXUIElementGetPid;
use core_foundation::{base::TCFType, string::CFString};
use std::time::{Duration, Instant};

use crate::{Window, active_windows::get_active_windows, mru_list::MruList};

const SETTLE_DURATION: Duration = Duration::from_millis(350);
const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

pub struct WindowTracker {
    mru_windows: MruList<Window>,
    settle_until: Option<Instant>,
    settle_target: Option<(i32, Option<u32>, usize)>,
    last_refresh: Instant,
}

impl WindowTracker {
    pub fn new() -> Self {
        let mut tracker = Self {
            mru_windows: MruList::new(),
            settle_until: None,
            settle_target: None,
            last_refresh: Instant::now(),
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

    pub fn is_empty(&self) -> bool {
        self.mru_windows.is_empty()
    }

    pub fn is_switching(&self) -> bool {
        self.mru_windows.is_switching()
    }

    pub fn windows(&self) -> impl Iterator<Item = &Window> {
        self.mru_windows.iter()
    }

    pub fn begin_switching(&mut self) {
        self.refresh_windows_now();
        self.mru_windows.begin_switching();
    }

    pub fn advance_selection(&mut self) -> Option<&Window> {
        self.mru_windows.advance_selection()
    }

    pub fn reverse_selection(&mut self) -> Option<&Window> {
        self.mru_windows.reverse_selection()
    }

    pub fn selected_window(&self) -> Option<Window> {
        self.mru_windows.selected_item().cloned()
    }

    pub fn selected_position(&self) -> Option<usize> {
        self.mru_windows.selected_position()
    }

    /// Snapshot of windows around the selection for the overlay list.
    pub fn overlay_entries(&self, radius: usize) -> Vec<(usize, Window, bool)> {
        let len = self.mru_windows.len();
        if len == 0 {
            return Vec::new();
        }

        let selected = self.selected_position().unwrap_or(0);
        let start = selected.saturating_sub(radius);
        let end = (selected + radius + 1).min(len);

        (start..end)
            .filter_map(|index| {
                self.mru_windows
                    .get(index)
                    .cloned()
                    .map(|window| (index, window, index == selected))
            })
            .collect()
    }

    pub fn finish_switching(&mut self) {
        if let Some(window) = self.selected_window() {
            self.settle_target = Some((window.pid, window.cg_window_id, window.id.ax_ref));
            self.settle_until = Some(Instant::now() + SETTLE_DURATION);
        }
        self.mru_windows.finish_switching();
    }

    pub fn cancel_switching(&mut self) {
        self.mru_windows.cancel_switching();
    }

    pub fn observe_focused_window(&mut self) {
        if self.is_settling() {
            if let Some(focused) = focused_window() {
                if self.matches_settle_target(&focused) {
                    self.clear_settle();
                } else {
                    return;
                }
            } else {
                return;
            }
        }

        let Some(focused_window) = focused_window() else {
            return;
        };

        self.move_focused_to_front(focused_window);
    }

    pub fn maybe_refresh_windows(&mut self) {
        if self.mru_windows.is_switching() {
            return;
        }
        if self.last_refresh.elapsed() < REFRESH_INTERVAL {
            return;
        }
        self.refresh_windows_now();
    }

    fn refresh_windows_now(&mut self) {
        self.last_refresh = Instant::now();
        let fresh = get_active_windows();
        if fresh.is_empty() {
            return;
        }

        self.mru_windows.retain_by(|known| {
            fresh.iter().any(|window| known.same_window(window))
        });

        for window in &fresh {
            self.mru_windows
                .update_matching_by(|known| known.same_window(window), |known| {
                    known.window_name = window.window_name.clone();
                    known.app_name = window.app_name.clone();
                    known.onscreen = window.onscreen;
                    known.element = window.element.clone();
                    known.is_app_fallback = window.is_app_fallback;
                    known.cg_window_id = window.cg_window_id;
                    known.id = window.id.clone();
                });
        }

        for window in fresh {
            self.push_back_unique(window);
        }
    }

    fn is_settling(&self) -> bool {
        self.settle_until
            .is_some_and(|until| Instant::now() < until)
    }

    fn clear_settle(&mut self) {
        self.settle_until = None;
        self.settle_target = None;
    }

    fn matches_settle_target(&self, element: &AXUIElement) -> bool {
        let Some((pid, cg_window_id, ax_ref)) = self.settle_target else {
            return false;
        };

        let element_pid = element_pid(element).unwrap_or_default();
        if element_pid != pid {
            return false;
        }

        if let Some(cg_window_id) = cg_window_id {
            if self
                .mru_windows
                .iter()
                .any(|window| window.cg_window_id == Some(cg_window_id) && window.same_element(element))
            {
                return true;
            }
        }

        ax_ref == element.as_CFTypeRef() as usize
            || self
                .mru_windows
                .iter()
                .any(|window| window.pid == pid && window.same_element(element))
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
        // Prefer element match; fall back to title+pid so CG fallback rows
        // can donate their cg_window_id when focus observation upgrades them.
        let cg_window_id = self
            .mru_windows
            .iter()
            .find(|window| window.same_element(&focused_window))
            .or_else(|| {
                self.mru_windows.iter().find(|window| {
                    window.pid == pid
                        && !window.window_name.is_empty()
                        && window.window_name == title
                })
            })
            .and_then(|window| window.cg_window_id);
        let focused_window = Window::new(
            pid,
            cg_window_id,
            app_name,
            title,
            true,
            false,
            focused_window,
        );

        self.mru_windows
            .observe_front_by(focused_window, |known_window, focused_window| {
                known_window.same_window(focused_window)
            });
    }

    fn push_back_unique(&mut self, window: Window) {
        self.mru_windows
            .push_back_unique_by(window, |known_window, window| {
                known_window.same_window(window)
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
