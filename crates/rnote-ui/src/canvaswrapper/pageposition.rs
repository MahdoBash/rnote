/// One-based page at the viewport centre, clamped to the vertical document.
/// Uses document coordinates, so canvas margins and zoom do not affect it.
pub(super) fn page_position(
    center_y: f64,
    document_y: f64,
    document_height: f64,
    page_height: f64,
) -> Option<(u32, u32)> {
    if !center_y.is_finite()
        || !document_y.is_finite()
        || !document_height.is_finite()
        || !page_height.is_finite()
        || document_height <= 0.0
        || page_height <= 0.0
    {
        return None;
    }
    let total = (document_height / page_height).ceil().max(1.0) as u32;
    let current =
        (((center_y - document_y) / page_height).floor() + 1.0).clamp(1.0, f64::from(total)) as u32;
    Some((current, total))
}

#[cfg(test)]
mod tests {
    use super::page_position;

    #[test]
    fn page_boundaries_and_partial_last_page() {
        assert_eq!(page_position(999.0, 0.0, 2500.0, 1000.0), Some((1, 3)));
        assert_eq!(page_position(1000.0, 0.0, 2500.0, 1000.0), Some((2, 3)));
        assert_eq!(page_position(2400.0, 0.0, 2500.0, 1000.0), Some((3, 3)));
    }

    #[test]
    fn overscroll_stays_within_document() {
        assert_eq!(page_position(-500.0, 0.0, 3000.0, 1000.0), Some((1, 3)));
        assert_eq!(page_position(5000.0, 0.0, 3000.0, 1000.0), Some((3, 3)));
        assert_eq!(page_position(2000.0, 0.0, 1000.0, 1000.0), Some((1, 1)));
    }

    #[test]
    fn document_origin_is_respected() {
        assert_eq!(page_position(1500.0, 500.0, 3000.0, 1000.0), Some((2, 3)));
    }

    #[test]
    fn document_growth_and_format_changes_update_count() {
        assert_eq!(page_position(1500.0, 0.0, 2000.0, 1000.0), Some((2, 2)));
        assert_eq!(page_position(1500.0, 0.0, 3000.0, 1000.0), Some((2, 3)));
        assert_eq!(page_position(1500.0, 0.0, 3000.0, 500.0), Some((4, 6)));
    }

    #[test]
    fn invalid_geometry_has_no_page_number() {
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(page_position(0.0, 0.0, 3000.0, invalid), None);
            assert_eq!(page_position(0.0, 0.0, invalid, 1000.0), None);
        }
        assert_eq!(page_position(f64::NAN, 0.0, 3000.0, 1000.0), None);
    }
}
