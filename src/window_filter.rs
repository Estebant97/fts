/// Pure CGWindowList filters used by the macOS enumerator.
/// Kept platform-free so they can be unit-tested without AppKit.

const MIN_VISIBLE_ALPHA: f64 = 0.01;

/// Normal application windows are layer 0. Skip menu bar, overlays, and
/// fully (or nearly) transparent surfaces.
pub fn should_include_cg_window(layer: i64, alpha: f64) -> bool {
    layer == 0 && alpha.is_finite() && alpha > MIN_VISIBLE_ALPHA
}

#[cfg(test)]
mod tests {
    use super::should_include_cg_window;

    #[test]
    fn keeps_opaque_normal_windows() {
        assert!(should_include_cg_window(0, 1.0));
        assert!(should_include_cg_window(0, 0.5));
    }

    #[test]
    fn drops_system_layers_and_invisible_windows() {
        assert!(!should_include_cg_window(25, 1.0));
        assert!(!should_include_cg_window(0, 0.0));
        assert!(!should_include_cg_window(0, 0.001));
        assert!(!should_include_cg_window(0, f64::NAN));
    }
}
