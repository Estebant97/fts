/// Pure CG + AX merge used by the macOS enumerator.
/// Kept platform-free so pairing and dedupe can be unit-tested anywhere.

use std::collections::HashSet;

use crate::focus_resolve::UNTITLED_WINDOW_TITLE;

/// A layer-0/3 CGWindowList row after size/alpha filtering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumeratedCgWindow {
    pub pid: i32,
    pub owner: String,
    pub name: String,
    pub onscreen: bool,
    pub window_number: u32,
}

/// An AXWindows entry. `cg_window_id` comes from `_AXUIElementGetWindow`
/// when available; otherwise the merge falls back to title matching.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumeratedAxWindow {
    pub title: String,
    pub cg_window_id: Option<u32>,
    /// Caller-owned index into the live AXUIElement list.
    pub ax_index: usize,
}

/// One switcher row produced from CG + AX for a single pid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedWindow {
    pub pid: i32,
    pub cg_window_id: Option<u32>,
    pub app_name: String,
    pub window_name: String,
    pub onscreen: bool,
    pub is_app_fallback: bool,
    /// Index into the AX list, or `None` for CG-only fallback rows.
    pub ax_index: Option<usize>,
}

/// Find an unmatched CG row for this AX window.
///
/// Prefer an exact CG window id. When that id was filtered out of the CG list
/// (size/layer), fall through to title matching so the visible CG surface is
/// consumed instead of becoming a phantom fallback. Empty-name CG pairing is
/// only used when Accessibility did not report a CG id (Ghostty / no Screen
/// Recording).
fn match_cg_for_ax<'a>(
    ax: &EnumeratedAxWindow,
    title: &str,
    cg_windows: &'a [EnumeratedCgWindow],
    matched_cg_numbers: &HashSet<u32>,
) -> Option<&'a EnumeratedCgWindow> {
    let unmatched = |cg: &&EnumeratedCgWindow| !matched_cg_numbers.contains(&cg.window_number);

    if let Some(id) = ax.cg_window_id {
        if let Some(cg) = cg_windows
            .iter()
            .find(|cg| unmatched(cg) && cg.window_number == id)
        {
            return Some(cg);
        }
        // Id missing from the filtered CG list — try title before giving up.
        if !title.is_empty() && title != UNTITLED_WINDOW_TITLE {
            return cg_windows
                .iter()
                .find(|cg| unmatched(cg) && !cg.name.is_empty() && cg.name == title);
        }
        return None;
    }

    if !title.is_empty() && title != UNTITLED_WINDOW_TITLE {
        if let Some(cg) = cg_windows
            .iter()
            .find(|cg| unmatched(cg) && !cg.name.is_empty() && cg.name == title)
        {
            return Some(cg);
        }
    }

    // No CG id from AX: pair with an empty CG name (Ghostty / blank titles).
    cg_windows
        .iter()
        .find(|cg| unmatched(cg) && cg.name.is_empty())
}

/// Merge CG and AX windows for one application.
///
/// Pairing prefers CG window ids from Accessibility, then title / empty-name
/// matching. CG-only app-fallback rows are emitted **only when AX lists no
/// windows** (e.g. Ghostty when Accessibility cannot see its surfaces). When
/// AX has any windows, unmatched CG rows are dropped — they are almost always
/// helper panels (Zoom share sheets, AutoFill, permission dialogs) that cannot
/// be raised and break the switcher.
pub fn merge_cg_and_ax(
    pid: i32,
    app_name: &str,
    cg_windows: &[EnumeratedCgWindow],
    ax_windows: &[EnumeratedAxWindow],
) -> Vec<MergedWindow> {
    let mut matched_cg_numbers = HashSet::new();
    let mut emitted_cg_ids = HashSet::new();
    let mut window_list = Vec::new();

    for ax in ax_windows {
        let title = if ax.title.is_empty() {
            UNTITLED_WINDOW_TITLE.to_string()
        } else {
            ax.title.clone()
        };

        let matched = match_cg_for_ax(ax, &title, cg_windows, &matched_cg_numbers);

        let (cg_window_id, onscreen) = if let Some(cg) = matched {
            matched_cg_numbers.insert(cg.window_number);
            (Some(cg.window_number), cg.onscreen)
        } else if let Some(id) = ax.cg_window_id {
            // AX knows the CG id even when the CG row was filtered out
            // (other Space / brief off-screen). Keep the id for focus.
            (Some(id), true)
        } else {
            (None, true)
        };

        if let Some(id) = cg_window_id {
            if !emitted_cg_ids.insert(id) {
                continue;
            }
        }

        window_list.push(MergedWindow {
            pid,
            cg_window_id,
            app_name: app_name.to_string(),
            window_name: title,
            onscreen,
            is_app_fallback: false,
            ax_index: Some(ax.ax_index),
        });
    }

    // Only invent CG-only rows when Accessibility saw nothing for this app.
    if ax_windows.is_empty() {
        for cg in cg_windows {
            if matched_cg_numbers.contains(&cg.window_number) {
                continue;
            }
            if !emitted_cg_ids.insert(cg.window_number) {
                continue;
            }

            let window_name = if cg.name.is_empty() {
                app_name.to_string()
            } else {
                cg.name.clone()
            };

            window_list.push(MergedWindow {
                pid,
                cg_window_id: Some(cg.window_number),
                app_name: cg.owner.clone(),
                window_name,
                onscreen: cg.onscreen,
                is_app_fallback: true,
                ax_index: None,
            });
        }
    }

    window_list
}

#[cfg(test)]
mod tests {
    use super::{
        EnumeratedAxWindow, EnumeratedCgWindow, MergedWindow, merge_cg_and_ax,
    };

    fn cg(number: u32, name: &str) -> EnumeratedCgWindow {
        EnumeratedCgWindow {
            pid: 42,
            owner: "Google Chrome".into(),
            name: name.into(),
            onscreen: true,
            window_number: number,
        }
    }

    fn ax(index: usize, title: &str, cg_id: Option<u32>) -> EnumeratedAxWindow {
        EnumeratedAxWindow {
            title: title.into(),
            cg_window_id: cg_id,
            ax_index: index,
        }
    }

    #[test]
    fn two_chrome_windows_same_page_title_stay_distinct() {
        let cg_windows = vec![
            cg(10, "Inbox - Work - Google Chrome"),
            cg(11, "Inbox - Personal - Google Chrome"),
        ];
        let ax_windows = vec![
            ax(0, "Inbox - Work - Google Chrome", Some(10)),
            ax(1, "Inbox - Personal - Google Chrome", Some(11)),
        ];

        let merged = merge_cg_and_ax(42, "Google Chrome", &cg_windows, &ax_windows);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].cg_window_id, Some(10));
        assert_eq!(merged[1].cg_window_id, Some(11));
        assert!(!merged[0].is_app_fallback);
        assert!(!merged[1].is_app_fallback);
    }

    #[test]
    fn same_title_without_cg_ids_still_emits_two_ax_rows() {
        let ax_windows = vec![ax(0, "Inbox", None), ax(1, "Inbox", None)];
        let merged = merge_cg_and_ax(7, "Google Chrome", &[], &ax_windows);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].ax_index, Some(0));
        assert_eq!(merged[1].ax_index, Some(1));
        assert!(merged.iter().all(|w| w.cg_window_id.is_none()));
    }

    #[test]
    fn ghostty_empty_cg_name_keeps_ax_window() {
        let cg_windows = vec![EnumeratedCgWindow {
            pid: 9,
            owner: "Ghostty".into(),
            name: String::new(),
            onscreen: true,
            window_number: 55,
        }];
        let ax_windows = vec![ax(0, "~", None)];

        let merged = merge_cg_and_ax(9, "Ghostty", &cg_windows, &ax_windows);
        assert_eq!(
            merged,
            vec![MergedWindow {
                pid: 9,
                cg_window_id: Some(55),
                app_name: "Ghostty".into(),
                window_name: "~".into(),
                onscreen: true,
                is_app_fallback: false,
                ax_index: Some(0),
            }]
        );
    }

    #[test]
    fn ghostty_cg_only_empty_title_becomes_fallback_named_after_app() {
        let cg_windows = vec![EnumeratedCgWindow {
            pid: 9,
            owner: "Ghostty".into(),
            name: String::new(),
            onscreen: true,
            window_number: 55,
        }];
        let merged = merge_cg_and_ax(9, "Ghostty", &cg_windows, &[]);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].is_app_fallback);
        assert_eq!(merged[0].window_name, "Ghostty");
        assert_eq!(merged[0].cg_window_id, Some(55));
    }

    #[test]
    fn pairs_by_title_when_ax_lacks_cg_id() {
        let cg_windows = vec![cg(3, "README.md — zed")];
        let ax_windows = vec![ax(0, "README.md — zed", None)];
        let merged = merge_cg_and_ax(1, "Zed", &cg_windows, &ax_windows);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].cg_window_id, Some(3));
        assert!(!merged[0].is_app_fallback);
    }

    #[test]
    fn ax_cg_id_without_cg_row_is_kept() {
        let ax_windows = vec![ax(0, "Docs", Some(99))];
        let merged = merge_cg_and_ax(5, "Safari", &[], &ax_windows);
        assert_eq!(merged[0].cg_window_id, Some(99));
        assert!(!merged[0].is_app_fallback);
    }

    #[test]
    fn duplicate_ax_claim_on_same_cg_id_keeps_first() {
        let cg_windows = vec![cg(10, "A")];
        let ax_windows = vec![ax(0, "A", Some(10)), ax(1, "B", Some(10))];
        let merged = merge_cg_and_ax(42, "Google Chrome", &cg_windows, &ax_windows);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].window_name, "A");
        assert_eq!(merged[0].ax_index, Some(0));
    }

    #[test]
    fn unmatched_cg_helpers_dropped_when_ax_has_windows() {
        // Zoom-style noise: real meeting window in AX, plus CG-only panels.
        let cg_windows = vec![
            cg(100, "Zoom Meeting"),
            cg(101, "Breakout rooms"),
            cg(102, "Select a window or an application that you want to share"),
            cg(103, "Zoom Client Healthcheck"),
        ];
        let ax_windows = vec![ax(0, "Zoom Meeting", Some(100))];

        let merged = merge_cg_and_ax(8, "Zoom", &cg_windows, &ax_windows);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].window_name, "Zoom Meeting");
        assert!(!merged[0].is_app_fallback);
        assert!(merged.iter().all(|w| !w.is_app_fallback));
    }

    #[test]
    fn ax_id_miss_pairs_by_title_instead_of_emitting_fallback() {
        // AX reports a filtered-out CG id; the visible CG row shares the title.
        let cg_windows = vec![cg(50, "Inbox - Gmail")];
        let ax_windows = vec![ax(0, "Inbox - Gmail", Some(999))];

        let merged = merge_cg_and_ax(3, "Google Chrome", &cg_windows, &ax_windows);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].cg_window_id, Some(50));
        assert!(!merged[0].is_app_fallback);
    }

    #[test]
    fn autofill_style_cg_only_app_still_emits_when_ax_empty() {
        let cg_windows = vec![cg(1, "AutoFill")];
        let merged = merge_cg_and_ax(2, "AutoFill", &cg_windows, &[]);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].is_app_fallback);
    }
}
