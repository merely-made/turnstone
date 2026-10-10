// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Fold controls over explicit host membership, using portable scene facts.
use super::*;
use cambium::button;

pub(super) fn requested(root: &Element) -> Result<Option<String>, String> {
    let query = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|search| web_sys::UrlSearchParams::new_with_str(&search).ok())
        .and_then(|params| params.get("grouping"));
    let kind = query.or_else(|| root.get_attribute("data-grouping-kind"));
    if kind.as_ref().is_some_and(|k| k.trim().is_empty()) {
        return Err("Grouping refused: name a membership relationship kind".into());
    }
    Ok(kind)
}

pub(super) fn controls(page: &TreePage) -> Option<Child> {
    let grouped = page.grouping.as_ref()?;
    let projection = grouped.projection().ok()?;
    let mut children: Vec<Child> = vec![Box::new(el("h2", "Folds"))];
    let mut breadcrumbs: Vec<Child> =
        vec![Box::new(button("Whole graph", |page: &mut TreePage, _| {
            page.change_group(|state| state.entered = None);
        }))];
    for id in &projection.breadcrumbs {
        let label = grouped.label(id).to_owned();
        let id = id.clone();
        breadcrumbs.push(Box::new(button(label, move |page: &mut TreePage, _| {
            page.change_group(|state| state.entered = Some(id.clone()));
        })));
    }
    children.push(Box::new(
        el("nav", breadcrumbs).attr("aria-label", "Graph location"),
    ));
    children.push(Box::new(el("p", format!("{} visible of {} disclosed occurrences; {} external context items. Group changes pause physics and preserve coordinates.", projection.visible.len(), grouped.total_occurrences(), projection.boundary.len())).attr("role", "status")));
    let mut rows: Vec<Child> = Vec::new();
    for id in grouped
        .hierarchy
        .groups()
        .filter(|id| projection.visible.contains(*id))
    {
        let entered = grouped.state.entered.as_deref() == Some(id);
        let expanded = entered || grouped.state.expanded.contains(id);
        let label = grouped.label(id);
        let id_for_toggle = id.to_owned();
        let id_for_enter = id.to_owned();
        let summary = format!(
            "{} · {} members{} · {} internal dependencies",
            label,
            grouped.hierarchy.children(id).map_or(0, |c| c.len()),
            if projection.boundary.contains(id) {
                " · external context"
            } else {
                ""
            },
            projection
                .internal_relationships
                .get(id)
                .map_or(0, Vec::len)
        );
        let toggle = (!entered).then(|| {
            Box::new(
                button(
                    format!("{} {label}", if expanded { "Collapse" } else { "Expand" }),
                    move |page: &mut TreePage, _| {
                        page.change_group(|state| {
                            if !state.expanded.remove(&id_for_toggle) {
                                state.expanded.insert(id_for_toggle.clone());
                            }
                        });
                    },
                )
                .attr("aria-expanded", if expanded { "true" } else { "false" }),
            ) as Child
        });
        let enter = (!entered).then(|| {
            Box::new(button(
                format!("Enter {label}"),
                move |page: &mut TreePage, _| {
                    page.change_group(|state| state.entered = Some(id_for_enter.clone()));
                },
            )) as Child
        });
        rows.push(Box::new(el("li", (el("span", summary), toggle, enter))));
    }
    children.push(Box::new(el("ul", rows).attr("role", "list")));
    children.push(Box::new(
        button(
            if page.grouping_evidence {
                "Hide relationship evidence"
            } else {
                "Show relationship evidence"
            },
            |page: &mut TreePage, _| {
                page.grouping_evidence = !page.grouping_evidence;
            },
        )
        .attr(
            "aria-expanded",
            if page.grouping_evidence {
                "true"
            } else {
                "false"
            },
        ),
    ));
    if page.grouping_evidence {
        for (id, witnesses) in &projection.internal_relationships {
            children.push(Box::new(el(
                "p",
                format!("Dependencies inside {}", grouped.label(id)),
            )));
            for id in witnesses {
                if let Some(witness) = grouped.relationship(id) {
                    children.push(Box::new(el("p", evidence(witness))));
                }
            }
        }
    }
    if let Some(error) = &page.grouping_error {
        children.push(Box::new(el("p", error.clone()).attr("role", "alert")));
    }
    Some(Box::new(
        el("section", children)
            .attr("class", "tree-grouping")
            .attr("role", "region")
            .attr("aria-label", "Folds"),
    ))
}

pub(super) fn evidence(witness: &graphshell::projection_compile::DisclosedRelationship) -> String {
    format!(
        "{}: {} → {}. {} Source: {} / {} / {}, revision {}; {} v{}, provider {}. Evidence: {}.",
        witness.id,
        witness.from_occurrence,
        witness.to_occurrence,
        witness.explanation,
        witness.provenance.source.authority,
        witness.provenance.source.domain,
        witness.provenance.source.resource,
        witness.provenance.source_revision.as_str(),
        witness.provenance.method,
        witness.provenance.method_version,
        witness.provenance.provider,
        witness
            .provenance
            .evidence
            .iter()
            .map(|s| format!("{}:{}", s.adapter, s.id))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

impl TreePage {
    fn change_group(
        &mut self,
        change: impl FnOnce(&mut graphshell::host_dataset_view::FoldViewState),
    ) {
        let Some(grouped) = &mut self.grouping else {
            return;
        };
        let before = grouped.state.clone();
        let mut canvas = self.shared.canvas.borrow_mut();
        change(&mut grouped.state);
        match grouped.apply_to_canvas_with(&mut canvas, |view| {
            if let Some(history) = &self.history {
                history.mark_view(view);
            }
        }) {
            Ok(relations) => {
                self.picked = canvas.focused_url().map(str::to_owned);
                self.nodes = canvas.graph().node_count();
                self.dataset = HostedDataset::Loaded(relations);
                self.grouping_error = None;
                // Slots in an arrangement transition name the previous view.
                self.physics.transition = None;
                self.shared.dirty.set(true);
            },
            Err(error) => {
                grouped.state = before;
                self.grouping_error = Some(error);
            },
        }
    }
}
