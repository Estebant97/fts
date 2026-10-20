/// Platform-free helpers for mapping Option+digit chords to Mission Control
/// Space indexes.

/// Per-display Space lists: membership for focus detection, user Spaces for Option+N.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplaySpaces {
    /// All Space IDs on this display (user, fullscreen, system) — used to find
    /// which display owns the currently active Space.
    pub membership_ids: Vec<u64>,
    /// User desktop Spaces only, in Mission Control order — Option+N indexes these.
    pub user_ids: Vec<u64>,
}

/// Map a physical digit key (`0`–`9`) to a 1-based workspace index.
/// Digits `1`–`9` map to workspaces 1–9. Digit `0` is reserved (no mapping).
pub fn workspace_index_for_digit(digit: u8) -> Option<u8> {
    match digit {
        1..=9 => Some(digit),
        _ => None,
    }
}

/// Select the space id at 1-based `index` from an ordered list.
pub fn space_id_at_index(space_ids: &[u64], index: u8) -> Option<u64> {
    let idx = usize::from(index).checked_sub(1)?;
    space_ids.get(idx).copied()
}

/// One MRU row considered for post–Space-switch menu-bar focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpaceFocusCandidate {
    pub onscreen: bool,
    pub is_app_fallback: bool,
    /// True if this window was onscreen *before* the CGS Space switch.
    /// While CG lags, previous-Space windows can still look onscreen — those
    /// must not be focused or Option+N bounces back.
    pub was_onscreen_before_switch: bool,
}

/// True once CG inventory reflects the Space change: a prior onscreen window
/// went offscreen, and/or a window that was offscreen is now onscreen.
pub fn space_inventory_settled(candidates: &[SpaceFocusCandidate]) -> bool {
    candidates
        .iter()
        .any(|c| c.was_onscreen_before_switch && !c.onscreen)
        || candidates
            .iter()
            .any(|c| c.onscreen && !c.was_onscreen_before_switch)
}

/// After a Mission Control Space switch, which MRU entry should own the menu bar?
///
/// Returns `None` while CG onscreen bits are still stale (would refocus the
/// previous Space), or when the destination Space has no onscreen windows —
/// callers should retry then activate Finder.
pub fn preferred_space_focus_index(candidates: &[SpaceFocusCandidate]) -> Option<usize> {
    if !space_inventory_settled(candidates) {
        return None;
    }

    candidates
        .iter()
        .position(|c| c.onscreen && !c.is_app_fallback && !c.was_onscreen_before_switch)
        .or_else(|| {
            candidates
                .iter()
                .position(|c| c.onscreen && !c.was_onscreen_before_switch)
        })
        // Inventory settled but only sticky / all-Spaces windows remain visible.
        .or_else(|| {
            candidates
                .iter()
                .position(|c| c.onscreen && !c.is_app_fallback)
        })
        .or_else(|| candidates.iter().position(|c| c.onscreen))
}

/// Among per-display Space lists, pick the user-Space list for the display that
/// owns `active_space_id` (including when that Space is fullscreen / non-user).
/// Falls back to the first display that has any user Spaces.
pub fn select_display_spaces<'a>(
    displays: &'a [DisplaySpaces],
    active_space_id: u64,
) -> Option<&'a [u64]> {
    displays
        .iter()
        .find(|display| display.membership_ids.contains(&active_space_id))
        .or_else(|| displays.iter().find(|display| !display.user_ids.is_empty()))
        .map(|display| display.user_ids.as_slice())
}

#[cfg(test)]
mod tests {
    use super::{
        preferred_space_focus_index, select_display_spaces, space_id_at_index,
        space_inventory_settled, workspace_index_for_digit, DisplaySpaces, SpaceFocusCandidate,
    };

    fn cand(onscreen: bool, is_app_fallback: bool, was_onscreen_before_switch: bool) -> SpaceFocusCandidate {
        SpaceFocusCandidate {
            onscreen,
            is_app_fallback,
            was_onscreen_before_switch,
        }
    }

    fn display(membership: &[u64], user: &[u64]) -> DisplaySpaces {
        DisplaySpaces {
            membership_ids: membership.to_vec(),
            user_ids: user.to_vec(),
        }
    }

    #[test]
    fn digits_one_through_nine_map() {
        for digit in 1u8..=9 {
            assert_eq!(workspace_index_for_digit(digit), Some(digit));
        }
    }

    #[test]
    fn digit_zero_is_unmapped() {
        assert_eq!(workspace_index_for_digit(0), None);
    }

    #[test]
    fn space_id_at_index_is_one_based() {
        let spaces = [10u64, 20, 30];
        assert_eq!(space_id_at_index(&spaces, 1), Some(10));
        assert_eq!(space_id_at_index(&spaces, 3), Some(30));
        assert_eq!(space_id_at_index(&spaces, 4), None);
        assert_eq!(space_id_at_index(&spaces, 0), None);
    }

    #[test]
    fn select_prefers_display_containing_active_space() {
        let displays = vec![
            display(&[1, 2], &[1, 2]),
            display(&[3, 4, 5], &[3, 4, 5]),
        ];
        assert_eq!(select_display_spaces(&displays, 4), Some(&[3u64, 4, 5][..]));
    }

    #[test]
    fn select_uses_membership_when_active_is_fullscreen() {
        // Display 0: user desktops 1,2 plus fullscreen 99 currently active.
        // Display 1: user desktops 3,4 — must not steal focus via fallback.
        let displays = vec![
            display(&[1, 2, 99], &[1, 2]),
            display(&[3, 4], &[3, 4]),
        ];
        assert_eq!(select_display_spaces(&displays, 99), Some(&[1u64, 2][..]));
    }

    #[test]
    fn select_falls_back_to_first_non_empty_user_list() {
        let displays = vec![display(&[], &[]), display(&[7, 8], &[7, 8])];
        assert_eq!(select_display_spaces(&displays, 99), Some(&[7u64, 8][..]));
    }

    #[test]
    fn select_returns_empty_user_list_when_membership_matches_but_no_user_spaces() {
        let displays = vec![display(&[1], &[]), display(&[2], &[])];
        assert_eq!(select_display_spaces(&displays, 1), Some(&[][..]));
    }

    #[test]
    fn select_returns_none_when_no_display_has_user_spaces_or_membership() {
        let displays = vec![display(&[], &[]), display(&[], &[])];
        assert_eq!(select_display_spaces(&displays, 1), None);
    }

    #[test]
    fn preferred_focus_picks_newly_visible_real_window() {
        // Previous Ghostty still listed; destination Chrome newly onscreen.
        let windows = [
            cand(false, false, true),
            cand(true, false, false),
            cand(true, true, false),
        ];
        assert!(space_inventory_settled(&windows));
        assert_eq!(preferred_space_focus_index(&windows), Some(1));
    }

    #[test]
    fn preferred_focus_falls_back_to_newly_visible_app_fallback() {
        let windows = [cand(false, false, true), cand(true, true, false)];
        assert_eq!(preferred_space_focus_index(&windows), Some(1));
    }

    #[test]
    fn preferred_focus_none_when_destination_empty_after_settle() {
        // Previous windows went offscreen; nothing new appeared.
        let windows = [cand(false, false, true), cand(false, true, true)];
        assert!(space_inventory_settled(&windows));
        assert_eq!(preferred_space_focus_index(&windows), None);
    }

    #[test]
    fn preferred_focus_skips_stale_previous_space_while_cg_lags() {
        // CGS switched, but CG still reports the old front window onscreen and
        // the destination window has not flipped on yet — must not focus.
        let windows = [cand(true, false, true), cand(false, false, false)];
        assert!(!space_inventory_settled(&windows));
        assert_eq!(preferred_space_focus_index(&windows), None);
    }

    #[test]
    fn preferred_focus_allows_sticky_after_previous_windows_leave() {
        let windows = [
            cand(false, false, true), // left with the old Space
            cand(true, false, true),  // sticky / all-Spaces
        ];
        assert!(space_inventory_settled(&windows));
        assert_eq!(preferred_space_focus_index(&windows), Some(1));
    }

    #[test]
    fn preferred_focus_prefers_newly_visible_over_sticky() {
        let windows = [
            cand(true, false, true),  // sticky
            cand(true, false, false), // newly visible on destination
        ];
        assert_eq!(preferred_space_focus_index(&windows), Some(1));
    }
}
