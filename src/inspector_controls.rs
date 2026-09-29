// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Inspector's graph-object handling facts. Turnstone owns viewer writes,
//! engine capabilities and zoom; the pane mirrors these from its followed member.

use crate::app::App;

/// The selectable viewer lanes, in radio order. `Auto` clears the override
/// (the routing policy decides); the named lanes pin an engine id.
#[cfg(feature = "weld")]
pub const VIEWER_OPTIONS: [&str; 4] = ["Auto", "genet.livery", "genet.reader", "weld.chromium"];
#[cfg(not(feature = "weld"))]
pub const VIEWER_OPTIONS: [&str; 3] = ["Auto", "genet.livery", "genet.reader"];

/// The viewer override a radio index maps to.
pub fn viewer_for_index(index: usize) -> Option<String> {
    match index {
        0 => None,
        i => VIEWER_OPTIONS.get(i).map(|s| s.to_string()),
    }
}

/// The radio index a sidecar override maps to (unknown overrides show Auto).
pub fn index_for_viewer(viewer: Option<&str>) -> usize {
    match viewer {
        Some(v) => VIEWER_OPTIONS.iter().position(|o| *o == v).unwrap_or(0),
        None => 0,
    }
}

pub fn capabilities(app: &App, member: Option<uuid::Uuid>) -> Vec<String> {
    // The REQUESTED scale, which the sidecar holds whether or not a live
    // engine ever applied it, plus the effective level beside it on the
    // lanes that read one back (retained sessions do; hosted ones do not).
    let requested_zoom = crate::app::page_zoom_display(
        member
            .and_then(|member| app.browser.get(member))
            .and_then(|state| state.page_scale),
        member
            .and_then(|member| app.content.page_zoom(member))
            .map(|zoom| zoom.applied),
    );
    let capabilities = member
        .and_then(|member| app.content.facts(member))
        .map(|facts| {
            vec![
                format!(
                    "Find in page: {}",
                    facts.capabilities.find_in_page.describe()
                ),
                format!("Page zoom: {}", facts.capabilities.page_zoom.describe()),
                format!("Requested page zoom: {requested_zoom}"),
                format!(
                    "Page capture: {}",
                    facts.capabilities.page_capture.describe()
                ),
                format!(
                    "Navigation controls: {}",
                    facts.capabilities.navigation.describe()
                ),
            ]
        })
        .unwrap_or_else(|| vec!["No live document capabilities".to_string()]);
    capabilities
}
