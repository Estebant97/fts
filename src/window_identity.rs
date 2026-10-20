/// Platform-free window identity matching used by the MRU list.
/// Kept free of Accessibility types so it can be unit-tested anywhere.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowFields<'a> {
    pub pid: i32,
    pub cg_window_id: Option<u32>,
    pub window_name: &'a str,
    pub is_app_fallback: bool,
}

/// Returns true when `left` and `right` refer to the same window.
///
/// `elements_equal` is the AX element equality result from the caller; it is
/// only consulted when neither side is an app-level fallback row.
///
/// When both sides have a CG window id, those ids are authoritative: equal
/// ids match and unequal ids never match, even if titles are identical.
/// Title+pid matching is reserved for upgrading a CG/AX fallback entry into
/// the real window once focus observation resolves a concrete AXUIElement.
pub fn same_window_by_fields(
    left: &WindowFields<'_>,
    right: &WindowFields<'_>,
    elements_equal: bool,
) -> bool {
    if left.pid != right.pid {
        return false;
    }

    match (left.cg_window_id, right.cg_window_id) {
        (Some(left_id), Some(right_id)) => return left_id == right_id,
        _ => {}
    }

    if !left.is_app_fallback && !right.is_app_fallback && elements_equal {
        return true;
    }

    // Title match only bridges a fallback row to a real window (or two
    // fallbacks). Two concrete AX windows with the same title must not merge
    // when CG ids are missing — that is how Chrome profiles used to collapse.
    if left.is_app_fallback || right.is_app_fallback {
        return !left.window_name.is_empty() && left.window_name == right.window_name;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{WindowFields, same_window_by_fields};

    fn fields(
        pid: i32,
        cg: Option<u32>,
        title: &str,
        fallback: bool,
    ) -> WindowFields<'_> {
        WindowFields {
            pid,
            cg_window_id: cg,
            window_name: title,
            is_app_fallback: fallback,
        }
    }

    #[test]
    fn matches_by_cg_window_id() {
        let left = fields(10, Some(42), "A", false);
        let right = fields(10, Some(42), "B", true);
        assert!(same_window_by_fields(&left, &right, false));
    }

    #[test]
    fn different_cg_ids_do_not_match() {
        let left = fields(10, Some(1), "A", false);
        let right = fields(10, Some(2), "A", false);
        assert!(!same_window_by_fields(&left, &right, true));
    }

    #[test]
    fn same_title_without_cg_ids_does_not_merge_real_windows() {
        let left = fields(10, None, "Inbox", false);
        let right = fields(10, None, "Inbox", false);
        assert!(!same_window_by_fields(&left, &right, false));
    }

    #[test]
    fn matches_non_fallback_by_element_equality() {
        let left = fields(10, None, "", false);
        let right = fields(10, None, "", false);
        assert!(same_window_by_fields(&left, &right, true));
        assert!(!same_window_by_fields(&left, &right, false));
    }

    #[test]
    fn fallback_and_real_match_by_title() {
        let fallback = fields(7, Some(99), "README.md — zed", true);
        let real = fields(7, None, "README.md — zed", false);
        assert!(same_window_by_fields(&fallback, &real, false));
        assert!(same_window_by_fields(&real, &fallback, false));
    }

    #[test]
    fn empty_title_does_not_match() {
        let left = fields(7, None, "", true);
        let right = fields(7, None, "", false);
        assert!(!same_window_by_fields(&left, &right, false));
    }

    #[test]
    fn different_pids_do_not_match() {
        let left = fields(1, None, "Doc", true);
        let right = fields(2, None, "Doc", false);
        assert!(!same_window_by_fields(&left, &right, false));
    }
}
