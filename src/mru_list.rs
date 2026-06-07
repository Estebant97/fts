use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct MruList<T> {
    items: VecDeque<T>,
    selected_index: usize,
    is_switching: bool,
    did_advance_selection: bool,
}

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

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
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

    pub fn observe_front_by<F>(&mut self, item: T, same_item: F)
    where
        F: Fn(&T, &T) -> bool,
    {
        if self.is_switching {
            return;
        }

        if self
            .items
            .front()
            .is_some_and(|known_item| same_item(known_item, &item))
        {
            return;
        }

        if let Some(index) = self
            .items
            .iter()
            .position(|known_item| same_item(known_item, &item))
        {
            if let Some(item) = self.items.remove(index) {
                self.items.push_front(item);
            }
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
}
