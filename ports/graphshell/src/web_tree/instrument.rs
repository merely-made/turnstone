// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Instrument presentation of the existing local mere and its scene.
//! The graph/session, source keys, Forme arrangement and application appearance
//! retain their current owners. This module is absent from the read-only viewer.

use super::{Child, TreePage};
use cambium::{WorkspacePaneStatus, el, title_bar, workspace_pane_header};

pub(super) const SHEET: &str = r#"
.tree-instrument { --workspace-font-interface:IBM Plex Sans,sans-serif; --workspace-font-reading:Newsreader,serif; }
.tree-instrument > .cambium-title-bar h1 { margin:0; font-size:17px; font-weight:500; }
.tree-instrument .workspace-app-context { margin:2px 0 0; font-size:12px; color:var(--workspace-muted); }
.tree-instrument .workspace-app-identity { display:flex; flex-direction:column; }
.tree-instrument .workspace-brand { font-size:14px; font-weight:500; color:var(--tabard-color-accent, var(--workspace-accent)); }
.tree-instrument .tree-status { color:var(--workspace-muted); font-size:12px; }
.tree-instrument .tree-controls { padding:6px 12px; border-bottom:1px solid var(--workspace-rule); }
.tree-instrument > .tree-product { position:static; flex:0 0 auto; max-width:none; max-height:140px; overflow-y:auto; padding:6px 12px; border-bottom:1px solid var(--workspace-rule); }
.tree-instrument .tree-controls button, .tree-instrument .tree-tools button, .tree-instrument .tree-product button { border-radius:0; font-size:13px; }
.tree-instrument .tree-body { border-top:1px solid var(--workspace-rule); }
.tree-instrument .tree-tools { padding:0 10px 10px; border-left:1px solid var(--workspace-rule); }
.tree-instrument .tree-tools > .workspace-pane-header { padding-left:0; padding-right:0; margin-bottom:10px; }
.tree-instrument .tree-graph > .workspace-pane-header { margin:0; }
.tree-instrument .tree-tools .detail-section-title { font-weight:500; }
.tree-instrument .tree-tools .detail-key { font-size:12px; color:var(--workspace-muted); }
.tree-instrument .tree-tools .detail-value { margin-bottom:10px; }
.tree-instrument .tools-section + .tools-section { padding-top:10px; }
.tree-instrument .reading-pane { font-family:var(--tabard-font-reading, var(--workspace-font-reading)),var(--workspace-font-reading); font-size:var(--tabard-font-reading-size, 16px); line-height:var(--tabard-font-reading-line-height, 1.6); }
.tree-instrument .reading-pane button { font-family:var(--tabard-font-interface, var(--workspace-font-interface)),var(--workspace-font-interface); font-size:var(--tabard-font-interface-size, 13px); }
.tree-instrument .tree-product, .tree-instrument .tree-detail { border-radius:0; }
"#;

pub(super) fn enabled(page: &TreePage) -> bool {
    page.product.is_some() && page.presenter.is_none()
}

pub(super) fn app_header(page: &TreePage) -> Option<Child> {
    if !enabled(page) {
        return None;
    }
    let product = page.product.as_ref()?;
    Some(title_bar(
        Box::new(el("span", "mere").attr("class", "workspace-brand")),
        Box::new(
            el(
                "div",
                (
                    el("h1", "Graphshell"),
                    el("p", "Local mere / retained material")
                        .attr("class", "workspace-app-context"),
                ),
            )
            .attr("class", "workspace-app-identity")
            .attr("data-mere-session", product.session.clone()),
        ),
        Box::new(el("div", ())),
        Box::new(el("div", ())),
    ))
}

pub(super) fn scene_header(page: &TreePage, workbench: bool) -> Option<Child> {
    if !enabled(page) {
        return None;
    }
    page.product.as_ref()?;
    Some(workspace_pane_header(
        if workbench { "Workbench" } else { "Dataspace" },
        format!(
            "{} items / {}",
            page.nodes,
            if workbench {
                "Tiled presentation"
            } else {
                "Graph presentation"
            }
        ),
        WorkspacePaneStatus::default(),
        Box::new(el("div", ())),
    ))
}

pub(super) fn inspector_header(page: &TreePage) -> Option<Child> {
    if !enabled(page) {
        return None;
    }
    let product = page.product.as_ref()?;
    Some(workspace_pane_header(
        "Inspector",
        "Selection, scene and presentation",
        WorkspacePaneStatus {
            selected: product.selected.is_some(),
            loading: product.saving,
            ..Default::default()
        },
        Box::new(el("div", ())),
    ))
}
