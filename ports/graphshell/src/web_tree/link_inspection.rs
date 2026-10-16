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
    page.product
        .as_ref()
        .ok_or("Assertion inspection requires the local graph")?
        .inspect_statement(statement)
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
                                    page.inspected_statement = None;
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

fn render_history(history: LinkHistoryInspection, children: &mut Vec<Child>) {
    children.push(Box::new(detail_panel(&[DetailSection::new(
        "Displayed accesses",
        vec![
            DetailRow::new("First access", &history.first.address),
            DetailRow::new("First identity", history.first.member.to_string()),
            DetailRow::new("Second access", &history.second.address),
            DetailRow::new("Second identity", history.second.member.to_string()),
        ],
    )])));
    for note in history.notes {
        children.push(Box::new(el("p", note).attr("role", "status")));
    }
    if history.entries.is_empty() {
        children.push(Box::new(el(
            "p",
            "No retained assertion history held for this pair",
        )));
    }
    for (index, entry) in history.entries.into_iter().enumerate() {
        let assertion = entry.assertion;
        let first = assertion
            .first_asserted_ms
            .map(|time| time.to_string())
            .unwrap_or_else(|| "Undated".into());
        let lifecycle_end = match &assertion.withdrawn {
            Some(withdrawal) => withdrawal
                .at_ms
                .map(|time| format!("withdrawn at {time}"))
                .unwrap_or_else(|| "withdrawn; time unavailable".into()),
            None => "open (currently held)".into(),
        };
        let basis_method = assertion
            .basis
            .method
            .as_ref()
            .map(|method| match &method.version {
                Some(version) => format!("{} ({version})", method.iri),
                None => method.iri.clone(),
            })
            .unwrap_or_else(|| "No method held".into());
        let rows = vec![
            DetailRow::new("Assertion handle", &assertion.id),
            DetailRow::new("Predicate", &assertion.predicate),
            DetailRow::new(
                "Displayed direction",
                format!("{} → {}", entry.displayed_from, entry.displayed_to),
            ),
            DetailRow::new("Source stratum", format!("{:?}", entry.owner.stratum)),
            DetailRow::new(
                "First to lifecycle end",
                format!("{first} to {lifecycle_end}"),
            ),
            DetailRow::new(
                "Withdrawal by",
                assertion
                    .withdrawn
                    .as_ref()
                    .and_then(|withdrawal| withdrawal.by.clone())
                    .unwrap_or_else(|| {
                        if assertion.withdrawn.is_some() {
                            "Attribution not held here".into()
                        } else {
                            "Currently held".into()
                        }
                    }),
            ),
            DetailRow::new(
                "Last confirmation (ms)",
                assertion
                    .asserted_at_ms
                    .map(|time| time.to_string())
                    .unwrap_or_else(|| "No confirmation time held".into()),
            ),
            DetailRow::new(
                "Basis note",
                assertion
                    .basis
                    .note
                    .clone()
                    .unwrap_or_else(|| "No basis note held".into()),
            ),
            DetailRow::new("Basis method", basis_method),
            DetailRow::new(
                "Basis evidence",
                if assertion.basis.evidence.is_empty() {
                    "No evidence held".into()
                } else {
                    assertion.basis.evidence.join(", ")
                },
            ),
        ];
        children.push(Box::new(detail_panel(&[DetailSection::new(
            format!("Retained assertion {}", index + 1),
            rows,
        )])));
        if assertion.withdrawn.is_none() {
            let handle = assertion.id.clone();
            children.push(Box::new(
                button(
                    format!("Inspect held assertion {}", index + 1),
                    move |page: &mut TreePage, _| {
                        page.inspected_statement = Some(handle.clone());
                    },
                )
                .attr("data-inspect-statement", assertion.id),
            ));
        }
    }
    if !history.coverage.limits.is_empty() {
        children.push(Box::new(el("h3", "Known limits")));
        children.push(Box::new(el(
            "ul",
            history
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
            page.link_history_open = false;
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
    if page.link_history_open {
        match read_history(page) {
            Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
            Ok(history) => render_history(history, &mut children),
        }
    } else {
        match read(page) {
            Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
            Ok(inspected) => {
                children.push(Box::new(detail_panel(&[DetailSection::new(
                    "Displayed accesses",
                    vec![
                        DetailRow::new("First access", &inspected.first.address),
                        DetailRow::new("First identity", inspected.first.member.to_string()),
                        DetailRow::new("Second access", &inspected.second.address),
                        DetailRow::new("Second identity", inspected.second.member.to_string()),
                    ],
                )])));
                for note in inspected.notes {
                    children.push(Box::new(el("p", note).attr("role", "status")));
                }
                if inspected.records.is_empty() {
                    children.push(Box::new(el(
                        "p",
                        "No contributing source records held for this pair",
                    )));
                }
                let mut assertion_number = 0;
                for (index, record) in inspected.records.into_iter().enumerate() {
                    let displayed_address = |id| {
                        if id == inspected.first.member {
                            &inspected.first.address
                        } else {
                            &inspected.second.address
                        }
                    };
                    let mut rows = vec![DetailRow::new(
                        "Displayed direction",
                        format!(
                            "{} → {}",
                            displayed_address(record.displayed_from),
                            displayed_address(record.displayed_to)
                        ),
                    )];
                    rows.extend(owner_rows(&record.owner));
                    children.push(Box::new(detail_panel(&[DetailSection::new(
                        format!("Source record {}", index + 1),
                        rows,
                    )])));
                    for family in record.families {
                        let label = if family.family == mere::kernel::graph::EdgeFamily::Semantic {
                            "Semantic grouping summary".into()
                        } else {
                            format!("{:?} data", family.family)
                        };
                        children.push(Box::new(detail_panel(&[DetailSection::new(
                            label,
                            vec![DetailRow::new(
                                "Held values",
                                serde_json::to_string(&family.data).expect("JSON value serializes"),
                            )],
                        )])));
                    }
                    for statement in record.statements {
                        assertion_number += 1;
                        let handle = statement.statement_id.clone();
                        children.push(Box::new(detail_panel(&[DetailSection::new(
                            format!("Assertion {assertion_number}"),
                            statement_rows(&statement),
                        )])));
                        children.push(Box::new(
                            button(
                                format!("Inspect assertion {assertion_number}"),
                                move |page: &mut TreePage, _| {
                                    page.inspected_statement = Some(handle.clone());
                                },
                            )
                            .attr("data-inspect-statement", statement.statement_id),
                        ));
                    }
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
    }
    if page.inspected_statement.is_some() {
        children.push(Box::new(el("h3", "Exact assertion")));
        children.push(Box::new(button(
            "Close assertion inspection",
            |page: &mut TreePage, _| {
                page.inspected_statement = None;
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
