// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn fit(app: &mut App, chrome: &crate::shell_services::ShellChromeConfig) {
    // Research rows carry source identities and excerpts. Estimate height at
    // the standard card width; narrower cards use their bounded scroll owner.
    let mut extra = 0.0;
    let mut fitted = 0;
    for suggestion in &app.omnibar.suggestions {
        extra += crate::ui::chrome_review_row_extra_height(
            &crate::chrome_view::row_text(suggestion),
            &chrome.appearance,
        );
        let capacity = crate::ui::visible_row_limit_with_extra_height(
            chrome.omnibar.row_limit,
            &chrome.omnibar.placement,
            app.viewport.1 / chrome.layout_scale(0),
            chrome.appearance.ui_zoom,
            extra,
        );
        if fitted >= capacity {
            break;
        }
        fitted += 1;
    }
    app.omnibar.suggestions.truncate(fitted.max(1));
}
