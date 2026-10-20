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
        select_display_spaces, space_id_at_index, workspace_index_for_digit, DisplaySpaces,
    };

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
}
