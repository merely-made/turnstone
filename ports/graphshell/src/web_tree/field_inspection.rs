// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained field inspection; explicit discovery never changes the target.
use super::*;
use cambium::{
    DetailRow, DetailSection, PointerButton, PointerEvent, PointerPhase, button, detail_panel,
};
use graphshell::field_inspection::{
    self as model, CouplingInspection, FieldTarget, ForceEligibility, FormeInspection,
    RegionCandidate,
};
use mere::kernel::graph::{CouplingId, FieldId, Graph};

#[derive(Default)]
pub(super) struct Pane {
    pub(super) open: bool,
    pub(super) picking: bool,
    // Retained across cancellation/closing so an intercepted press cannot leak
    // its release into normal selection or Forme arrangement gestures.
    pub(super) pressed: bool,
    pub(super) point: Option<[f32; 2]>,
    pub(super) target: Option<FieldTarget>,
    pub(super) coupling: Option<CouplingId>,
    // One explicit inspection moves focus once; ordinary rebuilds never do.
    pub(super) reveal: bool,
    pub(super) editor: super::coupling::Pane,
}

pub(super) fn graph(page: &TreePage) -> Result<&Graph, String> {
    page.product
        .as_ref()
        .ok_or("Field inspection requires the local graph")?
        .source_graph()
}

pub(super) fn forme(page: &TreePage, id: uuid::Uuid) -> Result<FormeInspection, String> {
    let pane = page
        .forme
        .as_ref()
        .ok_or("No Forme workspace is presented here")?;
    model::inspect_forme(pane.current(), id)
}

pub(super) fn candidates(page: &TreePage, canvas: &Canvas) -> Result<Vec<RegionCandidate>, String> {
    let point = page
        .field_inspector
        .point
        .ok_or("Choose a point to discover regions")?;
    let forme = page
        .forme
        .as_ref()
        .map(|pane| model::inspect_forme(pane.current(), pane.current().document.id.as_uuid()))
        .transpose()?;
    model::region_candidates(
        graph(page)?,
        point,
        |id| canvas.field_visible(id),
        forme.as_ref(),
    )
}

fn choose(page: &mut TreePage, target: FieldTarget) {
    page.field_inspector.target = Some(target);
    page.field_inspector.coupling = None;
    page.field_inspector.open = true;
    page.field_inspector.reveal = true;
    page.tools_open = true;
}

pub(super) fn reveal(ctx: &mut AppCtx<'_, TreePage, Logic, Child>) {
    if !ctx.runner.state().field_inspector.reveal {
        return;
    }
    if let Some(node) = ctx.runner.focus() {
        ctx.scroll_into_view(node, cambium_rootstock::ScrollAlign::Start);
    }
    ctx.runner
        .update(|page| page.field_inspector.reveal = false);
}

pub(super) fn focused_detail(ctx: &AppCtx<'_, TreePage, Logic, Child>) -> &'static str {
    let Some(focus) = ctx.runner.focus() else {
        return "none";
    };
    let dom = ctx.runner.dom();
    let dom = dom.borrow();
    for (id, label) in [
        ("field-inspected-field", "field"),
        ("field-inspected-coupling", "coupling"),
    ] {
        if taproot::matching(&dom, &Selector::class(id)).contains(&focus) {
            return label;
        }
    }
    "none"
}

fn detail_heading(page: &TreePage, label: &str, exact_coupling: bool) -> Child {
    Box::new(cambium::request_focus(
        cambium::focusable(el("h3", label.to_string()).attr("tabindex", "0").attr(
            "class",
            if exact_coupling {
                "field-inspected-coupling"
            } else {
                "field-inspected-field"
            },
        )),
        page.field_inspector.reveal && exact_coupling == page.field_inspector.coupling.is_some(),
    ))
}

pub(super) fn pointer(page: &mut TreePage, event: &PointerEvent) -> bool {
    let pane = &mut page.field_inspector;
    if event.button != PointerButton::Primary || (!pane.picking && !pane.pressed) {
        return false;
    }
    match event.phase {
        PointerPhase::Cancel => {
            pane.pressed = false;
            pane.picking = false;
        },
        PointerPhase::Down if pane.picking => {
            pane.pressed = true;
            let (x, y) = page.shared.canvas.borrow().world_point_at(event.local);
            pane.point = Some([x, y]);
        },
        PointerPhase::Up => {
            if pane.picking && pane.open {
                page.tools_open = true;
            }
            pane.pressed = false;
            pane.picking = false;
        },
        _ => {},
    }
    page.shared.dirty.set(true);
    true
}

pub(super) fn cancel_pick(page: &mut TreePage, key: &Key) -> bool {
    if matches!(key, Key::Named(NamedKey::Escape))
        && (page.field_inspector.picking || page.field_inspector.pressed)
    {
        page.field_inspector.picking = false;
        if page.field_inspector.open {
            page.tools_open = true;
        }
        return true;
    }
    false
}

pub(super) fn open_button(_page: &TreePage) -> Child {
    Box::new(button("Inspect fields", |page: &mut TreePage, _| {
        page.field_inspector.open = true;
        page.tools_open = true;
    }))
}

fn target_id(target: FieldTarget) -> String {
    match target {
        FieldTarget::Numen(id) => format!("numen:{}", id.as_uuid()),
        FieldTarget::Forme(id) => format!("forme:{id}"),
    }
}

fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("held field data serializes")
}

pub(super) fn eligibility(value: &ForceEligibility) -> &'static str {
    match value {
        ForceEligibility::Recognized => "Recognized by Canvas rebuild; effect not evaluated",
        ForceEligibility::RetiredField => "Retired owner; omitted by Canvas rebuild",
        ForceEligibility::MissingField => "Owner not held here; omitted by Canvas rebuild",
        ForceEligibility::OpenResponse => "Open response held; Canvas force consumer ignores it",
        ForceEligibility::DefinitionMismatch => {
            "Definition shape does not match the Canvas response"
        },
    }
}

fn coupling_rows(inspected: &CouplingInspection) -> Vec<DetailRow> {
    let coupling = &inspected.coupling;
    vec![
        DetailRow::new("Coupling identity", coupling.id.as_uuid().to_string()),
        DetailRow::new("Field identity", coupling.field.as_uuid().to_string()),
        DetailRow::new("Owner held here", inspected.owner.is_some().to_string()),
        DetailRow::new("Selector", json(&coupling.selector)),
        DetailRow::new("Response", json(&coupling.response)),
        DetailRow::new("Strength", coupling.strength.to_string()),
        DetailRow::new(
            "Canvas rebuild",
            eligibility(&inspected.rebuild_eligibility),
        ),
        DetailRow::new(
            "Held selector matches",
            inspected.selector_members.len().to_string(),
        ),
    ]
}

fn roster(page: &TreePage, canvas: &Canvas) -> Child {
    let mut children: Vec<Child> = vec![Box::new(el("h3", "Held fields"))];
    match graph(page) {
        Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
        Ok(graph) => {
            let fields = model::field_roster(graph);
            if fields.is_empty() {
                children.push(Box::new(el("p", "No Numen fields held here")));
            }
            for field in fields {
                let id = field.id;
                let label = field.name.unwrap_or_else(|| id.as_uuid().to_string());
                children.push(Box::new(button(
                    format!("Inspect field: {label}"),
                    move |page: &mut TreePage, _| choose(page, FieldTarget::Numen(id)),
                )));
                children.push(Box::new(el(
                    "p",
                    format!(
                        "{} · {:?} · {}",
                        id.as_uuid(),
                        field.lifecycle,
                        if canvas.field_visible(id) {
                            "boundary enabled"
                        } else {
                            "boundary hidden"
                        }
                    ),
                )));
                // This is view visibility only, never a lifecycle/coupling edit.
                children.push(Box::new(button(
                    format!(
                        "{} boundary: {label}",
                        if canvas.field_visible(id) {
                            "Hide"
                        } else {
                            "Show"
                        }
                    ),
                    move |page: &mut TreePage, _| {
                        page.shared.canvas.borrow_mut().toggle_field_visible(id);
                        page.shared.dirty.set(true);
                    },
                )));
            }
        },
    }
    if let Some(pane) = &page.forme {
        let id = pane.current().document.id.as_uuid();
        children.push(Box::new(button(
            "Inspect Forme",
            move |page: &mut TreePage, _| choose(page, FieldTarget::Forme(id)),
        )));
    }
    Box::new(el("div", children))
}

fn discovery(page: &TreePage, canvas: &Canvas) -> Child {
    let mut children: Vec<Child> = vec![Box::new(el("h3", "Region discovery"))];
    children.push(Box::new(el("p", "Choose a point, then inspect a candidate. Discovery lists visible rectangular boundaries; it does not determine selector membership or effect priority. Hidden, retired and other extents remain in the roster.")));
    if page.forme.as_ref().is_some_and(|pane| pane.workbench) {
        children.push(Box::new(el(
            "p",
            "Return to Mere to pick a point on the graph.",
        )));
    } else {
        children.push(Box::new(button(
            if page.field_inspector.picking {
                "Cancel region pick"
            } else {
                "Pick field regions"
            },
            |page: &mut TreePage, _| {
                page.field_inspector.picking = !page.field_inspector.picking;
                if page.field_inspector.picking && !tools_docked(page) {
                    // Make the graph reachable under the narrow tools overlay;
                    // reopen tools only after release (or explicit cancellation).
                    page.tools_open = false;
                }
            },
        )));
    }
    if page.field_inspector.picking {
        children.push(Box::new(el("p", "Click the graph to list regions at that point. Escape in the graph cancels the pick.").attr("role", "status")));
    }
    if let Some(pane) = &page.forme {
        if !pane.current().members().is_empty() {
            children.push(Box::new(button(
                "Discover regions at Forme center",
                |page: &mut TreePage, _| {
                    if let Some(pane) = &page.forme {
                        let [x, y, r, b] = pane.current().bounds;
                        page.field_inspector.point = Some([x * 0.5 + r * 0.5, y * 0.5 + b * 0.5]);
                        page.field_inspector.picking = false;
                    }
                },
            )));
        }
    }
    if let Some(point) = page.field_inspector.point {
        children.push(Box::new(el(
            "p",
            format!("World point: {}, {}", point[0], point[1]),
        )));
        match candidates(page, canvas) {
            Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
            Ok(candidates) if candidates.is_empty() => children.push(Box::new(el(
                "p",
                "No visible rectangular regions at this point",
            ))),
            Ok(candidates) => {
                for candidate in candidates {
                    let target = candidate.target;
                    let label = candidate.name.unwrap_or_else(|| match target {
                        FieldTarget::Forme(_) => "Forme".into(),
                        _ => "Unnamed field".into(),
                    });
                    children.push(Box::new(button(
                        format!("Inspect region: {label}"),
                        move |page: &mut TreePage, _| choose(page, target),
                    )));
                    children.push(Box::new(el(
                        "p",
                        format!("{} · {:?}", target_id(target), candidate.bounds),
                    )));
                }
            },
        }
    }
    Box::new(el("div", children))
}

fn numen_detail(page: &TreePage, id: FieldId, canvas: &Canvas) -> Child {
    let mut children: Vec<Child> = Vec::new();
    match graph(page).and_then(|graph| model::inspect_field(graph, id)) {
        Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
        Ok(inspected) => {
            children.push(Box::new(detail_panel(&[DetailSection::new(
                "Held field",
                vec![
                    DetailRow::new("Field identity", id.as_uuid().to_string()),
                    DetailRow::new(
                        "Name",
                        inspected.field.name.unwrap_or_else(|| "Unnamed".into()),
                    ),
                    DetailRow::new("Lifecycle", format!("{:?}", inspected.field.lifecycle)),
                    DetailRow::new(
                        "Boundary visibility enabled",
                        canvas.field_visible(id).to_string(),
                    ),
                    DetailRow::new("Extent", json(&inspected.field.extent)),
                    DetailRow::new("Definition", json(&inspected.field.definition)),
                ],
            )])));
            children.push(Box::new(el("p", "Boundary visibility does not disable couplings. Only active rectangular boundaries are painted. In the current Canvas bridge, the selector supplies members independently of extent. These are held records; inspection does not evaluate the field.")));
            if inspected.couplings.is_empty() {
                children.push(Box::new(el("p", "No couplings held for this field")));
            }
            for coupling in inspected.couplings {
                let id = coupling.coupling.id;
                children.push(Box::new(detail_panel(&[DetailSection::new(
                    "Held coupling",
                    coupling_rows(&coupling),
                )])));
                children.push(Box::new(button(
                    format!("Inspect coupling: {}", id.as_uuid()),
                    move |page: &mut TreePage, _| {
                        page.field_inspector.coupling = Some(id);
                        page.field_inspector.reveal = true;
                    },
                )));
            }
            children.push(Box::new(el("h3", "Known graph limits")));
            children.push(Box::new(el("p", "Coverage is supplied by the graph owner. It has no field-specific scope; an empty note is not a completeness claim.")));
            for limit in inspected.coverage.limits {
                children.push(Box::new(el("p", json(&limit))));
            }
        },
    }
    Box::new(el("div", children))
}

fn forme_detail(page: &TreePage, id: uuid::Uuid) -> Child {
    match forme(page, id) {
        Err(error) => Box::new(el("p", error).attr("role", "status")),
        Ok(inspected) => {
            let pane = page.forme.as_ref().expect("resolved Forme");
            let state = if pane.saving {
                "Save pending"
            } else if pane.previewing() {
                "Placement preview"
            } else if pane.session.editing() {
                "Arrangement draft"
            } else {
                "Current arrangement"
            };
            Box::new(detail_panel(&[DetailSection::new(
                "Forme arrangement owner",
                vec![
                    DetailRow::new("Forme identity", inspected.id.to_string()),
                    DetailRow::new("Graph identity", inspected.graph_id.to_string()),
                    DetailRow::new("Name", inspected.label.unwrap_or_else(|| "Forme".into())),
                    DetailRow::new("Read state", state),
                    DetailRow::new("Locked", inspected.locked.to_string()),
                    DetailRow::new("Boundary visibility enabled", inspected.visible.to_string()),
                    DetailRow::new("Bounds", json(&inspected.bounds)),
                    DetailRow::new("Members", json(&inspected.members)),
                    DetailRow::new("Arrangement", json(&inspected.arrangement)),
                ],
            )]))
        },
    }
}

pub(super) fn section(page: &TreePage) -> Child {
    if !page.field_inspector.open {
        return Box::new(el("div", ()));
    }
    let canvas = page.shared.canvas.borrow();
    let mut children: Vec<Child> = vec![Box::new(el("h2", "Field inspection"))];
    children.push(Box::new(button(
        "Close field inspection",
        |page: &mut TreePage, _| {
            let pane = &mut page.field_inspector;
            pane.open = false;
            pane.picking = false;
            pane.point = None;
            pane.target = None;
            pane.coupling = None;
            pane.reveal = false;
        },
    )));
    children.push(roster(page, &canvas));
    children.push(discovery(page, &canvas));
    if let Some(target) = page.field_inspector.target {
        children.push(detail_heading(page, "Inspected field", false));
        children.push(match target {
            FieldTarget::Numen(id) => numen_detail(page, id, &canvas),
            FieldTarget::Forme(id) => forme_detail(page, id),
        });
    }
    if let Some(id) = page.field_inspector.coupling {
        children.push(detail_heading(page, "Exact coupling", true));
        children.push(Box::new(button(
            "Close coupling inspection",
            |page: &mut TreePage, _| {
                page.field_inspector.coupling = None;
                page.field_inspector.reveal = true;
            },
        )));
        match graph(page).and_then(|graph| model::inspect_coupling(graph, id)) {
            Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
            Ok(inspected) => {
                children.push(Box::new(detail_panel(&[DetailSection::new(
                    "Exact coupling",
                    coupling_rows(&inspected),
                )])));
                children.push(Box::new(el(
                    "ul",
                    inspected
                        .selector_members
                        .into_iter()
                        .map(|access| {
                            el(
                                "li",
                                format!(
                                    "{} · {} · {}",
                                    access.title, access.address, access.member
                                ),
                            )
                        })
                        .collect::<Vec<_>>(),
                )));
            },
        }
    }
    Box::new(
        el("section", (children, super::coupling::section(page)))
            .attr("class", "tree-inspection")
            .attr("aria-label", "Field inspector"),
    )
}

#[path = "field_inspection_probe.rs"]
mod probe;
pub(super) use probe::fields;
