// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Independent, read-only link and exact-assertion inspection.
use super::*;
use cambium::{DetailRow, DetailSection, button, detail_panel};
use graphshell::link_inspection::{
    HeldStatement, LinkHistoryInspection, LinkInspection, LinkOwner, LinkTarget,
    StatementInspection,
};
use mere::kernel::graph::{GraphStratum, SemanticSubKind};
use uuid::Uuid;

fn read(page: &TreePage) -> Result<LinkInspection, String> {
    let target = page.inspected_link.ok_or("Choose a link to inspect")?;
    page.product
        .as_ref()
        .ok_or("Link inspection requires the local graph")?
        .inspect_link(target)
}

fn exact(page: &TreePage) -> Result<StatementInspection, String> {
    let statement = page
        .inspected_statement
        .as_deref()
        .ok_or("Choose an assertion to inspect")?;
    let product = page
        .product
        .as_ref()
        .ok_or("Assertion inspection requires the local graph")?;
    if let Some(target) = page
        .inspected_link_member
        .as_ref()
        .filter(|target| target.assertion_id == statement)
    {
        if product.link_source()? != target.source {
            return Err("The assertion's source context changed; reopen the link".into());
        }
        let stratum = match target.link.stratum {
            scenograph::link::LinkStratum::Surface => GraphStratum::Surface,
            scenograph::link::LinkStratum::Resource => GraphStratum::Resource,
        };
        let from =
            Uuid::parse_str(&target.from).map_err(|_| "Invalid assertion source endpoint")?;
        let to = Uuid::parse_str(&target.to).map_err(|_| "Invalid assertion source endpoint")?;
        product.inspect_statement_at_owner(statement, stratum, from, to)
    } else {
        product.inspect_statement(statement)
    }
}

fn read_history(page: &TreePage) -> Result<LinkHistoryInspection, String> {
    let target = page.inspected_link.ok_or("Choose a link to inspect")?;
    page.product
        .as_ref()
        .ok_or("Link history inspection requires the local graph")?
        .inspect_link_history(target)
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let target = page.inspected_link;
    let snapshot = snapshot
        .with_field("link-inspector-open", target.is_some().to_string())
        .with_field(
            "link-inspector-first",
            target.map(|t| t.first.to_string()).unwrap_or_default(),
        )
        .with_field(
            "link-inspector-second",
            target.map(|t| t.second.to_string()).unwrap_or_default(),
        )
        .with_field(
            "link-inspector-scope",
            if page.link_history_open {
                "history"
            } else {
                "held"
            },
        );
    let snapshot = match read(page) {
        Ok(inspected) => {
            let directions = inspected
                .records
                .iter()
                .map(|record| {
                    let address = |id| {
                        if id == inspected.first.member {
                            &inspected.first.address
                        } else {
                            &inspected.second.address
                        }
                    };
                    format!(
                        "{} -> {}",
                        address(record.displayed_from),
                        address(record.displayed_to)
                    )
                })
                .collect::<Vec<_>>()
                .join("|");
            let mut strata: Vec<_> = inspected
                .records
                .iter()
                .map(|record| match record.owner.stratum {
                    GraphStratum::Surface => "surface",
                    GraphStratum::Resource => "resource",
                })
                .collect();
            strata.sort();
            strata.dedup();
            snapshot
                .with_field("link-inspector-ready", "true")
                .with_field(
                    "link-inspector-records",
                    inspected.records.len().to_string(),
                )
                .with_field(
                    "link-inspector-statements",
                    inspected
                        .records
                        .iter()
                        .map(|record| record.statements.len())
                        .sum::<usize>()
                        .to_string(),
                )
                .with_field("link-inspector-directions", directions)
                .with_field("link-inspector-strata", strata.join("|"))
                .with_field("link-inspector-error", "")
        },
        Err(error) => snapshot
            .with_field("link-inspector-ready", "false")
            .with_field("link-inspector-records", "0")
            .with_field("link-inspector-statements", "0")
            .with_field("link-inspector-directions", "")
            .with_field("link-inspector-strata", "")
            .with_field(
                "link-inspector-error",
                if target.is_some() {
                    error
                } else {
                    String::new()
                },
            ),
    };
    let snapshot = if page.link_history_open {
        match read_history(page) {
            Ok(history) => snapshot
                .with_field("link-inspector-history-ready", "true")
                .with_field(
                    "link-inspector-history-count",
                    history.entries.len().to_string(),
                )
                .with_field(
                    "link-inspector-history-ids",
                    history
                        .entries
                        .iter()
                        .map(|entry| entry.assertion.id.as_str())
                        .collect::<Vec<_>>()
                        .join("|"),
                )
                .with_field(
                    "link-inspector-history-inspectable-ids",
                    history
                        .entries
                        .iter()
                        .filter(|entry| entry.assertion.withdrawn.is_none())
                        .map(|entry| entry.assertion.id.as_str())
                        .collect::<Vec<_>>()
                        .join("|"),
                )
                .with_field("link-inspector-history-error", ""),
            Err(error) => snapshot
                .with_field("link-inspector-history-ready", "false")
                .with_field("link-inspector-history-count", "0")
                .with_field("link-inspector-history-ids", "")
                .with_field("link-inspector-history-inspectable-ids", "")
                .with_field("link-inspector-history-error", error),
        }
    } else {
        snapshot
            .with_field("link-inspector-history-ready", "false")
            .with_field("link-inspector-history-count", "0")
            .with_field("link-inspector-history-ids", "")
            .with_field("link-inspector-history-inspectable-ids", "")
            .with_field("link-inspector-history-error", "")
    };
    let snapshot = match opened_reading(page) {
        Ok(reading) => {
            let layout = opened_layout(page, &reading);
            let now_ms = page
                .product
                .as_ref()
                .map(|product| product.opened_link_time_ms());
            match scenomise::compile_opened_link(
                &reading,
                &layout,
                scenograph::link::OpenedLinkOptions { now_ms },
            ) {
                Ok(opened) => snapshot
                    .with_field("opened-link-ready", "true")
                    .with_field(
                        "opened-link-layout",
                        match layout {
                            scenograph::link::OpenedLinkLayout::List { .. } => "list",
                            scenograph::link::OpenedLinkLayout::Timeline { .. } => "timeline",
                        },
                    )
                    .with_field("opened-link-members", opened.entries.len().to_string())
                    .with_field("opened-link-lanes", opened.lanes.len().to_string())
                    .with_field(
                        "opened-link-undated",
                        opened
                            .lanes
                            .iter()
                            .filter(|lane| lane.undated)
                            .map(|lane| lane.row_count)
                            .sum::<usize>()
                            .to_string(),
                    )
                    .with_field(
                        "opened-link-unknown-withdrawal",
                        opened
                            .entries
                            .iter()
                            .filter(|entry| {
                                entry.span.as_ref().is_some_and(|span| {
                                    matches!(
                                        span.end,
                                        scenograph::link::LinkSpanEnd::WithdrawalTimeUnknown
                                    )
                                })
                            })
                            .count()
                            .to_string(),
                    )
                    .with_field(
                        "opened-link-source-available",
                        matches!(
                            reading.source,
                            scenograph::link::LinkSource::Available { .. }
                        )
                        .to_string(),
                    )
                    .with_field(
                        "opened-link-error",
                        page.opened_link_error.as_deref().unwrap_or(""),
                    ),
                Err(error) => snapshot
                    .with_field("opened-link-ready", "false")
                    .with_field("opened-link-error", error.to_string()),
            }
        },
        Err(error) => snapshot
            .with_field("opened-link-ready", "false")
            .with_field(
                "opened-link-error",
                if page.inspected_link.is_some() {
                    error
                } else {
                    String::new()
                },
            ),
    };
    let snapshot = snapshot
        .with_field(
            "assertion-inspector-open",
            page.inspected_statement.is_some().to_string(),
        )
        .with_field(
            "assertion-inspector-handle",
            page.inspected_statement.clone().unwrap_or_default(),
        );
    match exact(page) {
        Ok(inspected) => snapshot
            .with_field("assertion-inspector-ready", "true")
            .with_field(
                "assertion-inspector-predicate",
                inspected.statement.predicate,
            )
            .with_field(
                "assertion-inspector-from",
                inspected.owner.from.id.to_string(),
            )
            .with_field("assertion-inspector-to", inspected.owner.to.id.to_string())
            .with_field("assertion-inspector-error", ""),
        Err(error) => snapshot
            .with_field("assertion-inspector-ready", "false")
            .with_field("assertion-inspector-predicate", "")
            .with_field("assertion-inspector-from", "")
            .with_field("assertion-inspector-to", "")
            .with_field(
                "assertion-inspector-error",
                if page.inspected_statement.is_some() {
                    error
                } else {
                    String::new()
                },
            ),
    }
}

pub(super) fn access_links(page: &TreePage, member: Uuid) -> Child {
    let mut children: Vec<Child> = vec![Box::new(el("h3", "Links held for this access"))];
    match page
        .product
        .as_ref()
        .ok_or_else(|| "Link inspection requires the local graph".to_owned())
        .and_then(|product| product.linked_accesses(member))
    {
        Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
        Ok(accesses) if accesses.is_empty() => {
            children.push(Box::new(el("p", "No links to held accesses")))
        },
        Ok(accesses) => {
            children.push(Box::new(el(
                "ul",
                accesses
                    .into_iter()
                    .map(|access| {
                        let target = LinkTarget::new(member, access.member);
                        let label = if access.title.is_empty() {
                            &access.address
                        } else {
                            &access.title
                        };
                        el(
                            "li",
                            button(
                                format!("Inspect links: {label}"),
                                move |page: &mut TreePage, _| {
                                    page.inspected_link = Some(target);
                                    page.opened_link_layout = None;
                                    page.opened_link_error = None;
                                    page.inspected_statement = None;
                                    page.inspected_link_member = None;
                                    page.tools_open = true;
                                },
                            )
                            .attr("data-inspect-link", access.member.to_string()),
                        )
                    })
                    .collect::<Vec<_>>(),
            )));
        },
    }
    Box::new(el("div", children))
}

fn owner_rows(owner: &LinkOwner) -> Vec<DetailRow> {
    let address = |endpoint: &graphshell::link_inspection::SourceEndpoint| {
        endpoint
            .address
            .clone()
            .unwrap_or_else(|| "Address not held here".into())
    };
    vec![
        DetailRow::new(
            "Source stratum",
            match owner.stratum {
                GraphStratum::Surface => "Access",
                GraphStratum::Resource => "Resource",
            },
        ),
        DetailRow::new("Owned from", address(&owner.from)),
        DetailRow::new("From identity", owner.from.id.to_string()),
        DetailRow::new("Owned to", address(&owner.to)),
        DetailRow::new("To identity", owner.to.id.to_string()),
    ]
}

fn statement_rows(statement: &HeldStatement) -> Vec<DetailRow> {
    let mut rows = vec![
        DetailRow::new("Assertion handle", &statement.statement_id),
        DetailRow::new("Predicate", &statement.predicate),
        DetailRow::new(
            "Recognized kind",
            statement
                .recognized_sub_kind
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "Open predicate".into()),
        ),
        DetailRow::new(
            "Label",
            statement
                .label
                .clone()
                .unwrap_or_else(|| "No held label".into()),
        ),
        DetailRow::new(
            "Graph scope",
            serde_json::to_string(&statement.graph_scope).expect("scope serializes"),
        ),
        DetailRow::new(
            "First asserted (ms)",
            statement
                .first_asserted_ms
                .map(|time| time.to_string())
                .unwrap_or_else(|| "Undated".into()),
        ),
        DetailRow::new(
            "Basis note",
            statement
                .basis
                .note
                .clone()
                .unwrap_or_else(|| "No basis note held".into()),
        ),
        DetailRow::new(
            "Basis method",
            statement
                .basis
                .method
                .as_ref()
                .map(|method| match &method.version {
                    Some(version) => format!("{} ({version})", method.iri),
                    None => method.iri.clone(),
                })
                .unwrap_or_else(|| "No method held".into()),
        ),
        DetailRow::new(
            "Basis evidence",
            if statement.basis.evidence.is_empty() {
                "No evidence held".into()
            } else {
                statement.basis.evidence.join(", ")
            },
        ),
        DetailRow::new(
            "Asserter",
            statement
                .provenance_iri
                .clone()
                .unwrap_or_else(|| "Attribution not held here".into()),
        ),
        DetailRow::new(
            "Assertion time (ms)",
            statement
                .asserted_at_ms
                .map(|time| time.to_string())
                .unwrap_or_else(|| "Time not held here".into()),
        ),
    ];
    if statement.recognized_sub_kind == Some(SemanticSubKind::AgentDerived) {
        rows.push(DetailRow::new(
            "Derivation",
            "Rule and inputs are not held in this assertion record",
        ));
    }
    rows
}

fn opened_reading(page: &TreePage) -> Result<scenograph::link::LinkPairReading, String> {
    let target = page.inspected_link.ok_or("Choose a link to inspect")?;
    page.product
        .as_ref()
        .ok_or("Link reading requires the local graph")?
        .opened_link(target, page.link_history_open)
}

fn opened_layout(
    page: &TreePage,
    reading: &scenograph::link::LinkPairReading,
) -> scenograph::link::OpenedLinkLayout {
    page.opened_link_layout
        .clone()
        .unwrap_or_else(|| scenograph::link::OpenedLinkLayout::default_for(reading))
}

fn opened_event(page: &mut TreePage, event: cambium::OpenedLinkEvent) {
    match event {
        cambium::OpenedLinkEvent::Layout(layout) => {
            page.opened_link_layout = Some(layout);
            page.opened_link_error = None;
        },
        cambium::OpenedLinkEvent::Member { target, action } => {
            // A retained UI callback resolves against today's source reading.
            // A view ordinal never becomes an assertion or an action target.
            let valid = opened_reading(page).is_ok_and(|reading| {
                reading.source == target.source
                    && reading.links.iter().any(|link| {
                        link.key == target.link
                            && link.buckets.iter().any(|bucket| {
                                bucket.source_from.id == target.from
                                    && bucket.source_to.id == target.to
                                    && bucket.assertions.iter().any(|assertion| {
                                        assertion.id == target.assertion_id
                                            && assertion.withdrawn.is_none()
                                    })
                            })
                    })
            });
            if !valid {
                page.opened_link_error = Some(
                    "This assertion is no longer available for that action; reopen the link."
                        .into(),
                );
            } else if action == "inspect" {
                page.inspected_statement = Some(target.assertion_id.clone());
                page.inspected_link_member = Some(target);
                page.opened_link_error = None;
            } else if action == "hide" {
                let canvas = page.shared.canvas.borrow();
                let result = page
                    .product
                    .as_mut()
                    .ok_or_else(|| "Assertion hiding requires the local graph".to_owned())
                    .and_then(|product| product.hide_assertion(&canvas, &target.assertion_id));
                match result {
                    Ok(()) => page.opened_link_error = None,
                    Err(error) => page.opened_link_error = Some(error),
                }
                page.shared.dirty.set(true);
            } else if action == "withdraw" {
                let can_withdraw = page
                    .product
                    .as_ref()
                    .is_some_and(|product| product.can_withdraw_link_assertion(&target));
                if !can_withdraw {
                    page.opened_link_error = Some(
                        "Only a personal assertion authored by the current persona can be withdrawn here.".into(),
                    );
                    return;
                }
                let result = page
                    .product
                    .as_mut()
                    .ok_or_else(|| "Assertion withdrawal requires the local graph".to_owned())
                    .and_then(|product| product.withdraw_link_assertion(&target));
                match result {
                    Ok(()) => page.opened_link_error = None,
                    Err(error) => page.opened_link_error = Some(error),
                }
                page.shared.dirty.set(true);
            } else {
                page.opened_link_error = Some("Unknown assertion view action".into());
            }
        },
    }
}

pub(super) fn section(page: &TreePage) -> Child {
    if page.inspected_link.is_none() {
        return Box::new(el("div", ()));
    }
    let mut children: Vec<Child> = vec![
        Box::new(el("h2", "Link inspector")),
        Box::new(button("Explain pair dynamics", |page: &mut TreePage, _| {
            if let Some(pair) = page.inspected_link {
                dynamics_inspection::choose(
                    page,
                    mere::canvas::DynamicsTarget::Pair {
                        first: pair.first,
                        second: pair.second,
                    },
                );
            }
        })),
        Box::new(button("Close link inspection", |page: &mut TreePage, _| {
            page.inspected_link = None;
            page.inspected_statement = None;
            page.inspected_link_member = None;
            page.link_history_open = false;
            page.opened_link_layout = None;
            page.opened_link_error = None;
        })),
        Box::new(button(
            if page.link_history_open {
                "Show held assertions"
            } else {
                "Show retained history"
            },
            |page: &mut TreePage, _| {
                page.link_history_open = !page.link_history_open;
            },
        )),
        Box::new(el(
            "p",
            if page.link_history_open {
                "Retained assertion history from this graph snapshot is shown, including withdrawals; this is not a journal replay. Coverage may be incomplete, and other records may exist elsewhere."
            } else {
                "Only currently held source records are shown. Coverage may be incomplete, and other records may exist elsewhere."
            },
        )),
    ];
    let hidden_assertion_count = page.shared.canvas.borrow().hidden_assertion_ids().len();
    if hidden_assertion_count > 0 {
        children.push(Box::new(
            button("Show hidden assertions", |page: &mut TreePage, _| {
                let canvas = page.shared.canvas.borrow();
                if let Some(product) = &mut page.product {
                    if let Err(error) = product.show_hidden_assertions(&canvas) {
                        product.status = error;
                    }
                }
                page.shared.dirty.set(true);
            })
            .attr("data-assertion-visibility", "show"),
        ));
    }
    if let Some(product) = &page.product {
        let (state, message) = product.assertion_visibility_state();
        if state != "idle" {
            children.push(Box::new(
                el("span", message.to_owned())
                    .attr("role", "status")
                    .attr("data-assertion-visibility-status", state),
            ));
        }
    }
    match opened_reading(page) {
        Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
        Ok(reading) => {
            let layout = opened_layout(page, &reading);
            let history = page.link_history_open;
            let now_ms = page
                .product
                .as_ref()
                .map(|product| product.opened_link_time_ms());
            let expected_source = reading.source.clone();
            for (index, link) in reading.links.iter().enumerate() {
                let key = link.key.clone();
                let key_token = key.stable_id();
                children.push(Box::new(
                    button(format!("Freeze {:?} link for review", key.stratum), {
                        let expected_source = expected_source.clone();
                        move |page: &mut TreePage, _| {
                            promoted_links::freeze(page, &key, &expected_source)
                        }
                    })
                    .attr("data-freeze-link", key_token),
                ));
                let swatch = scenograph::swatch::Swatch {
                    id: format!("opened-link-{index}"),
                    scope: scenograph::swatch::Scope::Link(link.key.clone()),
                    // This host resolves the ephemeral preset to `layout`;
                    // no authored recipe or pair entity is persisted.
                    mode: scenograph::swatch::SwatchMode::Projection {
                        definition_id: "opened-link".into(),
                        variant_id: None,
                    },
                };
                children.push(Box::new(cambium::opened_link_swatch(
                    &swatch,
                    &reading,
                    &layout,
                    scenograph::link::OpenedLinkOptions { now_ms },
                    history,
                    move |source, key, bucket, assertion| {
                        if assertion.withdrawn.is_some() {
                            return Vec::new();
                        }
                        let mut actions = vec![
                            cambium::LinkMemberAction {
                                id: "inspect".into(),
                                label: if history {
                                    "Inspect held assertion {number}"
                                } else {
                                    "Inspect assertion {number}"
                                }
                                .into(),
                            },
                            cambium::LinkMemberAction {
                                id: "hide".into(),
                                label: "Hide this assertion from the view {number}".into(),
                            },
                        ];
                        let target = cambium::LinkMemberTarget {
                            source: source.clone(),
                            link: key.clone(),
                            from: bucket.source_from.id.clone(),
                            to: bucket.source_to.id.clone(),
                            assertion_id: assertion.id.clone(),
                        };
                        if page
                            .product
                            .as_ref()
                            .is_some_and(|product| product.can_withdraw_link_assertion(&target))
                        {
                            actions.push(cambium::LinkMemberAction {
                                id: "withdraw".into(),
                                label: "Withdraw this personal assertion {number}".into(),
                            });
                        }
                        actions
                    },
                    |page: &mut TreePage, event| opened_event(page, event),
                )));
            }
        },
    }
    if let Some(error) = &page.opened_link_error {
        children.push(Box::new(el("p", error.clone()).attr("role", "status")));
    }
    if page.inspected_statement.is_some() {
        children.push(Box::new(el("h3", "Exact assertion")));
        children.push(Box::new(button(
            "Close assertion inspection",
            |page: &mut TreePage, _| {
                page.inspected_statement = None;
                page.inspected_link_member = None;
            },
        )));
        match exact(page) {
            Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
            Ok(inspected) => {
                children.push(Box::new(detail_panel(&[
                    DetailSection::new("Assertion owner", owner_rows(&inspected.owner)),
                    DetailSection::new("Held assertion", statement_rows(&inspected.statement)),
                ])));
            },
        }
    }
    Box::new(
        el("section", children)
            .attr("class", "tree-inspection")
            .attr("aria-label", "Link inspector"),
    )
}
