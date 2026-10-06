// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Inspector's graph-object handling facts. Turnstone owns viewer writes,
//! engine capabilities and zoom; the pane mirrors these from its followed member.

use crate::app::App;
use crate::content::{EngineAvailability, EngineDescriptor, EngineFamily};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewerOption {
    pub viewer: Option<String>,
    pub label: String,
    pub selectable: bool,
}

/// Registered and constructible lanes come first. Unavailable saved pins keep
/// their own selected row rather than being presented as Auto.
pub fn viewer_options(app: &App, saved: Option<&str>) -> Vec<ViewerOption> {
    let mut engines = app.engine_inventory.clone();
    if let Some(saved) = saved
        && !engines.iter().any(|engine| engine.id == saved)
    {
        engines.push(EngineDescriptor {
            id: saved.into(),
            label: saved.into(),
            family: EngineFamily::Unknown,
            availability: EngineAvailability::Unavailable {
                reason: "this host has no registration or construction path for the saved engine"
                    .into(),
            },
        });
    }
    engines.sort_by_key(|engine| {
        let rank = match engine.id.as_str() {
            "genet.livery" => 0,
            "genet.reader" => 1,
            _ => 2,
        };
        (
            !engine.availability.is_selectable(),
            rank,
            engine.id.clone(),
        )
    });
    std::iter::once(ViewerOption {
        viewer: None,
        label: "Auto".into(),
        selectable: true,
    })
    .chain(engines.into_iter().map(|engine| ViewerOption {
        viewer: Some(engine.id.clone()),
        label: match &engine.availability {
            EngineAvailability::Registered => engine.label.clone(),
            availability => format!("{} ({})", engine.label, availability.describe()),
        },
        selectable: engine.availability.is_selectable(),
    }))
    .collect()
}

pub fn index_for_viewer(options: &[ViewerOption], viewer: Option<&str>) -> usize {
    options
        .iter()
        .position(|option| option.viewer.as_deref() == viewer)
        .unwrap_or(options.len())
}

/// `None` refuses an unavailable/out-of-range row; `Some(None)` selects Auto.
pub fn viewer_for_index(options: &[ViewerOption], index: usize) -> Option<Option<String>> {
    options
        .get(index)
        .filter(|option| option.selectable)
        .map(|option| option.viewer.clone())
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
