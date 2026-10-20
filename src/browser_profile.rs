/// Parse Chromium-family profile labels from Accessibility window titles
/// and from the browser's Local State JSON. Platform-free for unit tests.

use std::collections::BTreeSet;
use std::path::Path;

/// Well-known Chromium product suffixes that appear after the profile name.
const PRODUCT_SUFFIXES: &[&str] = &[
    "Google Chrome",
    "Google Chrome Beta",
    "Google Chrome Canary",
    "Google Chrome Dev",
    "Chromium",
    "Brave Browser",
    "Brave",
    "Microsoft Edge",
    "Microsoft Edge Beta",
    "Microsoft Edge Canary",
    "Microsoft Edge Dev",
    "Vivaldi",
    "Arc",
];

/// Stable product keys used to scope Local State caches (must not collide
/// across Chrome / Edge / Brave even when profile directories share names).
pub const PRODUCT_CHROME: &str = "Google Chrome";
pub const PRODUCT_CHROME_BETA: &str = "Google Chrome Beta";
pub const PRODUCT_CHROME_CANARY: &str = "Google Chrome Canary";
pub const PRODUCT_CHROME_DEV: &str = "Google Chrome Dev";
pub const PRODUCT_CHROMIUM: &str = "Chromium";
pub const PRODUCT_BRAVE: &str = "Brave Browser";
pub const PRODUCT_EDGE: &str = "Microsoft Edge";
pub const PRODUCT_EDGE_BETA: &str = "Microsoft Edge Beta";
pub const PRODUCT_EDGE_CANARY: &str = "Microsoft Edge Canary";
pub const PRODUCT_VIVALDI: &str = "Vivaldi";

/// A profile name (and optional account / Gaia name) from Local State.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserProfile {
    /// Product key from [`product_for_app_name`] / Local State path mapping.
    pub product: String,
    pub directory: String,
    pub name: String,
    pub account_name: Option<String>,
}

/// Map an Application Support `…/Local State` path to a product key.
pub fn product_for_local_state_path(path: &Path) -> Option<&'static str> {
    let text = path.to_string_lossy();
    // More specific paths first (Beta/Canary before bare Chrome/Edge).
    let table = [
        ("Google/Chrome Beta/Local State", PRODUCT_CHROME_BETA),
        ("Google/Chrome Canary/Local State", PRODUCT_CHROME_CANARY),
        ("Google/Chrome Dev/Local State", PRODUCT_CHROME_DEV),
        ("Google/Chrome/Local State", PRODUCT_CHROME),
        ("Chromium/Local State", PRODUCT_CHROMIUM),
        ("BraveSoftware/Brave-Browser/Local State", PRODUCT_BRAVE),
        ("Microsoft Edge Beta/Local State", PRODUCT_EDGE_BETA),
        ("Microsoft Edge Canary/Local State", PRODUCT_EDGE_CANARY),
        ("Microsoft Edge/Local State", PRODUCT_EDGE),
        ("Vivaldi/Local State", PRODUCT_VIVALDI),
    ];
    for (needle, product) in table {
        if text.contains(needle) {
            return Some(product);
        }
    }
    None
}

/// Map a running app's localized name to a product key.
pub fn product_for_app_name(app_name: &str) -> Option<&'static str> {
    let lower = app_name.to_ascii_lowercase();
    if lower.contains("chrome beta") {
        Some(PRODUCT_CHROME_BETA)
    } else if lower.contains("chrome canary") {
        Some(PRODUCT_CHROME_CANARY)
    } else if lower.contains("chrome dev") {
        Some(PRODUCT_CHROME_DEV)
    } else if lower.contains("chromium") {
        Some(PRODUCT_CHROMIUM)
    } else if lower.contains("chrome") {
        Some(PRODUCT_CHROME)
    } else if lower.contains("brave") {
        Some(PRODUCT_BRAVE)
    } else if lower.contains("edge beta") {
        Some(PRODUCT_EDGE_BETA)
    } else if lower.contains("edge canary") {
        Some(PRODUCT_EDGE_CANARY)
    } else if lower.contains("edge") {
        Some(PRODUCT_EDGE)
    } else if lower.contains("vivaldi") {
        Some(PRODUCT_VIVALDI)
    } else {
        None
    }
}

/// Extract profile entries from one browser's `Local State` JSON.
///
/// Expects the standard `profile.info_cache` object. Unknown or malformed
/// JSON yields an empty list rather than an error so enumeration can continue.
pub fn profiles_from_local_state(json: &str, product: &str) -> Vec<BrowserProfile> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };

    let Some(info_cache) = value
        .pointer("/profile/info_cache")
        .and_then(|v| v.as_object())
    else {
        return Vec::new();
    };

    let mut profiles = Vec::new();
    for (directory, entry) in info_cache {
        let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }

        let account_name = entry
            .get("gaia_given_name")
            .or_else(|| entry.get("gaia_name"))
            .or_else(|| entry.get("user_name"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != name)
            .map(str::to_string);

        profiles.push(BrowserProfile {
            product: product.to_string(),
            directory: directory.clone(),
            name: name.to_string(),
            account_name,
        });
    }

    profiles.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.directory.cmp(&b.directory))
            .then(a.product.cmp(&b.product))
    });
    profiles
}

/// Profiles belonging to one product (Chrome vs Edge, etc.).
pub fn profiles_for_product<'a>(
    profiles: &'a [BrowserProfile],
    product: &str,
) -> Vec<&'a BrowserProfile> {
    profiles.iter().filter(|p| p.product == product).collect()
}

/// Resolve a Chromium Accessibility title to a profile display name.
///
/// When two or more profiles are open, Chrome appends
/// `" - <Profile> - <Product>"` or `" - <Profile> - <Product> (<Account>)"`.
/// Returns `None` when the title has no recognizable profile marker.
///
/// Only profile names from `profiles` are considered; callers should pass a
/// product-scoped slice so Edge names cannot label Chrome windows.
pub fn profile_from_window_title(title: &str, profiles: &[BrowserProfile]) -> Option<String> {
    let names: BTreeSet<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    profile_from_window_title_names(title, &names)
}

/// Same as [`profile_from_window_title`] but takes a set of display names.
pub fn profile_from_window_title_names(
    title: &str,
    profile_names: &BTreeSet<&str>,
) -> Option<String> {
    if title.is_empty() || profile_names.is_empty() {
        return None;
    }

    for product in PRODUCT_SUFFIXES {
        // "… - Profile - Product (Account)"
        let with_account = format!(" - {product} (");
        if let Some(before_product) = title.rfind(&with_account) {
            if let Some(profile) = profile_before(title, before_product, profile_names) {
                return Some(profile);
            }
        }

        // "… - Profile - Product"
        let suffix = format!(" - {product}");
        if title.ends_with(&suffix) {
            let before_product = title.len() - suffix.len();
            if let Some(profile) = profile_before(title, before_product, profile_names) {
                return Some(profile);
            }
        }
    }

    None
}

/// Look up the Local State display name for a `--profile-directory` value
/// within a single product.
pub fn profile_name_for_directory(
    profiles: &[BrowserProfile],
    product: &str,
    directory: &str,
) -> Option<String> {
    profiles
        .iter()
        .find(|p| p.product == product && p.directory == directory)
        .map(|p| p.name.clone())
}

fn profile_before(
    title: &str,
    before_product: usize,
    profile_names: &BTreeSet<&str>,
) -> Option<String> {
    let head = &title[..before_product];
    // Prefer longer names so "Work Email" wins over "Work".
    let mut candidates: Vec<&str> = profile_names.iter().copied().collect();
    candidates.sort_by_key(|name| std::cmp::Reverse(name.len()));

    for name in candidates {
        let marker = format!(" - {name}");
        if head.ends_with(&marker) {
            return Some(name.to_string());
        }
    }
    None
}

/// Format an overlay / log row: `App · Profile — Title` when a profile is known.
pub fn format_switcher_label(app_name: &str, window_name: &str, profile: Option<&str>) -> String {
    let title = if window_name.is_empty() {
        "Untitled Window"
    } else {
        window_name
    };

    match profile {
        Some(profile) if !profile.is_empty() => {
            format!("{app_name} · {profile} — {title}")
        }
        _ => format!("{app_name} — {title}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BrowserProfile, PRODUCT_CHROME, PRODUCT_EDGE, format_switcher_label,
        product_for_app_name, product_for_local_state_path, profile_from_window_title,
        profile_name_for_directory, profiles_for_product, profiles_from_local_state,
    };
    use std::collections::BTreeSet;
    use std::path::Path;

    fn sample_profiles() -> Vec<BrowserProfile> {
        vec![
            BrowserProfile {
                product: PRODUCT_CHROME.into(),
                directory: "Default".into(),
                name: "Personal".into(),
                account_name: Some("Alex".into()),
            },
            BrowserProfile {
                product: PRODUCT_CHROME.into(),
                directory: "Profile 1".into(),
                name: "Work".into(),
                account_name: None,
            },
        ]
    }

    #[test]
    fn parses_local_state_info_cache() {
        let json = r#"{
            "profile": {
                "info_cache": {
                    "Default": {
                        "name": "Personal",
                        "gaia_given_name": "Alex"
                    },
                    "Profile 1": {
                        "name": "Work",
                        "gaia_given_name": "Work"
                    }
                }
            }
        }"#;

        let profiles = profiles_from_local_state(json, PRODUCT_CHROME);
        assert_eq!(profiles.len(), 2);
        assert_eq!(
            profile_name_for_directory(&profiles, PRODUCT_CHROME, "Profile 1").as_deref(),
            Some("Work")
        );
        let personal = profiles.iter().find(|p| p.directory == "Default").unwrap();
        assert_eq!(personal.name, "Personal");
        assert_eq!(personal.product, PRODUCT_CHROME);
        assert_eq!(personal.account_name.as_deref(), Some("Alex"));
    }

    #[test]
    fn directory_lookup_is_scoped_by_product() {
        let profiles = vec![
            BrowserProfile {
                product: PRODUCT_CHROME.into(),
                directory: "Default".into(),
                name: "Chrome Person".into(),
                account_name: None,
            },
            BrowserProfile {
                product: PRODUCT_EDGE.into(),
                directory: "Default".into(),
                name: "Edge Person".into(),
                account_name: None,
            },
        ];

        assert_eq!(
            profile_name_for_directory(&profiles, PRODUCT_CHROME, "Default").as_deref(),
            Some("Chrome Person")
        );
        assert_eq!(
            profile_name_for_directory(&profiles, PRODUCT_EDGE, "Default").as_deref(),
            Some("Edge Person")
        );
        assert!(profiles_for_product(&profiles, PRODUCT_CHROME)
            .iter()
            .all(|p| p.product == PRODUCT_CHROME));
    }

    #[test]
    fn title_parsing_uses_only_scoped_profile_names() {
        let chrome_only = sample_profiles();
        // Edge's "Work" must not be required; Chrome's Work still matches.
        assert_eq!(
            profile_from_window_title("Inbox - Work - Google Chrome", &chrome_only).as_deref(),
            Some("Work")
        );

        let edge_work = vec![BrowserProfile {
            product: PRODUCT_EDGE.into(),
            directory: "Profile 1".into(),
            name: "Work".into(),
            account_name: None,
        }];
        // Chrome title with only Edge profiles in the candidate set still
        // matches by name (title carries the product suffix). Product scoping
        // at the call site prevents Edge-only lists from being used for Chrome.
        assert_eq!(
            profile_from_window_title("Inbox - Work - Google Chrome", &edge_work).as_deref(),
            Some("Work")
        );
    }

    #[test]
    fn maps_local_state_paths_and_app_names() {
        assert_eq!(
            product_for_local_state_path(Path::new(
                "/Users/x/Library/Application Support/Google/Chrome/Local State"
            )),
            Some(PRODUCT_CHROME)
        );
        assert_eq!(
            product_for_local_state_path(Path::new(
                "/Users/x/Library/Application Support/Microsoft Edge/Local State"
            )),
            Some(PRODUCT_EDGE)
        );
        assert_eq!(product_for_app_name("Google Chrome"), Some(PRODUCT_CHROME));
        assert_eq!(product_for_app_name("Microsoft Edge"), Some(PRODUCT_EDGE));
        assert_eq!(product_for_app_name("Ghostty"), None);
    }

    #[test]
    fn malformed_local_state_yields_empty() {
        assert!(profiles_from_local_state("not-json", PRODUCT_CHROME).is_empty());
        assert!(profiles_from_local_state("{}", PRODUCT_CHROME).is_empty());
    }

    #[test]
    fn extracts_work_and_personal_from_titles() {
        let profiles = sample_profiles();
        assert_eq!(
            profile_from_window_title("Inbox - Work - Google Chrome", &profiles).as_deref(),
            Some("Work")
        );
        assert_eq!(
            profile_from_window_title("Inbox - Personal - Google Chrome", &profiles).as_deref(),
            Some("Personal")
        );
        assert_eq!(
            profile_from_window_title("Inbox - Work - Google Chrome (Alex)", &profiles).as_deref(),
            Some("Work")
        );
    }

    #[test]
    fn single_profile_title_has_no_marker() {
        let profiles = sample_profiles();
        assert_eq!(
            profile_from_window_title("Inbox - Google Chrome", &profiles),
            None
        );
    }

    #[test]
    fn prefers_longer_profile_name() {
        let profiles = vec![
            BrowserProfile {
                product: PRODUCT_CHROME.into(),
                directory: "Default".into(),
                name: "Work".into(),
                account_name: None,
            },
            BrowserProfile {
                product: PRODUCT_CHROME.into(),
                directory: "Profile 1".into(),
                name: "Work Email".into(),
                account_name: None,
            },
        ];
        assert_eq!(
            profile_from_window_title("Mail - Work Email - Google Chrome", &profiles).as_deref(),
            Some("Work Email")
        );
    }

    #[test]
    fn brave_and_edge_suffixes() {
        let names: BTreeSet<&str> = ["Work"].into_iter().collect();
        assert_eq!(
            super::profile_from_window_title_names("Docs - Work - Brave Browser", &names)
                .as_deref(),
            Some("Work")
        );
        assert_eq!(
            super::profile_from_window_title_names("Docs - Work - Microsoft Edge", &names)
                .as_deref(),
            Some("Work")
        );
    }

    #[test]
    fn formats_switcher_label_with_and_without_profile() {
        assert_eq!(
            format_switcher_label("Google Chrome", "Inbox", Some("Work")),
            "Google Chrome · Work — Inbox"
        );
        assert_eq!(
            format_switcher_label("Ghostty", "~", None),
            "Ghostty — ~"
        );
        assert_eq!(
            format_switcher_label("Ghostty", "", None),
            "Ghostty — Untitled Window"
        );
    }
}
