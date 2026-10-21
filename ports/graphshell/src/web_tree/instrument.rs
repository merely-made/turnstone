// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Instrument presentation of the existing local mere and its scene.
//! The graph/session, source keys, Forme arrangement and application appearance
//! retain their current owners. This module is absent from the read-only viewer.

use super::{Child, TreePage};
use cambium::{
    ShellControl, ShellDensity, WorkspacePaneStatus, el, shell_control, shell_popup,
    shell_projection_switch, shell_toolbar, title_bar, workspace_pane_header,
};

#[derive(Default)]
pub(super) struct State {
    pub panes_open: bool,
    pub graph_controls: bool,
    pub document_controls: bool,
}

pub(super) const SHEET: &str = r#"
.tree-instrument { --workspace-font-interface:IBM Plex Sans,sans-serif; --workspace-font-reading:Newsreader,serif; }
.tree-instrument .cambium-title-bar h1 { margin:0; font-size:14px; font-weight:500; }
.tree-instrument .cambium-title-bar { --titlebar-min-height:40px; --titlebar-gap:6px; padding-top:0; padding-bottom:0; border-bottom:1px solid var(--workspace-rule); flex-wrap:nowrap; position:relative; }
.tree-instrument .title-bar-title { flex:1 1 auto; min-width:0; }
.tree-instrument .workspace-app-identity { font-size:12px; white-space:nowrap; overflow:hidden; }
.tree-instrument .workspace-app-identity h1 { display:inline; }
.tree-instrument .workspace-shell-header { position:relative; flex:0 0 auto; z-index:25; }
.tree-instrument .workspace-panes { position:absolute; top:100%; right:8px; z-index:25; width:220px; }
.tree-instrument .cambium-shell-toolbar { padding:0; }
.tree-instrument { --shell-background:var(--workspace-surface, #17232b); --shell-foreground:var(--workspace-ink, #dce3e8); --shell-rule:var(--workspace-rule, #637581); --shell-selection:var(--workspace-rule, #263640); }
.tree-instrument .workspace-app-context { margin:2px 0 0; font-size:12px; color:var(--workspace-muted); }
.tree-instrument .workspace-app-identity { display:flex; flex-direction:row; align-items:center; gap:4px; }
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
    // The document editor has its own reading/authoring toolbar and Close
    // action. Do not offer hidden graph projections while it owns the scene.
    let navigation_active = !super::blocks_active(page);
    let density = ShellDensity::for_width(page.size.0 as f32, 1.0);
    let narrow = density.is_compact();
    let mut trailing: Vec<Child> = Vec::new();
    if navigation_active && page.forme.is_some() {
        trailing.push(shell_projection_switch(
            !page.forme.as_ref().is_some_and(|pane| pane.workbench),
            density == ShellDensity::Tiny,
            |page: &mut TreePage, map| {
                super::forme::show_projection(page, map);
                page.instrument.panes_open = false;
            },
        ));
    }
    if navigation_active && !narrow {
        trailing.push(control(
            "Inspector",
            ShellControl {
                pressed: Some(page.tools_open),
                ..Default::default()
            },
            |page| {
                page.tools_open = !page.tools_open;
                page.instrument.panes_open = false;
            },
        ));
    }
    if navigation_active {
        trailing.push(control(
            "Panes",
            ShellControl {
                expanded: Some(page.instrument.panes_open),
                ..Default::default()
            },
            |page| page.instrument.panes_open = !page.instrument.panes_open,
        ));
    }
    let toolbar = shell_toolbar(
        "Map navigation",
        Vec::new(),
        Box::new(
            el(
                "div",
                (
                    el("h1", "Graphshell"),
                    (!narrow).then(|| el("span", format!(" · {} documents", page.nodes))),
                ),
            )
            .attr("class", "workspace-app-identity")
            .attr("data-mere-session", product.session.clone()),
        ),
        trailing,
    );
    let bar = title_bar(
        Box::new(el("span", "mere").attr("class", "workspace-brand")),
        toolbar,
        Box::new(el("div", ())),
        Box::new(el("div", ())),
    );
    let popup = (navigation_active && page.instrument.panes_open).then(|| {
        Box::new(
            el(
                "div",
                shell_popup(
                    "Panes",
                    vec![
                        control(
                            "Inspector",
                            ShellControl {
                                pressed: Some(page.tools_open),
                                ..Default::default()
                            },
                            |page| {
                                page.tools_open = !page.tools_open;
                                page.instrument.panes_open = false;
                            },
                        ),
                        control(
                            "Graph controls",
                            ShellControl {
                                pressed: Some(page.instrument.graph_controls),
                                ..Default::default()
                            },
                            |page| {
                                page.instrument.graph_controls = !page.instrument.graph_controls;
                                page.instrument.panes_open = false;
                            },
                        ),
                        control(
                            "Document tools",
                            ShellControl {
                                pressed: Some(page.instrument.document_controls),
                                ..Default::default()
                            },
                            |page| {
                                page.instrument.document_controls =
                                    !page.instrument.document_controls;
                                page.instrument.panes_open = false;
                            },
                        ),
                        control(
                            "Read selected",
                            ShellControl {
                                disabled: page.shared.canvas.borrow().selected_members().is_empty()
                                    || page.forme.is_none(),
                                ..Default::default()
                            },
                            |page| {
                                super::forme::open_selected(page);
                                page.instrument.panes_open = false;
                            },
                        ),
                        control("All commands…", ShellControl::default(), |page| {
                            page.instrument.panes_open = false;
                            super::commands::open_keyboard(page);
                        }),
                    ],
                ),
            )
            .attr("class", "workspace-panes"),
        ) as Child
    });
    Some(Box::new(cambium::on_key(
        el("div", (bar, popup)).attr("class", "workspace-shell-header"),
        |page: &mut TreePage, key: cambium::KeyEvent| {
            if matches!(key.key, cambium::Key::Named(cambium::NamedKey::Escape))
                && page.instrument.panes_open
            {
                page.instrument.panes_open = false;
                key.stop_propagation();
                key.prevent_default();
            }
        },
    )))
}

fn control(label: &str, state: ShellControl, activate: impl Fn(&mut TreePage) + 'static) -> Child {
    shell_control(label, label, state, move |page: &mut TreePage, _| {
        activate(page);
        page.shared.dirty.set(true);
    })
}

pub(super) fn graph_controls(page: &TreePage) -> bool {
    !enabled(page) || page.instrument.graph_controls
}

pub(super) fn document_controls(page: &TreePage) -> bool {
    !enabled(page) || page.instrument.document_controls
}

pub(super) fn dismiss(page: &TreePage) -> Option<Child> {
    (enabled(page) && !super::blocks_active(page) && page.instrument.panes_open).then(|| {
        Box::new(cambium::on_click(el("div", ())
            .attr("aria-hidden", "true")
            .attr("style", format!("position:absolute;top:0px;left:0px;width:{}px;height:{}px;z-index:24;background:transparent;", page.size.0, page.size.1)),
            |page: &mut TreePage, click: cambium::PointerClick| {
                page.instrument.panes_open = false;
                page.shared.dirty.set(true);
                click.stop_propagation();
            })) as Child
    })
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
