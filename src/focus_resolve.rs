/// Pure focus-target resolution policy used by the macOS focuser.
/// Kept platform-free so the Chrome-profile "do not guess" rule is tested.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusCandidate {
    ByCgWindowId(u32),
    ByTitle,
}

/// Decide how to pick an AX window for focus.
///
/// When a CG window id is known, callers must raise that exact window and
/// must not fall back to `AXFocusedWindow` — that is what kept the other
/// Chrome profile in front.
pub fn focus_candidate(cg_window_id: Option<u32>) -> FocusCandidate {
    match cg_window_id {
        Some(id) => FocusCandidate::ByCgWindowId(id),
        None => FocusCandidate::ByTitle,
    }
}

/// After a CG-id scan fails, should we try the focused window?
pub fn allow_focused_fallback_after_cg_miss(cg_window_id: Option<u32>) -> bool {
    cg_window_id.is_none()
}

#[cfg(test)]
mod tests {
    use super::{
        FocusCandidate, allow_focused_fallback_after_cg_miss, focus_candidate,
    };

    #[test]
    fn known_cg_id_requires_exact_match() {
        assert_eq!(
            focus_candidate(Some(42)),
            FocusCandidate::ByCgWindowId(42)
        );
        assert!(!allow_focused_fallback_after_cg_miss(Some(42)));
    }

    #[test]
    fn missing_cg_id_may_use_title_then_focused() {
        assert_eq!(focus_candidate(None), FocusCandidate::ByTitle);
        assert!(allow_focused_fallback_after_cg_miss(None));
    }
}
