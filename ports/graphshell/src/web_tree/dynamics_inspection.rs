// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Independent exact-target reads; no controller stepping or source writes.
use super::*;
use cambium::{DetailRow, DetailSection, button, detail_panel};
use mere::canvas::{DynamicsInspection, DynamicsSubject, DynamicsTarget};

mod explanation;

#[derive(Default)]
pub(super) struct Pane {
    target: Option<DynamicsTarget>,
    subject: Option<DynamicsSubject>,
    report: Option<Result<DynamicsInspection, String>>,
    details: bool,
}
fn read(page: &TreePage) -> Option<Result<DynamicsInspection, String>> {
    let pane = &page.dynamics_inspector;
    pane.target.map(|_| {
        let subject = pane
            .subject
            .as_ref()
            .ok_or("The inspected appearance is unavailable. Choose an appearance again.")?;
        page.shared.canvas.borrow().inspect_bound_dynamics(subject)
    })
}
pub(super) fn refresh(page: &TreePage) -> Option<Result<DynamicsInspection, String>> {
    let report = read(page);
    (page.dynamics_inspector.report != report)
        .then_some(report)
        .flatten()
}
pub(super) fn update(page: &mut TreePage, report: Result<DynamicsInspection, String>) {
    page.dynamics_inspector.report = Some(report);
}
pub(super) fn choose(page: &mut TreePage, target: DynamicsTarget) {
    if page.dynamics_inspector.target.is_none() {
        page.shared
            .canvas
            .borrow_mut()
            .set_coupling_trace_limit(std::num::NonZeroUsize::new(128));
    }
    page.dynamics_inspector.subject = page
        .shared
        .canvas
        .borrow()
        .bind_dynamics_target(target)
        .ok();
    page.dynamics_inspector.target = Some(target);
    page.dynamics_inspector.report = read(page);
    page.tools_open = true;
}
fn selected(page: &TreePage) -> Option<DynamicsTarget> {
    let ids = page.shared.canvas.borrow().selected_members();
    (ids.len() == 1).then(|| DynamicsTarget::Access { member: ids[0] })
}
fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| format!("Unavailable: {error}"))
}
pub(super) fn section(page: &TreePage) -> Child {
    if page.product.is_none() {
        return Box::new(el("div", ()));
    }
    let selected_target = selected(page);
    let choose_selected = button("Explain selected dynamics", |page: &mut TreePage, _| {
        if let Some(target) = selected(page) {
            choose(page, target);
        }
    });
    let mut children: Vec<Child> = vec![Box::new(if selected_target.is_none() {
        choose_selected.attr("disabled", "")
    } else {
        choose_selected
    })];
    let pane = &page.dynamics_inspector;
    if let Some(target) = pane.target {
        children.push(Box::new(el("h2", "Dynamics inspector")));
        children.push(Box::new(button(
            "Close dynamics inspection",
            |page: &mut TreePage, _| {
                page.shared
                    .canvas
                    .borrow_mut()
                    .set_coupling_trace_limit(None);
                page.dynamics_inspector = Pane::default();
            },
        )));
        if pane.details {
            children.push(Box::new(el("p", format!("Exact target {}", json(&target)))));
        }
        match &pane.report {
            Some(Err(error)) => children.push(Box::new(status_note("", error.clone()))),
            Some(Ok(report)) => {
                children.extend(explanation::summary(report));
                children.push(Box::new(
                    button(
                        if pane.details {
                            "Hide dynamics details"
                        } else {
                            "Show dynamics details"
                        },
                        |page: &mut TreePage, _| {
                            page.dynamics_inspector.details = !page.dynamics_inspector.details;
                        },
                    )
                    .attr("aria-expanded", pane.details.to_string()),
                ));
                if pane.details {
                    if let Some(count) = report.pair_controller_edges {
                        let source_name = match report.topology_mode {
                            mere::canvas::DynamicsTopologyMode::RelationRows => {
                                "visible relation rows"
                            },
                            mere::canvas::DynamicsTopologyMode::OccurrenceController => {
                                "disclosed occurrence inputs"
                            },
                        };
                        children.push(Box::new(el(
                        "p",
                        format!(
                            "This pair contributes {count} {source_name} to the controller topology."
                        ),
                    )));
                    }
                    if let Some(pair) = &report.pair_inputs {
                        children.push(Box::new(el(
                            "p",
                            format!(
                                "Held relation rows {} · visible inputs {} · hidden {}",
                                pair.held_rows,
                                pair.visible_rows,
                                pair.held_rows - pair.visible_rows
                            ),
                        )));
                    }
                    children.push(Box::new(detail_panel(&[DetailSection::new(
                        "Current view",
                        vec![
                            DetailRow::new("Physics paused", report.paused.to_string()),
                            DetailRow::new(
                                "Arrangement",
                                report
                                    .arrangement
                                    .as_deref()
                                    .unwrap_or("force-directed placement"),
                            ),
                            DetailRow::new(
                                "Law picker summary",
                                format!("{} · {}", report.law_label, report.law),
                            ),
                            DetailRow::new("Overlays", report.overlays.join(", ")),
                            DetailRow::new("Topology source", &report.topology_source),
                            DetailRow::new(
                                "Topology mode",
                                match report.topology_mode {
                                    mere::canvas::DynamicsTopologyMode::RelationRows => {
                                        "relation rows"
                                    },
                                    mere::canvas::DynamicsTopologyMode::OccurrenceController => {
                                        "disclosed occurrence inputs"
                                    },
                                },
                            ),
                            DetailRow::new(
                                "Controller inputs",
                                report.controller_edges.to_string(),
                            ),
                        ],
                    )])));
                    // These are two independent reads; a schedule's record is not its current stage.
                    for (label, result) in [
                        ("Current stage", &report.live_stage),
                        ("Dynamics record (including schedule)", &report.record),
                    ] {
                        children.push(Box::new(el("h3", label)));
                        match result {
                            Ok(value) => children.push(Box::new(el(
                                "pre",
                                serde_json::to_string_pretty(value).unwrap(),
                            ))),
                            Err(error) => {
                                children.push(Box::new(el("p", format!("Unavailable · {error}"))))
                            },
                        }
                    }
                    for member in &report.members {
                        children.push(Box::new(detail_panel(&[DetailSection::new(
                            format!("Access · {}", member.title),
                            vec![
                                DetailRow::new("Identity", member.member.to_string()),
                                DetailRow::new("Role policy", format!("{:?}", member.role)),
                                DetailRow::new(
                                    "Item role override",
                                    member
                                        .item_role
                                        .map_or_else(|| "None".into(), |role| format!("{role:?}")),
                                ),
                                DetailRow::new(
                                    "Role group",
                                    member.group.as_deref().unwrap_or("None"),
                                ),
                                DetailRow::new(
                                    "Explicit position pin",
                                    member.explicit_position_pin.to_string(),
                                ),
                                DetailRow::new(
                                    "Arrangement pin installed",
                                    member.arrangement_pin_installed.to_string(),
                                ),
                                DetailRow::new(
                                    "Arrangement slot available",
                                    member.has_arrangement_slot.to_string(),
                                ),
                                DetailRow::new(
                                    "Anchor target configured",
                                    member.anchor_target_configured.to_string(),
                                ),
                                DetailRow::new(
                                    "Anchor stiffness",
                                    member.anchor_stiffness.to_string(),
                                ),
                                DetailRow::new(
                                    "Forme owner",
                                    member
                                        .forme
                                        .map_or_else(|| "None".into(), |id| id.to_string()),
                                ),
                                DetailRow::new("Forme hold", member.forme_hold.to_string()),
                                DetailRow::new("Pointer drag", member.pointer_drag.to_string()),
                                DetailRow::new("Axis locks", &member.axis_locks),
                                DetailRow::new(
                                    "Face treatment",
                                    match member.face_treatment.as_str() {
                                        "gnode-representation-glyph" => "Glyph",
                                        "gnode-representation-card" => "Ordinary Canvas face",
                                        "gnode-representation-live-pane" => {
                                            "Live frame indicator; content not embedded"
                                        },
                                        _ => "Unknown treatment",
                                    },
                                ),
                                DetailRow::new("Rule face override", member.rule_face.to_string()),
                                DetailRow::new(
                                    "Body size in screen pixels",
                                    member.face_screen_size.map_or_else(
                                        || "Unavailable".into(),
                                        |size| size.to_string(),
                                    ),
                                ),
                            ],
                        )])));
                    }
                    children.extend(explanation::details(report));
                }
                for member in &report.members {
                    let title = member.title.clone();
                    let member = member.member;
                    children.push(Box::new(button(
                        format!("Preview appearance · {title}"),
                        move |page: &mut TreePage, _| {
                            presentation::choose(
                                page,
                                graphshell::presentation_binding::Occurrence::Access { member },
                            )
                        },
                    )));
                }
                if let Some(pair) = &report.pair_inputs {
                    let mut assertion_number = 0;
                    for (index, cell) in pair.cells.iter().enumerate() {
                        if pane.details {
                            children.push(Box::new(detail_panel(&[DetailSection::new(
                                format!("Input cell group {}", index + 1),
                                vec![
                                    DetailRow::new("Displayed from", cell.owner.from.to_string()),
                                    DetailRow::new("Displayed to", cell.owner.to.to_string()),
                                    DetailRow::new(
                                        "Source stratum",
                                        format!("{:?}", cell.owner.stratum),
                                    ),
                                    DetailRow::new(
                                        "Owned from",
                                        cell.owner.source_from.to_string(),
                                    ),
                                    DetailRow::new("Owned to", cell.owner.source_to.to_string()),
                                    DetailRow::new("Relation selector", json(&cell.owner.selector)),
                                    DetailRow::new("Projected input rows", cell.rows.to_string()),
                                    DetailRow::new(
                                        "Whole cell/link hidden",
                                        cell.hidden.to_string(),
                                    ),
                                    DetailRow::new(
                                        "Held assertion handles",
                                        cell.assertions
                                            .iter()
                                            .cloned()
                                            .collect::<Vec<_>>()
                                            .join(", "),
                                    ),
                                ],
                            )])));
                        }
                        for handle in &cell.assertions {
                            assertion_number += 1;
                            let handle = handle.clone();
                            let pair = graphshell::link_inspection::LinkTarget::new(
                                cell.owner.from,
                                cell.owner.to,
                            );
                            children.push(Box::new(button(
                                format!("Inspect dynamics input assertion {assertion_number}"),
                                move |page: &mut TreePage, _| {
                                    page.inspected_link = Some(pair);
                                    page.inspected_statement = Some(handle.clone());
                                },
                            )));
                        }
                    }
                }
                if pane.details {
                    for limit in &report.limits {
                        children.push(Box::new(el("p", limit.clone())));
                    }
                }
            },
            None => {},
        }
    }
    Box::new(
        el("section", children)
            .attr("class", "tools-section tree-inspection tree-dynamics")
            .attr("aria-label", "Dynamics inspector"),
    )
}
pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let pane = &page.dynamics_inspector;
    let report = pane.report.as_ref().and_then(|result| result.as_ref().ok());
    snapshot
        .with_field(
            "dynamics-inspector-path-response",
            report
                .and_then(|r| r.path_response.as_ref())
                .map(json)
                .unwrap_or_default(),
        )
        .with_field(
            "dynamics-inspector-coupling-observation",
            report
                .map(|r| json(&r.coupling_observation))
                .unwrap_or_default(),
        )
        .with_field("dynamics-inspector-details", pane.details.to_string())
        .with_field("dynamics-inspector-open", pane.target.is_some().to_string())
        .with_field(
            "dynamics-inspector-target",
            pane.target.map(|target| json(&target)).unwrap_or_default(),
        )
        .with_field("dynamics-inspector-ready", report.is_some().to_string())
        .with_field(
            "dynamics-inspector-report",
            report.map(json).unwrap_or_default(),
        )
        .with_field(
            "dynamics-inspector-paused",
            report.is_some_and(|report| report.paused).to_string(),
        )
        .with_field(
            "dynamics-inspector-law",
            report.map(|report| report.law.clone()).unwrap_or_default(),
        )
        .with_field(
            "dynamics-inspector-topology-mode",
            report.map_or_else(String::new, |report| match report.topology_mode {
                mere::canvas::DynamicsTopologyMode::RelationRows => "relation_rows".into(),
                mere::canvas::DynamicsTopologyMode::OccurrenceController => {
                    "occurrence_controller".into()
                },
            }),
        )
        .with_field(
            "dynamics-inspector-pair-inputs",
            report
                .and_then(|report| report.pair_controller_edges)
                .map_or_else(String::new, |count| count.to_string()),
        )
        .with_field(
            "dynamics-inspector-pair-edge-slots",
            report
                .and_then(|report| report.pair_controller_edges)
                .map_or_else(String::new, |count| count.to_string()),
        )
        .with_field(
            "dynamics-inspector-controller-edge-slots",
            report.map_or_else(String::new, |report| report.controller_edges.to_string()),
        )
        .with_field(
            "dynamics-inspector-controller-edge-runs",
            report.map_or_else(String::new, |report| {
                report.controller_edge_runs.to_string()
            }),
        )
        .with_field(
            "dynamics-inspector-pair-edge-runs",
            report
                .and_then(|report| report.pair_controller_edge_runs)
                .map_or_else(String::new, |count| count.to_string()),
        )
        .with_field(
            "dynamics-inspector-pair-topology-rows",
            report
                .and_then(|report| report.pair_topology_rows.as_ref())
                .and_then(|rows| serde_json::to_string(rows).ok())
                .unwrap_or_default(),
        )
        .with_field(
            "dynamics-inspector-held-rows",
            report
                .and_then(|report| report.pair_inputs.as_ref())
                .map_or_else(String::new, |pair| pair.held_rows.to_string()),
        )
        .with_field(
            "dynamics-inspector-visible-rows",
            report
                .and_then(|report| report.pair_inputs.as_ref())
                .map_or_else(String::new, |pair| pair.visible_rows.to_string()),
        )
        .with_field(
            "dynamics-inspector-forme-holds",
            report
                .map_or(0, |report| {
                    report
                        .members
                        .iter()
                        .filter(|member| member.forme_hold)
                        .count()
                })
                .to_string(),
        )
        .with_field(
            "dynamics-inspector-explicit-pins",
            report
                .map_or(0, |report| {
                    report
                        .members
                        .iter()
                        .filter(|member| member.explicit_position_pin)
                        .count()
                })
                .to_string(),
        )
        .with_field(
            "dynamics-inspector-rule-faces",
            report
                .map_or(0, |report| {
                    report
                        .members
                        .iter()
                        .filter(|member| member.rule_face)
                        .count()
                })
                .to_string(),
        )
        .with_field(
            "dynamics-inspector-face-treatments",
            report
                .map(|report| {
                    report
                        .members
                        .iter()
                        .map(|member| member.face_treatment.clone())
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .unwrap_or_default(),
        )
        .with_field(
            "dynamics-inspector-error",
            pane.report
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .cloned()
                .unwrap_or_default(),
        )
}
