// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn workspace_area_for_window(
    chrome: &crate::shell_services::ShellChromeConfig,
    slot: usize,
    width: u32,
    height: u32,
) -> crate::surface::Rect {
    let primary = slot == 0;
    // The chrome is laid out in layout pixels; the plan is in physical ones.
    let scale = chrome.layout_scale(slot);
    let title = navigation_height(chrome.appearance.zoom()).max(
        chrome
            .titlebar_insets
            .get(&slot)
            .copied()
            .unwrap_or_default()
            .height,
    ) * scale;
    let visible = chrome.projects_shellbar();
    let status = if visible {
        status_height(chrome.appearance.zoom()) * scale
    } else {
        0.0
    };
    let top_status = visible
        && matches!(
            chrome.shellbar.placement,
            ChromePlacement::Docked(ChromeEdge::Top)
        );
    let top = title + if top_status { status } else { 0.0 };
    let bottom = (if top_status { 0.0 } else { status })
        + if visible && primary && chrome.layout.toolbar_bottom {
            navigation_height(chrome.appearance.zoom()) * scale
        } else {
            0.0
        };
    let top = top.min(height as f32);
    let rail = if primary && visible {
        ((if chrome.layout.activity_rail {
            cambium::ShellMetrics::ACTIVITY_RAIL_WIDTH
        } else {
            0.0
        }) + if chrome.layout.document_rail {
            cambium::ShellMetrics::DOCUMENT_NAV_WIDTH
        } else {
            0.0
        }) * chrome.appearance.zoom()
            * scale
    } else {
        0.0
    };
    let left = rail.min(width as f32);
    crate::surface::Rect::new(
        left,
        top,
        (width as f32 - left).max(0.0),
        (height as f32 - top - bottom).max(0.0),
    )
}
