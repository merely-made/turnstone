// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A read-only inspector whose access target is independent of selection.
use super::*;
use cambium::{DetailRow, DetailSection, button, detail_panel};
use graphshell::access_inspection::{AccessInspection, AccessSummary};

pub(super) fn read(page: &TreePage) -> Result<AccessInspection, String> {
    let member = page.inspected_access.ok_or("Choose an access to inspect")?;
    page.product
        .as_ref()
        .ok_or("Access inspection requires the local graph")?
        .inspect_access(member)
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let snapshot = snapshot
        .with_field(
            "inspector-open",
            page.inspected_access.is_some().to_string(),
        )
        .with_field(
            "inspector-member",
            page.inspected_access
                .map(|id| id.to_string())
                .unwrap_or_default(),
        );
    match read(page) {
        Ok(inspected) => snapshot
            .with_field("inspector-ready", "true")
            .with_field("inspector-address", inspected.access.address)
            .with_field("inspector-title", inspected.access.title)
            .with_field(
                "inspector-resource",
                inspected
                    .resource_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "inspector-resource-address",
                inspected
                    .resource
                    .as_ref()
                    .map(|source| source.address.clone())
                    .unwrap_or_default(),
            )
            .with_field(
                "inspector-other-accesses",
                inspected
                    .resource
                    .as_ref()
                    .map(|source| {
                        source
                            .other_accesses
                            .iter()
                            .map(|access| access.member.to_string())
                            .collect::<Vec<_>>()
                            .join("|")
                    })
                    .unwrap_or_default(),
            )
            .with_field(
                "inspector-coverage-count",
                inspected.coverage.limits.len().to_string(),
            )
            .with_field("inspector-error", ""),
        Err(error) => snapshot
            .with_field("inspector-ready", "false")
            .with_field("inspector-address", "")
            .with_field("inspector-title", "")
            .with_field("inspector-resource", "")
            .with_field("inspector-resource-address", "")
            .with_field("inspector-other-accesses", "")
            .with_field("inspector-coverage-count", "0")
            .with_field(
                "inspector-error",
                if page.inspected_access.is_some() {
                    error
                } else {
                    String::new()
                },
            ),
    }
}

pub(super) fn open_button(page: &TreePage) -> Child {
    let selected = page.product.as_ref().and_then(|product| product.selected);
    Box::new(
        button("Inspect selected access", |page: &mut TreePage, _| {
            if let Some(member) = page.product.as_ref().and_then(|product| product.selected) {
                page.inspected_access = Some(member);
                page.tools_open = true;
            }
        })
        .attr(
            "aria-disabled",
            if selected.is_none() { "true" } else { "false" },
        ),
    )
}

fn access_rows(access: &AccessSummary) -> Vec<DetailRow> {
    vec![
        DetailRow::new("Title", &access.title),
        DetailRow::new("Address", &access.address),
        DetailRow::new("Access identity", access.member.to_string()),
    ]
}

pub(super) fn section(page: &TreePage) -> Child {
    if page.inspected_access.is_none() {
        return Box::new(el("div", ()));
    }
    let mut children: Vec<Child> = vec![
        Box::new(el("h2", "Access inspector")),
        Box::new(button(
            "Close access inspection",
            |page: &mut TreePage, _| {
                page.inspected_access = None;
            },
        )),
    ];
    match read(page) {
        Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
        Ok(inspected) => {
            children.push(Box::new(detail_panel(&[DetailSection::new(
                "This access",
                access_rows(&inspected.access),
            )])));
            match inspected.resource {
                Some(source) => {
                    children.push(Box::new(detail_panel(&[DetailSection::new(
                        "Shared Resource",
                        vec![
                            DetailRow::new("Address", &source.address),
                            DetailRow::new("Resource identity", source.id.to_string()),
                            DetailRow::new(
                                "Tags",
                                if source.tags.is_empty() {
                                    "No held tags".to_owned()
                                } else {
                                    source.tags.join(", ")
                                },
                            ),
                        ],
                    )])));
                    children.push(Box::new(el("p", "Resource facts are shared by its accesses. Titles above describe this access.")));
                    if source.facts.is_empty() {
                        children.push(Box::new(el("p", "No Resource facts held here")));
                    } else {
                        children.push(Box::new(detail_panel(&[DetailSection::new(
                            "Resource facts",
                            source
                                .facts
                                .iter()
                                .map(|fact| {
                                    DetailRow::new(
                                        &fact.key,
                                        serde_json::to_string(&fact.value)
                                            .expect("JSON value serializes"),
                                    )
                                })
                                .collect(),
                        )])));
                    }
                    children.push(Box::new(el("h3", "Other accesses held here")));
                    children.push(Box::new(el("p", "Only accesses held in this graph are listed. Other accesses may exist elsewhere.")));
                    if source.other_accesses.is_empty() {
                        children.push(Box::new(el("p", "No other accesses held here")));
                    } else {
                        children.push(Box::new(el(
                            "ul",
                            source
                                .other_accesses
                                .into_iter()
                                .map(|access| {
                                    let member = access.member;
                                    let label = if access.title.is_empty() {
                                        &access.address
                                    } else {
                                        &access.title
                                    };
                                    el(
                                        "li",
                                        (
                                            detail_panel(&[DetailSection::new(
                                                "Access",
                                                access_rows(&access),
                                            )]),
                                            button(
                                                format!("Inspect access: {label}"),
                                                move |page: &mut TreePage, _| {
                                                    page.inspected_access = Some(member);
                                                },
                                            )
                                            .attr("data-inspect-access", member.to_string()),
                                        ),
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )));
                    }
                },
                None => {
                    let message = match inspected.resource_id {
                        Some(id) => format!("The recorded Resource {id} is not held here"),
                        None => "No Resource association is recorded for this access".to_owned(),
                    };
                    children.push(Box::new(el("p", message).attr("role", "status")));
                },
            }
            if !inspected.coverage.limits.is_empty() {
                children.push(Box::new(el("h3", "Known limits")));
                children.push(Box::new(el(
                    "ul",
                    inspected
                        .coverage
                        .limits
                        .into_iter()
                        .map(|limit| {
                            let count = limit
                                .count
                                .map(|count| format!(" · {count}"))
                                .unwrap_or_default();
                            el("li", format!("{:?}: {}{count}", limit.layer, limit.reason))
                        })
                        .collect::<Vec<_>>(),
                )));
            }
        },
    }
    Box::new(
        el("section", children)
            .attr("class", "tree-inspection")
            .attr("aria-label", "Access inspector"),
    )
}
