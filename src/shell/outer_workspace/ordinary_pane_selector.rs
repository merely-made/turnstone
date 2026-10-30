// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn ordinary_pane_selector(
    renderers: &super::super::renderers::PaneRenderers,
    pane: PaneId,
    family: &str,
    selector: &taproot::Selector,
) -> Result<Option<(f32, f32)>, &'static str> {
    match family {
        "workbench" => renderers
            .workbench
            .get(&pane)
            .map(|view| view.selector_point(selector))
            .unwrap_or(Ok(None)),
        "settings" => renderers
            .settings
            .get(&pane)
            .map(|view| view.selector_point(selector))
            .unwrap_or(Ok(None)),
        "inspector" => renderers
            .inspector
            .get(&pane)
            .map(|view| view.selector_point(selector))
            .unwrap_or(Ok(None)),
        _ => renderers.inline_selector(pane, selector),
    }
}
