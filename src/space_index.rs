/// Platform-free helpers for mapping Option+digit chords to Mission Control
/// Space indexes.

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

/// Among per-display space lists, pick the list that contains `active_space_id`.
/// Falls back to the first non-empty list.
pub fn select_display_spaces<'a>(
    displays: &'a [Vec<u64>],
    active_space_id: u64,
) -> Option<&'a [u64]> {
    displays
        .iter()
        .find(|spaces| spaces.contains(&active_space_id))
        .or_else(|| displays.iter().find(|spaces| !spaces.is_empty()))
        .map(|spaces| spaces.as_slice())
}

#[cfg(test)]
mod tests {
    use super::{select_display_spaces, space_id_at_index, workspace_index_for_digit};

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
        let displays = vec![vec![1u64, 2], vec![3, 4, 5]];
        assert_eq!(select_display_spaces(&displays, 4), Some(&[3u64, 4, 5][..]));
    }

    #[test]
    fn select_falls_back_to_first_non_empty() {
        let displays = vec![vec![], vec![7u64, 8]];
        assert_eq!(select_display_spaces(&displays, 99), Some(&[7u64, 8][..]));
    }

    #[test]
    fn select_returns_none_when_all_empty() {
        let displays = vec![vec![], vec![]];
        assert_eq!(select_display_spaces(&displays, 1), None);
    }
}
