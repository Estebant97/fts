/// Pure CGWindowList filters used by the macOS enumerator.
/// Kept platform-free so they can be unit-tested without AppKit.

const MIN_VISIBLE_ALPHA: f64 = 0.01;
/// Normal app windows are layer 0; Ghostty "Float on Top" (and similar
/// always-on-top windows) sit at NSFloatingWindowLevel ≈ layer 3.
const MAX_APP_LAYER: i64 = 3;
/// Chrome and others use tiny windows for find bars / omnibox popovers.
const MIN_SWITCHABLE_EDGE: f64 = 200.0;

/// Owners that show up in CGWindowList but are never useful switcher targets
/// (system UI, credential prompts, panel services). Compared after
/// [`normalize_owner_key`].
const EXCLUDED_OWNER_NAMES: &[&str] = &[
    "loginwindow",
    "coreautha",
    "wifipicker",
    "wi-fi",
    "wifi",
    "autofill",
    "open and save panel service",
    "control center",
    "systemuiserver",
    "notification center",
    "dock",
    "spotlight",
    "window server",
    "wallpaper",
    "universal control",
    "airplayuiagent",
    "user notification center",
];

/// Substrings of CG window titles that are helper / picker chrome, not
/// switchable documents. Matched against a lowercased title.
const HELPER_TITLE_NEEDLES: &[&str] = &[
    "healthcheck",
    "select a window or an application",
    "invite people to join",
];

/// Keep normal and floating application windows; skip menu bar, dock,
/// overlays (layer ≥ 24), and nearly transparent surfaces.
pub fn should_include_cg_window(layer: i64, alpha: f64) -> bool {
    layer >= 0
        && layer <= MAX_APP_LAYER
        && alpha.is_finite()
        && alpha > MIN_VISIBLE_ALPHA
}

/// Reject HUD-sized popovers that are not useful switcher targets.
pub fn is_switchable_window_size(width: f64, height: f64) -> bool {
    width.is_finite()
        && height.is_finite()
        && width > MIN_SWITCHABLE_EDGE
        && height > MIN_SWITCHABLE_EDGE
}

/// Normalize an owner/title key for denylist comparison: trim, map unicode
/// dashes to ASCII `-`, lowercase.
pub fn normalize_owner_key(owner: &str) -> String {
    owner
        .trim()
        .chars()
        .map(|c| match c {
            // Hyphen / dash variants commonly seen in system UI names.
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            c => c,
        })
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Drop system / helper process names that pollute CGWindowList.
pub fn is_switchable_owner_name(owner: &str) -> bool {
    let key = normalize_owner_key(owner);
    if key.is_empty() {
        return false;
    }
    !EXCLUDED_OWNER_NAMES.iter().any(|excluded| key == *excluded)
}

/// True for CG titles that are picker / health UI, not real switch targets.
pub fn is_helper_window_title(title: &str) -> bool {
    let lower = title.to_ascii_lowercase();
    HELPER_TITLE_NEEDLES
        .iter()
        .any(|needle| lower.contains(needle))
}

/// Whether an unmatched CG row should become an app-fallback switcher entry.
///
/// - Denied owners never emit (merge-level backstop for the CG collector).
/// - Helper title needles never emit.
/// - When AX already listed windows: never emit. Accessibility reports real
///   OS windows; leftover CG surfaces are tabs (Ghostty) or helper chrome
///   (Zoom share sheets). Emitting them made the switcher list every Ghostty
///   tab and then fail focus with `AppFallback` because tabs have no AX
///   window. Separate Ghostty / Chrome windows each appear as their own AX
///   row.
/// - When AX is empty: keep CG rows (including empty names renamed to owner)
///   so Ghostty still appears without Accessibility / Screen Recording.
pub fn should_emit_cg_fallback(
    owner: &str,
    cg_name: &str,
    ax_titles: &std::collections::HashSet<&str>,
    ax_has_windows: bool,
) -> bool {
    if !is_switchable_owner_name(owner) {
        return false;
    }

    if is_helper_window_title(cg_name) || is_helper_window_title(owner) {
        return false;
    }

    // `ax_titles` is retained in the signature so callers can keep collecting
    // titles for logging / future heuristics without another API break.
    let _ = ax_titles;

    if ax_has_windows {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{
        is_helper_window_title, is_switchable_owner_name, is_switchable_window_size,
        normalize_owner_key, should_emit_cg_fallback, should_include_cg_window,
    };
    use std::collections::HashSet;

    #[test]
    fn keeps_opaque_normal_windows() {
        assert!(should_include_cg_window(0, 1.0));
        assert!(should_include_cg_window(0, 0.5));
    }

    #[test]
    fn keeps_floating_layer_windows() {
        assert!(should_include_cg_window(3, 1.0));
        assert!(should_include_cg_window(1, 0.8));
    }

    #[test]
    fn drops_system_layers_and_invisible_windows() {
        assert!(!should_include_cg_window(25, 1.0));
        assert!(!should_include_cg_window(24, 1.0));
        assert!(!should_include_cg_window(0, 0.0));
        assert!(!should_include_cg_window(0, 0.001));
        assert!(!should_include_cg_window(0, f64::NAN));
        assert!(!should_include_cg_window(-1, 1.0));
    }

    #[test]
    fn rejects_tiny_popover_windows() {
        assert!(!is_switchable_window_size(180.0, 400.0));
        assert!(!is_switchable_window_size(400.0, 180.0));
        assert!(!is_switchable_window_size(200.0, 200.0));
        assert!(!is_switchable_window_size(f64::NAN, 400.0));
    }

    #[test]
    fn keeps_full_size_windows() {
        assert!(is_switchable_window_size(201.0, 201.0));
        assert!(is_switchable_window_size(1280.0, 800.0));
    }

    #[test]
    fn drops_system_and_helper_owners() {
        assert!(!is_switchable_owner_name("loginwindow"));
        assert!(!is_switchable_owner_name("AutoFill"));
        assert!(!is_switchable_owner_name("Wi-Fi"));
        assert!(!is_switchable_owner_name("Open and Save Panel Service"));
        assert!(!is_switchable_owner_name("coreautha"));
        assert!(!is_switchable_owner_name(""));
    }

    #[test]
    fn normalizes_unicode_hyphens_in_owner_names() {
        // U+2011 non-breaking hyphen, as seen in some system UI strings.
        assert_eq!(normalize_owner_key("Wi\u{2011}Fi"), "wi-fi");
        assert!(!is_switchable_owner_name("Wi\u{2011}Fi"));
        assert!(!is_switchable_owner_name("  AutoFill  "));
    }

    #[test]
    fn keeps_normal_app_owners() {
        assert!(is_switchable_owner_name("Ghostty"));
        assert!(is_switchable_owner_name("Google Chrome"));
        assert!(is_switchable_owner_name("Zoom"));
        assert!(is_switchable_owner_name("ChatGPT"));
    }

    #[test]
    fn detects_helper_window_titles() {
        assert!(is_helper_window_title(
            "Select a window or an application that you want to share"
        ));
        assert!(is_helper_window_title("Zoom Client Healthcheck"));
        assert!(is_helper_window_title("Invite people to join meeting 123"));
        assert!(!is_helper_window_title("Zoom Meeting"));
        assert!(!is_helper_window_title("~/dev/fts"));
    }

    #[test]
    fn cg_fallback_drops_all_rows_beside_ax() {
        // Ghostty tabs show up as distinct CG titles next to one AX window.
        // They must not become switcher entries (no AX → AppFallback on focus).
        let ax_titles: HashSet<&str> = ["fts"].into_iter().collect();
        assert!(!should_emit_cg_fallback(
            "Ghostty",
            "Post yesterday’s growth pulse",
            &ax_titles,
            true
        ));
        assert!(!should_emit_cg_fallback("Ghostty", "fts", &ax_titles, true));
        assert!(!should_emit_cg_fallback("Ghostty", "Ghostty", &ax_titles, true));
        assert!(!should_emit_cg_fallback("Ghostty", "", &ax_titles, true));
        assert!(!should_emit_cg_fallback(
            "Zoom",
            "Select a window or an application that you want to share",
            &ax_titles,
            true
        ));
    }

    #[test]
    fn cg_fallback_rejects_denied_owners_even_when_ax_empty() {
        let empty = HashSet::new();
        assert!(!should_emit_cg_fallback("AutoFill", "AutoFill", &empty, false));
        assert!(should_emit_cg_fallback("Ghostty", "", &empty, false));
    }
}
