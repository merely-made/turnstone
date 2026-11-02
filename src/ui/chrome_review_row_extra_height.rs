// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// How much taller the intact install review is than an ordinary row. Both
/// use the real card's containing width and live sheet, including zoom.
pub(crate) fn chrome_review_row_extra_height(
    text: &str,
    appearance: &crate::shell_services::AppearanceConfig,
) -> f32 {
    let (review_height, _) = chrome_card_row_metrics(text, true, appearance);
    let (ordinary_height, _) = ordinary_chrome_row_metrics(appearance.ui_zoom);
    (review_height - ordinary_height).max(0.0)
}
