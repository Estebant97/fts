use crate::mru_list::MruList;

/// Whether a switcher session should promote the selection after a focus attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchCommit {
    /// Focus succeeded — promote selection and end the session.
    Finish,
    /// Focus failed — end the session without rewriting MRU order.
    Cancel,
}

pub fn switch_commit_after_focus(focus_succeeded: bool) -> SwitchCommit {
    if focus_succeeded {
        SwitchCommit::Finish
    } else {
        SwitchCommit::Cancel
    }
}

/// Apply the focus outcome to an MRU list.
pub fn apply_switch_commit<T>(list: &mut MruList<T>, commit: SwitchCommit) {
    match commit {
        SwitchCommit::Finish => list.finish_switching(),
        SwitchCommit::Cancel => list.cancel_switching(),
    }
}

#[cfg(test)]
mod tests {
    use super::{SwitchCommit, apply_switch_commit, switch_commit_after_focus};
    use crate::mru_list::MruList;

    fn same_item(left: &&str, right: &&str) -> bool {
        left == right
    }

    fn list_items(list: &MruList<&'static str>) -> Vec<&'static str> {
        list.iter().copied().collect()
    }

    #[test]
    fn focus_success_maps_to_finish() {
        assert_eq!(switch_commit_after_focus(true), SwitchCommit::Finish);
    }

    #[test]
    fn focus_failure_maps_to_cancel() {
        assert_eq!(switch_commit_after_focus(false), SwitchCommit::Cancel);
    }

    #[test]
    fn failed_focus_does_not_promote_selection() {
        let mut list = MruList::new();
        list.push_back_unique_by("front", same_item);
        list.push_back_unique_by("next", same_item);
        list.push_back_unique_by("third", same_item);
        list.begin_switching();
        list.advance_selection();

        let commit = switch_commit_after_focus(false);
        apply_switch_commit(&mut list, commit);

        assert!(!list.is_switching());
        assert_eq!(list_items(&list), vec!["front", "next", "third"]);
    }

    #[test]
    fn successful_focus_promotes_selection() {
        let mut list = MruList::new();
        list.push_back_unique_by("front", same_item);
        list.push_back_unique_by("next", same_item);
        list.push_back_unique_by("third", same_item);
        list.begin_switching();
        list.advance_selection();

        let commit = switch_commit_after_focus(true);
        apply_switch_commit(&mut list, commit);

        assert!(!list.is_switching());
        assert_eq!(list_items(&list), vec!["next", "front", "third"]);
    }
}
