/// Parse Chromium-family profile labels from Accessibility window titles
/// and from the browser's Local State JSON. Platform-free for unit tests.

use std::collections::BTreeSet;

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

/// A profile name (and optional account / Gaia name) from Local State.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserProfile {
    pub directory: String,
    pub name: String,
    pub account_name: Option<String>,
}

/// Extract profile entries from Chrome/Brave/Edge `Local State` JSON.
///
/// Expects the standard `profile.info_cache` object. Unknown or malformed
/// JSON yields an empty list rather than an error so enumeration can continue.
pub fn profiles_from_local_state(json: &str) -> Vec<BrowserProfile> {
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
            directory: directory.clone(),
            name: name.to_string(),
            account_name,
        });
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name).then(a.directory.cmp(&b.directory)));
    profiles
}

/// Resolve a Chromium Accessibility title to a profile display name.
///
/// When two or more profiles are open, Chrome appends
/// `" - <Profile> - <Product>"` or `" - <Profile> - <Product> (<Account>)"`.
/// Returns `None` when the title has no recognizable profile marker.
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

/// Look up the Local State display name for a `--profile-directory` value.
pub fn profile_name_for_directory(profiles: &[BrowserProfile], directory: &str) -> Option<String> {
    profiles
        .iter()
        .find(|p| p.directory == directory)
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
        BrowserProfile, format_switcher_label, profile_from_window_title,
        profile_name_for_directory, profiles_from_local_state,
    };
    use std::collections::BTreeSet;

    fn sample_profiles() -> Vec<BrowserProfile> {
        vec![
            BrowserProfile {
                directory: "Default".into(),
                name: "Personal".into(),
                account_name: Some("Alex".into()),
            },
            BrowserProfile {
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

        let profiles = profiles_from_local_state(json);
        assert_eq!(profiles.len(), 2);
        assert_eq!(
            profile_name_for_directory(&profiles, "Profile 1").as_deref(),
            Some("Work")
        );
        let personal = profiles.iter().find(|p| p.directory == "Default").unwrap();
        assert_eq!(personal.name, "Personal");
        assert_eq!(personal.account_name.as_deref(), Some("Alex"));
    }

    #[test]
    fn malformed_local_state_yields_empty() {
        assert!(profiles_from_local_state("not-json").is_empty());
        assert!(profiles_from_local_state("{}").is_empty());
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
            profile_from_window_title(
                "Inbox - Work - Google Chrome (Alex)",
                &profiles
            )
            .as_deref(),
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
                directory: "Default".into(),
                name: "Work".into(),
                account_name: None,
            },
            BrowserProfile {
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
