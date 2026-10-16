// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Review Resources promoted from a source-owned link reading.
use super::*;
use cambium::{TextInput, button, el, lens, text_field_typed};
use graphshell::promoted_links::{LinkDrift, PromotedLinkInspection};
use mere::kernel::graph::{FrozenLinkSource, FrozenSourceRevision, GraphStratum};
use mere::kernel::types::GraphScope;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone)]
struct PendingWrite {
    scope: (Uuid, String),
    owner: Uuid,
    expected_comment: Option<String>,
}

pub(super) struct Pane {
    pub(super) selected: Option<Uuid>,
    comment: TextInput,
    pending: Option<PendingWrite>,
    unconfirmed: BTreeSet<Uuid>,
    status: String,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            selected: None,
            comment: TextInput::new(""),
            pending: None,
            unconfirmed: BTreeSet::new(),
            status: String::new(),
        }
    }
}

pub(super) fn ready(page: &TreePage) -> bool {
    page.promoted_links.pending.is_some()
        && page.product.as_ref().is_some_and(|product| !product.saving)
}

pub(super) fn poll(page: &mut TreePage) {
    let Some(pending) = page.promoted_links.pending.clone() else {
        return;
    };
    let Some(product) = page.product.as_ref() else {
        page.promoted_links.pending = None;
        page.promoted_links.status = "The source is unavailable".into();
        return;
    };
    if product.retained_scope().as_ref() != Some(&pending.scope) {
        page.promoted_links.pending = None;
        page.promoted_links.status = "The promoted Resource belongs to another source owner".into();
        return;
    }
    if product.save_state == "error" {
        let owner = pending.owner;
        let was_freeze = pending.expected_comment.is_none();
        if was_freeze {
            page.promoted_links.unconfirmed.insert(owner);
        }
        page.promoted_links.pending = None;
        page.promoted_links.status = if was_freeze {
            format!("Promoted Resource {owner} is unconfirmed; the save failed")
        } else {
            format!("Review comment on {owner} is unconfirmed; the save failed")
        };
        return;
    }
    if product.selection_locked() {
        return;
    }
    if product.save_state != "saved" {
        return;
    }
    let inspection = product.inspect_promoted_link(pending.owner);
    match (inspection, pending.expected_comment.as_deref()) {
        (Ok(inspection), None) => {
            let owner = pending.owner;
            page.promoted_links.pending = None;
            page.promoted_links.selected = Some(owner);
            page.promoted_links.status = format!("Promoted Resource {owner} is saved and readable");
        },
        (Ok(inspection), Some(expected))
            if inspection
                .comments
                .iter()
                .any(|comment| comment.value == expected) =>
        {
            let owner = pending.owner;
            page.promoted_links.pending = None;
            page.promoted_links.status = format!("Review comment on {owner} is saved and readable");
        },
        (Err(error), _) => {
            page.promoted_links.pending = None;
            page.promoted_links.status = format!("Promoted Resource could not be reread: {error}");
        },
        _ => {},
    }
}

pub(super) fn freeze(
    page: &mut TreePage,
    key: &scenograph::link::LinkKey,
    expected_source: &scenograph::link::LinkSource,
) {
    let Some(scope) = page
        .product
        .as_ref()
        .and_then(|product| product.retained_scope())
    else {
        page.promoted_links.status = "No retained source owner is available".into();
        return;
    };
    let result = page
        .product
        .as_mut()
        .ok_or_else(|| "Saved graph unavailable".to_owned())
        .and_then(|product| {
            if product.link_source()? != *expected_source {
                return Err("The source changed; reopen the link before freezing".into());
            }
            product.freeze_link(key)
        });
    match result {
        Ok(owner) => {
            page.promoted_links.unconfirmed.remove(&owner);
            page.promoted_links.pending = Some(PendingWrite {
                scope,
                owner,
                expected_comment: None,
            });
            page.promoted_links.status = "Saving promoted link Resource…".into();
        },
        Err(error) => page.promoted_links.status = error,
    }
}

fn source_description(source: &FrozenLinkSource) -> String {
    match source {
        FrozenLinkSource::Unavailable { reason } => format!("source unavailable · {reason}"),
        FrozenLinkSource::Available { binding, revision } => {
            let revision = match revision {
                FrozenSourceRevision::Available { revision } => revision.clone(),
                FrozenSourceRevision::Unavailable { reason } => {
                    format!("revision unavailable · {reason}")
                },
            };
            format!(
                "{} / {} · {} · source revision {revision}",
                binding.authority, binding.domain, binding.resource
            )
        },
    }
}

fn statement_ids(inspection: &PromotedLinkInspection) -> Vec<String> {
    let mut ids: Vec<_> = inspection
        .frozen
        .statements()
        .iter()
        .flat_map(|edge| {
            edge.retained_assertions()
                .iter()
                .map(|assertion| assertion.statement_id.clone())
        })
        .collect();
    ids.sort();
    ids
}

fn withdrawn_statement_ids(inspection: &PromotedLinkInspection) -> Vec<String> {
    let mut ids: Vec<_> = inspection
        .frozen
        .statements()
        .iter()
        .flat_map(|edge| {
            edge.retained_assertions()
                .iter()
                .filter(|assertion| assertion.withdrawn.is_some())
                .map(|assertion| assertion.statement_id.clone())
        })
        .collect();
    ids.sort();
    ids
}

fn statement_rows(inspection: &PromotedLinkInspection) -> Vec<Child> {
    let mut rows = Vec::new();
    let first = inspection.frozen.first();
    let second = inspection.frozen.second();
    for edge in inspection.frozen.statements() {
        let endpoint_label = |id: &str| {
            if id == first.id.to_string() {
                first
                    .address
                    .as_deref()
                    .unwrap_or("first endpoint address unavailable")
                    .to_owned()
            } else if id == second.id.to_string() {
                second
                    .address
                    .as_deref()
                    .unwrap_or("second endpoint address unavailable")
                    .to_owned()
            } else {
                id.to_owned()
            }
        };
        let direction = format!(
            "{} to {}",
            endpoint_label(&edge.from_node_id),
            endpoint_label(&edge.to_node_id)
        );
        for assertion in edge.retained_assertions() {
            let method = assertion
                .basis
                .method
                .as_ref()
                .map(|method| match &method.version {
                    Some(version) => format!("{} ({version})", method.iri),
                    None => method.iri.clone(),
                })
                .unwrap_or_else(|| "no method held".into());
            let scope = match &assertion.graph_scope {
                GraphScope::Default => "default".to_owned(),
                GraphScope::Source => "source".to_owned(),
                GraphScope::User => "user".to_owned(),
                GraphScope::Agent => "agent".to_owned(),
                GraphScope::Moot => "moot".to_owned(),
                GraphScope::Custom(name) => format!("custom: {name}"),
            };
            let time = |value: Option<u64>| {
                value.map_or_else(|| "unknown".to_owned(), |ms| ms.to_string())
            };
            rows.push(Box::new(el(
                "p",
                format!(
                    "{} · {} · {} · {} · {} · scope {} · basis {} · method {} · evidence {} · first {} / last {}{}",
                    direction,
                    assertion.statement_id,
                    assertion.predicate,
                    assertion.label.as_deref().unwrap_or("no label"),
                    assertion.provenance_iri.as_deref().unwrap_or("asserter unavailable"),
                    scope,
                    assertion.basis.note.as_deref().unwrap_or("not recorded"),
                    method,
                    if assertion.basis.evidence.is_empty() { "none".into() } else { assertion.basis.evidence.join(", ") },
                    time(assertion.first_asserted_ms),
                    time(assertion.asserted_at_ms),
                    assertion.withdrawn.as_ref().map_or(String::new(), |withdrawal| format!(" · withdrawn at {} by {}", time(withdrawal.at_ms), withdrawal.by.as_deref().unwrap_or("unknown actor"))),
                ),
            )) as Child);
        }
    }
    rows
}

fn drift_rows(drift: &LinkDrift) -> Vec<Child> {
    let mut rows = vec![Box::new(
        el("p", format!("Current link: {}", drift.status)).attr("class", "promoted-link-current"),
    ) as Child];
    if let Some(reason) = &drift.reason {
        rows.push(Box::new(el("p", reason.clone())));
    }
    for (label, ids) in [
        ("Added since freeze", &drift.added),
        ("Withdrawn since freeze", &drift.withdrawn),
        ("Changed since freeze", &drift.changed),
        ("Missing from current source", &drift.missing),
    ] {
        if !ids.is_empty() {
            rows.push(Box::new(el("p", format!("{label}: {}", ids.join(", ")))));
        }
    }
    rows
}

fn promoted_rows(inspection: &PromotedLinkInspection) -> Vec<Child> {
    let frozen = &inspection.frozen;
    let first = frozen.first();
    let second = frozen.second();
    let stratum = match frozen.stratum() {
        GraphStratum::Surface => "Surface",
        GraphStratum::Resource => "Resource",
    };
    let mut rows: Vec<Child> = vec![
        Box::new(el("p", format!("Review Resource {}", inspection.owner))),
        Box::new(el(
            "p",
            format!(
                "{stratum} link between {} and {}",
                first.address.as_deref().unwrap_or("address unavailable"),
                second.address.as_deref().unwrap_or("address unavailable")
            ),
        )),
    ];
    rows.extend(drift_rows(&inspection.drift));
    for comment in &inspection.comments {
        rows.push(Box::new(
            el(
                "p",
                format!(
                    "Review comment · {} · by {} · recorded {}",
                    comment.value,
                    comment
                        .provenance_iri
                        .as_deref()
                        .unwrap_or("unknown author"),
                    comment
                        .asserted_at_ms
                        .map_or_else(|| "unknown".to_owned(), |ms| ms.to_string())
                ),
            )
            .attr("class", "promoted-link-review-comment"),
        ));
    }
    let metadata: Vec<Child> = vec![
        Box::new(el(
            "p",
            format!(
                "Frozen source revision {} · local graph revision {}",
                source_description(frozen.source()),
                frozen.revision()
            ),
        )),
        Box::new(el(
            "p",
            format!(
                "Frozen assertion copies: {}",
                statement_ids(inspection).join(", ")
            ),
        )),
        Box::new(el(
            "p",
            format!("Coverage limits: {}", inspection.coverage.limits.len()),
        )),
    ];
    rows.extend(metadata);
    for (label, coverage) in [
        ("At freeze", frozen.coverage()),
        ("Here now", &inspection.coverage),
    ] {
        for limit in &coverage.limits {
            let layer = serde_json::to_value(limit.layer)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "unknown".into());
            rows.push(Box::new(el(
                "p",
                format!(
                    "{label} · {layer}: {} · {} Resource references / {} Surface references",
                    limit.reason,
                    limit.resources.len(),
                    limit.surfaces.len()
                ),
            )));
        }
    }
    rows.extend(statement_rows(inspection));
    rows
}

fn add_comment(page: &mut TreePage, owner: Uuid) {
    let text = page.promoted_links.comment.text().trim().to_owned();
    let Some(scope) = page
        .product
        .as_ref()
        .and_then(|product| product.retained_scope())
    else {
        page.promoted_links.status = "No retained source owner is available".into();
        return;
    };
    match page
        .product
        .as_mut()
        .ok_or_else(|| "Saved graph unavailable".to_owned())
        .and_then(|product| product.comment_promoted_link(owner, &text))
    {
        Ok(()) => {
            page.promoted_links.pending = Some(PendingWrite {
                scope,
                owner,
                expected_comment: Some(text),
            });
            page.promoted_links.status = "Saving review comment…".into();
            page.promoted_links.comment = TextInput::new("");
        },
        Err(error) => page.promoted_links.status = error,
    }
}

pub(super) fn section(page: &TreePage) -> Child {
    let mut children: Vec<Child> = vec![
        Box::new(el("h3", "Promoted links for review")),
        Box::new(el(
            "p",
            "Each Resource keeps one frozen link reading and can be compared with its live source.",
        )),
    ];
    let owners = visible_owners(page);
    for (index, owner) in owners.iter().copied().enumerate() {
        let selected = page.promoted_links.selected == Some(owner);
        children.push(Box::new(
            button(
                format!("Open promoted link {}", index + 1),
                move |page: &mut TreePage, _| {
                    page.promoted_links.selected = Some(owner);
                    page.promoted_links.status.clear();
                },
            )
            .attr("data-promoted-link-owner", owner.to_string())
            .attr("aria-pressed", selected.to_string()),
        ));
    }
    if let Some(owner) = page.promoted_links.selected {
        match page
            .product
            .as_ref()
            .ok_or_else(|| "Saved graph unavailable".to_owned())
            .and_then(|product| product.inspect_promoted_link(owner))
        {
            Ok(inspection) => {
                children.extend(promoted_rows(&inspection));
                if !page
                    .product
                    .as_ref()
                    .is_some_and(|product| product.selection_locked())
                {
                    children.push(Box::new(el(
                        "label",
                        (
                            "Review comment",
                            lens(
                                |input: &mut TextInput| {
                                    text_field_typed(input).attr("aria-label", "Review comment")
                                },
                                |page: &mut TreePage| &mut page.promoted_links.comment,
                            ),
                        ),
                    )));
                    children.push(Box::new(button(
                        "Add review comment",
                        move |page: &mut TreePage, _| add_comment(page, owner),
                    )));
                }
            },
            Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
        }
    }
    if !page.promoted_links.status.is_empty() {
        children.push(Box::new(
            el("p", page.promoted_links.status.clone())
                .attr("role", "status")
                .attr("data-promoted-link-status", "true"),
        ));
    }
    Box::new(
        el("section", children)
            .attr("class", "tools-section tree-inspection")
            .attr("aria-label", "Promoted links"),
    )
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let owners = visible_owners(page);
    let owner = page.promoted_links.selected;
    let inspection =
        owner.and_then(|owner| page.product.as_ref()?.inspect_promoted_link(owner).ok());
    let ids = inspection.as_ref().map(statement_ids).unwrap_or_default();
    snapshot
        .with_field("promoted-link-count", owners.len().to_string())
        .with_field(
            "promoted-link-owner",
            owner.map(|owner| owner.to_string()).unwrap_or_default(),
        )
        .with_field("promoted-link-frozen-assertion-ids", ids.join("|"))
        .with_field(
            "promoted-link-frozen-withdrawn-ids",
            inspection
                .as_ref()
                .map(withdrawn_statement_ids)
                .unwrap_or_default()
                .join("|"),
        )
        .with_field(
            "promoted-link-frozen-withdrawn-count",
            inspection
                .as_ref()
                .map(|i| withdrawn_statement_ids(i).len().to_string())
                .unwrap_or_default(),
        )
        .with_field(
            "promoted-link-drift",
            inspection
                .as_ref()
                .map(|i| i.drift.status.clone())
                .unwrap_or_default(),
        )
        .with_field(
            "promoted-link-withdrawn-ids",
            inspection
                .as_ref()
                .map(|i| i.drift.withdrawn.join("|"))
                .unwrap_or_default(),
        )
        .with_field(
            "promoted-link-added-ids",
            inspection
                .as_ref()
                .map(|i| i.drift.added.join("|"))
                .unwrap_or_default(),
        )
        .with_field(
            "promoted-link-comment-count",
            inspection
                .as_ref()
                .map(|i| i.comments.len().to_string())
                .unwrap_or_default(),
        )
        .with_field(
            "promoted-link-comment-texts",
            inspection
                .as_ref()
                .map(|i| {
                    i.comments
                        .iter()
                        .map(|comment| comment.value.as_str())
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .unwrap_or_default(),
        )
        .with_field(
            "promoted-link-write-status",
            page.promoted_links.status.clone(),
        )
}

fn visible_owners(page: &TreePage) -> Vec<Uuid> {
    let pending_freeze = page
        .promoted_links
        .pending
        .as_ref()
        .filter(|pending| pending.expected_comment.is_none())
        .map(|pending| pending.owner);
    page.product
        .as_ref()
        .and_then(|product| product.promoted_links().ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|owner| {
            Some(*owner) != pending_freeze && !page.promoted_links.unconfirmed.contains(owner)
        })
        .collect()
}

pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    if runner.state().promoted_links.selected.is_none()
        || runner
            .state()
            .product
            .as_ref()
            .is_none_or(|product| product.selection_locked())
    {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let matches = taproot::matching(
        &dom,
        &taproot::Selector::role("textbox").containing("Review comment"),
    );
    if !matches.contains(&node) {
        return None;
    }
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(|page| &page.promoted_links.comment),
        get_mut: Box::new(|page| &mut page.promoted_links.comment),
    })
}
