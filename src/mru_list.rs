use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct MruList<T> {
    items: VecDeque<T>,
    selected_index: usize,
    is_switching: bool,
    did_advance_selection: bool,
}

#[allow(dead_code)]
impl<T> MruList<T> {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            selected_index: 0,
            is_switching: false,
            did_advance_selection: false,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn is_switching(&self) -> bool {
        self.is_switching
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items.iter_mut()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    pub fn begin_switching(&mut self) {
        self.is_switching = true;
        self.selected_index = 0;
        self.did_advance_selection = false;
    }

    pub fn advance_selection(&mut self) -> Option<&T> {
        if self.items.len() < 2 {
            return None;
        }

        self.selected_index = (self.selected_index + 1) % self.items.len();
        self.did_advance_selection = true;
        self.items.get(self.selected_index)
    }

    pub fn reverse_selection(&mut self) -> Option<&T> {
        if self.items.len() < 2 {
            return None;
        }

        if self.selected_index == 0 {
            self.selected_index = self.items.len() - 1;
        } else {
            self.selected_index -= 1;
        }
        self.did_advance_selection = true;
        self.items.get(self.selected_index)
    }

    pub fn selected_item(&self) -> Option<&T> {
        if !self.did_advance_selection {
            return None;
        }

        self.items.get(self.selected_index)
    }

    pub fn selected_position(&self) -> Option<usize> {
        self.did_advance_selection.then_some(self.selected_index)
    }

    pub fn finish_switching(&mut self) {
        self.is_switching = false;
        self.did_advance_selection = false;
        if self.selected_index == 0 {
            return;
        }

        if let Some(item) = self.items.remove(self.selected_index) {
            self.items.push_front(item);
        }

        self.selected_index = 0;
    }

    /// End a session without promoting the selection (Escape / dismiss).
    pub fn cancel_switching(&mut self) {
        self.is_switching = false;
        self.did_advance_selection = false;
        self.selected_index = 0;
    }

    pub fn observe_front_by<F>(&mut self, item: T, same_item: F)
    where
        F: Fn(&T, &T) -> bool,
    {
        if self.is_switching {
            return;
        }

        // Replace the matched entry with the fresh observation so fallback
        // rows upgrade to real windows instead of leaving stale identity.
        if self
            .items
            .front()
            .is_some_and(|known_item| same_item(known_item, &item))
        {
            self.items[0] = item;
            return;
        }

        if let Some(index) = self
            .items
            .iter()
            .position(|known_item| same_item(known_item, &item))
        {
            self.items.remove(index);
            self.items.push_front(item);
            return;
        }

        self.items.push_front(item);
    }

    pub fn push_back_unique_by<F>(&mut self, item: T, same_item: F)
    where
        F: Fn(&T, &T) -> bool,
    {
        if self
            .items
            .iter()
            .any(|known_item| same_item(known_item, &item))
        {
            return;
        }

        self.items.push_back(item);
    }

    pub fn retain_by<F>(&mut self, mut keep: F)
    where
        F: FnMut(&T) -> bool,
    {
        let selected = self
            .did_advance_selection
            .then(|| self.items.get(self.selected_index).map(|_| self.selected_index))
            .flatten();

        let mut removed_before_selected = 0usize;
        let mut idx = 0usize;
        self.items.retain(|item| {
            let keep_item = keep(item);
            if !keep_item {
                if let Some(selected_index) = selected {
                    if idx < selected_index {
                        removed_before_selected += 1;
                    }
                }
            }
            idx += 1;
            keep_item
        });

        if self.items.is_empty() {
            self.selected_index = 0;
            self.did_advance_selection = false;
            return;
        }

        if self.did_advance_selection {
            let next = selected
                .map(|index| index.saturating_sub(removed_before_selected))
                .unwrap_or(0)
                .min(self.items.len() - 1);
            self.selected_index = next;
        } else {
            self.selected_index = self.selected_index.min(self.items.len() - 1);
        }
    }

    pub fn update_matching_by<F, U>(&mut self, mut same_item: F, mut update: U)
    where
        F: FnMut(&T) -> bool,
        U: FnMut(&mut T),
    {
        if let Some(item) = self.items.iter_mut().find(|item| same_item(item)) {
            update(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MruList;

    fn same_item(left: &&str, right: &&str) -> bool {
        left == right
    }

    fn list_items(list: &MruList<&'static str>) -> Vec<&'static str> {
        list.iter().copied().collect()
    }

    #[test]
    fn ignores_duplicate_startup_items() {
        let mut list = MruList::new();

        list.push_back_unique_by("zed-a", same_item);
        list.push_back_unique_by("chrome", same_item);
        list.push_back_unique_by("zed-a", same_item);

        assert_eq!(list_items(&list), vec!["zed-a", "chrome"]);
    }

    #[test]
    fn observed_focus_moves_existing_item_to_front() {
        let mut list = MruList::new();

        list.push_back_unique_by("zed-a", same_item);
        list.push_back_unique_by("chrome", same_item);
        list.push_back_unique_by("zed-b", same_item);
        list.observe_front_by("chrome", same_item);

        assert_eq!(list_items(&list), vec!["chrome", "zed-a", "zed-b"]);
    }

    #[test]
    fn first_tab_selects_previous_window() {
        let mut list = MruList::new();

        list.push_back_unique_by("zed-b", same_item);
        list.push_back_unique_by("chrome", same_item);
        list.push_back_unique_by("zed-a", same_item);
        list.begin_switching();

        assert_eq!(list.advance_selection(), Some(&"chrome"));
        assert_eq!(list.selected_position(), Some(1));
    }

    #[test]
    fn finish_switching_promotes_selected_window() {
        let mut list = MruList::new();

        list.push_back_unique_by("zed-b", same_item);
        list.push_back_unique_by("chrome", same_item);
        list.push_back_unique_by("zed-a", same_item);
        list.begin_switching();
        list.advance_selection();
        list.finish_switching();

        assert_eq!(list_items(&list), vec!["chrome", "zed-b", "zed-a"]);
    }

    #[test]
    fn focus_observation_is_ignored_while_switching() {
        let mut list = MruList::new();

        list.push_back_unique_by("zed-b", same_item);
        list.push_back_unique_by("chrome", same_item);
        list.push_back_unique_by("zed-a", same_item);
        list.begin_switching();
        list.observe_front_by("zed-a", same_item);

        assert_eq!(list_items(&list), vec!["zed-b", "chrome", "zed-a"]);
    }

    #[test]
    fn advance_wraps_around() {
        let mut list = MruList::new();
        list.push_back_unique_by("a", same_item);
        list.push_back_unique_by("b", same_item);
        list.push_back_unique_by("c", same_item);
        list.begin_switching();

        assert_eq!(list.advance_selection(), Some(&"b"));
        assert_eq!(list.advance_selection(), Some(&"c"));
        assert_eq!(list.advance_selection(), Some(&"a"));
    }

    #[test]
    fn reverse_selects_last_from_front() {
        let mut list = MruList::new();
        list.push_back_unique_by("a", same_item);
        list.push_back_unique_by("b", same_item);
        list.push_back_unique_by("c", same_item);
        list.begin_switching();

        assert_eq!(list.reverse_selection(), Some(&"c"));
        assert_eq!(list.selected_position(), Some(2));
        assert_eq!(list.reverse_selection(), Some(&"b"));
    }

    #[test]
    fn cancel_switching_does_not_promote() {
        let mut list = MruList::new();
        list.push_back_unique_by("a", same_item);
        list.push_back_unique_by("b", same_item);
        list.push_back_unique_by("c", same_item);
        list.begin_switching();
        list.advance_selection();
        list.cancel_switching();

        assert!(!list.is_switching());
        assert_eq!(list.selected_item(), None);
        assert_eq!(list_items(&list), vec!["a", "b", "c"]);
    }

    #[test]
    fn finish_without_advance_leaves_order() {
        let mut list = MruList::new();
        list.push_back_unique_by("a", same_item);
        list.push_back_unique_by("b", same_item);
        list.begin_switching();
        list.finish_switching();

        assert_eq!(list_items(&list), vec!["a", "b"]);
        assert!(!list.is_switching());
    }

    #[test]
    fn single_window_cannot_advance() {
        let mut list = MruList::new();
        list.push_back_unique_by("only", same_item);
        list.begin_switching();

        assert_eq!(list.advance_selection(), None);
        assert_eq!(list.reverse_selection(), None);
        assert_eq!(list.selected_item(), None);
    }

    #[test]
    fn unknown_focused_item_is_pushed_front() {
        let mut list = MruList::new();
        list.push_back_unique_by("a", same_item);
        list.push_back_unique_by("b", same_item);
        list.observe_front_by("new", same_item);

        assert_eq!(list_items(&list), vec!["new", "a", "b"]);
    }

    #[test]
    fn retain_removes_closed_windows() {
        let mut list = MruList::new();
        list.push_back_unique_by("a", same_item);
        list.push_back_unique_by("b", same_item);
        list.push_back_unique_by("c", same_item);
        list.retain_by(|item| *item != "b");

        assert_eq!(list_items(&list), vec!["a", "c"]);
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Tracked {
        id: &'static str,
        generation: u32,
    }

    fn same_tracked(left: &Tracked, right: &Tracked) -> bool {
        left.id == right.id
    }

    #[test]
    fn observe_front_replaces_matched_entry() {
        let mut list = MruList::new();
        list.push_back_unique_by(
            Tracked {
                id: "doc",
                generation: 1,
            },
            same_tracked,
        );
        list.push_back_unique_by(
            Tracked {
                id: "other",
                generation: 1,
            },
            same_tracked,
        );

        list.observe_front_by(
            Tracked {
                id: "doc",
                generation: 2,
            },
            same_tracked,
        );

        assert_eq!(
            list.get(0),
            Some(&Tracked {
                id: "doc",
                generation: 2,
            })
        );
        assert_eq!(list.len(), 2);
    }
}
