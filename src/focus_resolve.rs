/// Pure focus-target resolution policy used by the macOS focuser.
/// Kept platform-free so the Chrome-profile "do not guess" rule is tested.

/// Placeholder stored when Accessibility reports an empty window title.
pub const UNTITLED_WINDOW_TITLE: &str = "Untitled Window";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusCandidate {
    ByCgWindowId(u32),
    /// No CG id: prefer the cached AX element, then a title scan.
    CachedThenTitle,
}

/// Decide how to pick an AX window for focus.
///
/// When a CG window id is known, callers must raise that exact window and
/// must not fall back to `AXFocusedWindow` — that is what kept the other
/// Chrome profile in front.
pub fn focus_candidate(cg_window_id: Option<u32>) -> FocusCandidate {
    match cg_window_id {
        Some(id) => FocusCandidate::ByCgWindowId(id),
        None => FocusCandidate::CachedThenTitle,
    }
}

/// After a CG-id scan fails, should we try the focused window?
///
/// Never: the caller should use the cached AX element instead. Guessing
/// `AXFocusedWindow` is what kept the wrong Chrome profile in front.
pub fn allow_focused_fallback_after_cg_miss(cg_window_id: Option<u32>) -> bool {
    let _ = cg_window_id;
    false
}

/// Compare a stored switcher title to a live AX title.
///
/// Enumeration stores empty AX titles as [`UNTITLED_WINDOW_TITLE`]; live AX
/// still reports `""`. Treat those as equal so Ghostty / blank-title rows
/// can be raised by title when needed.
pub fn ax_title_matches_stored(stored: &str, live: &str) -> bool {
    if stored == live {
        return true;
    }
    (stored == UNTITLED_WINDOW_TITLE && live.is_empty())
        || (stored.is_empty() && live.is_empty())
}

/// Pick among title matches when the cached element is absent from the live list.
///
/// - Exactly one match → that window
/// - Zero or many → `None` (caller keeps the cached element / fails closed)
pub fn take_unique_match<T>(mut matches: Vec<T>) -> Option<T> {
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FocusCandidate, UNTITLED_WINDOW_TITLE, allow_focused_fallback_after_cg_miss,
        ax_title_matches_stored, focus_candidate, take_unique_match,
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
    fn missing_cg_id_uses_cached_then_title() {
        assert_eq!(focus_candidate(None), FocusCandidate::CachedThenTitle);
        assert!(!allow_focused_fallback_after_cg_miss(None));
    }

    #[test]
    fn untitled_placeholder_matches_empty_live_title() {
        assert!(ax_title_matches_stored(UNTITLED_WINDOW_TITLE, ""));
        assert!(ax_title_matches_stored("Inbox", "Inbox"));
        assert!(!ax_title_matches_stored(UNTITLED_WINDOW_TITLE, "Inbox"));
        assert!(!ax_title_matches_stored("Inbox", ""));
    }

    #[test]
    fn unique_title_match_rejects_ambiguity() {
        assert_eq!(take_unique_match(vec![3]), Some(3));
        assert_eq!(take_unique_match(Vec::<i32>::new()), None);
        assert_eq!(take_unique_match(vec![1, 4]), None);
    }
}
