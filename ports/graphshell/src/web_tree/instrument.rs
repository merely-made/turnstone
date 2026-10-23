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
    shell_toolbar, title_bar, workspace_pane_header,
};

#[derive(Default)]
pub(super) struct State {
    pub panes_open: bool,
    pub graph_controls: bool,
    pub document_controls: bool,
    pub document_rail: bool,
    pub binding_open: bool,
    pub navigation: super::navigation::State,
    pub scope: super::scope_navigation::State,
}

pub(super) const SHEET: &str = r#"
.tree-instrument .workspace-app-identity { flex-wrap:wrap; }
.tree-instrument .scope-overmap { padding:24px; background:var(--workspace-surface); height:100%; }
.tree-instrument .scope-overmap h2 { font-size:16px; font-weight:500; margin:0 0 16px; }
.tree-instrument { --workspace-font-interface:IBM Plex Sans,sans-serif; --workspace-font-reading:Newsreader,serif; }
.tree-instrument .cambium-title-bar h1 { margin:0; font-size:14px; font-weight:500; }
.tree-instrument .cambium-title-bar { --titlebar-min-height:36px; --titlebar-gap:6px; padding-top:0; padding-bottom:0; border-bottom:1px solid var(--workspace-rule); flex-wrap:nowrap; position:relative; }
.tree-instrument .title-bar-title { flex:1 1 auto; min-width:0; }
.tree-instrument .workspace-app-identity { font-size:12px; white-space:nowrap; overflow:hidden; }
.tree-instrument .workspace-app-identity h1 { display:inline; }
.tree-instrument .workspace-shell-header { position:relative; flex:0 0 auto; z-index:25; }
.tree-instrument .workspace-panes { position:absolute; top:100%; right:8px; z-index:25; width:220px; }
.tree-instrument .workspace-related { position:absolute; top:100%; right:8px; z-index:25; width:360px; max-width:90%; max-height:70vh; overflow-y:auto; }
.tree-instrument .workspace-related-heading { margin:0; font-weight:500; }
.tree-instrument .tree-tools > .cambium-source-binding { margin-bottom:8px; }
.tree-instrument .workspace-related .shell-navigation-detail { overflow-wrap:anywhere; }
.tree-instrument .cambium-shell-toolbar { padding:0; }
.tree-instrument { --shell-background:var(--workspace-surface, #17232b); --shell-foreground:var(--workspace-ink, #dce3e8); --shell-rule:var(--workspace-rule, #637581); --shell-selection:var(--workspace-rule, #263640); }
.tree-instrument .workspace-app-context { margin:2px 0 0; font-size:12px; color:var(--workspace-muted); }
.tree-instrument .workspace-app-identity { display:flex; flex-direction:row; align-items:center; gap:4px; }
.tree-instrument .workspace-brand { font-size:14px; font-weight:500; color:var(--tabard-color-accent, var(--workspace-accent)); }
.tree-instrument .tree-status { color:var(--workspace-muted); font-size:12px; }
.tree-instrument .tree-controls { padding:6px 12px; border-bottom:1px solid var(--workspace-rule); }
.tree-instrument > .tree-product { position:static; flex:0 0 auto; max-width:none; max-height:140px; overflow-y:auto; padding:6px 12px; border-bottom:1px solid var(--workspace-rule); }
.tree-instrument .tree-controls button, .tree-instrument .tree-tools button, .tree-instrument .tree-product button { border-radius:0; font-size:13px; }
.tree-instrument .tree-body > .cambium-activity-rail, .tree-instrument .tree-body > .cambium-shell-documents { height:auto; align-self:stretch; }
.tree-instrument .tree-body { border-top:1px solid var(--workspace-rule); }
.tree-instrument .tree-tools { padding:0 10px 10px; border-left:1px solid var(--workspace-rule); }
.tree-instrument .tree-tools > .workspace-pane-header { padding-left:0; padding-right:0; margin-bottom:8px; }
.tree-instrument .tree-graph > .workspace-pane-header { margin:0; }
.tree-instrument .tree-tools .detail-section-title { font-weight:500; }
.tree-instrument .tree-tools .detail-key { font-size:12px; color:var(--workspace-muted); }
.tree-instrument .tree-tools .detail-value { margin-bottom:8px; }
.tree-instrument .tools-section + .tools-section { padding-top:10px; }
.tree-instrument .reading-pane { font-family:var(--tabard-font-reading, var(--workspace-font-reading)),var(--workspace-font-reading); font-size:var(--tabard-font-reading-size, 18px); line-height:var(--tabard-font-reading-line-height, 1.6); }
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
    if navigation_active {
        trailing.push(super::navigation::control(page));
        trailing.push(control(
            "Panes",
            ShellControl {
                expanded: Some(page.instrument.panes_open),
                ..Default::default()
            },
            |page| {
                page.instrument.panes_open = !page.instrument.panes_open;
                page.instrument.navigation.open = false;
            },
        ));
    }
    let toolbar = shell_toolbar(
        "Map navigation",
        if navigation_active {vec![super::scope_navigation::controls(page)]} else {Vec::new()},
        Box::new(
            el(
                "div",
                (
                    el("h1", "Graphshell"),
                    super::scope_navigation::trail(page),
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
        el("div", (bar, popup, super::navigation::popup(page))).attr("class", "workspace-shell-header"),
        |page: &mut TreePage, key: cambium::KeyEvent| {
            if matches!(key.key, cambium::Key::Named(cambium::NamedKey::Escape))
                && (page.instrument.panes_open || page.instrument.navigation.open)
            {
                page.instrument.panes_open = false;
                page.instrument.navigation.open = false;
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
    (enabled(page) && !super::blocks_active(page) && (page.instrument.panes_open || page.instrument.navigation.open)).then(|| {
        Box::new(cambium::on_click(el("div", ())
            .attr("aria-hidden", "true")
            .attr("style", format!("position:absolute;top:0px;left:0px;width:{}px;height:{}px;z-index:24;background:transparent;", page.size.0, page.size.1)),
            |page: &mut TreePage, click: cambium::PointerClick| {
                page.instrument.panes_open = false;
                page.instrument.navigation.open = false;
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
            selected: super::source_binding::member(page).is_some(),
            loading: product.saving,
            ..Default::default()
        },
        Box::new(el("div", ())),
    ))
}

/// The same shared shell rail as the native browser, with this host's smaller tool set.
pub(super) fn activity_rail(page: &TreePage) -> Option<Child> {
    if !enabled(page) || super::blocks_active(page) { return None; }
    let map = !page.forme.as_ref().is_some_and(|pane| pane.workbench);
    let ready = page.forme.is_some();
    let mut projections = Vec::new();
    for (mark, label, is_map) in [("▤", "Read", false), ("◇", "Map", true)] {
        projections.push(activity(mark, label, label, ShellControl { pressed: Some(map == is_map), disabled: !ready, ..Default::default() }, move |p| super::forme::show_projection(p, is_map)));
    }
    let tools = vec![
        activity("ⓘ", "Inspect", "Inspector", ShellControl { pressed: Some(page.tools_open), ..Default::default() }, |p| p.tools_open = !p.tools_open),
        activity("▥", "Tabs", "Reading tabs", ShellControl { pressed: Some(page.instrument.document_rail), ..Default::default() }, |p| p.instrument.document_rail = !p.instrument.document_rail),
        activity("⌕", "Search", "All commands…", ShellControl::default(), super::commands::open_keyboard),
    ];
    let utilities = vec![activity("⌖", "Fit", "Fit graph", ShellControl::default(), |p| {
        super::forme::show_projection(p, true);
        p.shared.fit_pending.set(true);
    })];
    Some(cambium::shell_activity_rail(projections, tools, utilities))
}

fn activity(mark: &str, label: &str, name: &str, state: ShellControl, activate: impl Fn(&mut TreePage) + 'static) -> Child {
    cambium::shell_activity_control(mark, label, name, state, move |p: &mut TreePage, _| {
        activate(p);
        p.instrument.panes_open = false;
        p.shared.dirty.set(true);
    })
}

pub(super) fn document_rail(page: &TreePage) -> Option<Child> {
    if !enabled(page) || super::blocks_active(page) || !page.instrument.document_rail { return None; }
    let graph = page.shared.canvas.borrow().graph().clone();
    let groups = page.forme.as_ref().map(|pane| {
        let layout = pane.current().layout();
        layout.slot_views().enumerate().map(|(i, slot)| {
            (format!("Tile {}", i + 1), slot.members.iter().enumerate().map(|(index, member)| {
                let member = *member;
                let title = graph.get_node_by_id(member).map(|(key, _)| graph.node_display_label(key)).unwrap_or_else(|| "Unavailable document".into());
                shell_control(title.clone(), title, ShellControl { pressed: Some(slot.active == index), ..Default::default() }, move |p: &mut TreePage, _| {
                    super::forme::activate_from_rail(p, member);
                })
            }).collect())
        }).collect::<Vec<_>>()
    }).unwrap_or_default();
    let groups = if groups.is_empty() { vec![("Reading tiles".into(), vec![control("No arranged documents", ShellControl { disabled:true, ..Default::default() }, |_| {})])] } else { groups };
    Some(cambium::shell_document_rail(groups))
}
