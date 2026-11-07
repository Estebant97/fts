//! Locate desktop buttons in either the Dock or WindowManager AX layout.

pub trait MissionControlNode: Clone {
    fn identifier(&self) -> Option<String>;
    fn children(&self) -> Vec<Self>;
    fn display_id(&self) -> Option<u32>;
}

/// Roots are Dock's `mc` group and, on macOS 27+, WindowManager itself.
/// Missing or partially populated trees return None so the caller can retry.
pub fn space_buttons<N: MissionControlNode>(roots: &[N], display_id: u32) -> Option<Vec<N>> {
    for root in roots {
        for display in root.children() {
            if display.identifier().as_deref() != Some("mc.display")
                || display.display_id() != Some(display_id)
            {
                continue;
            }
            for spaces in display.children() {
                if spaces.identifier().as_deref() != Some("mc.spaces") {
                    continue;
                }
                for list in spaces.children() {
                    if list.identifier().as_deref() == Some("mc.spaces.list") {
                        let buttons = list.children();
                        if !buttons.is_empty() {
                            return Some(buttons);
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Node {
        id: &'static str,
        display: Option<u32>,
        children: Vec<Node>,
    }
    impl MissionControlNode for Node {
        fn identifier(&self) -> Option<String> {
            Some(self.id.into())
        }
        fn children(&self) -> Vec<Self> {
            self.children.clone()
        }
        fn display_id(&self) -> Option<u32> {
            self.display
        }
    }
    fn node(id: &'static str, children: Vec<Node>) -> Node {
        Node {
            id,
            display: None,
            children,
        }
    }
    fn display(id: u32) -> Node {
        Node {
            id: "mc.display",
            display: Some(id),
            children: vec![
                node("window thumbnail", vec![]),
                node(
                    "mc.spaces",
                    vec![
                        node("mc.spaces.add", vec![]),
                        node(
                            "mc.spaces.list",
                            vec![node("desktop 1", vec![]), node("desktop 2", vec![])],
                        ),
                    ],
                ),
            ],
        }
    }
    #[test]
    fn dock_layout_finds_only_desktop_buttons() {
        let buttons = space_buttons(&[node("mc", vec![display(11)])], 11).unwrap();
        assert_eq!(
            buttons.iter().map(|b| b.id).collect::<Vec<_>>(),
            ["desktop 1", "desktop 2"]
        );
    }
    #[test]
    fn macos_27_empty_dock_stub_does_not_hide_windowmanager_tree() {
        let roots = [node("mc", vec![]), node("WindowManager", vec![display(11)])];
        assert_eq!(space_buttons(&roots, 11).unwrap().len(), 2);
    }
    #[test]
    fn partial_tree_is_retried_until_requested_display_is_ready() {
        let mut roots = [node(
            "WindowManager",
            vec![
                display(12),
                Node {
                    id: "mc.display",
                    display: Some(11),
                    children: vec![],
                },
            ],
        )];
        assert!(space_buttons(&roots, 11).is_none());
        roots[0].children[1] = display(11);
        assert_eq!(space_buttons(&roots, 11).unwrap().len(), 2);
    }
    #[test]
    fn wrong_display_and_closed_overview_do_not_produce_buttons() {
        assert!(space_buttons(&[node("mc", vec![display(12)])], 11).is_none());
        assert!(space_buttons::<Node>(&[], 11).is_none());
        assert!(space_buttons(&[node("mc", vec![])], 11).is_none());
    }
}
