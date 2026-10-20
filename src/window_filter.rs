/// Pure CGWindowList filters used by the macOS enumerator.
/// Kept platform-free so they can be unit-tested without AppKit.

const MIN_VISIBLE_ALPHA: f64 = 0.01;
/// Normal app windows are layer 0; Ghostty "Float on Top" (and similar
/// always-on-top windows) sit at NSFloatingWindowLevel ≈ layer 3.
const MAX_APP_LAYER: i64 = 3;
/// Chrome and others use tiny windows for find bars / omnibox popovers.
const MIN_SWITCHABLE_EDGE: f64 = 200.0;

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

#[cfg(test)]
mod tests {
    use super::{is_switchable_window_size, should_include_cg_window};

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
}
