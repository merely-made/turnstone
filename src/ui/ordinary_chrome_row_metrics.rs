// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn ordinary_chrome_row_metrics(ui_zoom: f32) -> (f32, f32) {
    let appearance = crate::shell_services::AppearanceConfig {
        ui_zoom,
        ..Default::default()
    };
    let zoom = appearance.zoom();
    CHROME_ROW_METRICS.with(|cached| {
        if let Some((previous_zoom, metrics)) = *cached.borrow() {
            if previous_zoom == zoom {
                return metrics;
            }
        }
        let metrics = chrome_card_row_metrics("x", false, &appearance);
        *cached.borrow_mut() = Some((zoom, metrics));
        metrics
    })
}
